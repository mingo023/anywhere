use agents::locals::is_local;
use std::path::{Path, PathBuf};

pub const LOCAL_ICON: &str = "laptop";
pub const WORKTREE_ICON: &str = "worktree";

/// The icon a tree's row shows, by its key: a Local's id, else a worktree's path.
pub fn tree_icon(key: &str) -> &'static str {
    if is_local(key) { LOCAL_ICON } else { WORKTREE_ICON }
}

pub fn basename(path: &str) -> String {
    folder(path).to_string()
}

pub fn folder(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or_default()
}

/// "/Users/me/code/app" reads as "~/code/app".
pub fn tilde(path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    match path.strip_prefix(&home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => format!("~{rest}"),
        _ => path.to_string(),
    }
}

/// Two letters for the rail, from the repository's last dash-separated word: "app-android" is AN.
pub fn initials(name: &str) -> String {
    let word = name.rsplit('-').find(|w| !w.is_empty()).unwrap_or(name);
    word.chars().filter(|c| c.is_alphanumeric()).take(2).collect::<String>().to_uppercase()
}

/// Reads `.gitignore`s up to the repository root, so call it off the UI thread.
pub fn list_dir(dir: &Path, files: &store::prefs::files::Files) -> Vec<(bool, PathBuf)> {
    let git = files.hide_ignored;
    let walk = ignore::WalkBuilder::new(dir).standard_filters(false).max_depth(Some(1)).parents(git).git_ignore(git).git_exclude(git).git_global(git).build();
    let mut entries: Vec<(bool, PathBuf)> = walk
        .flatten()
        .filter(|e| e.depth() == 1 && files.shows(&e.file_name().to_string_lossy()))
        .map(|e| (e.file_type().is_some_and(|t| t.is_dir()), e.into_path()))
        .collect();
    files.sort(&mut entries);
    entries
}

/// Moves `path` to the Trash; false when it couldn't.
pub fn trash(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSFileManager, NSString, NSURL};
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        NSFileManager::defaultManager().trashItemAtURL_resultingItemURL_error(&url, None).is_ok()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        false
    }
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn ago(ms: i64, now: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    match (now - ms).max(0) / 60_000 {
        0 => "now".into(),
        m if m < 60 => format!("{m}m"),
        m if m < 24 * 60 => format!("{}h", m / 60),
        m if m < 48 * 60 => "yesterday".into(),
        m => format!("{}d", m / (24 * 60)),
    }
}

/// "12m ago", but "now" and "yesterday" as they are.
pub fn ago_long(ms: i64, now: i64) -> String {
    match ago(ms, now) {
        a if a.is_empty() || a == "now" || a == "yesterday" => a,
        a => format!("{a} ago"),
    }
}

#[cfg(test)]
mod tests {
    use super::{LOCAL_ICON, WORKTREE_ICON, ago, ago_long, folder, initials, list_dir, tilde, tree_icon};
    use store::prefs::files::Files;

    #[test]
    fn formats_times_like_the_design() {
        let min = 60_000;
        assert_eq!(ago(0, min), "");
        assert_eq!(ago(1, 30_000), "now");
        assert_eq!(ago(1, 1 + 22 * min), "22m");
        assert_eq!(ago(1, 1 + 3 * 60 * min + 5 * min), "3h");
        assert_eq!(ago(1, 1 + 30 * 60 * min), "yesterday");
        assert_eq!(ago(1, 1 + 72 * 60 * min), "3d");
        assert_eq!(ago_long(1, 1 + 14 * min), "14m ago");
        assert_eq!(ago_long(1, 30_000), "now");
    }

    #[test]
    fn initials_come_from_the_last_word() {
        assert_eq!(initials("app-android"), "AN");
        assert_eq!(initials("cs"), "CS");
        assert_eq!(initials("shared-ui"), "UI");
    }

    #[test]
    fn tilde_shortens_home_only() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(tilde(&format!("{home}/code/app")), "~/code/app");
        assert_eq!(tilde(&format!("{home}x/app")), format!("{home}x/app"));
    }

    #[test]
    fn the_explorer_leaves_out_what_git_ignores_unless_asked_not_to() {
        let dir = std::env::temp_dir().join(format!("pocket-ignored-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        assert!(std::process::Command::new("git").arg("init").arg("-q").arg(&dir).status().unwrap().success());
        std::fs::write(dir.join(".gitignore"), "target\n*.log\n").unwrap();
        for f in ["a.log", "main.rs", "src/b.log", "src/lib.rs"] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        let names = |d: &str, files: &Files| list_dir(&dir.join(d), files).into_iter().map(|(_, p)| p.file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>();
        let shown = Files { hide_ignored: false, ..Files::default() };
        assert_eq!(names("", &Files::default()), ["src", ".gitignore", "main.rs"]);
        assert_eq!(names("src", &Files::default()), ["lib.rs"]);
        assert_eq!(names("", &shown), ["src", "target", ".gitignore", "a.log", "main.rs"]);
        assert_eq!(names("src", &shown), ["b.log", "lib.rs"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_is_the_paths_last_part_even_with_a_trailing_slash() {
        assert_eq!((folder("/w/app-login"), folder("/w/app/"), folder("app")), ("app-login", "app", "app"));
    }

    #[test]
    fn a_local_shows_a_laptop_and_a_worktree_its_own_icon() {
        assert_eq!((tree_icon("local-1"), tree_icon("/w/app-login")), (LOCAL_ICON, WORKTREE_ICON));
    }
}
