use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The GitHub pictures saved so far; whoever has none yet shows their initial.
#[derive(Default)]
pub(crate) struct Avatars {
    asked: HashSet<String>,
    pub(super) saved: HashMap<String, PathBuf>,
}

impl Avatars {
    pub(crate) fn path(&self, login: &str) -> Option<&Path> {
        self.saved.get(login).map(PathBuf::as_path)
    }

    /// Those of `logins` not asked for before, now marked asked, so each is fetched once a run.
    pub(super) fn ask<'a>(&mut self, logins: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        logins.into_iter().filter(|l| self.asked.insert(l.to_string())).map(str::to_string).collect()
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
