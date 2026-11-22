//! The machines this Mac's pocketd links over SSH, as its `host.*` messages carry them.

use crate::Event;
use serde::{Deserialize, Serialize};

/// How ssh reaches a host.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Target {
    /// A `Host` in `~/.ssh/config`; ssh brings its user, port, key and jump host.
    Alias { name: String },
    Address {
        addr: String,
        port: u16,
        /// Empty signs in as the user on this Mac.
        #[serde(default)]
        user: String,
        #[serde(rename = "keyFile", default, skip_serializing_if = "Option::is_none")]
        key_file: Option<String>,
    },
}

/// A host pocketd links, with the socket its tunnel listens on here.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub target: Target,
    /// Its pocketd is reached on its owner TCP port with a token, not on its socket.
    #[serde(default)]
    pub windows: bool,
    pub socket: String,
    pub link: Link,
}

/// A host's connection.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(from = "LinkWire")]
pub enum Link {
    Connecting,
    Online { version: String, uptime: u64 },
    Retrying(Failure),
    Failed(Failure),
    /// A state a newer pocketd reports, or one missing its failure.
    Unknown,
}

#[derive(Deserialize)]
struct LinkWire {
    state: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    uptime: u64,
    failure: Option<Failure>,
}

impl From<LinkWire> for Link {
    fn from(w: LinkWire) -> Self {
        match (w.state.as_str(), w.failure) {
            ("connecting", _) => Link::Connecting,
            ("online", _) => Link::Online { version: w.version, uptime: w.uptime },
            ("retrying", Some(f)) => Link::Retrying(f),
            ("failed", Some(f)) => Link::Failed(f),
            _ => Link::Unknown,
        }
    }
}

/// Why a connection failed, by what the user can do about it.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    /// Wrong key, no key, a key with a passphrase and no agent, or a wrong user: ssh can't tell them apart.
    Auth,
    UnknownKey,
    KeyChanged,
    NoSuchHost,
    Refused,
    TimedOut,
    /// The link went quiet, or the host closed it.
    Dropped,
    /// sshd's penalty for failed logins, which also blocks good ones for about 20 seconds.
    Throttled,
    NotRunning,
    /// A Windows host's pocketd refused this Mac's owner token, even a newly paired one.
    Unpaired,
    PathTooLong,
    #[serde(other)]
    Other,
}

/// A line of the install checklist, in the order pocketd's bootstrap reports them.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    Detect,
    Download,
    Verify,
    Unpack,
    Link,
    Service,
    Start,
}

impl Step {
    pub const ALL: [Step; 7] = [Step::Detect, Step::Download, Step::Verify, Step::Unpack, Step::Link, Step::Service, Step::Start];
}

/// How an install or update on a host goes.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(try_from = "ProgressWire")]
pub enum Progress {
    Step(Step),
    Warn(String),
    Running,
    Done,
    Error(String),
}

#[derive(Deserialize)]
struct ProgressWire {
    kind: String,
    step: Option<Step>,
    #[serde(default)]
    text: String,
}

impl TryFrom<ProgressWire> for Progress {
    type Error = String;

    fn try_from(w: ProgressWire) -> Result<Self, String> {
        Ok(match (w.kind.as_str(), w.step) {
            ("step", Some(step)) => Progress::Step(step),
            ("warn", _) => Progress::Warn(w.text),
            ("running", _) => Progress::Running,
            ("done", _) => Progress::Done,
            ("error", _) => Progress::Error(w.text),
            (kind, _) => return Err(format!("unknown progress {kind}")),
        })
    }
}

pub(crate) const HOST: &str = "host-";
const CODES: [&str; 5] = ["unknown_host", "host_exists", "invalid_host", "host_busy", "host_ssh"];

/// The host frames, read apart from `Frame`, whose `host` is this Mac's state.
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Frame {
    id: String,
    hosts: Vec<Entry>,
    host: Option<Entry>,
    host_id: String,
    update: bool,
    progress: Option<Progress>,
    code: String,
    detail: String,
    message: String,
}

