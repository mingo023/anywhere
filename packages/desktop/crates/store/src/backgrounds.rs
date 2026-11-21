use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// What the image decoder reads; HEIC isn't among them.
const EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];

pub fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Copies `from` into `~/.anywhere/backgrounds/` (`home` is the user's home) under a fresh name and drops the copies before it.
/// The fresh name keeps a re-pick of the current copy from truncating itself, and the image cache, keyed by path, from showing the old picture.
pub fn keep(home: &Path, from: &Path) -> io::Result<PathBuf> {
    let dir = home.join(".anywhere/backgrounds");
    std::fs::create_dir_all(&dir)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let to = dir.join(format!("{stamp}-{}", from.file_name().unwrap_or_default().to_string_lossy()));
    std::fs::copy(from, &to)?;
    for old in std::fs::read_dir(&dir)?.flatten().map(|e| e.path()).filter(|p| *p != to) {
        let _ = std::fs::remove_file(old);
    }
    Ok(to)
}

/// The name the user's file had, before `keep` put a stamp in front of it.
pub fn original_name(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.split_once('-').map_or(name, |(_, original)| original)
}

#[cfg(test)]
mod tests {
    use super::{is_image, keep, original_name};
    use std::path::Path;

    #[test]
    fn only_files_the_decoder_reads_are_images() {
        let ok = |p: &str| is_image(Path::new(p));
        assert!(ok("/a/b.PNG") && ok("d.jpeg") && ok("e.webp"));
        assert!(!ok("c.heic") && !ok("e.pdf") && !ok("noext"));
    }

    #[test]
    fn the_original_name_drops_the_stamp_but_keeps_later_dashes() {
        assert_eq!([original_name("/h/.anywhere/backgrounds/1700-my-sky.png"), original_name("plain.png")], ["my-sky.png", "plain.png"]);
    }

    #[test]
    fn keeping_an_image_copies_it_and_drops_the_one_before_even_when_re_picked() {
        let dir = std::env::temp_dir().join(format!("pocket-backgrounds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sky.png"), b"sky").unwrap();
        let home = dir.join("home");
        let first = keep(&home, &dir.join("sky.png")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = keep(&home, &first).unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read(&second).unwrap(), b"sky");
        assert!(second.starts_with(home.join(".anywhere/backgrounds")));
        assert_eq!(std::fs::read_dir(home.join(".anywhere/backgrounds")).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
