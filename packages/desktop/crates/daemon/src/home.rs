//! Where pocketd keeps its home on this machine, found the way pocketd finds it: mirrors `config.DefaultHome`,
//! `config.SharedHome`, `config.AccountsDir` and `config.Moved`.

use std::path::{Path, PathBuf};

const RELEASE_LEGACY: [&str; 2] = [".coding-pocket", ".anywhere"];

/// The oldest legacy home still holding the home, else where it goes.
pub fn default_home(user: &Path) -> PathBuf {
    default_home_for(user, channel::is_release())
}

fn default_home_for(user: &Path, release: bool) -> PathBuf {
    if release {
        current(&under(user, &RELEASE_LEGACY), &user.join(".zoes"))
    } else {
        current(&[user.join(".coding-pocket-dev")], &shared_home(user).join("dev"))
    }
}

/// Holds the worktrees and Scratch folders of every channel, and the dev home: `~/.anywhere` until the release home is
/// `~/.zoes`.
pub fn shared_home(user: &Path) -> PathBuf {
    let zoes = user.join(".zoes");
    if current(&under(user, &RELEASE_LEGACY), &zoes) == zoes { zoes } else { user.join(".anywhere") }
}

/// Holds the managed accounts, under `~/.anywhere` where they were made there: Claude Code names an account's Keychain
/// item by its config folder's path.
pub fn accounts_dir(user: &Path) -> PathBuf {
    let made = user.join(".anywhere/accounts");
    if made.exists() { made } else { shared_home(user).join("accounts") }
}

/// Each legacy home that leads into where it moved, as (legacy, home), the release home's first.
pub fn moved(user: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut add = |legacy: &[PathBuf], to: &Path| {
        for l in legacy.iter().filter(|l| l.as_path() != to && same(l, to)) {
            out.push((l.to_string_lossy().into_owned(), to.to_string_lossy().into_owned()));
        }
    };
    add(&under(user, &RELEASE_LEGACY), &user.join(".zoes"));
    if !channel::is_release() {
        add(&[user.join(".coding-pocket-dev")], &default_home_for(user, false));
    }
    out
}

/// `path` with a moved home in front of it swapped for where that went; `None` when none is.
pub fn rehome(moves: &[(String, String)], path: &str) -> Option<String> {
    moves.iter().find_map(|(from, to)| {
        let rest = path.strip_prefix(from.as_str())?;
        (rest.is_empty() || rest.starts_with('/')).then(|| format!("{to}{rest}"))
    })
}

fn current(legacy: &[PathBuf], home: &Path) -> PathBuf {
    (0..legacy.len()).find(|&i| holds(legacy, i, home)).map_or_else(|| home.to_path_buf(), |i| legacy[i].clone())
}

/// Whether `legacy[i]` is a home of its own: a folder that leads neither into `home` nor, as the link a move leaves,
/// into a newer legacy home.
fn holds(legacy: &[PathBuf], i: usize, home: &Path) -> bool {
    let l = &legacy[i];
    if !l.is_dir() || same(l, home) {
        return false;
    }
    !l.is_symlink() || !legacy[i + 1..].iter().any(|n| same(l, n))
}

fn same(a: &Path, b: &Path) -> bool {
    std::fs::canonicalize(a).ok().zip(std::fs::canonicalize(b).ok()).is_some_and(|(a, b)| a == b)
}

fn under(user: &Path, names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(|n| user.join(n)).collect()
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Case {
        name: String,
        layout: Vec<String>,
        release: String,
        dev: String,
    }

    fn user(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pocket-home-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::canonicalize(dir).unwrap()
    }

    fn lay(user: &Path, layout: &[impl AsRef<str>]) {
        for e in layout {
            match e.as_ref().split_once(" -> ") {
                Some((from, to)) => std::os::unix::fs::symlink(user.join(to), user.join(from)).unwrap(),
                None => std::fs::create_dir_all(user.join(e.as_ref())).unwrap(),
            }
        }
    }

    #[test]
    fn each_layout_has_the_home_pocketd_finds_too() {
        let raw = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../pocketd/internal/config/testdata/homes.json")).unwrap();
        let cases: Vec<Case> = serde_json::from_slice(&raw).unwrap();
        for (n, c) in cases.iter().enumerate() {
            let user = user(&format!("table-{n}"));
            lay(&user, &c.layout);
            assert_eq!(default_home_for(&user, true), user.join(&c.release), "{}, release", c.name);
            assert_eq!(default_home_for(&user, false), user.join(&c.dev), "{}, dev", c.name);
            std::fs::remove_dir_all(&user).unwrap();
        }
    }

    #[test]
    fn a_home_moved_to_zoes_rehomes_paths_under_each_old_name() {
        let user = user("moved");
        lay(&user, &[".zoes/dev/", ".zoes/accounts/", ".anywhere -> .zoes", ".coding-pocket -> .anywhere"]);
        let moves = moved(&user);
        let (u, zoes) = (user.to_string_lossy().into_owned(), user.join(".zoes").to_string_lossy().into_owned());
        assert_eq!(moves.iter().map(|(f, _)| f.as_str()).collect::<Vec<_>>(), [format!("{u}/.coding-pocket"), format!("{u}/.anywhere")]);
        assert_eq!(rehome(&moves, &format!("{u}/.anywhere/worktrees/app/x")), Some(format!("{zoes}/worktrees/app/x")));
        assert_eq!(rehome(&moves, &format!("{u}/.anywhere")), Some(zoes.clone()));
        for kept in [format!("{u}/.anywhere-other/x"), format!("//m1{u}/.anywhere/x"), "/w/app/.anywhere/config.json".into()] {
            assert_eq!(rehome(&moves, &kept), None, "{kept}");
        }
        assert_eq!(accounts_dir(&user), user.join(".anywhere/accounts"), "accounts keep the path Claude's Keychain item is named by");
        std::fs::remove_dir_all(&user).unwrap();
    }

    #[test]
    fn nothing_has_moved_before_pocketd_moves_it() {
        let user = user("unmoved");
        lay(&user, &[".anywhere/dev/", ".coding-pocket -> .anywhere"]);
        assert!(moved(&user).is_empty());
        assert_eq!(shared_home(&user), user.join(".anywhere"));
        std::fs::remove_dir_all(&user).unwrap();
    }
}
