use crate::desktop::Desktop;
use git::github;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The GitHub pictures saved so far; whoever has none yet shows their initial.
#[derive(Default)]
pub(crate) struct Avatars {
    asked: HashSet<String>,
    saved: HashMap<String, PathBuf>,
}

impl Avatars {
    pub(crate) fn path(&self, login: &str) -> Option<&Path> {
        self.saved.get(login).map(PathBuf::as_path)
    }

    /// Those of `logins` not asked for before, now marked asked, so each is fetched once a run.
    fn ask<'a>(&mut self, logins: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        logins.into_iter().filter(|l| self.asked.insert(l.to_string())).map(str::to_string).collect()
    }
}

impl Desktop {
    /// Fetches the pictures of everyone on `tree`'s PR, and the user's own, each in its own background task.
    pub(crate) fn fetch_avatars(&mut self, tree: &str, cx: &mut Context<Self>) {
        let Some(pr) = self.prs.get(tree) else { return };
        let me = self.login(&self.gh_in(tree));
        let dir = self.store.home().join("avatars");
        let wanted = self.pr.avatars.ask(github::people(pr).into_iter().chain([me.as_str()]));
        for login in wanted {
            let Some(url) = github::avatar_url(&pr.url, &login) else { continue };
            let path = dir.join(format!("{login}.png"));
            let dir = dir.clone();
            let task =
                cx.background_executor().spawn(async move { (std::fs::create_dir_all(&dir).is_ok() && github::fetch_avatar(&url, &path)).then_some(path) });
            cx.spawn(async move |this, cx| {
                let Some(path) = task.await else { return };
                this.update(cx, |d, cx| {
                    d.pr.avatars.saved.insert(login, path);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Avatars;

    #[test]
    fn each_login_is_asked_for_once() {
        let mut avatars = Avatars::default();
        assert_eq!(avatars.ask(["rina", "tom"]), ["rina", "tom"]);
        assert_eq!(avatars.ask(["tom", "mingo"]), ["mingo"]);
    }
}
