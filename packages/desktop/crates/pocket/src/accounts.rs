//! Claude Code and Codex sign-ins on each machine: what its pocketd lists, their usage, and the terminals that sign in.

use crate::desktop::Desktop;
use crate::util::now_ms;
use chrono::{DateTime, TimeZone};
use daemon::{Account, Msg};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Display;
use std::path::PathBuf;
use std::time::Duration;

/// Each provider's own login: `~/.claude` or `~/.codex`.
pub(crate) const BUILT_IN: &str = "builtin";
/// Seconds between usage checks.
pub(crate) const CHECK_EVERY: u64 = 300;
// Usage waits on the providers' own servers.
const TIMEOUT: Duration = Duration::from_secs(15);

/// A machine by its host id; `None` is this Mac.
pub(crate) type Host = Option<String>;

/// One account: `builtin` is an id under each provider, so the provider is part of it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Key {
    pub(crate) host: Host,
    pub(crate) provider: String,
    pub(crate) id: String,
}

impl Key {
    pub(crate) fn of(host: &Host, a: &Account) -> Self {
        Self { host: host.clone(), provider: a.provider.clone(), id: a.id.clone() }
    }

    pub(crate) fn op(&self, op: &str) -> Value {
        json!({"op": op, "key": self.provider, "id": self.id})
    }
}

/// An account's name, edited in its row; leaving it saves, as Enter does.
pub(crate) struct Rename {
    pub(crate) key: Key,
    pub(crate) input: Entity<InputState>,
    was: String,
    _sub: Subscription,
}

#[derive(Default)]
pub(crate) struct Accounts {
    /// Each machine's accounts, as its pocketd last listed them.
    pub(crate) lists: HashMap<Host, Vec<Account>>,
    /// Whether each machine moves an agent at its account's limit to another; one too old to isn't listed.
    pub(crate) auto_switch: HashMap<Host, bool>,
    /// Machines whose list or usage is being read.
    pub(crate) checking: HashSet<Host>,
    /// Machines to read again once their read in flight ends.
    again: HashSet<Host>,
    /// When usage was last read, in ms.
    pub(crate) checked_at: i64,
    /// The account whose ⋯ menu is open.
    pub(crate) menu: Option<Key>,
    pub(crate) rename: Option<Rename>,
    pub(crate) signins: Vec<SignIn>,
}

/// A sign-in running in a terminal on its machine, shown under its provider's accounts rather than in a tab.
pub(crate) struct SignIn {
    /// The machine and provider; the id is empty when it adds an account.
    pub(crate) key: Key,
    /// pocketd's terminal, tagged with its host, once it answers.
    pub(crate) terminal: Option<String>,
    /// The account pocketd made for a new sign-in, to drop if it's cancelled.
    added: String,
    cancelled: bool,
    /// A code the browser shows, typed back into the terminal.
    pub(crate) code: Entity<InputState>,
    pub(crate) output_open: bool,
    _sub: Subscription,
}

impl SignIn {
    pub(crate) fn on(&self, host: &Host, provider: &str) -> bool {
        &self.key.host == host && self.key.provider == provider
    }
}

impl Accounts {
    pub(crate) fn list(&self, host: &Host) -> &[Account] {
        self.lists.get(host).map_or(&[], Vec::as_slice)
    }

    /// Shows a machine's accounts as its pocketd answered with them.
    fn take(&mut self, host: &Host, m: Msg) {
        self.lists.insert(host.clone(), m.accounts);
        match m.auto_switch {
            Some(on) => _ = self.auto_switch.insert(host.clone(), on),
            None => _ = self.auto_switch.remove(host),
        }
    }

    /// The account a session row names, while its provider has more than one on the session's machine.
    pub(crate) fn session_label(&self, session: &str, provider: &str, account: &str) -> Option<String> {
        let host = hosts::session::untag(session).0.map(str::to_string);
        session_label(self.list(&host), provider, account)
    }

