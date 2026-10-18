use crate::fs::{Fs, fs_at};
use git::Exec;
use hosts::remote::Client;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

/// Makes `name` under `parent`, with any folders a "/" in it names; a trailing "/" makes a folder.
pub fn create(parent: &Path, name: &str, folder: bool) -> io::Result<PathBuf> {
    let folder = folder || name.ends_with('/');
    let path = parent.join(name.trim_matches('/'));
    if let Fs::Remote(host) = fs_at(parent) {
        return create_on(&host, &path, folder).map(|()| path);
    }
    fs_at(parent).local()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    if folder {
        fs::create_dir(&path)?;
    } else {
        fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
    }
    Ok(path)
}

/// Renames `path` to `name` in its folder, refusing to replace anything but itself under another case.
pub fn rename(path: &Path, name: &str) -> io::Result<PathBuf> {
    let to = path.with_file_name(name);
    if let Fs::Remote(host) = fs_at(path) {
        let (from, onto) = (text(path), text(&to));
        if from.to_lowercase() != onto.to_lowercase() && host.stat(&onto)?.is_some() {
            return Err(io::Error::new(ErrorKind::AlreadyExists, format!("“{name}” already exists")));
        }
        return host.rename(&from, &onto).map(|()| to);
    }
    fs_at(path).local()?;
    #[cfg(unix)]
    let inode = |p: &Path| p.symlink_metadata().map(|m| std::os::unix::fs::MetadataExt::ino(&m));
    // Windows has no inode to hand; the path it resolves to, in the case it's stored in, names the file.
    #[cfg(windows)]
    let inode = |p: &Path| p.symlink_metadata().and_then(|_| fs::canonicalize(p));
    if let Ok(taken) = inode(&to)
        && inode(path).ok() != Some(taken)
    {
        return Err(io::Error::new(ErrorKind::AlreadyExists, format!("“{name}” already exists")));
    }
    fs::rename(path, &to)?;
    Ok(to)
}

/// Moves `trashed` back to `to`, unless something took its place meanwhile.
pub fn restore(trashed: &Path, to: &Path) -> io::Result<()> {
    if let Fs::Remote(host) = fs_at(to) {
        if host.stat(&text(to))?.is_some() {
            return Err(io::Error::new(ErrorKind::AlreadyExists, "taken"));
        }
        return host.rename(&text(trashed), &text(to));
    }
    fs_at(to).local()?;
    if to.symlink_metadata().is_ok() {
        return Err(io::Error::new(ErrorKind::AlreadyExists, "taken"));
    }
    fs::rename(trashed, to)
}

fn create_on(host: &Client, path: &Path, folder: bool) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        host.mkdir(&text(dir), true)?;
    }
    if folder { host.mkdir(&text(path), false) } else { host.write(&text(path), b"", true) }
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::{create, rename, restore};
    use std::fs;
    use std::io::ErrorKind;
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pocket-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_slash_in_a_new_name_makes_the_folders_before_it() {
        let dir = temp("nested");
        let made = create(&dir, "a/b/c.rs", false).unwrap();
        assert_eq!(made, dir.join("a/b/c.rs"));
        assert!(made.is_file());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn nothing_is_made_on_this_mac_for_a_folder_on_a_host() {
        let dir = temp("host");
        let on_host = PathBuf::from(hosts::place("devbox", &dir.to_string_lossy()));
        assert_eq!(create(&on_host, "a.rs", false).unwrap_err().kind(), ErrorKind::NotConnected);
        assert!(!dir.join("a.rs").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_trailing_slash_makes_a_folder() {
        let dir = temp("trailing");
        let made = create(&dir, "src/", false).unwrap();
        assert_eq!(made, dir.join("src"));
        assert!(made.is_dir());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn creating_never_overwrites_a_file() {
        let dir = temp("exists");
        fs::write(dir.join("a.rs"), "kept").unwrap();
        assert_eq!(create(&dir, "a.rs", false).unwrap_err().kind(), ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(dir.join("a.rs")).unwrap(), "kept");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn renaming_onto_another_file_is_refused_but_a_case_change_is_not() {
        let dir = temp("rename");
        fs::write(dir.join("a.rs"), "a").unwrap();
        fs::write(dir.join("b.rs"), "b").unwrap();
        assert_eq!(rename(&dir.join("a.rs"), "b.rs").unwrap_err().kind(), ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(dir.join("b.rs")).unwrap(), "b");
        assert_eq!(rename(&dir.join("a.rs"), "A.rs").unwrap(), dir.join("A.rs"));
        assert_eq!(rename(&dir.join("A.rs"), "c.rs").unwrap(), dir.join("c.rs"));
        assert_eq!(fs::read_to_string(dir.join("c.rs")).unwrap(), "a");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn restoring_puts_a_file_back_unless_its_place_was_taken() {
        let dir = temp("restore");
        fs::write(dir.join("trashed"), "old").unwrap();
        fs::write(dir.join("taken.rs"), "new").unwrap();
        assert_eq!(restore(&dir.join("trashed"), &dir.join("taken.rs")).unwrap_err().kind(), ErrorKind::AlreadyExists);
        restore(&dir.join("trashed"), &dir.join("a.rs")).unwrap();
        assert_eq!(fs::read_to_string(dir.join("a.rs")).unwrap(), "old");
        fs::remove_dir_all(&dir).unwrap();
    }
}
