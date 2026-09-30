use std::path::{Path, PathBuf};

pub fn basename(path: &str) -> String {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or_default().to_string()
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

pub fn list_dir(dir: &Path) -> Vec<(bool, PathBuf)> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut entries: Vec<(bool, PathBuf)> =
        read.flatten().filter(|e| e.file_name() != ".git").map(|e| (e.file_type().is_ok_and(|t| t.is_dir()), e.path())).collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    entries
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
    use super::{ago, ago_long, initials, tilde};

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
}