    /// Drops a create's account unless its machine lists it: an older pocketd refuses keys it doesn't know.
    pub(crate) fn offer(&self, spec: &mut Value) {
        let host = host_of(spec["project"].as_str());
        let (provider, id) = (spec["provider"].as_str().unwrap_or_default(), spec["account"].as_str().unwrap_or_default());
        if !listed(self.list(&host), provider).any(|a| a.id == id)
            && let Some(spec) = spec.as_object_mut()
        {
            spec.remove("account");
        }
    }
}

pub(crate) fn host_of(project: Option<&str>) -> Host {
    project.and_then(|p| hosts::split(p).0).map(str::to_string)
}

/// A terminal's screen as lines, with the rows a long line wraps onto joined to it.
pub(crate) fn screen_lines(cells: &[term::Cell], cols: u16) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for row in cells.chunks(cols.max(1) as usize) {
        line.extend(row.iter().map(term::Cell::ch));
        // Wrapping fills a row to its last cell, so a row ending blank ends its line.
        if row.last().is_none_or(|c| c.ch() == ' ') {
            lines.push(std::mem::take(&mut line).trim_end().to_string());
        }
    }
    lines.push(line.trim_end().to_string());
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

/// The first web link a sign-in prints: the page that approves it.
pub(crate) fn sign_in_link(lines: &[String]) -> Option<String> {
    lines.iter().find_map(|l| {
        let at = l.find("https://")?;
        web::link_at(l, l[..at].chars().count())
    })
}

/// The one-time code Codex's device sign-in prints on the line after asking for it.
pub(crate) fn device_code(lines: &[String]) -> Option<String> {
    let asked = lines.iter().position(|l| l.contains("one-time code"))?;
    lines[asked + 1..].iter().map(|l| l.trim()).find(|l| !l.is_empty()).map(str::to_string)
}

/// A provider's accounts on a machine that a session can start on: past the sign-ins still running and a built-in login nobody uses.
pub(crate) fn listed<'a>(list: &'a [Account], provider: &str) -> impl Iterator<Item = &'a Account> {
    list.iter().filter(move |a| a.provider == provider && !a.pending && !never_signed_in(a))
}

/// Whether a provider has accounts to choose between on a machine.
pub(crate) fn shared(list: &[Account], provider: &str) -> bool {
    listed(list, provider).nth(1).is_some()
}

fn session_label(list: &[Account], provider: &str, account: &str) -> Option<String> {
    let id = if account.is_empty() { BUILT_IN } else { account };
    shared(list, provider).then(|| listed(list, provider).find(|a| a.id == id)).flatten().map(|a| name(a).to_string())
}

/// The account `id` of a provider, while its machine lists it to pick.
pub(crate) fn picked<'a>(list: &'a [Account], provider: &str, id: &str) -> Option<&'a Account> {
    listed(list, provider).find(|a| a.id == id)
}

/// The account a provider's new sessions start on when nothing picks another.
pub(crate) fn default_of<'a>(list: &'a [Account], provider: &str) -> Option<&'a Account> {
    listed(list, provider).find(|a| a.default)
}

/// The accounts a project picks, as (provider, name), while its machine has others to pick from; one removed since starts on the default.
pub(crate) fn project_picks(list: &[Account], picks: &BTreeMap<String, String>) -> Vec<(String, String)> {
    picks
        .iter()
        .filter(|(provider, _)| shared(list, provider))
        .filter_map(|(provider, id)| picked(list, provider, id).map(|a| (provider.clone(), name(a).to_string())))
        .collect()
}

/// The account a session shows, by the id its summary names; empty is the built-in login. A signed-out one counts.
pub(crate) fn in_use<'a>(list: &'a [Account], provider: &str, account: &str) -> Option<&'a Account> {
    let id = if account.is_empty() { BUILT_IN } else { account };
    list.iter().find(|a| a.provider == provider && a.id == id && !a.pending)
}

/// The fuller of an account's 5-hour and weekly windows, in whole percent; none before its usage is read.
pub(crate) fn peak(a: &Account) -> Option<u32> {
    let usage = a.usage.as_ref()?;
    [&usage.session, &usage.week].into_iter().flatten().map(|w| w.percent.round().clamp(0., 100.) as u32).max()
}

