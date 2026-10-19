use std::ops::Range;

/// What a prompt's text marks out: the command it opens with and the files it mentions, as byte ranges.
#[derive(Debug, Default, PartialEq)]
pub struct Marks {
    pub command: Option<Range<usize>>,
    pub mentions: Vec<Range<usize>>,
}

/// The `/command` that opens `text`, as [`crate::slash`] reads it, and each `@path` at the start or after whitespace.
pub fn marks(text: &str) -> Marks {
    let command = crate::slash(text).map(|(name, _)| {
        let start = text.len() - text.trim_start().len();
        start..start + 1 + name.len()
    });
    let part = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '/' | '-');
    let mut mentions = Vec::new();
    let mut after_space = true;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '@' && after_space {
            let mut end = i + 1;
            while let Some(&(j, c)) = chars.peek().filter(|(_, c)| part(*c)) {
                end = j + c.len_utf8();
                chars.next();
            }
            if end > i + 1 {
                mentions.push(i..end);
            }
            after_space = false;
            continue;
        }
        after_space = c.is_whitespace();
    }
    Marks { command, mentions }
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

    fn marked(text: &str) -> (Option<&str>, Vec<&str>) {
        let m = marks(text);
        (m.command.map(|r| &text[r]), m.mentions.into_iter().map(|r| &text[r]).collect())
    }

    #[test]
    fn a_command_is_marked_only_where_the_text_opens() {
        assert_eq!(marked("  /review 482").0, Some("/review"));
        assert_eq!(marked("/goal tests pass").0, Some("/goal"));
        assert_eq!(marked("why /model?").0, None);
        assert_eq!(marked("/usage/report.md is empty").0, None);
    }

    #[test]
    fn mentions_start_the_text_or_follow_whitespace() {
        let (_, found) = marked("@src/chat.rs and\n@a_b-c.md, then @docs/");
        assert_eq!(found, ["@src/chat.rs", "@a_b-c.md", "@docs/"]);
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
