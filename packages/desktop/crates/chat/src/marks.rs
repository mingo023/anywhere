use std::iter::Peekable;
use std::ops::Range;
use std::str::CharIndices;

/// What a prompt's text marks out: the commands and skills it names and the files it mentions, as byte ranges.
#[derive(Debug, Default, PartialEq)]
pub struct Marks {
    pub commands: Vec<Range<usize>>,
    pub mentions: Vec<Range<usize>>,
}

/// Each `/skill` and `@path` in `text`, at its start or after whitespace. A skill's name starts with a letter and may
/// end in punctuation, as `/review,`, but not run on into a path, as `/usr/bin` or `/notes.md`. An MCP server's tag
/// names no file.
pub fn marks(text: &str) -> Marks {
    let mut marks = Marks::default();
    let mut after_space = true;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if after_space && c == '@' {
            let end = take(&mut chars, i + 1, |c| c.is_alphanumeric() || matches!(c, '_' | '.' | '/' | '-'));
            if end > i + 1 && !text[i..end].starts_with(crate::mcp::TAG) {
                marks.mentions.push(i..end);
            }
        } else if after_space && c == '/' && text[i + 1..].starts_with(char::is_alphabetic) {
            let name = take(&mut chars, i + 1, |c| c.is_alphanumeric() || matches!(c, '_' | '-' | ':'));
            let end = i + 1 + text[i + 1..name].trim_end_matches(':').len();
            let tail = take(&mut chars, name, |c| !c.is_whitespace());
            if text[end..tail].chars().all(|c| !c.is_alphanumeric() && c != '/') {
                marks.commands.push(i..end);
            }
        }
        after_space = c.is_whitespace();
    }
    marks
}

/// Takes the chars `part` holds off the front of `chars`, and returns where they end; `from` when there are none.
fn take(chars: &mut Peekable<CharIndices>, from: usize, part: impl Fn(char) -> bool) -> usize {
    let mut end = from;
    while let Some((j, c)) = chars.next_if(|&(_, c)| part(c)) {
        end = j + c.len_utf8();
    }
    end
}

/// A prompt's code, as byte ranges.
#[derive(Debug, PartialEq)]
pub enum Code {
    /// A span between single backticks on one line, its backticks included.
    Inline(Range<usize>),
    /// A fenced block: its opening fence with the language, and its closing fence.
    Block { open: Range<usize>, close: Range<usize> },
}

impl Code {
    /// Where the code starts and ends, its backticks included.
    pub fn range(&self) -> Range<usize> {
        match self {
            Code::Inline(r) => r.clone(),
            Code::Block { open, close, .. } => open.start..close.end,
        }
    }
}

/// Each closed code span and fenced block in `text`. Nothing after an unclosed fence is code yet, as it may still be
/// the block being typed.
pub fn code(text: &str) -> Vec<Code> {
    const FENCE: &str = "```";
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find(FENCE).map(|i| from + i) {
        found.extend(inline_code(text, from..open));
        let after = open + FENCE.len();
        let Some(close) = text[after..].find(FENCE).map(|i| after + i) else { return found };
        let language = text[after..close].find('\n').map(|n| after + n).filter(|&end| text[after..end].chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '+' | '-')));
        found.push(Code::Block { open: open..language.unwrap_or(after), close: close..close + FENCE.len() });
        from = close + FENCE.len();
    }
    found.extend(inline_code(text, from..text.len()));
    found
}

/// The spans in `within` between single backticks on one line; a backtick right after another opens none.
fn inline_code(text: &str, within: Range<usize>) -> Vec<Code> {
    let mut found = Vec::new();
    let mut from = within.start;
    while let Some(start) = text[from..within.end].find('`').map(|i| from + i) {
        let body = start + 1;
        if text[..start].ends_with('`') {
            from = body;
            continue;
        }
        match text[body..within.end].find(['`', '\n']).map(|i| body + i) {
            Some(end) if end > body && text[end..].starts_with('`') => {
                found.push(Code::Inline(start..end + 1));
                from = end + 1;
            }
            Some(end) => from = end.max(body),
            None => break,
        }
    }
    found
}

/// Whether any of `r` is in `code`.
pub(crate) fn in_code(code: &[Code], r: &Range<usize>) -> bool {
    code.iter().map(Code::range).any(|c| c.start < r.end && r.start < c.end)
}

/// `text`'s marks outside its code, which reads as typed, and its code.
pub fn marks_outside_code(text: &str) -> (Marks, Vec<Code>) {
    let code = code(text);
    let mut marks = marks(text);
    marks.commands.retain(|r| !in_code(&code, r));
    marks.mentions.retain(|r| !in_code(&code, r));
    (marks, code)
}