/// How full a percent of a limit reads: high from 80, full at 100.
pub(crate) fn tone(percent: u32) -> Tone {
    match percent {
        100.. => Tone::Bad,
        80.. => Tone::Warn,
        _ => Tone::Plain,
    }
}

/// "2h 10m" until `at`, "45m" within the hour, "3d 4h" past a day.
pub(crate) fn until(at: i64, now: i64) -> String {
    let minutes = ((at - now).max(0) + 59_999) / 60_000;
    match (minutes / 1440, minutes % 1440 / 60, minutes % 60) {
        (0, 0, m) => format!("{m}m"),
        (0, h, m) => format!("{h}h {m}m"),
        (d, h, _) => format!("{d}d {h}h"),
    }
}

/// When an account at its limit is back, as "Back at 15:12"; none while it has room.
pub(crate) fn back_at<Tz: TimeZone>(a: &Account, now: i64, tz: &Tz) -> Option<String>
where
    Tz::Offset: Display,
{
    limited(a).map(|at| format!("Back at {}", when(at, now, tz)))
}

/// What an account is called: its label, else its email.
pub(crate) fn name(a: &Account) -> &str {
    [&a.label, &a.email].into_iter().find(|s| !s.is_empty()).map_or("Not signed in", String::as_str)
}

/// A built-in login nobody signed in to here.
pub(crate) fn never_signed_in(a: &Account) -> bool {
    !a.signed_in && a.email.is_empty()
}

