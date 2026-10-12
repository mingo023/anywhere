use git::Exec;
use hosts::remote::Client;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Where a tree's files are read and changed: this Mac, a linked host, or a host that isn't linked.
#[derive(Clone)]
pub enum Fs {
    Local,
    NotConnected,
    Remote(Arc<Client>),
}

/// The most of a file a host sends in one read.
const MAX_READ: usize = 16 << 20;

fn not_connected() -> io::Error {
    io::Error::new(ErrorKind::NotConnected, "host not connected")
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl Fs {
    /// Ok on this Mac, so the file IO after it runs here; a host's paths fail before touching the same path on this Mac.
    pub fn local(&self) -> io::Result<()> {
        match self {
            Fs::Local => Ok(()),
            Fs::NotConnected | Fs::Remote(_) => Err(not_connected()),
        }
    }

    /// An unreachable host's paths count as there, so losing the link forgets none of its trees.
    pub fn exists(&self, path: &Path) -> bool {
        match self {
            Fs::Local => path.exists(),
            Fs::NotConnected => true,
            Fs::Remote(c) => c.stat(&text(path)).map_or(true, |s| s.is_some()),
        }
    }

    /// One folder's entries on a host, `(is_dir, key)`, filtered and sorted as `files` asks.
    pub fn list(&self, dir: &Path, files: &store::prefs::files::Files) -> io::Result<Vec<(bool, PathBuf)>> {
        let Fs::Remote(c) = self else { return Err(not_connected()) };
        let mut entries: Vec<(bool, PathBuf)> = c
            .list(&text(dir))?
            .into_iter()
            .filter(|e| files.shows(&e.name) && !(files.hide_ignored && e.ignored))
            .map(|e| (e.dir, dir.join(e.name)))
            .collect();
        files.sort(&mut entries);
        Ok(entries)
    }

    /// The file's size, then its bytes once `take` says they're wanted.
    pub fn read(&self, path: &Path, take: impl FnOnce(u64) -> bool) -> io::Result<Option<Vec<u8>>> {
        match self {
            Fs::Local => {
                let len = std::fs::metadata(path)?.len();
                if !take(len) {
                    return Ok(None);
                }
                std::fs::read(path).map(Some)
            }
            Fs::NotConnected => Err(not_connected()),
            Fs::Remote(c) => {
                let len = c.stat(&text(path))?.ok_or(ErrorKind::NotFound)?.size;
                if !take(len) {
                    return Ok(None);
                }
                c.read(&text(path), MAX_READ).map(Some)
            }
        }
    }

    pub fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        match self {
            Fs::Local => std::fs::write(path, bytes),
            Fs::NotConnected => Err(not_connected()),
            Fs::Remote(c) => c.write(&text(path), bytes, false),
        }
    }
}

/// The files of the tree at `key`.
pub fn fs_for(key: &str) -> Fs {
    match hosts::split(key) {
        (None, _) => Fs::Local,
        (Some(id), _) => hosts::remote::connected(id).map_or(Fs::NotConnected, Fs::Remote),
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
        assert_eq!(fs_for(&key).read(Path::new(&key), |_| true).unwrap_err().kind(), ErrorKind::NotConnected);
        assert!(fs_at(&here).local().is_ok());
    }

    #[test]
    fn a_tree_on_an_unreachable_host_is_not_taken_for_gone() {
        assert!(fs_for("//devbox/nowhere").exists(Path::new("//devbox/nowhere")));
        assert!(!Fs::Local.exists(Path::new("/nowhere/at/all")));
    }
}
