use super::catalog::{Look, Setting};
use super::toasts::Toast;
use super::{Settings, dropdown, focus_ring, provider, ring_around};
use crate::status::Status;
use agents::providers;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::LaunchPick;
use store::prefs::agents::{ACCESSES, CHAT_TO};
use theme::*;

const NEW: &str = "New sessions";

pub(super) const ROWS: &[Setting] = &[
    Setting::custom("providers", "Providers", "Providers", |d, _, cx| d.providers_row(cx)).hint("The agent CLIs found on your login PATH"),
    Setting::choice("default-agent", NEW, "Default agent", &PROVIDER_NAMES, Look::Dropdown, default_index, |s, i| {
        s.agents.default = LaunchPick::PROVIDERS[i].into();
        s.agents.enable(LaunchPick::PROVIDERS[i]);
    })
    .hint("Starts in a project that hasn't picked one, and in new agent tabs")
    .offers(offered_provider)
    .shows(|s, live| index_of(s.agents.default_agent(live))),
    Setting::choice(
        "permission-mode",
        NEW,
        "Permission mode",
        &["Agent's settings", "Ask", "Edits", "Auto", "Full"],
        Look::Segmented,
        |s| access_index(s.agents.access()),
        |s, i| s.agents.access = ACCESSES[i].into(),
    )
    .hint("Agent's settings uses the agent's own config. Full skips every approval"),
    Setting::switch("plan-mode", NEW, "Start in plan mode", |s| s.agents.plan, |s, on| s.agents.plan = on).hint("For providers that support plan mode"),
    Setting::switch("start-as-chat", NEW, "Start as a chat", |s| s.agents.start_as_chat, |s, on| s.agents.start_as_chat = on)
        .hint("Experimental. New chat in a tab's + menu always starts one"),
    Setting::choice(
        "runs-in",
        NEW,
        "Runs in",
        &["Local", "Worktree"],
        Look::Segmented,
        |s| s.agents.new_worktree as usize,
        |s, i| s.agents.new_worktree = i == 1,
    )
    .hint("A new worktree keeps the agent's edits off your branch"),
    Setting::choice(
        "agent-exits",
        "Tabs",
        "When an agent exits",
        &["Return to shell", "Close tab"],
        Look::Segmented,
        |s| s.agents.close_on_exit as usize,
        |s, i| s.agents.close_on_exit = i == 1,
    ),
    Setting::choice(
        "new-tabs-open",
        "Tabs",
        "New tabs and splits open",
        &["Shell", "Default agent"],
        Look::Segmented,
        |s| s.agents.tabs_open_agent as usize,
        |s, i| s.agents.tabs_open_agent = i == 1,
    ),
    Setting::choice(
        "add-to-chat",
        "Composer",
        "\u{201c}Add to chat\u{201d}",
        &["Send now", "Paste only"],
        Look::Segmented,
        |s| (!s.agents.send_now) as usize,
        |s, i| s.agents.send_now = i == 0,
    )
    .hint("Paste only lets you edit before sending"),
    Setting::choice(
        "add-to-chat-to",
        "Composer",
        "Send \u{201c}Add to chat\u{201d} to",
        &["Focused session", "Most recent agent", "Ask each time"],
        Look::Dropdown,
        |s| chat_to_index(s.agents.chat_to()),
        |s, i| s.agents.chat_to = CHAT_TO[i].into(),
    )
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

/// What a provider's row says under its name: how Anywhere reads its status and which statuses that gives, as "Hooks ·
/// Needs you, Done, Failed". An agent that never asks has no Needs you, and says why. `found` is where it was found.
pub(super) fn status_line(source: &str, never_asks: bool, found: Option<&str>) -> String {
    let [needs, done, failed] = [Status::NeedsYou, Status::Done, Status::Failed].map(Status::label);
    let reports = if never_asks { format!("{done}, {failed} \u{b7} always full access") } else { format!("{needs}, {done}, {failed}") };
    let found = found.filter(|p| !p.is_empty());
    std::iter::once(source.to_string()).chain([reports]).chain(found.map(String::from)).collect::<Vec<_>>().join(" \u{b7} ")
}

/// How Anywhere reads a provider's status, for its page's Status row.
pub(super) fn status_source(provider: &str) -> String {
    let how = providers::known(provider).status_via;
    let [needs, done, failed] = [Status::NeedsYou, Status::Done, Status::Failed].map(Status::label);
    format!("Built in \u{b7} {how} for {needs}, {done} and {failed}")
}

/// What the list says of a provider pocketd didn't find, under its name.
const NOT_FOUND: &str = "Not found on your login PATH";

/// The listed providers (by index into `LaunchPick::PROVIDERS`, with whether pocketd found each), split into those shown as
/// rows and those folded under "N not installed".
fn split_installed(listed: &[(usize, bool)]) -> (Vec<usize>, Vec<usize>) {
    let (found, missing): (Vec<_>, Vec<_>) = listed.iter().copied().partition(|(_, installed)| *installed);
    (found.into_iter().map(|(i, _)| i).collect(), missing.into_iter().map(|(i, _)| i).collect())
}

fn not_installed_label(n: usize) -> String {
    format!("{n} not installed")
}

/// Whether the switch can turn `provider` off: the last agent on offer stays on.
fn can_disable(prefs: &store::prefs::agents::Agents, provider: &str, live: &::agents::Agents) -> bool {
    prefs.disabled.contains(provider) || prefs.offered(live) != [provider]
}

/// Makes `provider` the default for new sessions, turning it on if it's off.
pub(super) fn make_default(prefs: &mut store::prefs::agents::Agents, provider: &str) {
    prefs.default = provider.into();
    prefs.enable(provider);
}

/// The command that installs a provider this build knows one for.
fn install_command(provider: &str) -> Option<&'static str> {
    providers::get(provider).map(|b| b.install.command).filter(|c| !c.is_empty())
}

