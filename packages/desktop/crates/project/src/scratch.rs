use std::io::ErrorKind;

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

/// Makes a folder under `root` with a name no other there has, as an empty repository, and returns its path.
/// Taking the name with `create_dir` keeps two creates at once from sharing a folder.
pub fn create(root: &str, seed: usize) -> Result<String, String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    for attempt in 0..TRIES {
        let path = format!("{root}/{}", name(seed, attempt));
        match std::fs::create_dir(&path) {
            Ok(()) => {
                return match git::init_empty(&path) {
                    Ok(()) => Ok(path),
                    Err(e) => {
                        std::fs::remove_dir_all(&path).ok();
                        Err(e)
                    }
                };
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Every session name is taken".into())
}

/// Whether `path` is a folder directly in `root`, the only kind deleting a scratch folder may remove.
pub fn holds(root: &str, path: &str) -> bool {
    path.strip_prefix(root).and_then(|rest| rest.strip_prefix('/')).is_some_and(|name| !name.is_empty() && !name.contains('/') && name != "." && name != "..")
}

/// The folder's uncommitted files and the commits made since `create`'s own; `None` when git can't read it.
pub fn work(path: &str) -> Option<(usize, usize)> {
    Some((git::dirty_files(&git::Runner::Local, path)?, git::commit_count(&git::Runner::Local, path)?.saturating_sub(1)))
}

/// Whether deleting a folder needs no asking: git could read it and it holds no work, and no terminals are open in it.
pub fn nothing_to_lose(work: Option<(usize, usize)>, terminals: usize) -> bool {
    work == Some((0, 0)) && terminals == 0
}

#[cfg(test)]
mod tests {
    use super::{TRIES, create, holds, name, nothing_to_lose, work};
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
    fn every_try_draws_a_different_name() {
        let mut names: Vec<String> = (0..TRIES).map(|attempt| name(3, attempt)).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), TRIES);
    }

    fn git_in(dir: &str, args: &[&str]) {
        let out = std::process::Command::new("git").args(["-C", dir, "-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn uncommitted_files_and_later_commits_are_work_but_the_first_commit_is_not() {
        let root = temp_root("work");
        let path = create(&root, 0).unwrap();
        assert_eq!(work(&path), Some((0, 0)));
        std::fs::write(format!("{path}/notes"), "x").unwrap();
        assert_eq!(work(&path), Some((1, 0)));
        git_in(&path, &["add", "."]);
        git_in(&path, &["commit", "-q", "-m", "notes"]);
        assert_eq!(work(&path), Some((0, 1)));
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
        for path in [root, "/home/me/.anywhere/sessions/", "/home/me/.anywhere/sessions/a/b", "/home/me/.anywhere/sessions/..", "/home/me/.anywhere/sessions-other/a", "/home/me"] {
            assert!(!holds(root, path), "{path}");
        }
    }
}
