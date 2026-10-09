use crate::desktop::Desktop;
use agents::automations::valid_time;
use daemon::Device;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use serde_json::{Map, Value, json};
use store::Store;

/// What Settings reads from pocketd, and the fields its own rows type into.
pub(crate) struct Host {
    /// pocketd's last status, read when Settings opens.
    pub status: Option<daemon::Status>,
    /// None until pocketd lists them.
    pub devices: Option<Vec<Device>>,
    pub time: Entity<InputState>,
    pub starter: Entity<InputState>,
    pub port: Entity<InputState>,
    pub renaming: Option<Rename>,
}

/// A paired phone whose name is open for editing.
pub(crate) struct Rename {
    pub id: String,
    pub field: Entity<InputState>,
    _commits: Subscription,
}

impl Host {
    pub fn new(store: &Store, window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let time = cx.new(|cx| InputState::new(window, cx).placeholder("09:00").default_value(store.automations.time.clone()));
        let starter = cx.new(|cx| InputState::new(window, cx).placeholder("Add a starter prompt"));
        let port = cx.new(|cx| InputState::new(window, cx).placeholder(channel::phone_port().to_string()).default_value(port_field(store.phone.port)));
        let subs = vec![
            cx.subscribe_in(&time, window, |this, field, ev: &InputEvent, _, cx| {
                let time = field.read(cx).value().trim().to_string();
                if matches!(ev, InputEvent::Change) && valid_time(&time) && time != this.store.automations.time {
                    this.store.automations.time = time;
                    this.save_soon(cx);
                }
            }),
            cx.subscribe_in(&starter, window, |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.add_starter(window, cx);
                }
            }),
            cx.subscribe_in(&port, window, |this, _, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    this.commit_port(window, cx);
                }
            }),
        ];
        (Self { status: None, devices: None, time, starter, port, renaming: None }, subs)
    }
}

/// Copies pocketd's settings into the store; true when one changed.
pub(crate) fn mirror(store: &mut Store, config: &Map<String, Value>) -> bool {
    let before = (store.general.clone(), store.automations.grace_hours, store.automations.history, store.phone.clone(), store.agents.providers.clone());
    let flag = |key: &str| config.get(key).and_then(Value::as_bool);
    let number = |key: &str| config.get(key).and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok());
    if let Some(on) = flag("restore.resumeAgents") {
        store.general.resume_agents = on;
    }
    if let Some(on) = flag("awake.enabled") {
        store.general.keep_awake = on;
    }
    if let Some(n) = number("awake.lingerMinutes") {
        store.general.linger_minutes = n;
    }
    if let Some(n) = number("automations.graceHours") {
        store.automations.grace_hours = n;
    }
    if let Some(n) = number("automations.history") {
        store.automations.history = n;
    }
    if let Some(a) = config.get("phone.maxAccess").and_then(Value::as_str) {
        store.phone.max_access = a.into();
    }
    if let Some(mode) = config.get("listen").and_then(Value::as_str) {
        store.phone.tailnet = mode == "auto";
    }
    if let Some(n) = number("port") {
        store.phone.port = if n == channel::phone_port() { 0 } else { n };
    }
    for p in store::LaunchPick::PROVIDERS {
        if let Some(command) = config.get(&format!("{p}.command")).and_then(Value::as_str)
            && store.agents.provider(p).command != command
        {
            store.agents.provider_mut(p).command = command.into();
        }
    }
    before != (store.general.clone(), store.automations.grace_hours, store.automations.history, store.phone.clone(), store.agents.providers.clone())
}

/// What the Port field shows for a stored port, the default spelled out.
pub(crate) fn port_field(port: u32) -> String {
    if port == 0 { channel::phone_port() } else { port }.to_string()
}

/// A stored port as pocketd's config-set takes it: empty for the default.
pub(crate) fn port_text(port: u32) -> String {
    if port == 0 { String::new() } else { port.to_string() }
}

/// The port typed into the Port field, 0 when it's empty or the default; the same range pocketd takes.
fn parse_port(text: &str, default: u32) -> Result<u32, &'static str> {
    match text.trim() {
        "" => Ok(0),
        t => match t.parse::<u32>() {
            Ok(n) if n == default => Ok(0),
            Ok(n) if (1024..=65535).contains(&n) => Ok(n),
            _ => Err("The port is a whole number from 1024 to 65535"),
        },
    }
}

/// "Connected · 100.64.0.1": the first address pocketd serves beyond this Mac.
pub(crate) fn tailnet_line(listen: &[String]) -> String {
    let ip = listen.iter().filter_map(|a| a.parse::<std::net::SocketAddr>().ok()).map(|a| a.ip()).find(|ip| !ip.is_loopback());
    ip.map_or("Connected".into(), |ip| format!("Connected \u{b7} {ip}"))
}

