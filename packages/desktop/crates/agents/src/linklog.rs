use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAX: u64 = 5 << 20;

/// `logs/desktop.log`: an append-only file that rotates by size as pocketd's `pocketd.log` does, shared by every link.
/// It holds drops, reconnects and what became of prompts, never a prompt's text.
#[derive(Clone)]
pub struct LinkLog(Arc<Mutex<Inner>>);

struct Inner {
    path: PathBuf,
    max: u64,
    file: File,
    size: u64,
}

impl LinkLog {
    pub fn open(path: &Path) -> Option<Self> {
        Self::with_max(path, MAX)
    }

    fn with_max(path: &Path, max: u64) -> Option<Self> {
        let file = open_file(path, false)?;
        let size = file.metadata().ok()?.len();
        Some(LinkLog(Arc::new(Mutex::new(Inner { path: path.to_path_buf(), max, file, size }))))
    }

    pub fn line(&self, text: &str) {
        let Ok(mut log) = self.0.lock() else { return };
        log.write(&format!("{} {text}\n", chrono::Local::now().format("%Y/%m/%d %H:%M:%S")));
    }
}

impl Inner {
    fn write(&mut self, line: &str) {
        let len = line.len() as u64;
        if self.size > 0 && self.size + len > self.max {
            self.rotate();
        }
        if self.file.write_all(line.as_bytes()).is_ok() {
            self.size += len;
        }
    }

    fn rotate(&mut self) {
        let _ = std::fs::rename(&self.path, format!("{}.1", self.path.display()));
        if let Some(file) = open_file(&self.path, true) {
            (self.file, self.size) = (file, 0);
        }
    }
}

fn open_file(path: &Path, truncate: bool) -> Option<File> {
    if let Some(dir) = path.parent() {
        let mut dirs = std::fs::DirBuilder::new();
        dirs.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut dirs, 0o700);
        dirs.create(dir).ok()?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create(true);
    if truncate {
        options.truncate(true);
    } else {
        options.append(true);
    }
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("linklog-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn each_line_is_stamped_and_appended_to_the_file_in_a_new_logs_directory() {
        let dir = dir("lines");
        let path = dir.join("logs/desktop.log");
        let log = LinkLog::open(&path).unwrap();
        log.line("link down");
        log.line("link up");
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with(" link down") && lines[1].ends_with(" link up"));
        assert_eq!(lines[0].len(), "2026/11/25 10:00:00 link down".len(), "stamped like pocketd.log");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_line_that_would_pass_the_limit_moves_the_file_to_dot_1_and_starts_a_new_one() {
        let dir = dir("rotate");
        let path = dir.join("desktop.log");
        let log = LinkLog::with_max(&path, 80).unwrap();
        for n in 0..6 {
            log.line(&format!("entry {n}"));
        }
        let (now, before) = (std::fs::read_to_string(&path).unwrap(), std::fs::read_to_string(dir.join("desktop.log.1")).unwrap());
        assert!(now.lines().count() > 0 && now.len() <= 80 && before.len() <= 80);
        assert!(now.contains("entry 5") && !now.contains("entry 0"));
        assert!(before.lines().count() > 0 && !dir.join("desktop.log.2").exists(), "one old file is kept");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_log_reopened_keeps_counting_from_the_size_it_has() {
        let dir = dir("reopen");
        let path = dir.join("desktop.log");
        LinkLog::with_max(&path, 60).unwrap().line("first");
        let log = LinkLog::with_max(&path, 60).unwrap();
        log.line("second");
        log.line("third");
        assert!(dir.join("desktop.log.1").exists());
        assert!(std::fs::read_to_string(&path).unwrap().contains("third"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_log_is_private_to_its_user() {
        use std::os::unix::fs::PermissionsExt;
        let dir = dir("mode");
        let path = dir.join("desktop.log");
        LinkLog::open(&path).unwrap().line("x");
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
