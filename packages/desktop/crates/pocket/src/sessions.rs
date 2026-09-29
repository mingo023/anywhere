use daemon::{Info, Msg};
use term::Term;

pub struct Session {
    pub info: Info,
    pub term: Option<Term>,
    pub exit: Option<i32>,
    pub closed: bool,
    pub exit_seen: bool,
}

impl Session {
    /// A session this window closed was killed, so its exit code says nothing about the agent.
    pub fn failed(&self) -> bool {
        !self.closed && self.exit.is_some_and(|c| c != 0)
    }

    /// The command running in the foreground, while the terminal is up and not at its prompt.
    pub fn busy(&self) -> Option<&str> {
        Some(self.info.foreground.as_str()).filter(|f| self.exit.is_none() && !f.is_empty())
    }

    pub fn activity(&self) -> String {
        match (self.exit, self.busy()) {
            (Some(c), _) => format!("exited {c}"),
            (None, Some(f)) => f.to_string(),
            (None, None) => "at prompt".into(),
        }
    }
}

/// pocketd's sessions this window is attached to. Exited ones stay until closed so their last screen can be read.
#[derive(Default)]
pub struct Sessions {
    pub items: Vec<Session>,
}

impl Sessions {
    /// Adopts the daemon's session list and returns the ids that still need an attach.
    pub fn sync(&mut self, sessions: Vec<Info>) -> Vec<String> {
        self.items.retain(|s| s.exit.is_some() || sessions.iter().any(|i| i.id == s.info.id));
        let mut added = Vec::new();
        for info in sessions {
            match self.get_mut(&info.id) {
                Some(s) => s.info = info,
                None => {
                    added.push(info.id.clone());
                    self.items.push(Session { info, term: None, exit: None, closed: false, exit_seen: false });
                }
            }
        }
        added
    }

    pub fn apply(&mut self, m: &Msg) {
        let Some(s) = self.items.iter_mut().find(|s| s.info.id == m.id) else { return };
        match m.ev.as_str() {
            "snapshot" => {
                let mut t = Term::new(m.cols.max(1), m.rows.max(1));
                t.write(&m.bytes());
                s.term = Some(t);
            }
            "output" => s.term.iter_mut().for_each(|t| t.write(&m.bytes())),
            "resize" => s.term.iter_mut().for_each(|t| t.resize(m.cols.max(1), m.rows.max(1))),
            "exit" => s.exit = Some(m.code),
            "foreground" => s.info.foreground = m.text.clone(),
            _ => {}
        }
    }

    pub fn see_exits(&mut self, panes: &[String]) {
        for s in self.items.iter_mut().filter(|s| s.exit.is_some() && panes.contains(&s.info.id)) {
            s.exit_seen = true;
        }
    }

    pub fn get(&self, id: &str) -> Option<&Session> {
        self.items.iter().find(|s| s.info.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Session> {
        self.items.iter_mut().find(|s| s.info.id == id)
    }

    pub fn remove(&mut self, id: &str) {
        self.items.retain(|s| s.info.id != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: &str) -> Info {
        Info { id: id.into(), cmd: "claude".into(), cwd: "/w".into(), ..Default::default() }
    }

    fn msg(ev: &str, id: &str, data: &str) -> Msg {
        let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data);
        Msg { ev: ev.into(), id: id.into(), cols: 20, rows: 3, data: Some(data), ..Default::default() }
    }

    fn ids(s: &Sessions) -> Vec<&str> {
        s.items.iter().map(|s| s.info.id.as_str()).collect()
    }

    #[test]
    fn sync_attaches_new_sessions_and_drops_gone_ones() {
        let mut s = Sessions::default();
        assert_eq!(s.sync(vec![info("a"), info("b")]), vec!["a", "b"]);
        assert_eq!(s.sync(vec![info("c"), info("b")]), vec!["c"]);
        assert_eq!(ids(&s), vec!["b", "c"]);
    }

    #[test]
    fn snapshot_then_output_render_into_the_terminal() {
        let mut s = Sessions::default();
        s.sync(vec![info("a")]);
        s.apply(&msg("snapshot", "a", "hi"));
        s.apply(&msg("output", "a", "!"));
        let (f, cells) = s.get_mut("a").unwrap().term.as_mut().unwrap().frame();
        let text: String = cells[..3].iter().map(|c| char::from_u32(c.cp).unwrap()).collect();
        assert_eq!((text.as_str(), f.cols, f.cursor_x), ("hi!", 20, 3));
    }

    #[test]
    fn exited_sessions_outlive_the_list_until_removed() {
        let mut s = Sessions::default();
        s.sync(vec![info("a"), info("b")]);
        s.apply(&Msg { code: 2, ..msg("exit", "a", "") });
        s.sync(vec![info("b")]);
        assert_eq!(ids(&s), vec!["a", "b"]);
        assert_eq!(s.get("a").unwrap().exit, Some(2));
        s.remove("a");
        assert_eq!(ids(&s), vec!["b"]);
    }

    #[test]
    fn a_closed_session_killed_by_its_close_has_not_failed() {
        let mut s = Sessions::default();
        s.sync(vec![info("a"), info("b")]);
        s.get_mut("a").unwrap().closed = true;
        s.apply(&Msg { code: -1, ..msg("exit", "a", "") });
        s.apply(&Msg { code: -1, ..msg("exit", "b", "") });
        assert!(!s.get("a").unwrap().failed());
        assert!(s.get("b").unwrap().failed());
    }

    #[test]
    fn sync_refreshes_what_the_list_says_about_known_sessions() {
        let mut s = Sessions::default();
        s.sync(vec![info("a")]);
        assert_eq!(s.sync(vec![Info { last_title: "Fix CI".into(), ..info("a") }]), Vec::<String>::new());
        assert_eq!(s.get("a").unwrap().info.last_title, "Fix CI");
    }

    #[test]
    fn busy_follows_the_foreground_until_exit() {
        let mut s = Sessions::default();
        s.sync(vec![info("a")]);
        assert_eq!(s.get("a").unwrap().busy(), None);
        s.apply(&Msg { ev: "foreground".into(), id: "a".into(), text: "npm run dev".into(), ..Default::default() });
        assert_eq!(s.get("a").unwrap().busy(), Some("npm run dev"));
        s.apply(&Msg { code: 1, ..msg("exit", "a", "") });
        assert_eq!(s.get("a").unwrap().busy(), None);
    }

    #[test]
    fn activity_reads_the_foreground_the_prompt_or_the_exit_code() {
        let mut s = Sessions::default();
        s.sync(vec![info("a"), info("b"), Info { foreground: "npm run dev".into(), ..info("c") }]);
        s.apply(&Msg { code: 1, ..msg("exit", "a", "") });
        let got: Vec<String> = s.items.iter().map(Session::activity).collect();
        assert_eq!(got, vec!["exited 1", "at prompt", "npm run dev"]);
    }

    #[test]
    fn only_exits_on_screen_are_seen() {
        let mut s = Sessions::default();
        s.sync(vec![info("a"), info("b"), info("c")]);
        for id in ["a", "b"] {
            s.apply(&Msg { code: 1, ..msg("exit", id, "") });
        }
        s.see_exits(&["a".into(), "c".into()]);
        let got: Vec<bool> = s.items.iter().map(|s| s.exit_seen).collect();
        assert_eq!(got, vec![true, false, false]);
    }
}
