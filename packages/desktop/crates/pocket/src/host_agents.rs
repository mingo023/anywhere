//! Agents on hosts next to this Mac's: each host's pocketd agent channel, opened once the host serves projects.

use crate::desktop::Desktop;
use futures::StreamExt;
use gpui_kit::*;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct HostAgents {
    channels: HashMap<String, Task<()>>,
    /// Each host's home, from its `status`: its worktrees go under it.
    homes: HashMap<String, String>,
}

impl HostAgents {
    /// The home on the machine holding `key`; `~` for a host not yet heard from.
    pub(crate) fn home_of(&self, key: &str) -> String {
        match hosts::split(key) {
            (None, _) => crate::modals::form::home(),
            (Some(host), _) => self.homes.get(host).cloned().unwrap_or_else(|| "~".into()),
        }
    }
}

impl Desktop {
    /// Sends a connected host its projects and their settings, which its pocketd reads where the desktop isn't asked.
    pub(crate) fn sync_host_registry(&self, id: &str, cx: &mut Context<Self>) {
        if let Some(client) = hosts::remote::connected(id) {
            let registry = self.store.host_registry(id);
            cx.background_executor().spawn(async move { _ = client.sync_registry(&registry) }).detach();
        }
    }

    /// Lists the host's agents beside this Mac's, and lists its projects where its pocketd looks for them.
    pub(crate) fn open_host_agents(&mut self, id: &str, endpoint: &daemon::Endpoint, home: String, window: &mut Window, cx: &mut Context<Self>) {
        self.host_agents.homes.insert(id.into(), home);
        self.sync_host_registry(id, cx);
        // Its channel rides out the link's drops on its own, as the terminals' does.
        if self.outbox.has(id) {
            return;
        }
        let (outbox, mut rx) = agents::connect_with(&endpoint.path, endpoint.token.clone());
        self.outbox.add(id, outbox);
        let host = id.to_string();
        let task = cx.spawn_in(window, async move |this, cx| {
            while let Some(mut ev) = rx.next().await {
                if !hosts::session::tag_event(&host, &mut ev) {
                    continue;
                }
                if this.update_in(cx, |d, window, cx| d.on_agents_from(Some(&host), ev, window, cx)).is_err() {
                    break;
                }
            }
        });
        self.host_agents.channels.insert(id.into(), task);
    }

    /// Drops a host taken off this Mac, and its agents with it.
    pub(crate) fn close_host_agents(&mut self, id: &str) {
        self.host_agents.channels.remove(id);
        self.host_agents.homes.remove(id);
        self.outbox.remove(id);
        self.agents.forget(id);
    }
}
