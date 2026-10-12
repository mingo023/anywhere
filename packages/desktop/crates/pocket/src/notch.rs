use crate::desktop::Desktop;
use crate::desktop::alerts::Notice;
use agents::{Decision, Permission};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui_kit::*;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use theme::{provider_name, provider_svg};

/// A permission ask the island offers to answer.
pub struct Ask {
    pub agent: String,
    pub request: String,
    pub provider: String,
    pub title: String,
    pub place: String,
    pub verb: String,
    pub subject: String,
    pub shell: bool,
    pub feedback: bool,
}

impl Ask {
    pub fn new(p: &Permission, provider: &str, title: &str, place: String) -> Self {
        let (verb, subject) = p.parts();
        Self {
            agent: p.agent_id.clone(),
            request: p.request_id.clone(),
            provider: provider.into(),
            title: title.into(),
            place,
            verb,
            subject,
            shell: p.detail.kind == "shell",
            feedback: p.feedback,
        }
    }

    fn json(&self) -> Value {
        json!({
            "agent": self.agent,
            "request": self.request,
            "provider": self.provider,
            // "Claude" for Claude Code.
            "name": provider_name(&self.provider).split(' ').next(),
            "icon": String::from_utf8_lossy(provider_svg(&self.provider)),
            "title": self.title,
            "place": self.place,
            "verb": self.verb,
            "subject": self.subject,
            "shell": self.shell,
            "feedback": self.feedback,
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum Answer {
    Allow,
    Deny,
    Reply(String),
}

/// What the island asks of the desktop, a line at a time.
#[derive(Debug, PartialEq)]
pub enum Said {
    Answered { agent: String, request: String, answer: Answer },
    Open(String),
}

impl Said {
    fn parse(line: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(line).ok()?;
        let field = |k: &str| v.get(k).and_then(Value::as_str).map(String::from);
        let answer = match v.get("answer")?.as_str()? {
            "open" => return Some(Said::Open(field("agent")?)),
            "allow" => Answer::Allow,
            "deny" => Answer::Deny,
            "reply" => Answer::Reply(field("text")?),
            _ => return None,
        };
        Some(Said::Answered { agent: field("agent")?, request: field("request")?, answer })
    }
}

/// The asks shown as notices and still pending, oldest first: what the island offers.
#[derive(Default)]
struct Shown(Vec<Ask>);

impl Shown {
    /// Adds the `asks` not shown before and drops those no longer `pending`; whether that changed anything.
    fn update(&mut self, asks: Vec<Ask>, pending: &[&str]) -> bool {
        let before: Vec<String> = self.0.iter().map(|a| a.request.clone()).collect();
        for ask in asks {
            if !self.0.iter().any(|a| a.request == ask.request) {
                self.0.push(ask);
            }
        }
        self.0.retain(|a| pending.contains(&a.request.as_str()));
        !self.0.iter().map(|a| &a.request).eq(&before)
    }

    fn offers(&self, request: &str) -> bool {
        self.0.iter().any(|a| a.request == request)
    }

    /// The line the island reads.
    fn line(&self) -> String {
        json!({ "asks": self.0.iter().map(Ask::json).collect::<Vec<_>>() }).to_string()
    }
}

/// The island that grows out of the notch, drawn by the `anywhere-notch` helper (`packages/desktop/notch`).
pub struct Notch {
    shown: Shown,
    helper: Option<(Child, ChildStdin)>,
    said: UnboundedSender<Said>,
    _said: Task<()>,
}

impl Notch {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> Self {
        let (said, mut rx) = unbounded();
        let _said = cx.spawn_in(window, async move |this, cx| {
            while let Some(said) = rx.next().await {
                if this.update_in(cx, |d, window, cx| d.on_island(said, window, cx)).is_err() {
                    break;
                }
            }
        });
        Self { shown: Shown::default(), helper: None, said, _said }
    }

    /// Tells the helper what to show, starting it for the first ask, and again once if it quit.
    fn send(&mut self) {
        for _ in 0..2 {
            if self.helper.is_none() && !self.shown.0.is_empty() {
                self.helper = self.spawn();
            }
            let Some((child, stdin)) = &mut self.helper else { return };
            if writeln!(stdin, "{}", self.shown.line()).is_ok() {
                return;
            }
            let _ = child.kill();
            let _ = child.wait();
            self.helper = None;
        }
    }

    fn spawn(&self) -> Option<(Child, ChildStdin)> {
        let mut child = Command::new(helper()?).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().ok()?;
        let (stdin, stdout) = (child.stdin.take()?, child.stdout.take()?);
        let said = self.said.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(s) = Said::parse(&line) {
                    let _ = said.unbounded_send(s);
                }
            }
        });
        Some((child, stdin))
    }
}

/// Beside the desktop's binary in the app; else where `swift build -c release` leaves it.
fn helper() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok()?.with_file_name("anywhere-notch");
    let built = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../notch/.build/release/anywhere-notch"));
    [beside, built].into_iter().find(|p| p.exists())
}

