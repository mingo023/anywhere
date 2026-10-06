use git::Worktree;
use std::collections::HashMap;

/// Each saved project's git worktrees, keyed by the project's folder.
pub type Worktrees = HashMap<String, Vec<Worktree>>;

fn under(cwd: &str, project: &str) -> bool {
    cwd == project || cwd.strip_prefix(project).is_some_and(|rest| rest.starts_with('/'))
}

/// The saved projects, then the folders of terminals none of them holds.
pub fn projects<'a>(saved: &[String], cwds: impl IntoIterator<Item = &'a str>, worktrees: &Worktrees) -> Vec<String> {
    let mut out = saved.to_vec();
    for cwd in cwds {
        if project_of(cwd, &out, worktrees).is_none() {
            out.push(cwd.to_string());
        }
    }
    out
}

/// The project owning `cwd`: the one whose folder or worktree holds it most closely.
pub fn project_of<'a>(cwd: &str, projects: &'a [String], worktrees: &Worktrees) -> Option<&'a String> {
    let reach = |p: &String| {
        let trees = worktrees.get(p).into_iter().flatten().map(|w| w.path.as_str());
        std::iter::once(p.as_str()).chain(trees).filter(|root| under(cwd, root)).map(str::len).max()
    };
    projects.iter().filter_map(|p| Some((reach(p)?, p))).max_by_key(|(n, _)| *n).map(|(_, p)| p)
}

/// The listed worktree holding `cwd` most closely.
pub fn worktree_of<'a>(cwd: &str, worktrees: &'a Worktrees, tracked: impl Fn(&str) -> bool) -> Option<&'a Worktree> {
    worktrees.values().flatten().filter(|w| under(cwd, &w.path) && (w.main || tracked(&w.path))).max_by_key(|w| w.path.len())
}

/// The worktrees a project lists: its main one and those `tracked`.
pub fn listed(trees: &[Worktree], tracked: impl Fn(&str) -> bool) -> Vec<Worktree> {
    trees.iter().filter(|w| w.main || tracked(&w.path)).cloned().collect()
}

/// The worktrees git has that a project doesn't list. Their folders belong to the project's main tree.
pub fn untracked(trees: &[Worktree], tracked: impl Fn(&str) -> bool) -> Vec<Worktree> {
    trees.iter().filter(|w| !w.main && !tracked(&w.path)).cloned().collect()
}

/// The folders a git refresh reads: the project, its `sessions`' folders, its worktrees and the tree on screen.
pub fn git_cwds(project: Option<&str>, sessions: Vec<String>, worktrees: &Worktrees, shown: Option<String>) -> Vec<String> {
    let mut cwds: Vec<String> = project.map(str::to_string).into_iter().chain(sessions).collect();
    if let Some(p) = project {
        cwds.extend(worktrees.get(p).into_iter().flatten().map(|w| w.path.clone()));
    }
    cwds.extend(shown);
    cwds.sort();
    cwds.dedup();
    cwds
}

