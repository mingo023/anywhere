use std::path::{Path, PathBuf};

const AVOID: [&str; 10] = ["workspace", "waiting", "attention", "blocked", "unread", "acknowledged", "finished", "running", "busy", "mark all read"];
const ALLOWED: [&str; 1] = ["This session is not running."];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The string literals of `code` up to its test module, skipping `//` lines.
fn literals(code: &str) -> Vec<String> {
    let code = code.split("#[cfg(test)]").next().unwrap_or_default();
    let mut found = Vec::new();
    for line in code.lines().filter(|l| !l.trim_start().starts_with("//")) {
        let mut chars = line.chars();
        let mut current: Option<String> = None;
        while let Some(c) = chars.next() {
            match (&mut current, c) {
                (None, '"') => current = Some(String::new()),
                (Some(s), '"') => {
                    found.push(std::mem::take(s));
                    current = None;
                }
                (Some(s), '\\') => s.extend(chars.next()),
                (Some(s), c) => s.push(c),
                (None, _) => {}
            }
        }
    }
    found
}

/// Words split like copy reads, so ids such as "commit-busy" stay one word.
fn avoided(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_')).collect();
    AVOID.into_iter().find(|a| if a.contains(' ') { lower.contains(a) } else { words.contains(a) })
}

#[test]
fn desktop_copy_uses_no_avoid_list_words() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = Vec::new();
    sources(&crates.join("pocket/src"), &mut files);
    sources(&crates.join("ui/src"), &mut files);
    files.sort();
    let mut hits = Vec::new();
    for file in files {
        for text in literals(&std::fs::read_to_string(&file).unwrap()) {
            if let Some(word) = avoided(&text).filter(|_| !ALLOWED.contains(&text.as_str())) {
                hits.push(format!("{}: \"{text}\" uses \"{word}\"", file.strip_prefix(crates).unwrap().display()));
            }
        }
    }
    assert!(hits.is_empty(), "avoid-list words in desktop copy:\n{}", hits.join("\n"));
}