impl Desktop {
    /// Offers the pending asks among `shown`, the notices just shown, oldest first, and drops those no longer pending.
    pub(crate) fn sync_island(&mut self, shown: &[Notice]) {
        let pending = &self.agents.pending;
        let asks = pending
            .iter()
            .filter_map(|p| {
                let n = shown.iter().find(|n| n.ask.as_ref() == Some(&p.request_id))?;
                let a = self.agents.get(&n.id)?;
                Some(Ask::new(p, &a.provider, &n.title, self.alert_place(a)))
            })
            .collect();
        let ids: Vec<&str> = pending.iter().map(|p| p.request_id.as_str()).collect();
        if self.notch.shown.update(asks, &ids) {
            self.notch.send();
        }
    }

    fn on_island(&mut self, said: Said, window: &mut Window, cx: &mut Context<Self>) {
        match said {
            Said::Answered { agent, request, answer } => {
                if self.notch.shown.offers(&request) {
                    match answer {
                        Answer::Allow => self.outbox.resolve(&request, Decision::Allow),
                        Answer::Deny => self.outbox.resolve(&request, Decision::Deny),
                        Answer::Reply(message) => self.outbox.reply(&request, &message),
                    }
                }
                cx.dismiss_system_notification(&agent);
            }
            Said::Open(agent) => {
                cx.activate(true);
                self.focus_agent(&agent, window, cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Answer, Ask, Said, Shown};

    fn ask(request: &str) -> Ask {
        Ask {
            agent: format!("agent-{request}"),
            request: request.into(),
            provider: "claude".into(),
            title: "Fix the flaky test".into(),
            place: "app · main".into(),
            verb: "Wants to run".into(),
            subject: "ls /tmp".into(),
            shell: true,
            feedback: true,
        }
    }

    fn requests(shown: &Shown) -> Vec<&str> {
        shown.0.iter().map(|a| a.request.as_str()).collect()
    }

    #[test]
    fn a_new_ask_waits_behind_those_shown_before() {
        let mut shown = Shown::default();
        shown.update(vec![ask("q1")], &["q1"]);
        let changed = shown.update(vec![ask("q2")], &["q1", "q2"]);
        assert_eq!((changed, requests(&shown)), (true, vec!["q1", "q2"]));
    }

    #[test]
    fn an_ask_shown_before_is_not_offered_twice() {
        let mut shown = Shown::default();
        shown.update(vec![ask("q1")], &["q1"]);
        let changed = shown.update(vec![ask("q1")], &["q1"]);
        assert_eq!((changed, requests(&shown)), (false, vec!["q1"]));
    }

    #[test]
    fn an_ask_answered_elsewhere_leaves_the_island() {
        let mut shown = Shown::default();
        shown.update(vec![ask("q1"), ask("q2")], &["q1", "q2"]);
        let changed = shown.update(vec![], &["q2"]);
        assert_eq!((changed, requests(&shown)), (true, vec!["q2"]));
    }

    #[test]
    fn an_answer_counts_only_for_an_ask_the_island_still_offers() {
        let mut shown = Shown::default();
        shown.update(vec![ask("q1"), ask("q2")], &["q1", "q2"]);
        shown.update(vec![], &["q2"]);
        assert_eq!((shown.offers("q1"), shown.offers("q2"), shown.offers("q3")), (false, true, false));
    }

    #[test]
    fn the_island_reads_each_ask_with_its_providers_short_name_and_icon() {
        let mut shown = Shown::default();
        shown.update(vec![ask("q1")], &["q1"]);
        let line: serde_json::Value = serde_json::from_str(&shown.line()).unwrap();
        let first = &line["asks"][0];
        assert_eq!((first["request"].as_str(), first["name"].as_str()), (Some("q1"), Some("Claude")));
        assert!(first["icon"].as_str().unwrap().starts_with("<svg"));
    }

    #[test]
    fn the_island_says_which_ask_it_answered_and_how() {
        let said = |line: &str| Said::parse(line);
        let answered = |answer| Some(Said::Answered { agent: "a1".into(), request: "q1".into(), answer });
        assert_eq!(said(r#"{"answer":"allow","agent":"a1","request":"q1"}"#), answered(Answer::Allow));
        assert_eq!(said(r#"{"answer":"deny","agent":"a1","request":"q1"}"#), answered(Answer::Deny));
        assert_eq!(said(r#"{"answer":"reply","agent":"a1","request":"q1","text":"no"}"#), answered(Answer::Reply("no".into())));
        assert_eq!(said(r#"{"answer":"open","agent":"a1"}"#), Some(Said::Open("a1".into())));
    }

    #[test]
    fn a_line_the_island_could_not_have_said_is_ignored() {
        assert_eq!(Said::parse("not json"), None);
        assert_eq!(Said::parse(r#"{"answer":"reply","agent":"a1","request":"q1"}"#), None);
        assert_eq!(Said::parse(r#"{"answer":"maybe","agent":"a1","request":"q1"}"#), None);
    }
}