/// When an account at a limit can start sessions again: the last of its full windows to reset.
fn limited(a: &Account) -> Option<i64> {
    let usage = a.usage.as_ref().filter(|u| a.signed_in && !u.stale)?;
    [&usage.session, &usage.week].into_iter().flatten().filter(|w| w.percent >= 100.).map(|w| w.resets_at).max()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Tone {
    Plain,
    Accent,
    Warn,
    Bad,
}

/// The tags after an account's name; `shared` while its provider has others on the machine.
pub(crate) fn tags<Tz: TimeZone>(a: &Account, shared: bool, now: i64, tz: &Tz) -> Vec<(String, Tone)>
where
    Tz::Offset: Display,
{
    let built_in = (a.id == BUILT_IN).then(|| ("Built-in".to_string(), Tone::Plain));
    let default = (a.default && shared).then(|| ("New agents".to_string(), Tone::Accent));
    let limited = limited(a).map(|at| (format!("Limited \u{b7} back {}", when(at, now, tz)), Tone::Bad));
    let out = (!a.signed_in && !never_signed_in(a)).then(|| ("Signed out".to_string(), Tone::Plain));
    let stale = a.usage.as_ref().filter(|u| a.signed_in && u.stale).map(|u| (format!("As of {}", when(u.at, now, tz)), Tone::Warn));
    [built_in, default, limited, out, stale].into_iter().flatten().collect()
}

/// The line under an account's name: its email when a label hides it, its plan and its running sessions.
pub(crate) fn subline(a: &Account, provider: &str, host: &str) -> String {
    if never_signed_in(a) {
        return format!("{provider} isn't signed in on {host}");
    }
    let email = (!a.label.is_empty() && !a.email.is_empty()).then(|| a.email.clone());
    let plan = (!a.plan.is_empty()).then(|| a.plan.clone());
    let running = match a.running {
        0 => None,
        1 => Some("1 session open".to_string()),
        n => Some(format!("{n} sessions open")),
    };
    [email, plan, running].into_iter().flatten().collect::<Vec<_>>().join(" \u{b7} ")
}

/// A usage window's whole percent, how full it reads, and when it resets: "back 14:05" once it's full.
pub(crate) fn meter<Tz: TimeZone>(w: &daemon::Window, now: i64, tz: &Tz) -> (u32, Tone, String)
where
    Tz::Offset: Display,
{
    let percent = w.percent.round().clamp(0., 100.) as u32;
    let tone = tone(percent);
    let at = if w.resets_at > 0 { when(w.resets_at, now, tz) } else { String::new() };
    let reset = if tone == Tone::Bad && !at.is_empty() { format!("back {at}") } else { at };
    (percent, tone, reset)
}

/// "14:05" today, else "Fri 14:05".
fn when<Tz: TimeZone>(ms: i64, now: i64, tz: &Tz) -> String
where
    Tz::Offset: Display,
{
    let local = |ms| DateTime::from_timestamp_millis(ms).unwrap_or_default().with_timezone(tz);
    let at = local(ms);
    at.format(if at.date_naive() == local(now).date_naive() { "%H:%M" } else { "%a %H:%M" }).to_string()
}

/// The toolbar's word on usage.
pub(crate) fn checked(checking: bool, at: i64, now: i64) -> String {
    match (checking, (now - at) / 60_000) {
        (true, _) => "Checking usage\u{2026}".into(),
        _ if at == 0 => "Usage not checked yet".into(),
        (_, ..=0) => "Usage checked just now".into(),
        (_, 1) => "Usage checked 1 min ago".into(),
        (_, n) => format!("Usage checked {n} min ago"),
    }
}

/// Where a machine's pocketd listens. Finding a host's reads a file, so call it off the UI thread.
fn endpoint(host: &Option<(hosts::Host, PathBuf)>) -> daemon::Endpoint {
    match host {
        Some((host, home)) => host.endpoint(home),
        None => daemon::Endpoint::own(),
    }
}

impl Desktop {
    /// The machine's host and the app's home, to reach its pocketd; `None` for a host since removed.
    fn machine(&self, host: &Host) -> Option<Option<(hosts::Host, PathBuf)>> {
        let Some(id) = host else { return Some(None) };
        let host = self.store.hosts.iter().find(|h| &h.id == id)?.clone();
        Some(Some((host, self.store.home().to_path_buf())))
    }

    /// "this Mac", else the host's name, to sit inside a sentence.
    pub(crate) fn machine_name(&self, host: &Host) -> String {
        host.as_deref().map_or_else(|| "this Mac".into(), |id| self.host_name(id))
    }

    /// Reads the machine's accounts, then, with `usage`, has its pocketd fetch their usage. A failed read keeps what was shown.
    pub(crate) fn check_accounts(&mut self, host: Host, usage: bool, cx: &mut Context<Self>) {
        let Some(reach) = self.machine(&host) else { return };
        if !self.accounts.checking.insert(host.clone()) {
            self.accounts.again.insert(host);
            return;
        }
        let bg = cx.background_executor().clone();
        let ask = move |op: &'static str| {
            let reach = reach.clone();
            bg.spawn(async move { daemon::request_to(&endpoint(&reach), &json!({"op": op}), TIMEOUT) })
        };
        let take = |host: &Host, reply: Result<Msg, String>, d: &mut Desktop| {
            if let Ok(m) = reply {
                d.accounts.take(host, m);
            }
        };
        cx.spawn(async move |this, cx| {
            let listed = ask("accounts").await;
            let ok = listed.is_ok();
            _ = this.update(cx, |d, cx| {
                take(&host, listed, d);
                cx.notify();
            });
            if usage && ok {
                let fetched = ask("accounts.usage").await;
                _ = this.update(cx, |d, _| {
                    if fetched.is_ok() {
                        d.accounts.checked_at = now_ms();
                    }
                    take(&host, fetched, d);
                });
            }
            _ = this.update(cx, |d, cx| {
                d.accounts.checking.remove(&host);
                if d.accounts.again.remove(&host) {
                    d.check_accounts(host, true, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Reads this Mac's accounts and every connected host's, with their usage.
    pub(crate) fn refresh_accounts(&mut self, cx: &mut Context<Self>) {
        let online: Vec<Host> = self.store.hosts.iter().filter(|h| self.fleet.links.online(&h.id)).map(|h| Some(h.id.clone())).collect();
        for host in std::iter::once(None).chain(online) {
            self.check_accounts(host, true, cx);
        }
        cx.notify();
    }

    /// Runs an accounts op on the machine and shows the list it answers with, or its error.
    pub(crate) fn account_op(&mut self, host: Host, msg: Value, cx: &mut Context<Self>) {
        let Some(reach) = self.machine(&host) else { return };
        let reply = cx.background_executor().spawn(async move { daemon::request_to(&endpoint(&reach), &msg, TIMEOUT) });
        cx.spawn(async move |this, cx| {
            let reply = reply.await;
            _ = this.update(cx, |d, cx| {
                match reply {
                    Ok(m) => d.accounts.take(&host, m),
                    Err(e) => d.error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Turns the machine's switching at a limit on or off, showing it at once.
    pub(crate) fn set_auto_switch(&mut self, host: Host, on: bool, cx: &mut Context<Self>) {
        self.accounts.auto_switch.insert(host.clone(), on);
        self.account_op(host, json!({"op": "accounts.autoswitch", "text": if on { "on" } else { "off" }}), cx);
        cx.notify();
    }

    /// Signs in to `key`, or adds an account when its id is empty, in a terminal on its machine; one per provider there at a time.
    pub(crate) fn sign_in_account(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) {
        self.accounts.menu = None;
        if self.accounts.signins.iter().any(|s| s.on(&key.host, &key.provider)) {
            return;
        }
        let Some(reach) = self.machine(&key.host) else { return };
        let msg = key.op("accounts.signin");
        let signin = self.new_sign_in(key.clone(), window, cx);
        self.accounts.signins.push(signin);
        let reply = cx.background_executor().spawn(async move { daemon::request_to(&endpoint(&reach), &msg, TIMEOUT) });
        cx.spawn(async move |this, cx| {
            let reply = reply.await;
            _ = this.update(cx, |d, cx| {
                match reply {
                    Ok(m) if m.ev == "spawned" => d.sign_in_spawned(&key, m.id, m.text, cx),
                    Ok(m) => {
                        d.accounts.signins.retain(|s| !s.on(&key.host, &key.provider));
                        d.accounts.take(&key.host, m);
                    }
                    Err(e) => {
                        d.accounts.signins.retain(|s| !s.on(&key.host, &key.provider));
                        d.error = Some(e);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn new_sign_in(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) -> SignIn {
        let code = cx.new(|cx| InputState::new(window, cx).placeholder("Code from the page, if any"));
        let (host, provider) = (key.host.clone(), key.provider.clone());
        let sub = cx.subscribe_in(&code, window, move |this, _, ev: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                this.send_sign_in_code(&host, &provider, window, cx);
            }
        });
        SignIn { key, terminal: None, added: String::new(), cancelled: false, code, output_open: false, _sub: sub }
    }

    /// Attaches to the sign-in's terminal without showing it; one cancelled before it started is closed straight away.
    fn sign_in_spawned(&mut self, key: &Key, theirs: String, added: String, cx: &mut Context<Self>) {
        let id = match &key.host {
            Some(h) => hosts::session::tag(h, &theirs),
            None => theirs,
        };
        let Some(s) = self.accounts.signins.iter_mut().find(|s| s.on(&key.host, &key.provider) && s.terminal.is_none()) else {
            self.daemon.send(json!({"op": "close", "id": id}));
            return self.check_accounts(key.host.clone(), false, cx);
        };
        (s.terminal, s.added) = (Some(id), added);
        self.daemon.send_to(key.host.as_deref(), json!({"op": "list"}));
        self.check_accounts(key.host.clone(), false, cx);
    }

    /// Ends the sign-in; an account it was adding is dropped rather than left failed.
    pub(crate) fn cancel_sign_in(&mut self, host: &Host, provider: &str, cx: &mut Context<Self>) {
        let Some(s) = self.accounts.signins.iter_mut().find(|s| s.on(host, provider)) else { return };
        s.cancelled = true;
        match s.terminal.clone() {
            Some(id) => {
                if self.terminals.close(&id) {
                    self.daemon.send(json!({"op": "close", "id": id}));
                }
                self.sign_in_ended(&id, cx);
            }
            None => self.accounts.signins.retain(|s| !s.on(host, provider)),
        }
        cx.notify();
    }

    /// Types the code from the browser into the sign-in's terminal, as a paste followed by Return.
    pub(crate) fn send_sign_in_code(&mut self, host: &Host, provider: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.accounts.signins.iter().find(|s| s.on(host, provider)) else { return };
        let (Some(id), text) = (s.terminal.clone(), s.code.read(cx).value().trim().to_string()) else { return };
        let Some(bracketed) = self.terminals.sessions.term(&id).map(|t| t.mode(2004)) else { return };
        if text.is_empty() {
            return;
        }
        let mut bytes = term::paste_bytes(&text, bracketed);
        bytes.push(b'\r');
        self.daemon.input(&id, &bytes);
        s.code.update(cx, |c, cx| c.set_value("", window, cx));
    }

    /// Edits the account's name in its row, starting from the one shown, all selected.
    pub(crate) fn start_account_rename(&mut self, key: Key, was: String, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |s, cx| {
            s.set_value(was.clone(), window, cx);
            s.focus(window, cx);
            s.select_all(window, cx);
        });
        let sub = cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, _, cx| match ev {
            InputEvent::PressEnter { .. } | InputEvent::Blur => this.save_account_rename(cx),
            _ => {}
        });
        self.accounts.menu = None;
        self.accounts.rename = Some(Rename { key, input, was, _sub: sub });
        cx.notify();
    }

    /// An emptied name shows the email again.
    fn save_account_rename(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.accounts.rename.take() else { return };
        let text = r.input.read(cx).value().trim().to_string();
        if text != r.was {
            let mut msg = r.key.op("accounts.rename");
            msg["text"] = text.into();
            self.account_op(r.key.host.clone(), msg, cx);
        }
        cx.notify();
    }

    /// pocketd keeps or drops an account once its sign-in's terminal ends.
    pub(crate) fn sign_in_ended(&mut self, terminal: &str, cx: &mut Context<Self>) {
        let Some(at) = self.accounts.signins.iter().position(|s| s.terminal.as_deref() == Some(terminal)) else { return };
        let s = self.accounts.signins.remove(at);
        self.terminals.close(terminal);
        let host = s.key.host.clone();
        let dismiss = (s.cancelled && !s.added.is_empty()).then(|| Key { id: s.added.clone(), ..s.key.clone() }.op("accounts.remove"));
        // pocketd reads the sign-in only after it has told us the terminal exited.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            _ = this.update(cx, |d, cx| match dismiss {
                Some(op) => d.account_op(host, op, cx),
                None => d.check_accounts(host, true, cx),
            });
        })
        .detach();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Accounts, BUILT_IN, Tone, checked, default_of, device_code, in_use, meter, name, peak, project_picks, screen_lines, session_label, shared, sign_in_link, subline, tags, tone, until};
    use std::collections::BTreeMap;
    use chrono::Utc;
    use daemon::Account;
    use serde_json::json;
    use daemon::{Usage, Window};

    const NOW: i64 = 1_760_000_000_000;

    fn account(provider: &str, id: &str) -> Account {
        Account { id: id.into(), provider: provider.into(), email: format!("{id}@x.com"), signed_in: true, ..Default::default() }
    }

    fn window(percent: f64, resets_at: i64) -> Option<Window> {
        Some(Window { percent, resets_at })
    }

    #[test]
    fn an_account_is_named_by_its_label_then_its_email() {
        let mut a = account("claude", "a");
        assert_eq!(name(&a), "a@x.com");
        a.label = "Work".into();
        assert_eq!(name(&a), "Work");
        assert_eq!(name(&Account::default()), "Not signed in");
    }

    #[test]
    fn new_agents_is_tagged_only_while_there_is_another_account_to_pick() {
        let a = Account { default: true, ..account("claude", BUILT_IN) };
        let tags = |shared| tags(&a, shared, NOW, &Utc).into_iter().map(|(t, _)| t).collect::<Vec<_>>();
        assert_eq!(tags(false), ["Built-in"]);
        assert_eq!(tags(true), ["Built-in", "New agents"]);
    }

    #[test]
    fn an_account_at_a_limit_says_when_it_is_back() {
        let full = Usage { session: window(100., NOW + 3_600_000), week: window(40., NOW + 86_400_000), ..Default::default() };
        let a = Account { usage: Some(full), ..account("codex", "a") };
        assert_eq!(tags(&a, false, NOW, &Utc), [("Limited \u{b7} back 09:53".to_string(), Tone::Bad)]);
        let stale = Account { usage: Some(Usage { stale: true, at: NOW - 60_000, ..a.usage.clone().unwrap() }), ..a };
        assert_eq!(tags(&stale, false, NOW, &Utc), [("As of 08:52".to_string(), Tone::Warn)]);
    }

    #[test]
    fn a_signed_out_account_says_so_but_a_never_used_login_does_not() {
        let out = Account { signed_in: false, ..account("claude", "a") };
        assert_eq!(tags(&out, false, NOW, &Utc), [("Signed out".to_string(), Tone::Plain)]);
        let never = Account { id: BUILT_IN.into(), provider: "codex".into(), ..Default::default() };
        assert_eq!(tags(&never, false, NOW, &Utc), [("Built-in".to_string(), Tone::Plain)]);
        assert_eq!(subline(&never, "Codex", "devbox"), "Codex isn't signed in on devbox");
    }

    #[test]
    fn the_subline_shows_the_email_only_when_a_label_hides_it() {
        let a = Account { plan: "Max".into(), running: 2, ..account("claude", "a") };
        assert_eq!(subline(&a, "Claude Code", "This Mac"), "Max \u{b7} 2 sessions open");
        let labelled = Account { label: "Work".into(), running: 0, ..a };
        assert_eq!(subline(&labelled, "Claude Code", "This Mac"), "a@x.com \u{b7} Max");
    }

    #[test]
    fn a_meter_reads_full_from_100_and_high_from_80() {
        let at = NOW + 3_600_000;
        assert_eq!(meter(&Window { percent: 42.4, resets_at: at }, NOW, &Utc), (42, Tone::Plain, "09:53".into()));
        assert_eq!(meter(&Window { percent: 80., resets_at: 0 }, NOW, &Utc), (80, Tone::Warn, String::new()));
        assert_eq!(meter(&Window { percent: 104., resets_at: at + 86_400_000 }, NOW, &Utc), (100, Tone::Bad, "back Fri 09:53".into()));
    }

    #[test]
    fn a_provider_shares_only_when_it_has_several_accounts() {
        let list = [account("claude", BUILT_IN), account("claude", "work"), account("codex", "x")];
        assert!(shared(&list, "claude") && !shared(&list, "codex"));
    }

    #[test]
    fn a_session_names_its_account_only_while_its_provider_has_several() {
        let list = [Account { label: "Home".into(), ..account("claude", BUILT_IN) }, account("claude", "work"), account("codex", "x")];
        assert_eq!(session_label(&list, "claude", ""), Some("Home".into()));
        assert_eq!(session_label(&list, "claude", "work"), Some("work@x.com".into()));
        assert_eq!(session_label(&list, "codex", "x"), None);
    }

    #[test]
    fn a_project_names_the_accounts_it_picks_only_while_there_are_others() {
        let list = [Account { label: "Home".into(), default: true, ..account("claude", BUILT_IN) }, account("claude", "work"), account("codex", "x")];
        let picks = BTreeMap::from([("claude".to_string(), "work".to_string()), ("codex".to_string(), "x".to_string())]);
        assert_eq!(project_picks(&list, &picks), [("claude".to_string(), "work@x.com".to_string())]);
        let gone = BTreeMap::from([("claude".to_string(), "removed".to_string())]);
        assert!(project_picks(&list, &gone).is_empty());
        assert_eq!(default_of(&list, "claude").map(name), Some("Home"));
    }

    #[test]
    fn a_session_shows_the_account_it_runs_on_even_signed_out_with_its_fuller_window() {
        let home = Account { signed_in: false, usage: Some(Usage { session: window(42.4, 0), week: window(81., 0), ..Default::default() }), ..account("claude", BUILT_IN) };
        let list = [home, account("claude", "work")];
        let shown = in_use(&list, "claude", "").unwrap();
        assert_eq!((shown.id.as_str(), peak(shown)), (BUILT_IN, Some(81)));
        assert_eq!(peak(in_use(&list, "claude", "work").unwrap()), None);
        assert!(in_use(&list, "codex", "").is_none());
        assert_eq!([tone(79), tone(80), tone(100)], [Tone::Plain, Tone::Warn, Tone::Bad]);
    }

    #[test]
    fn a_reset_counts_down_in_minutes_then_hours_then_days() {
        assert_eq!(until(NOW + 45 * 60_000, NOW), "45m");
        assert_eq!(until(NOW + 130 * 60_000 - 1, NOW), "2h 10m");
        assert_eq!(until(NOW + (3 * 24 + 4) * 3_600_000, NOW), "3d 4h");
        assert_eq!(until(NOW - 1, NOW), "0m");
    }

    #[test]
    fn a_sign_in_still_running_is_not_an_account_to_pick() {
        let list = [account("claude", BUILT_IN), Account { pending: true, ..account("claude", "new") }];
        assert!(!shared(&list, "claude"));
    }

    #[test]
    fn a_built_in_login_nobody_signed_in_to_is_not_an_account_to_pick() {
        let unused = Account { signed_in: false, email: String::new(), ..account("claude", BUILT_IN) };
        let list = [unused, account("claude", "work")];
        assert!(!shared(&list, "claude"));
    }

    #[test]
    fn a_create_names_an_account_only_its_machine_lists() {
        let mut accounts = Accounts::default();
        accounts.lists.insert(Some("devbox".into()), vec![account("claude", "work")]);
        let mut spec = json!({"project": "//devbox/src/app", "provider": "claude", "account": "work"});
        accounts.offer(&mut spec);
        assert_eq!(spec["account"], "work");
        let mut here = json!({"project": "/src/app", "provider": "claude", "account": "work"});
        accounts.offer(&mut here);
        assert!(here.get("account").is_none());
    }

    fn screen(output: &str) -> Vec<String> {
        let mut t = term::Term::new(24, 8);
        t.write(output.as_bytes());
        let (f, cells) = t.frame();
        screen_lines(cells, f.cols)
    }

    #[test]
    fn a_sign_in_link_wrapped_across_rows_reads_whole() {
        let lines = screen("Opening browser\r\nvisit: https://claude.com/oauth?code=true&state=abc\r\nPaste code here >");
        assert_eq!(lines[1], "visit: https://claude.com/oauth?code=true&state=abc");
        assert_eq!(sign_in_link(&lines).as_deref(), Some("https://claude.com/oauth?code=true&state=abc"));
        assert_eq!(device_code(&lines), None);
    }

    #[test]
    fn codex_s_device_code_is_the_line_after_it_is_asked_for() {
        let lines = screen("1. Open this link\r\n   https://auth.openai.com/codex/device\r\n2. Enter this one-time code\r\n\r\n   ABCD-1234");
        assert_eq!(sign_in_link(&lines).as_deref(), Some("https://auth.openai.com/codex/device"));
        assert_eq!(device_code(&lines).as_deref(), Some("ABCD-1234"));
        assert_eq!(device_code(&screen("2. Enter this one-time code")), None);
    }

    #[test]
    fn the_toolbar_counts_minutes_since_usage_was_read() {
        assert_eq!(checked(true, NOW, NOW), "Checking usage\u{2026}");
        assert_eq!(checked(false, 0, NOW), "Usage not checked yet");
        assert_eq!(checked(false, NOW - 30_000, NOW), "Usage checked just now");
        assert_eq!(checked(false, NOW - 5 * 60_000, NOW), "Usage checked 5 min ago");
    }
}
