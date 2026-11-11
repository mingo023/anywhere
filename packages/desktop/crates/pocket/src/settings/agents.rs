use super::catalog::{Look, Setting};
use super::models::{count_line, not_found};
use agents::providers;
use crate::desktop::Desktop;
use crate::status::Status;
use crate::util::tilde;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::LaunchPick;
use store::prefs::agents::{ACCESSES, CHAT_TO};
use theme::*;

const NEW: &str = "New sessions";

pub(super) const ROWS: &[Setting] = &[
    Setting::custom("providers", "Providers", "Providers", |d, _, cx| d.providers_row(cx)).hint("The agent CLIs found on your login PATH"),
    Setting::custom("provider-status", "Providers", "Status", |d, _, _| d.provider_status()).hint("How Anywhere reads status"),
    Setting::choice("default-agent", NEW, "Default agent", &PROVIDER_NAMES, Look::Dropdown, default_index, |s, i| {
        s.agents.default = LaunchPick::PROVIDERS[i].into();
        s.agents.enable(LaunchPick::PROVIDERS[i]);
    })
    .hint("Starts in a project that hasn't picked one, and in new agent tabs")
    .offers(offered_provider)
    .shows(|s, live| index_of(s.agents.default_agent(live))),
    Setting::choice("permission-mode", NEW, "Permission mode", &["Agent's settings", "Ask", "Edits", "Auto", "Full"], Look::Segmented, |s| access_index(s.agents.access()), |s, i| {
        s.agents.access = ACCESSES[i].into()
    })
    .hint("Agent's settings uses the agent's own config. Full skips every approval"),
    Setting::switch("plan-mode", NEW, "Start in plan mode", |s| s.agents.plan, |s, on| s.agents.plan = on).hint("For providers that support plan mode"),
    Setting::switch("start-as-chat", NEW, "Start as a chat", |s| s.agents.start_as_chat, |s, on| s.agents.start_as_chat = on)
        .hint("Experimental. New chat in a tab's + menu always starts one"),
    Setting::choice("runs-in", NEW, "Runs in", &["Local", "Worktree"], Look::Segmented, |s| s.agents.new_worktree as usize, |s, i| s.agents.new_worktree = i == 1)
        .hint("A new worktree keeps the agent's edits off your branch"),
    Setting::choice("agent-exits", "Tabs", "When an agent exits", &["Return to shell", "Close tab"], Look::Segmented, |s| s.agents.close_on_exit as usize, |s, i| {
        s.agents.close_on_exit = i == 1
    }),
    Setting::choice("new-tabs-open", "Tabs", "New tabs and splits open", &["Shell", "Default agent"], Look::Segmented, |s| s.agents.tabs_open_agent as usize, |s, i| {
        s.agents.tabs_open_agent = i == 1
    }),
    Setting::choice("add-to-chat", "Composer", "\u{201c}Add to chat\u{201d}", &["Send now", "Paste only"], Look::Segmented, |s| (!s.agents.send_now) as usize, |s, i| {
        s.agents.send_now = i == 0
    })
    .hint("Paste only lets you edit before sending"),
    Setting::choice("add-to-chat-to", "Composer", "Send \u{201c}Add to chat\u{201d} to", &["Focused session", "Most recent agent", "Ask each time"], Look::Dropdown, |s| chat_to_index(s.agents.chat_to()), |s, i| {
        s.agents.chat_to = CHAT_TO[i].into()
    })
    .advanced(),
];

/// Whether a provider picker lists the `i`th of `LaunchPick::PROVIDERS`: only the ones left on that `live` found installed.
pub(super) fn offered_provider(s: &store::Store, live: &::agents::Agents, i: usize) -> bool {
    s.agents.offered(live).contains(&LaunchPick::PROVIDERS[i])
}

/// The agent saved as the default, for Reset; the menu shows `Agents::default_agent`, which is one that's offered.
fn default_index(s: &store::Store) -> usize {
    index_of(LaunchPick::known_provider(&s.agents.default))
}

fn index_of(provider: &str) -> usize {
    providers::index(provider).unwrap_or(0)
}

/// The agents Settings gives a row: the two every pocketd runs, and each other this pocketd named, found or not. One it
/// never named gets none: a pocketd older than the agent can't run it, so its install hint would lead nowhere.
fn listed_providers(live: &::agents::Agents) -> Vec<&'static str> {
    providers::ALL.iter().filter(|b| b.assumed_installed || live.providers.iter().any(|p| p.id == b.id)).map(|b| b.id).collect()
}