/// The worktree a folder belongs to: the listed worktree holding it, else the project it is in.
/// `projects` runs only when no worktree holds `cwd`, as listing them walks every terminal.
pub fn tree_of(cwd: &str, worktrees: &Worktrees, tracked: impl Fn(&str) -> bool, projects: impl FnOnce() -> Vec<String>) -> Option<String> {
    worktree_of(cwd, worktrees, tracked).map(|w| w.path.clone()).or_else(|| project_of(cwd, &projects(), worktrees).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trees(project: &str, paths: &[&str]) -> Worktrees {
        let list = paths.iter().enumerate().map(|(i, p)| Worktree { path: p.to_string(), branch: format!("b{i}"), main: i == 0 }).collect();
        HashMap::from([(project.to_string(), list)])
    }

    fn list(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn a_project_holds_its_folder_and_what_is_below_it() {
        let projects = list(&["/a/foo"]);
        let none = Worktrees::new();
        assert_eq!(project_of("/a/foo", &projects, &none), Some(&projects[0]));
        assert_eq!(project_of("/a/foo/src", &projects, &none), Some(&projects[0]));
    }

    #[test]
    fn a_project_does_not_hold_a_sibling_sharing_its_prefix() {
        let projects = list(&["/a/foo"]);
        assert_eq!(project_of("/a/foobar", &projects, &Worktrees::new()), None);
        assert_eq!(project_of("/a", &projects, &Worktrees::new()), None);
    }

    #[test]
    fn a_project_saved_with_a_trailing_slash_holds_only_that_exact_path() {
        let projects = list(&["/a/foo/"]);
        let none = Worktrees::new();
        assert_eq!(project_of("/a/foo/", &projects, &none), Some(&projects[0]));
        assert_eq!(project_of("/a/foo", &projects, &none), None);
        assert_eq!(project_of("/a/foo/src", &projects, &none), None);
    }

    #[test]
    fn the_innermost_of_nested_projects_owns_a_folder() {
        let projects = list(&["/a/foo/pkg", "/a/foo"]);
        let none = Worktrees::new();
        assert_eq!(project_of("/a/foo/pkg/src", &projects, &none), Some(&projects[0]));
        assert_eq!(project_of("/a/foo/src", &projects, &none), Some(&projects[1]));
    }

    #[test]
    fn a_project_owns_its_worktrees_outside_its_folder() {
        let projects = list(&["/a/foo", "/w"]);
        let worktrees = trees("/a/foo", &["/a/foo", "/w/foo-fix"]);
        assert_eq!(project_of("/w/foo-fix/src", &projects, &worktrees), Some(&projects[0]));
        assert_eq!(project_of("/w/other", &projects, &worktrees), Some(&projects[1]));
    }

    #[test]
    fn a_later_project_inside_a_repo_ties_with_the_repo_and_takes_its_folders() {
        let projects = list(&["/a/foo", "/a/foo/pkg"]);
        let mut worktrees = trees("/a/foo", &["/a/foo"]);
        worktrees.extend(trees("/a/foo/pkg", &["/a/foo"]));
        assert_eq!(project_of("/a/foo/src", &projects, &worktrees), Some(&projects[1]));
    }

    #[test]
    fn a_folder_belongs_to_the_innermost_worktree_holding_it() {
        let worktrees = trees("/a/foo", &["/a/foo", "/a/foo/.worktrees/fix"]);
        let path = |cwd| worktree_of(cwd, &worktrees, |_| true).map(|w| w.path.as_str());
        assert_eq!(path("/a/foo/.worktrees/fix/src"), Some("/a/foo/.worktrees/fix"));
        assert_eq!(path("/a/foo/src"), Some("/a/foo"));
        assert_eq!(path("/a/foobar"), None);
    }

    #[test]
    fn a_folders_tree_is_its_worktree_else_its_project() {
        let worktrees = trees("/a/foo", &["/a/foo", "/w/fix"]);
        let projects = || list(&["/a/foo", "/b"]);
        assert_eq!(tree_of("/w/fix/src", &worktrees, |_| true, projects), Some("/w/fix".to_string()));
        assert_eq!(tree_of("/b/src", &worktrees, |_| true, projects), Some("/b".to_string()));
        assert_eq!(tree_of("/c", &worktrees, |_| true, projects), None);
    }

    #[test]
    fn a_folder_in_an_untracked_worktree_belongs_to_its_projects_main_tree() {
        let worktrees = trees("/a/foo", &["/a/foo", "/w/fix", "/a/foo/.worktrees/spike"]);
        let projects = || list(&["/a/foo"]);
        assert_eq!(tree_of("/w/fix/src", &worktrees, |_| false, projects), Some("/a/foo".to_string()));
        assert_eq!(tree_of("/a/foo/.worktrees/spike", &worktrees, |_| false, projects), Some("/a/foo".to_string()));
    }

    #[test]
    fn terminals_outside_every_project_add_their_folders_after_the_saved_ones() {
        let saved = list(&["/b", "/a/foo"]);
        let worktrees = trees("/a/foo", &["/a/foo", "/w/fix"]);
        let cwds = ["/a/foo/src", "/x", "/w/fix", "/c", "/x"];
        assert_eq!(projects(&saved, cwds, &worktrees), list(&["/b", "/a/foo", "/x", "/c"]));
    }

    #[test]
    fn an_unsaved_folder_absorbs_later_terminals_below_it_but_not_earlier_ones() {
        let none = Worktrees::new();
        assert_eq!(projects(&[], ["/x", "/x/y"], &none), list(&["/x"]));
        assert_eq!(projects(&[], ["/x/y", "/x"], &none), list(&["/x/y", "/x"]));
    }

    #[test]
    fn a_terminal_left_in_a_removed_project_shows_its_own_folder() {
        let stale = trees("/a/foo", &["/a/foo", "/w/fix"]);
        assert_eq!(projects(&list(&["/b"]), ["/a/foo/src", "/w/fix"], &stale), list(&["/b", "/a/foo/src", "/w/fix"]));
    }

    #[test]
    fn git_refresh_reads_the_projects_folders_sorted_once_each() {
        let mut worktrees = trees("/a/foo", &["/a/foo", "/w/fix"]);
        worktrees.extend(trees("/b", &["/b", "/w/other"]));
        let sessions = list(&["/a/foo/src", "/w/fix"]);
        let cwds = git_cwds(Some("/a/foo"), sessions, &worktrees, Some("/w/fix".to_string()));
        assert_eq!(cwds, list(&["/a/foo", "/a/foo/src", "/w/fix"]));
    }

    #[test]
    fn git_refresh_without_a_project_reads_only_the_tree_on_screen() {
        let worktrees = trees("/a/foo", &["/a/foo", "/w/fix"]);
        assert_eq!(git_cwds(None, Vec::new(), &worktrees, Some("/w/fix".to_string())), list(&["/w/fix"]));
        assert_eq!(git_cwds(None, Vec::new(), &worktrees, None), list(&[]));
    }

    #[test]
    fn a_project_lists_its_main_worktree_and_only_the_tracked_ones() {
        let worktrees = trees("/a/foo", &["/a/foo", "/w/fix", "/w/spike"]);
        let tracked = |t: &str| t == "/w/fix";
        let shown: Vec<String> = listed(&worktrees["/a/foo"], tracked).into_iter().map(|w| w.path).collect();
        assert_eq!(shown, list(&["/a/foo", "/w/fix"]));
        let hidden: Vec<String> = untracked(&worktrees["/a/foo"], tracked).into_iter().map(|w| w.path).collect();
        assert_eq!(hidden, list(&["/w/spike"]));
    }
}
