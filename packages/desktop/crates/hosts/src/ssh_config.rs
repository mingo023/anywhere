//! Reads `~/.ssh/config` for the hosts it names, to suggest them. Never writes it.

use std::path::{Path, PathBuf};

/// The hosts `~/.ssh/config` and the files it includes name, in order; patterns like `*.internal` name none.
pub fn aliases(home: &Path) -> Vec<String> {
    let ssh = home.join(".ssh");
    let mut out = Vec::new();
    read(&ssh.join("config"), &ssh, home, &mut out, 0);
    out
}

fn read(path: &Path, ssh: &Path, home: &Path, out: &mut Vec<String>, depth: u8) {
    let Ok(text) = std::fs::read_to_string(path) else { return };
    for line in text.lines() {
        let Some((key, value)) = split(line) else { continue };
        match key.to_ascii_lowercase().as_str() {
            "host" => {
                for alias in value.split_whitespace().filter(|a| !a.contains(['*', '?', '!'])) {
                    if !out.iter().any(|a| a == alias) {
                        out.push(alias.into());
                    }
                }
            }
            "include" if depth < 4 => {
                for file in value.split_whitespace().flat_map(|p| included(p, ssh, home)) {
                    read(&file, ssh, home, out, depth + 1);
                }
            }
            _ => {}
        }
    }
}

fn split(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let at = line.find(|c: char| c.is_whitespace() || c == '=')?;
    let value = line[at..].trim_start_matches(|c: char| c.is_whitespace() || c == '=').trim();
    Some((&line[..at], value))
}

/// The files an `Include` names: relative ones under `~/.ssh`, with one `*` in the file name.
fn included(pattern: &str, ssh: &Path, home: &Path) -> Vec<PathBuf> {
    let path = match pattern.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => ssh.join(pattern),
    };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let Some((head, tail)) = name.split_once('*') else { return vec![path] };
    let Some(dir) = path.parent() else { return Vec::new() };
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().map(|n| n.to_string_lossy()).is_some_and(|n| n.len() >= head.len() + tail.len() && n.starts_with(head) && n.ends_with(tail)))
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(name: &str, config: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!("pocket-ssh-config-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".ssh")).unwrap();
        std::fs::write(home.join(".ssh/config"), config).unwrap();
        home
    }

    #[test]
    fn suggests_named_hosts_but_not_patterns() {
        let home = home("patterns", "Host *\n  ServerAliveInterval 30\nHost devbox gpu-?\n  HostName 10.0.4.12\n  User mingo\nHost *.internal !bastion\n");
        assert_eq!(aliases(&home), ["devbox"]);
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn reads_keys_in_any_case_and_with_an_equals_sign() {
        let home = home("case", "host=build\n# Host commented\nHOST ci\n");
        assert_eq!(aliases(&home), ["build", "ci"]);
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn follows_includes_under_ssh() {
        let home = home("include", "Include config.d/*.conf\nHost devbox\n");
        std::fs::create_dir_all(home.join(".ssh/config.d")).unwrap();
        std::fs::write(home.join(".ssh/config.d/b.conf"), "Host gpu\n").unwrap();
        std::fs::write(home.join(".ssh/config.d/a.conf"), "Host pi\n  HostName 192.168.1.20\n").unwrap();
        std::fs::write(home.join(".ssh/config.d/notes.txt"), "Host skipped\n").unwrap();
        assert_eq!(aliases(&home), ["pi", "gpu", "devbox"]);
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn no_config_suggests_nothing() {
        assert!(aliases(Path::new("/nonexistent")).is_empty());
    }
}