pub(crate) fn decode(kind: &str, raw: &str) -> Option<Event> {
    if !matches!(kind, "host.list" | "host.update" | "host.progress" | "error" | "ack") {
        return None;
    }
    let f: Frame = serde_json::from_str(raw).ok()?;
    match kind {
        "host.list" => Some(Event::Hosts(f.hosts)),
        "host.update" => Some(Event::HostUpdate { request: f.id, host: f.host? }),
        "host.progress" => Some(Event::HostProgress { host: f.host_id, update: f.update, progress: f.progress? }),
        "error" if f.id.starts_with(HOST) || CODES.contains(&f.code.as_str()) => {
            let detail = (f.code == "host_ssh").then(|| serde_json::from_value(f.detail.into()).ok()).flatten();
            Some(Event::HostError { request: f.id, code: f.code, detail, message: f.message })
        }
        "ack" if f.id.starts_with(HOST) => Some(Event::HostDone(f.id)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Outbox;
    use serde_json::{Value, json};

    macro_rules! golden_text {
        ($name:literal) => {
            include_str!(concat!("../../../../pocketd/internal/proto/testdata/golden/", $name))
        };
    }

    fn event(raw: &str) -> Option<Event> {
        let kind = serde_json::from_str::<Value>(raw).unwrap()["type"].as_str().unwrap().to_string();
        decode(&kind, raw)
    }

    #[test]
    fn host_frames_become_host_events() {
        let Some(Event::Hosts(list)) = event(golden_text!("server/host_list.json")) else { panic!("no list") };
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].id.as_str(), &list[0].target, list[0].socket.as_str()), ("devbox", &Target::Alias { name: "devbox".into() }, "/Users/me/.coding-pocket/hosts/devbox.sock"));
        assert_eq!(list[0].link, Link::Online { version: "1.4.2".into(), uptime: 259_200 });
        assert!(matches!(event(golden_text!("server/host_list_push.json")), Some(Event::Hosts(list)) if list.is_empty()));
        let Some(Event::HostUpdate { request, host }) = event(golden_text!("server/host_update_added.json")) else { panic!("no add") };
        assert_eq!((request.as_str(), host.link), ("h2", Link::Connecting));
        let Some(Event::HostUpdate { request, host }) = event(golden_text!("server/host_update_failed.json")) else { panic!("no update") };
        assert_eq!((request.as_str(), host.windows, host.link), ("", true, Link::Failed(Failure::Auth)));
        let Some(Event::HostProgress { host, update, progress }) = event(golden_text!("server/host_progress_step.json")) else { panic!("no step") };
        assert_eq!((host.as_str(), update, progress), ("devbox", true, Progress::Step(Step::Download)));
        let Some(Event::HostProgress { progress, .. }) = event(golden_text!("server/host_progress_error.json")) else { panic!("no error") };
        assert_eq!(progress, Progress::Error("No release for linux/arm".into()));
        let f: Value = serde_json::from_str(golden_text!("server/stream_host_link.json")).unwrap();
        let item: crate::Item = serde_json::from_value(f["item"].clone()).unwrap();
        assert_eq!((item.kind.as_str(), item.hint.as_str(), item.reason.as_str()), ("host_link", "vps", "The build needs the GPU box."));
    }

    #[test]
    fn a_host_in_a_state_this_app_does_not_know_is_listed_with_the_rest() {
        let entry = |id: &str, link: Value| json!({"id": id, "name": id, "target": {"kind": "alias", "name": id}, "socket": format!("/h/{id}.sock"), "link": link});
        let frame = json!({"type": "host.list", "id": "h1", "hosts": [entry("a", json!({"state": "connecting"})), entry("b", json!({"state": "hibernating"})), entry("c", json!({"state": "retrying"}))]}).to_string();
        let Some(Event::Hosts(list)) = event(&frame) else { panic!("the list was dropped") };
        assert_eq!(list.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(list.iter().map(|h| h.link.clone()).collect::<Vec<_>>(), [Link::Connecting, Link::Unknown, Link::Unknown]);
    }

    #[test]
    fn a_host_error_names_its_request_and_failure() {
        let Some(Event::HostError { request, code, detail, message }) = event(golden_text!("server/error_host_ssh.json")) else { panic!("no error") };
        assert_eq!((request.as_str(), code.as_str(), detail, message.as_str()), ("h6", "host_ssh", Some(Failure::Auth), "Wrong password."));
        let refused = json!({"type": "error", "id": "host-3", "message": "devbox is already added", "code": "host_exists"}).to_string();
        assert!(matches!(event(&refused), Some(Event::HostError { detail: None, .. })));
        assert!(matches!(event(r#"{"type":"ack","id":"host-4"}"#), Some(Event::HostDone(id)) if id == "host-4"));
        assert!(event(r#"{"type":"ack","id":"prompt-1"}"#).is_none(), "another message's answer");
    }

    #[test]
    fn host_verbs_read_as_pocketds_goldens() {
        let (tx, rx) = std::sync::mpsc::channel();
        let out = Outbox::from(tx);
        let sent = |id: String| {
            let mut m: Value = rx.try_recv().unwrap();
            assert_eq!(m["id"], id.as_str());
            m["id"] = Value::Null;
            m
        };
        let golden = |raw: &str| {
            let mut m: Value = serde_json::from_str(raw).unwrap();
            m["id"] = Value::Null;
            m
        };
        assert_eq!(sent(out.host_add(&Target::Alias { name: "devbox".into() })), golden(golden_text!("client/host_add_alias.json")));
        let address = Target::Address { addr: "10.0.4.12".into(), port: 2222, user: "dev".into(), key_file: Some("~/.ssh/id_ed25519".into()) };
        let mut named = golden(golden_text!("client/host_add_address.json"));
        named.as_object_mut().unwrap().remove("name");
        assert_eq!(sent(out.host_add(&address)), named);
        assert_eq!(sent(out.host_remove("devbox", true)), golden(golden_text!("client/host_remove_uninstall.json")));
        out.host_retry("devbox");
        let retry: Value = rx.try_recv().unwrap();
        assert!(retry["id"].as_str().unwrap().starts_with(HOST));
        assert_eq!(json!({"type": retry["type"], "hostId": retry["hostId"]}), json!({"type": "host.retry", "hostId": "devbox"}));
        assert_eq!(sent(out.host_install("devbox", true, false, Some("/Users/me/src/anywhere/packages/pocketd/dist"))), golden(golden_text!("client/host_install_update.json")));
        assert_eq!(sent(out.host_install("devbox", true, true, None)), golden(golden_text!("client/host_install_latest.json")));
        assert_eq!(sent(out.host_sign_in("10-0-4-12", "hunter2")), golden(golden_text!("client/host_sign_in.json")));
    }
}
