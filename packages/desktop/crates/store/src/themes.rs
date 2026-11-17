use std::io;
use std::path::{Path, PathBuf};

fn dir(home: &Path) -> PathBuf {
    home.join("themes")
}

fn file(home: &Path, slug: &str) -> PathBuf {
    dir(home).join(format!("{slug}.json"))
}

/// The file name a theme called `name` is kept under: its words, lowercased, with dashes between and without the
/// characters a path gives meaning to or Windows refuses.
pub fn slug(name: &str) -> String {
    let words: Vec<String> = name.split(|c: char| c.is_whitespace() || c.is_control() || "/\\:.<>\"|?*".contains(c)).filter(|w| !w.is_empty()).map(str::to_lowercase).collect();
    if words.is_empty() { "theme".into() } else { words.join("-") }
}

/// Every imported theme file's slug and text, in slug order.
pub fn list(home: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(dir(home)) else { return Vec::new() };
    let mut files: Vec<(String, String)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let slug = path.file_name()?.to_str()?.strip_suffix(".json")?.to_string();
            Some((slug, std::fs::read_to_string(path).ok()?))
        })
        .collect();
    files.sort();
    files
}

/// Keeps `text` as the theme called `name`, in place of any kept under that name, and returns its slug.
pub fn keep(home: &Path, name: &str, text: &str) -> io::Result<String> {
    let slug = slug(name);
    std::fs::create_dir_all(dir(home))?;
    std::fs::write(file(home, &slug), text)?;
    Ok(slug)
}

pub fn remove(home: &Path, slug: &str) -> io::Result<()> {
    std::fs::remove_file(file(home, slug))
}

#[cfg(test)]
mod tests {
    use super::{keep, list, remove, slug};

    fn home(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pocket-themes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_slug_is_the_name_s_words_joined_by_dashes() {
        assert_eq!([slug("My  Theme"), slug("Rosé Pine"), slug("../etc/passwd"), slug(" . ")], ["my-theme", "rosé-pine", "etc-passwd", "theme"]);
        assert_eq!([slug("C++ Dark"), slug("C Dark"), slug("C:\\Dark\u{0}?")], ["c++-dark", "c-dark", "c-dark"]);
    }

    #[test]
    fn a_kept_theme_is_listed_until_removed() {
        let home = home("keep");
        assert_eq!(list(&home), []);
        assert_eq!([keep(&home, "Zed Dark", "z").unwrap(), keep(&home, "Ayu", "a").unwrap()], ["zed-dark", "ayu"]);
        assert_eq!(list(&home), [("ayu".to_string(), "a".to_string()), ("zed-dark".to_string(), "z".to_string())]);
        remove(&home, "ayu").unwrap();
        assert_eq!(list(&home).len(), 1);
        assert!(remove(&home, "ayu").is_err());
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn keeping_a_theme_under_a_name_already_kept_replaces_it() {
        let home = home("replace");
        keep(&home, "Mine", "old").unwrap();
        keep(&home, "mine", "new").unwrap();
        assert_eq!(list(&home), [("mine".to_string(), "new".to_string())]);
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn only_json_files_are_themes() {
        let home = home("json");
        keep(&home, "Mine", "{}").unwrap();
        std::fs::write(home.join("themes/notes.txt"), "x").unwrap();
        assert_eq!(list(&home).len(), 1);
        std::fs::remove_dir_all(home).unwrap();
    }
}