/// What a mention shows: its file's name, or a folder's name and `/`.
pub fn mention_name(path: &str) -> &str {
    let trimmed = path.strip_suffix('/').unwrap_or(path);
    let start = trimmed.rfind('/').map_or(0, |i| i + 1);
    &path[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked(text: &str) -> (Vec<&str>, Vec<&str>) {
        let m = marks(text);
        (m.commands.into_iter().map(|r| &text[r]).collect(), m.mentions.into_iter().map(|r| &text[r]).collect())
    }

    #[test]
    fn a_command_that_opens_the_text_is_marked() {
        assert_eq!(marked("  /review 482").0, ["/review"]);
        assert_eq!(marked("/goal tests pass").0, ["/goal"]);
        assert_eq!(marked("/usage/report.md is empty").0, Vec::<&str>::new());
    }

    #[test]
    fn every_skill_a_prompt_names_is_marked() {
        assert_eq!(marked("/prototype /apple-design cho mình xem thử").0, ["/prototype", "/apple-design"]);
        assert_eq!(marked("try /figma:figma-use\nthen /review").0, ["/figma:figma-use", "/review"]);
        assert_eq!(marked("@src/chat.rs then /review"), (vec!["/review"], vec!["@src/chat.rs"]));
    }

    #[test]
    fn a_skill_keeps_its_mark_before_punctuation() {
        let found = marked("/prototype, /apple-design. why /model? or /review: and /done…").0;
        assert_eq!(found, ["/prototype", "/apple-design", "/model", "/review", "/done"]);
    }

    #[test]
    fn paths_and_numbers_stay_plain() {
        assert_eq!(marked("see /usr/bin, /notes.md and and/or, 1 /2").0, Vec::<&str>::new());
    }

    #[test]
    fn mentions_start_the_text_or_follow_whitespace() {
        let (_, found) = marked("@src/chat.rs and\n@a_b-c.md, then @docs/");
        assert_eq!(found, ["@src/chat.rs", "@a_b-c.md", "@docs/"]);
    }

    #[test]
    fn an_mcp_tag_is_no_file() {
        assert_eq!(marked("use @mcp/fff and @mcp.json").1, ["@mcp.json"]);
    }

    #[test]
    fn emails_and_a_lone_at_stay_plain() {
        assert_eq!(marked("mail dev@pocket.app or a/b @ alone").1, Vec::<&str>::new());
    }

    fn coded(text: &str) -> Vec<&str> {
        code(text).into_iter().map(|c| &text[c.range()]).collect()
    }

    #[test]
    fn a_span_between_backticks_on_one_line_is_code() {
        assert_eq!(coded("Check `composer.rs` and `a b`"), ["`composer.rs`", "`a b`"]);
        for plain in ["`open", "a `` b", "`a\nb`", "``a``"] {
            assert_eq!(coded(plain), Vec::<&str>::new(), "{plain:?}");
        }
    }

    #[test]
    fn a_fenced_block_has_its_fence_and_language_then_its_close() {
        let text = "See\n```rust\nfn a() {}\n```\nthen `b`";
        let found = code(text);
        let Code::Block { open, close } = &found[0] else { panic!("{found:?}") };
        assert_eq!((&text[open.clone()], &text[close.clone()]), ("```rust", "```"));
        assert_eq!(&text[found[1].range()], "`b`");
    }

    #[test]
    fn a_fence_with_code_on_its_line_has_no_language() {
        let text = "```let a = 1;```";
        let Code::Block { open, .. } = &code(text)[0] else { panic!() };
        assert_eq!(&text[open.clone()], "```");
    }

    #[test]
    fn marks_inside_code_read_as_typed() {
        let text = "`@a.rs /x` and /review @b.rs";
        let (marks, code) = marks_outside_code(text);
        let read = |ranges: Vec<std::ops::Range<usize>>| ranges.into_iter().map(|r| &text[r]).collect::<Vec<_>>();
        assert_eq!((read(marks.commands), read(marks.mentions), code.len()), (vec!["/review"], vec!["@b.rs"], 1));
    }

    #[test]
    fn nothing_after_an_unclosed_fence_is_code_yet() {
        assert_eq!(coded("`a` then ```rust\nfn `b`"), ["`a`"]);
    }

    #[test]
    fn a_mention_shows_its_files_name_or_its_folders_name_and_slash() {
        assert_eq!(mention_name("src/parser.rs"), "parser.rs");
        assert_eq!(mention_name("tsconfig.json"), "tsconfig.json");
        assert_eq!(mention_name("packages/src/"), "src/");
    }
}