/// Turns `provider` on if it's off, else off: `Agents::disable` leaves the last one offered on.
pub(super) fn toggle_enabled(prefs: &mut store::prefs::agents::Agents, provider: &str, live: &::agents::Agents) {
    if prefs.disabled.contains(provider) { prefs.enable(provider) } else { prefs.disable(provider, live) }
}

fn chat_to_index(to: &str) -> usize {
    CHAT_TO.iter().position(|t| *t == to).unwrap_or(0)
}

fn access_index(access: &str) -> usize {
    ACCESSES.iter().position(|a| *a == access).unwrap_or(0)
}

/// What a provider's row says under its name: the models it lists and where pocketd found it, or how to install it.
/// `models` is `None` until its CLI has listed them.
fn provider_line(provider: &str, installed: bool, models: Option<usize>, path: Option<&str>) -> String {
    if !installed {
        return not_found(provider);
    }
    let found = path.filter(|p| !p.is_empty()).map(tilde);
    let parts: Vec<String> = models.map(count_line).into_iter().chain(found).collect();
    if parts.is_empty() { "Built in".into() } else { parts.join(" \u{b7} ") }
}

/// How Anywhere learns a provider's sessions need you, are done or failed.
pub(super) fn status_source(provider: &str) -> String {
    let how = providers::known(provider).status_via;
    let [needs, done, failed] = [Status::NeedsYou, Status::Done, Status::Failed].map(Status::label);
    format!("Built in \u{b7} {how} for {needs}, {done} and {failed}")
}

impl Desktop {
    fn provider_status(&self) -> Div {
        let rows = LaunchPick::PROVIDERS.into_iter().filter(|p| self.installed(p)).map(|p| {
            div()
                .px(px(14.))
                .py(px(8.))
                .flex()
                .items_center()
                .gap(px(12.))
                .border_t(px(0.5))
                .border_color(SEPARATOR)
                .child(div().w(px(96.)).flex_none().text_size(px(13.)).text_color(TEXT).child(self.agent_name(p)))
                .child(div().flex_1().min_w_0().text_size(px(12.)).text_color(TEXT_3).child(status_source(p)))
        });
        let caption = div().px(px(14.)).pt(px(9.)).pb(px(7.)).text_size(px(12.)).text_color(TEXT_3).child("Status \u{b7} how Anywhere reads it");
        div().flex().flex_col().child(caption).children(rows)
    }