/// "Connected · version 1.4.2 · up 3d", or why there is nothing to say.
pub(crate) fn service_line(connected: bool, status: Option<&daemon::Status>) -> String {
    match status {
        _ if !connected => "Not connected".into(),
        Some(s) if !s.version.is_empty() => format!("Connected \u{b7} version {} \u{b7} up {}", s.version, uptime(s.uptime)),
        _ => "Connected".into(),
    }
}

fn uptime(secs: u64) -> String {
    match secs / 60 {
        m if m < 60 => format!("{m}m"),
        m if m < 24 * 60 => format!("{}h", m / 60),
        m => format!("{}d", m / (24 * 60)),
    }
}

impl Desktop {
    /// Reads pocketd's status and paired phones for Settings, off the UI thread.
    pub(crate) fn load_host(&mut self, cx: &mut Context<Self>) {
        if self.capturing {
            return;
        }
        cx.spawn(async |this, cx| {
            let (status, devices) = cx.background_executor().spawn(async { (daemon::request(&json!({"op": "status"})), daemon::request(&json!({"op": "devices"}))) }).await;
            this.update(cx, |d, cx| {
                if let Ok(reply) = status {
                    if let Some(config) = reply.status.as_ref().and_then(|s| s.config.as_ref())
                        && mirror(&mut d.store, config)
                    {
                        d.save_soon(cx);
                    }
                    d.settings.host.status = reply.status;
                }
                if let Ok(m) = devices {
                    d.settings.host.devices = Some(m.devices);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// When pocketd refuses, as an older one does for keys it doesn't know, the row goes back to the value it keeps.
    pub(crate) fn set_host(&mut self, key: &'static str, text: String, cx: &mut Context<Self>) {
        if self.capturing {
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { daemon::request(&json!({"op": "config-set", "key": key, "text": text})) }).await;
            if let Err(e) = result {
                this.update(cx, |d, cx| {
                    d.error = Some(format!("The background service didn't save that setting: {e}"));
                    d.load_host(cx);
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Like `set_host`, then rereads status for the addresses pocketd now serves.
    pub(crate) fn set_reach(&mut self, key: &'static str, text: String, cx: &mut Context<Self>) {
        if self.capturing {
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { daemon::request(&json!({"op": "config-set", "key": key, "text": text})) }).await;
            this.update(cx, |d, cx| {
                if let Err(e) = result {
                    d.error = Some(format!("The background service didn't save that setting: {e}"));
                }
                d.load_host(cx);
            })
            .ok();
        })
        .detach();
    }

    /// pocketd keeps its old port when the new one won't bind, so a refusal puts the field and store back.
    fn commit_port(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let before = self.store.phone.port;
        let port = match parse_port(&self.settings.host.port.read(cx).value(), channel::phone_port()) {
            Ok(port) => port,
            Err(e) => {
                self.error = Some(e.into());
                self.settings.host.port.update(cx, |f, cx| f.set_value(port_field(before), window, cx));
                cx.notify();
                return;
            }
        };
        self.settings.host.port.update(cx, |f, cx| f.set_value(port_field(port), window, cx));
        if port == before || self.capturing {
            return;
        }
        self.store.phone.port = port;
        self.save_soon(cx);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let text = port_text(port);
            let result = cx.background_executor().spawn(async move { daemon::request(&json!({"op": "config-set", "key": "port", "text": text})) }).await;
            this.update_in(cx, |d, window, cx| {
                if let Err(e) = result {
                    d.error = Some(format!("Anywhere kept port {}: {e}", port_field(before)));
                    d.store.phone.port = before;
                    d.settings.host.port.update(cx, |f, cx| f.set_value(port_field(before), window, cx));
                }
                d.load_host(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Opens a phone's name for editing; Enter or leaving the field saves it.
    pub(crate) fn start_phone_rename(&mut self, id: String, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_phone_rename(cx);
        let field = cx.new(|cx| InputState::new(window, cx).placeholder("Phone name").default_value(name.to_string()));
        let commits = cx.subscribe_in(&field, window, |this, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                this.commit_phone_rename(cx);
            }
        });
        field.update(cx, |f, cx| f.focus(window, cx));
        self.settings.host.renaming = Some(Rename { id, field, _commits: commits });
        cx.notify();
    }

    /// pocketd trims the name and refuses an empty one; the list rereads either way.
    fn commit_phone_rename(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.settings.host.renaming.take() else { return };
        let name = r.field.read(cx).value().trim().to_string();
        let unchanged = self.settings.host.devices.iter().flatten().any(|d| d.id == r.id && d.name == name);
        cx.notify();
        if unchanged || self.capturing {
            return;
        }
        let id = r.id;
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { daemon::request(&json!({"op": "devices.rename", "id": id, "text": name})) }).await;
            this.update(cx, |d, cx| {
                if let Err(e) = result {
                    d.error = Some(format!("Couldn't rename the phone: {e}"));
                }
                d.load_host(cx);
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn revoke_device(&mut self, id: String, cx: &mut Context<Self>) {
        if self.capturing {
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { daemon::request(&json!({"op": "devices.revoke", "id": id})) }).await;
            this.update(cx, |d, cx| {
                if let Err(e) = result {
                    d.error = Some(format!("Couldn't remove the phone: {e}"));
                }
                d.load_host(cx);
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{mirror, parse_port, port_field, service_line, tailnet_line};
    use serde_json::json;
    use store::Store;

    #[test]
    fn pocketd_settings_land_in_the_store_and_report_a_change_once() {
        let mut store = Store::default();
        let config = json!({"restore.resumeAgents": false, "awake.enabled": false, "awake.lingerMinutes": 10, "automations.graceHours": 0, "automations.history": 200, "phone.maxAccess": "edits"});
        let config = config.as_object().unwrap();
        assert!(mirror(&mut store, config));
        assert_eq!((store.general.resume_agents, store.general.keep_awake, store.general.linger_minutes), (false, false, 10));
        assert_eq!((store.automations.grace_hours, store.automations.history, store.phone.max_access.as_str()), (0, 200, "edits"));
        assert!(!mirror(&mut store, config));
    }

    #[test]
    fn pocketd_network_settings_land_in_the_store_with_its_default_port_as_zero() {
        let mut store = Store::default();
        assert!(mirror(&mut store, json!({"listen": "loopback", "port": 45170}).as_object().unwrap()));
        assert_eq!((store.phone.tailnet, store.phone.port), (false, 45170));
        assert!(mirror(&mut store, json!({"listen": "auto", "port": channel::phone_port()}).as_object().unwrap()));
        assert_eq!((store.phone.tailnet, store.phone.port), (true, 0));
    }

    #[test]
    fn pocketd_s_provider_commands_land_in_the_store() {
        let mut store = Store::default();
        assert!(mirror(&mut store, json!({"claude.command": "/opt/claude", "codex.command": ""}).as_object().unwrap()));
        assert_eq!((store.agents.command("claude"), store.agents.command("codex")), ("/opt/claude".to_string(), "codex".to_string()));
        assert!(!mirror(&mut store, json!({"claude.command": "/opt/claude", "codex.command": ""}).as_object().unwrap()));
    }

    #[test]
    fn the_port_field_spells_out_the_default_port() {
        assert_eq!([port_field(0), port_field(45170)], [channel::phone_port().to_string(), "45170".into()]);
    }

    #[test]
    fn the_port_field_takes_empty_or_the_default_as_default_and_refuses_privileged_ports() {
        assert_eq!((parse_port(" ", 4518), parse_port("4518", 4518), parse_port("45170", 4518)), (Ok(0), Ok(0), Ok(45170)));
        assert!(parse_port("80", 4518).is_err() && parse_port("70000", 4518).is_err() && parse_port("x", 4518).is_err());
    }

    #[test]
    fn the_tailscale_line_names_the_first_address_beyond_this_mac() {
        assert_eq!(tailnet_line(&["127.0.0.1:4517".into(), "100.64.0.1:4517".into()]), "Connected \u{b7} 100.64.0.1");
        assert_eq!(tailnet_line(&["127.0.0.1:4517".into()]), "Connected");
    }

    #[test]
    fn a_key_pocketd_leaves_out_keeps_its_copy() {
        let mut store = Store::default();
        assert!(!mirror(&mut store, json!({"unknown": 1}).as_object().unwrap()));
        assert_eq!((store.general.keep_awake, store.automations.grace_hours), (Store::default().general.keep_awake, 12));
    }

    #[test]
    fn the_service_line_says_how_long_pocketd_has_been_up() {
        let status = daemon::Status { version: "1.4.2".into(), uptime: 3 * 86_400 + 5, ..Default::default() };
        assert_eq!(service_line(true, Some(&status)), "Connected \u{b7} version 1.4.2 \u{b7} up 3d");
        assert_eq!(service_line(true, None), "Connected");
        assert_eq!(service_line(false, Some(&status)), "Not connected");
    }
}
