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

    #[test]
    fn a_mention_shows_its_files_name_or_its_folders_name_and_slash() {
        assert_eq!(mention_name("src/parser.rs"), "parser.rs");
        assert_eq!(mention_name("tsconfig.json"), "tsconfig.json");
        assert_eq!(mention_name("packages/src/"), "src/");
    }
}