    /// One row per agent, as its page is opened from: its mark, name and what `provider_line` says; only an installed
    /// one can be turned off.
    fn providers_row(&mut self, cx: &mut Context<Self>) -> Div {
        let (default, listed) = (self.store.agents.default_agent(&self.agents), listed_providers(&self.agents));
        let rows = LaunchPick::PROVIDERS.into_iter().enumerate().filter(|(_, p)| listed.contains(p)).enumerate().map(|(row, (i, p))| {
            let (on, installed) = (!self.store.agents.disabled.contains(p), self.installed(p));
            let tile = div().size(px(28.)).flex().flex_none().items_center().justify_center().rounded(px(7.)).bg(FILL_2).child(provider_icon(p, 15., TEXT)).when(!installed, |d| d.opacity(0.5));
            let name = div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(TEXT)
                .child(self.agent_name(p))
                .when(p == default, |d| d.child(div().px(px(5.)).rounded(px(4.)).bg(FILL_3).text_size(px(10.5)).text_color(TEXT_2).child("Default")));
            let models = Some(self.models_of(p).len()).filter(|n| *n > 0);
            let line = provider_line(p, installed, models, self.settings.host.status.as_ref().and_then(|s| s.providers.get(p)).map(String::as_str));
            let text = div().flex_1().min_w_0().flex().flex_col().gap(px(1.)).child(name).child(div().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(line));
            let toggle = installed.then(|| {
                div().id(("provider-enabled", i)).flex_none().cursor_pointer().child(ui::toggle("switch", on)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    toggle_enabled(&mut this.store.agents, p, &this.agents);
                    this.save_soon(cx);
                    cx.notify();
                }))
            });
            div()
                .id(("provider", i))
                .px(px(14.))
                .py(px(9.))
                .flex()
                .items_center()
                .gap(px(12.))
                .cursor_pointer()
                .hover(|d| d.bg(FILL_1))
                .when(row > 0, |d| d.border_t(px(0.5)).border_color(SEPARATOR))
                .child(tile)
                .child(text)
                .children(toggle)
                .child(icon("chevron-right", 13., TEXT_4))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_provider(i, window, cx)))
        });
        div().flex().flex_col().children(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::{access_index, listed_providers, offered_provider, provider_line, status_source, toggle_enabled};
    use agents::providers::ProviderInfo;
    use store::Store;

    #[test]
    fn brands_follow_the_provider_table() {
        assert_eq!(theme::BRANDS.map(|b| b.id), agents::providers::IDS);
    }

    #[test]
    fn a_provider_picker_lists_only_the_providers_left_on_and_installed() {
        let mut s = Store::default();
        let offered = |s: &Store, live: &agents::Agents| (0..agents::providers::COUNT).filter(|i| offered_provider(s, live, *i)).collect::<Vec<_>>();
        let mut live = agents::Agents::default();
        assert_eq!(offered(&s, &live), [0, 1], "before pocketd says what is installed");
        live.providers = ["claude", "codex", "pi"].map(|id| ProviderInfo { id: id.into(), available: id != "codex", ..Default::default() }).to_vec();
        assert_eq!(offered(&s, &live), [0, 5]);
        s.agents.disable("pi", &live);
        assert_eq!(offered(&s, &live), [0]);
    }

    fn pocketd(named: &[(&str, bool)]) -> agents::Agents {
        agents::Agents { providers: named.iter().map(|(id, available)| ProviderInfo { id: (*id).into(), available: *available, ..Default::default() }).collect(), ..Default::default() }
    }

    #[test]
    fn settings_list_an_agent_only_when_pocketd_named_it() {
        assert_eq!(listed_providers(&agents::Agents::default()), ["claude", "codex"], "a pocketd that names none, as one older than the other agents");
        assert_eq!(listed_providers(&pocketd(&[("claude", true)])), ["claude", "codex"]);
        assert_eq!(listed_providers(&pocketd(&[("claude", true), ("codex", false), ("pi", false), ("hermes", true)])), ["claude", "codex", "pi", "hermes"], "found or not");
    }

    #[test]
    fn the_switch_leaves_the_last_agent_on_offer_on() {
        let mut s = Store::default();
        let live = pocketd(&[("claude", true), ("codex", true), ("pi", false)]);
        toggle_enabled(&mut s.agents, "claude", &live);
        toggle_enabled(&mut s.agents, "codex", &live);
        assert_eq!(s.agents.offered(&live), ["codex"], "Pi is on but not installed, so Codex is the last");
        toggle_enabled(&mut s.agents, "claude", &live);
        toggle_enabled(&mut s.agents, "codex", &live);
        assert_eq!(s.agents.offered(&live), ["claude"]);
    }

    #[test]
    fn each_provider_says_how_its_status_is_read() {
        assert_eq!(status_source("claude"), "Built in \u{b7} uses Claude Code hooks for Needs you, Done and Failed");
        assert_eq!(status_source("codex"), "Built in \u{b7} reads Codex session events for Needs you, Done and Failed");
    }

    #[test]
    fn an_installed_provider_says_how_many_models_it_lists_and_where_it_was_found() {
        assert_eq!(provider_line("claude", true, None, None), "Built in");
        assert_eq!(provider_line("claude", true, Some(3), Some("")), "3 models available");
        assert_eq!(provider_line("claude", true, Some(1), Some("/opt/homebrew/bin/claude")), "1 model available · /opt/homebrew/bin/claude");
        assert_eq!(provider_line("claude", true, None, Some("/opt/homebrew/bin/claude")), "/opt/homebrew/bin/claude");
    }

    #[test]
    fn a_provider_that_is_not_installed_says_how_to_install_it() {
        assert_eq!(provider_line("grok", false, Some(2), Some("")), "Grok Build not found (curl -fsSL https://x.ai/cli/install.sh | bash). Install it, or restart Anywhere if it is already installed.");
    }

    #[test]
    fn the_permission_row_starts_on_the_agent_s_own_settings() {
        assert_eq!((access_index("settings"), access_index("full")), (0, 4));
    }
}
