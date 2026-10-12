use std::io::{self, ErrorKind};
use std::path::Path;

/// Where a tree's files are read and changed: this Mac, or a host that isn't linked yet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fs {
    Local,
    NotConnected,
}

impl Fs {
    /// Ok on this Mac, so the file IO after it runs here; a host's paths fail before touching the same path on this Mac.
    pub fn local(self) -> io::Result<()> {
        match self {
            Fs::Local => Ok(()),
            Fs::NotConnected => Err(io::Error::new(ErrorKind::NotConnected, "host not connected")),
        }
    }

    /// An unreachable host's paths count as there, so losing the link forgets none of its trees.
    pub fn exists(self, path: &Path) -> bool {
        self.local().is_err() || path.exists()
    }
}

/// The files of the tree at `key`.
pub fn fs_for(key: &str) -> Fs {
    match hosts::split(key) {
        (None, _) => Fs::Local,
        (Some(_), _) => Fs::NotConnected,
    }
}

/// `fs_for` a path held as a `Path`.
pub fn fs_at(path: &Path) -> Fs {
    fs_for(&path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_path_is_never_read_on_this_mac() {
        let here = std::env::temp_dir();
        let key = hosts::place("devbox", &here.to_string_lossy());
        assert_eq!(fs_for(&key).local().unwrap_err().kind(), ErrorKind::NotConnected);
        assert!(fs_at(&here).local().is_ok());
    }

    #[test]
    fn a_tree_on_an_unreachable_host_is_not_taken_for_gone() {
        assert!(fs_for("//devbox/nowhere").exists(Path::new("//devbox/nowhere")));
        assert!(!Fs::Local.exists(Path::new("/nowhere/at/all")));
    }
}
