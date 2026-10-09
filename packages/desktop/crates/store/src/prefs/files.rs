use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum NewTabs {
    #[default]
    End,
    AfterCurrent,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Autosave {
    #[default]
    Off,
    AfterDelay,
    OnBlur,
}

/// Files and the editor.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct Files {
    #[serde(deserialize_with = "crate::lenient")]
    pub new_tabs: NewTabs,
    pub preview_tabs: bool,
    pub markdown_source: bool,
    pub soft_wrap: bool,
    pub line_numbers: bool,
    pub font_size: u32,
    pub tab_width: u32,
    #[serde(deserialize_with = "crate::lenient")]
    pub autosave: Autosave,
    pub hide_dotfiles: bool,
    pub hide_ignored: bool,
    pub folders_first: bool,
    pub case_sensitive: bool,
    /// Comma-separated globs the Explorer leaves out.
    pub also_hide: String,
}

impl Default for Files {
    fn default() -> Self {
        Self {
            new_tabs: NewTabs::default(),
            preview_tabs: true,
            markdown_source: false,
            soft_wrap: false,
            line_numbers: true,
            font_size: 13,
            tab_width: 2,
            autosave: Autosave::default(),
            hide_dotfiles: false,
            hide_ignored: true,
            folders_first: true,
            case_sensitive: true,
            also_hide: String::new(),
        }
    }
}

impl Files {
    /// Whether the Explorer lists an entry called `name`.
    pub fn shows(&self, name: &str) -> bool {
        name != ".git" && !(self.hide_dotfiles && name.starts_with('.')) && !self.also_hide.split(',').map(str::trim).any(|p| !p.is_empty() && matches(p, name))
    }

    /// Orders one folder's entries, each `(is_dir, path)`.
    pub fn sort(&self, entries: &mut [(bool, PathBuf)]) {
        entries.sort_by(|a, b| {
            let dirs = if self.folders_first { b.0.cmp(&a.0) } else { Ordering::Equal };
            let fold = |p: &PathBuf| p.to_string_lossy().to_lowercase();
            let names = if self.case_sensitive { Ordering::Equal } else { fold(&a.1).cmp(&fold(&b.1)) };
            dirs.then(names).then_with(|| a.1.cmp(&b.1))
        });
    }
}

/// Whether `name` matches `pattern`, where `*` stands for any run of characters.
fn matches(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == name,
        Some((head, rest)) => name.strip_prefix(head).is_some_and(|tail| tail.char_indices().map(|(i, _)| i).chain([tail.len()]).any(|i| matches(rest, &tail[i..]))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_from_before_these_settings_keep_todays_editor() {
        let f: Files = serde_json::from_str("{}").unwrap();
        assert_eq!(f, Files::default());
        assert_eq!((f.soft_wrap, f.line_numbers, f.font_size, f.autosave, f.case_sensitive), (false, true, 13, Autosave::Off, true));
    }

    #[test]
    fn an_unknown_autosave_falls_back_alone() {
        let f: Files = serde_json::from_str(r#"{"autosave":"often","font_size":15}"#).unwrap();
        assert_eq!((f.autosave, f.font_size), (Autosave::Off, 15));
    }

    fn names(f: &Files, entries: &[(bool, &str)]) -> Vec<String> {
        let mut e: Vec<(bool, PathBuf)> = entries.iter().map(|(d, n)| (*d, PathBuf::from(n))).collect();
        f.sort(&mut e);
        e.into_iter().map(|(_, p)| p.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn the_explorer_sorts_folders_first_and_by_case_unless_told_otherwise() {
        let entries = [(false, "b"), (true, "src"), (false, "README"), (false, "a")];
        let mut f = Files::default();
        assert_eq!(names(&f, &entries), ["src", "README", "a", "b"]);
        f.case_sensitive = false;
        assert_eq!(names(&f, &entries), ["src", "a", "b", "README"]);
        f.folders_first = false;
        assert_eq!(names(&f, &entries), ["a", "b", "README", "src"]);
    }

    #[test]
    fn the_explorer_hides_git_dotfiles_when_asked_and_names_matching_a_pattern() {
        let mut f = Files::default();
        assert!(!f.shows(".git") && f.shows(".env") && f.shows("node_modules"));
        f.hide_dotfiles = true;
        f.also_hide = "node_modules, *.log, build*".into();
        assert!(!f.shows(".env") && !f.shows("node_modules") && !f.shows("out.log") && !f.shows("build") && !f.shows("build.rs"));
        assert!(f.shows("src") && f.shows("log") && f.shows("rebuild"));
    }
}
