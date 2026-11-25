use git::Runner;
use std::io::{self, ErrorKind};

const DIR: &str = ".anywhere/sessions";
const TRIES: usize = 512;

pub fn root(home: &str) -> String {
    format!("{home}/{DIR}")
}

/// pocketd keeps a folder's title by path after the folder is gone, so a large word list keeps a new folder from drawing a deleted one's path.
fn name(seed: usize, attempt: usize) -> String {
    let words = petname::Petnames::medium();
    let (adjectives, nouns) = (&words.adjectives, &words.nouns);
    let n = seed.wrapping_add(attempt) % (adjectives.len() * nouns.len());
    format!("{}-{}", adjectives[n / nouns.len()], nouns[n % nouns.len()])
}

/// Makes a folder under `root` on this Mac with a name no other there has, as an empty repository, and returns its path.
pub fn create(root: &str, seed: usize) -> Result<String, String> {
    let make = |path: &str, parents: bool| if parents { std::fs::create_dir_all(path) } else { std::fs::create_dir(path) };
    create_on(&Runner::Local, root, seed, make, |path| _ = std::fs::remove_dir_all(path))
}

/// `create` on the machine `run` runs git on, where `make(path, parents)` makes a folder and `remove` deletes one.
/// Without `parents`, `make` must fail on a folder already there: taking the name that way keeps two creates at once from sharing a folder.
pub fn create_on(run: &Runner, root: &str, seed: usize, make: impl Fn(&str, bool) -> io::Result<()>, remove: impl Fn(&str)) -> Result<String, String> {
    make(root, true).map_err(|e| e.to_string())?;
    for attempt in 0..TRIES {
        let path = format!("{root}/{}", name(seed, attempt));
        match make(&path, false) {
            Ok(()) => {
                return match git::init_empty(run, &path) {
                    Ok(()) => Ok(path),
                    Err(e) => {
                        remove(&path);
                        Err(e)
                    }
                };
            }
            // A host's pocketd gives the reason only as text, so whether the name was taken is asked of the folder.
            Err(e) if e.kind() == ErrorKind::AlreadyExists || run.stat(&path).is_ok_and(|s| s.is_some()) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Every session name is taken".into())
}

/// Whether `path` is a folder directly in `root`, the only kind deleting a scratch folder may remove.
pub fn holds(root: &str, path: &str) -> bool {
    path.strip_prefix(root)
        .and_then(|rest| rest.strip_prefix('/'))
        .is_some_and(|name| !name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != "..")
}

/// Whether `key` names a folder `create` made, on whichever machine holds it: one directly in a `.anywhere/sessions` folder.
pub fn made(key: &str) -> bool {
    key.rsplit_once('/').is_some_and(|(parent, _)| parent.ends_with(&format!("/{DIR}")) && holds(parent, key))
}

/// The folder's uncommitted files and the commits made since `create`'s own; `None` when git can't read it.
pub fn work(run: &Runner, path: &str) -> Option<(usize, usize)> {
    Some((git::dirty_files(run, path)?, git::commit_count(run, path)?.saturating_sub(1)))
}

/// Whether deleting a folder needs no asking: git could read it and it holds no work, and no terminals are open in it.
pub fn nothing_to_lose(work: Option<(usize, usize)>, terminals: usize) -> bool {
    work == Some((0, 0)) && terminals == 0
}

#[cfg(test)]
mod tests {
    use super::{TRIES, create, create_on, holds, name, nothing_to_lose, work};
    use git::Runner;

    fn temp_root(tag: &str) -> String {
        let dir = std::env::temp_dir().join(format!("pocket-scratch-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        dir.to_string_lossy().into_owned()
    }

    #[test]
    fn a_new_folder_is_a_clean_repository_on_main_with_a_head() {
        let root = temp_root("repo");
        let path = create(&root, 0).unwrap();
        let trees = git::worktrees(&Runner::Local, &path);
        assert_eq!(trees.len(), 1);
        assert!(trees[0].main);
        assert_eq!(trees[0].branch, "main");
        assert_eq!(git::dirty_files(&Runner::Local, &path), Some(0));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_taken_name_moves_on_to_the_next() {
        let root = temp_root("taken");
        let first = create(&root, 7).unwrap();
        let second = create(&root, 7).unwrap();
        assert_ne!(first, second);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_name_a_host_refuses_in_words_moves_on_when_its_folder_is_there() {
        let root = temp_root("host-taken");
        let taken = create(&root, 7).unwrap();
        let make = |path: &str, parents: bool| match (parents, std::path::Path::new(path).exists()) {
            (false, true) => Err(std::io::Error::other(format!("mkdir {path}: file exists"))),
            _ => std::fs::create_dir_all(path),
        };
        let made = create_on(&Runner::Local, &root, 7, make, |_| {}).unwrap();
        assert_ne!(made, taken);
        let refused = create_on(
            &Runner::Local,
            &format!("{root}/x"),
            7,
            |p, parents| if parents { std::fs::create_dir_all(p) } else { Err(std::io::Error::other("permission denied")) },
            |_| {},
        );
        assert_eq!(refused, Err("permission denied".into()));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn every_try_draws_a_different_name() {
        let mut names: Vec<String> = (0..TRIES).map(|attempt| name(3, attempt)).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), TRIES);
    }

    fn git_in(dir: &str, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(["-C", dir, "-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn uncommitted_files_and_later_commits_are_work_but_the_first_commit_is_not() {
        let root = temp_root("work");
        let path = create(&root, 0).unwrap();
        assert_eq!(work(&Runner::Local, &path), Some((0, 0)));
        std::fs::write(format!("{path}/notes"), "x").unwrap();
        assert_eq!(work(&Runner::Local, &path), Some((1, 0)));
        git_in(&path, &["add", "."]);
        git_in(&path, &["commit", "-q", "-m", "notes"]);
        assert_eq!(work(&Runner::Local, &path), Some((0, 1)));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn only_a_readable_folder_with_no_work_or_terminals_goes_without_asking() {
        assert!(nothing_to_lose(Some((0, 0)), 0));
        for (held, terminals) in [(None, 0), (Some((1, 0)), 0), (Some((0, 1)), 0), (Some((0, 0)), 1)] {
            assert!(!nothing_to_lose(held, terminals), "{held:?} {terminals}");
        }
    }

    #[test]
    fn only_a_folder_directly_in_the_root_can_be_deleted() {
        let root = "/home/me/.anywhere/sessions";
        assert!(holds(root, "/home/me/.anywhere/sessions/calm-river"));
        let windows = [r"/home/me/.anywhere/sessions/..\..", "/home/me/.anywhere/sessions/C:"];
        for path in [
            root,
            "/home/me/.anywhere/sessions/",
            "/home/me/.anywhere/sessions/a/b",
            "/home/me/.anywhere/sessions/..",
            "/home/me/.anywhere/sessions-other/a",
            "/home/me",
        ]
        .into_iter()
        .chain(windows)
        {
            assert!(!holds(root, path), "{path}");
        }
    }
}