/// What Settings remembers of the Agents page itself.
#[derive(Default)]
pub(crate) struct View {
    /// The "N not installed" rows are showing.
    unfolded: bool,
}

impl Settings {
    /// Shows the providers pocketd didn't find, as their fold does.
    pub(crate) fn unfold_providers(&mut self, cx: &mut Context<Self>) {
        self.agents_view.unfolded = true;
        cx.notify();
    }
}

fn chip(label: &'static str) -> Div {
    div().px(px(5.)).rounded(px(4.)).bg(FILL_3).text_size(px(10.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child(label)
}

fn tile(provider: &str) -> Div {
    div().size(px(28.)).flex().flex_none().items_center().justify_center().rounded(px(7.)).bg(FILL_2).child(provider_icon(provider, 15., TEXT))
}

impl Settings {
    /// The providers as rows: an installed one opens its page from its name or chevron, and has a switch and a ⋯ menu; the
    /// ones pocketd didn't find fold under their count.
    fn providers_row(&mut self, cx: &mut Context<Self>) -> Div {
        let listed = listed_providers(&self.live);
        let listed: Vec<(usize, bool)> =
            LaunchPick::PROVIDERS.into_iter().enumerate().filter(|(_, p)| listed.contains(p)).map(|(i, p)| (i, self.installed(p))).collect();
        let (found, missing) = split_installed(&listed);
        let default = self.store.agents.default_agent(&self.live);
        let rows: Vec<Div> = found.iter().enumerate().map(|(row, i)| self.provider_row(*i, row > 0, default, cx)).collect();
        let fold = (!missing.is_empty()).then(|| {
            let open = self.agents_view.unfolded;
            let toggle = div()
                .id("providers-fold")
                .px(px(14.))
                .py(px(10.))
                .flex()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .tab_index(0)
                .hover(|d| d.bg(FILL_1))
                .text_size(px(12.5))
                .text_color(TEXT_2)
                .child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_4))
                .child(not_installed_label(missing.len()))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.agents_view.unfolded = !this.agents_view.unfolded;
                    cx.notify();
                }));
            div().when(!rows.is_empty(), |d| d.border_t(px(0.5)).border_color(SEPARATOR)).child(focus_ring(toggle))
        });
        let missing_rows: Vec<Div> = if self.agents_view.unfolded { missing.iter().map(|i| self.missing_row(*i, cx)).collect() } else { Vec::new() };
        div().flex().flex_col().children(rows).children(fold).children(missing_rows)
    }

    fn provider_row(&mut self, i: usize, divider: bool, default: &str, cx: &mut Context<Self>) -> Div {
        let p = LaunchPick::PROVIDERS[i];
        let on = !self.store.agents.disabled.contains(p);
        let line = status_line(providers::known(p).status_source, self.live.provider_info(p).never_asks(), None);
        let name = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(TEXT)
            .child(self.agent_name(p))
            .when(p == default, |d| d.child(chip("Default")))
            .when(!on, |d| d.child(chip("Off")));
        let text = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(name)
            .child(div().truncate().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(line));
        let open = cx.listener(move |this, _: &ClickEvent, window, cx| this.open_provider(i, window, cx));
        let name_area = focus_ring(
            div()
                .id(("provider-open", i))
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(12.))
                .cursor_pointer()
                .tab_index(0)
                .when(!on, |d| d.opacity(0.6))
                .child(tile(p))
                .child(text),
        )
        .on_click(open);
        let switch = div()
            .id(("provider-enabled", i))
            .flex_none()
            .cursor_pointer()
            .tab_index(0)
            .child(ui::toggle(("provider-switch", i), on))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.write(None, cx, move |s, live| toggle_enabled(&mut s.agents, p, live))));
        let chevron = div()
            .id(("provider-chevron", i))
            .flex_none()
            .cursor_pointer()
            .child(icon("chevron-right", 13., TEXT_4))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_provider(i, window, cx)));
        div()
            .px(px(14.))
            .py(px(9.))
            .flex()
            .items_center()
            .gap(px(12.))
            .hover(|d| d.bg(FILL_1))
            .when(divider, |d| d.border_t(px(0.5)).border_color(SEPARATOR))
            .child(name_area)
            .child(ring_around(switch, 9.))
            .child(self.provider_more(i, p == default, on, cx))
            .child(chevron)
    }

    /// The ⋯ button and, while it's open, its menu: Make default, Disable or Enable, then the provider's page.
    fn provider_more(&mut self, i: usize, is_default: bool, on: bool, cx: &mut Context<Self>) -> Div {
        let id = provider::MENU_IDS[i];
        let open = self.menu == Some(id);
        let button = focus_ring(ui::icon_button_sized(("provider-more", i), "more", 26., TEXT_3).tab_index(0)).when(open, |d| d.bg(FILL_3));
        let trigger = dropdown::trigger(button, id, open, cx);
        let menu = open.then(|| self.provider_menu(i, is_default, on, cx));
        div().relative().flex_none().child(trigger).children(menu.map(|m| ui::dropdown_right(30., m)))
    }

    fn provider_menu(&mut self, i: usize, is_default: bool, on: bool, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = LaunchPick::PROVIDERS[i];
        let id = provider::MENU_IDS[i];
        let off_row = |row: Stateful<Div>, enabled: bool| row.when(!enabled, |d| d.opacity(0.45).cursor_default());
        let make = off_row(ui::menu_row(("provider-menu-default", i), "star", "Make default", None), !is_default).when(!is_default, |d| {
            d.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.menu = None;
                this.write(None, cx, move |s, _| make_default(&mut s.agents, p));
            }))
        });
        let may = can_disable(&self.store.agents, p, &self.live);
        let (label, glyph) = if on { ("Disable", "x") } else { ("Enable", "check") };
        let toggle = off_row(ui::menu_row(("provider-menu-toggle", i), glyph, label, None), may).when(may, |d| {
            d.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.menu = None;
                this.write(None, cx, move |s, live| toggle_enabled(&mut s.agents, p, live));
            }))
        });
        let why = (!may).then(|| {
            div().px(px(10.)).pb(px(6.)).max_w(px(220.)).text_size(px(11.5)).line_height(px(15.)).text_color(TEXT_4).child("Anywhere keeps one agent on.")
        });
        let page =
            ui::menu_row(("provider-menu-page", i), "sliders", "Settings\u{2026}", None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.menu = None;
                this.open_provider(i, window, cx);
            }));
        let menu =
            ui::pop(div()).min_w(px(200.)).p(px(4.)).flex().flex_col().occlude().child(make).child(toggle).children(why).child(ui::menu_divider()).child(page);
        ui::menu_in(SharedString::from(format!("{id}-menu-in")), dropdown::shuts(menu, id, cx))
    }

    /// A provider pocketd didn't find: where to set its command, and how to install it when this build knows how.
    fn missing_row(&mut self, i: usize, cx: &mut Context<Self>) -> Div {
        let p = LaunchPick::PROVIDERS[i];
        let text = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(self.agent_name(p)))
            .child(div().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(NOT_FOUND));
        let set_command = super::action(("provider-set-command", i), "Set command\u{2026}")
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_provider_command(i, window, cx)));
        let install = install_command(p).map(|command| {
            let name = self.agent_name(p);
            super::action(("provider-install", i), "Install\u{2026}").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(command.to_string()));
                this.toast(Toast::tell(format!("Copied the install command for {name}. Run it in a terminal, then restart Anywhere.")), cx);
            }))
        });
        div()
            .px(px(14.))
            .py(px(9.))
            .flex()
            .items_center()
            .gap(px(12.))
            .border_t(px(0.5))
            .border_color(SEPARATOR)
            .child(tile(p).opacity(0.5))
            .child(text)
            .child(set_command)
            .children(install)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        access_index, can_disable, install_command, listed_providers, make_default, not_installed_label, offered_provider, split_installed, status_line,
        status_source, toggle_enabled,
    };
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
        agents::Agents {
            providers: named.iter().map(|(id, available)| ProviderInfo { id: (*id).into(), available: *available, ..Default::default() }).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn settings_list_an_agent_only_when_pocketd_named_it() {
        assert_eq!(listed_providers(&agents::Agents::default()), ["claude", "codex"], "a pocketd that names none, as one older than the other agents");
        assert_eq!(listed_providers(&pocketd(&[("claude", true)])), ["claude", "codex"]);
        assert_eq!(
            listed_providers(&pocketd(&[("claude", true), ("codex", false), ("pi", false), ("hermes", true)])),
            ["claude", "codex", "pi", "hermes"],
            "found or not"
        );
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
    fn a_provider_row_says_how_its_status_is_read_and_which_it_reports() {
        assert_eq!(status_line("Hooks", false, None), "Hooks \u{b7} Needs you, Done, Failed");
        assert_eq!(status_line("Session events", false, Some("~/bin/codex")), "Session events \u{b7} Needs you, Done, Failed \u{b7} ~/bin/codex");
        assert_eq!(status_line("Chat only", true, Some("")), "Chat only \u{b7} Done, Failed \u{b7} always full access");
    }

    #[test]
    fn providers_pocketd_did_not_find_fold_under_their_count() {
        let (shown, folded) = split_installed(&[(0, true), (1, true), (3, false), (5, true), (6, false), (7, false)]);
        assert_eq!((shown, folded.len()), (vec![0, 1, 5], 3));
        assert_eq!(not_installed_label(folded.len()), "3 not installed");
        assert_eq!(split_installed(&[(0, true)]).1, Vec::<usize>::new(), "nothing to fold, so no fold row");
    }

    #[test]
    fn the_last_agent_on_offer_cannot_be_disabled_from_the_menu() {
        let mut s = Store::default();
        let live = pocketd(&[("claude", true), ("codex", true), ("pi", false)]);
        assert!(can_disable(&s.agents, "claude", &live));
        toggle_enabled(&mut s.agents, "claude", &live);
        assert!(!can_disable(&s.agents, "codex", &live), "Claude is off and Pi isn't installed");
        assert!(can_disable(&s.agents, "claude", &live), "an off agent can always be turned on");
    }

    #[test]
    fn making_an_agent_the_default_turns_it_on() {
        let mut s = Store::default();
        let live = pocketd(&[("claude", true), ("codex", true)]);
        s.agents.disable("codex", &live);
        make_default(&mut s.agents, "codex");
        assert_eq!((s.agents.default_agent(&live), s.agents.disabled.contains("codex")), ("codex", false));
    }

    #[test]
    fn only_a_provider_this_build_knows_an_install_for_offers_install() {
        assert_eq!(install_command("claude"), None);
        assert!(install_command("grok").is_some());
        assert_eq!(install_command("nobody"), None);
    }

    #[test]
    fn the_permission_row_starts_on_the_agent_s_own_settings() {
        assert_eq!((access_index("settings"), access_index("full")), (0, 4));
    }
}
