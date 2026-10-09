use super::catalog::{Look, Setting};
use super::{action, row};
use crate::desktop::Desktop;
use crate::status::Status;
use crate::util::tilde;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::LaunchPick;
use store::prefs::agents::{ACCESSES, CHAT_TO};
use theme::*;

const NEW: &str = "New sessions";

/// A starter prompt being dragged, by its place in the list.
#[derive(Clone)]
struct DragStarter(usize);

pub(super) const ROWS: &[Setting] = &[
    Setting::custom("providers", "Providers", "Providers", |d, _, cx| d.providers_row(cx)).hint("Claude Code and Codex, found on your login PATH"),
    Setting::custom("provider-status", "Providers", "Status", |d, _, _| d.provider_status()).hint("How Anywhere reads status"),
    Setting::choice("default-agent", NEW, "Default agent", &["Claude Code", "Codex"], Look::Dropdown, default_index, |s, i| {
        s.agents.default = LaunchPick::PROVIDERS[i].into();
        s.agents.set_enabled(LaunchPick::PROVIDERS[i], true);
    })
    .hint("Starts in a project that hasn't picked one, and in new agent tabs"),
    Setting::choice("permission-mode", NEW, "Permission mode", &["Agent's settings", "Ask", "Edits", "Auto", "Full"], Look::Segmented, |s| access_index(s.agents.access()), |s, i| {
        s.agents.access = ACCESSES[i].into()
    })
    .hint("Agent's settings uses the agent's own config. Full skips every approval"),
    Setting::switch("plan-mode", NEW, "Start in plan mode", |s| s.agents.plan, |s, on| s.agents.plan = on).hint("For providers that support plan mode"),
    Setting::choice("runs-in", NEW, "Runs in", &["Current checkout", "New worktree"], Look::Segmented, |s| s.agents.new_worktree as usize, |s, i| s.agents.new_worktree = i == 1)
        .hint("A new worktree keeps the agent's edits off your branch"),
    Setting::choice("agent-exits", "Tabs", "When an agent exits", &["Return to shell", "Close tab"], Look::Segmented, |s| s.agents.close_on_exit as usize, |s, i| {
        s.agents.close_on_exit = i == 1
    }),
    Setting::choice("new-tabs-open", "Tabs", "New tabs and splits open", &["Shell", "Default agent"], Look::Segmented, |s| s.agents.tabs_open_agent as usize, |s, i| {
        s.agents.tabs_open_agent = i == 1
    }),
    Setting::custom("starter-prompts", "Composer", "Starter prompts", |d, s, cx| d.starters_row(s, cx))
        .resets(|s| format!("{} prompts", s.agents.starters.len()), |s, d| s.agents.starters = d.agents.starters.clone()).hint("Shown in the composer of an empty pane. Drag to reorder"),
    Setting::choice("add-to-chat", "Composer", "\u{201c}Add to chat\u{201d}", &["Send now", "Paste only"], Look::Segmented, |s| (!s.agents.send_now) as usize, |s, i| {
        s.agents.send_now = i == 0
    })
    .hint("Paste only lets you edit before sending"),
    Setting::choice("add-to-chat-to", "Composer", "Send \u{201c}Add to chat\u{201d} to", &["Focused session", "Most recent agent", "Ask each time"], Look::Dropdown, |s| chat_to_index(s.agents.chat_to()), |s, i| {
        s.agents.chat_to = CHAT_TO[i].into()
    })
    .advanced(),
];

/// Whether a provider picker lists the `i`th of `LaunchPick::PROVIDERS`: only enabled ones.
pub(super) fn enabled_provider(s: &store::Store, i: usize) -> bool {
    s.agents.enabled().contains(&LaunchPick::PROVIDERS[i])
}

fn default_index(s: &store::Store) -> usize {
    LaunchPick::PROVIDERS.iter().position(|p| *p == s.agents.default_agent()).unwrap_or(0)
}

fn chat_to_index(to: &str) -> usize {
    CHAT_TO.iter().position(|t| *t == to).unwrap_or(0)
}

fn access_index(access: &str) -> usize {
    ACCESSES.iter().position(|a| *a == access).unwrap_or(0)
}

/// "Built in · ~/bin/claude", or why there's no path.
fn provider_line(path: Option<&str>) -> String {
    match path {
        None => "Built in".into(),
        Some("") => "Built in · not found on your login PATH".into(),
        Some(p) => format!("Built in · {}", tilde(p)),
    }
}

/// How Anywhere learns a provider's sessions need you, are done or failed.
pub(super) fn status_source(provider: &str) -> String {
    let how = if provider == "codex" { "reads Codex session events" } else { "uses Claude Code hooks" };
    let [needs, done, failed] = [Status::NeedsYou, Status::Done, Status::Failed].map(Status::label);
    format!("Built in \u{b7} {how} for {needs}, {done} and {failed}")
}

impl Desktop {
    fn provider_status(&self) -> Div {
        let rows = LaunchPick::PROVIDERS.into_iter().map(|p| {
            div()
                .px(px(14.))
                .py(px(8.))
                .flex()
                .items_center()
                .gap(px(12.))
                .border_t(px(0.5))
                .border_color(SEPARATOR)
                .child(div().w(px(96.)).flex_none().text_size(px(13.)).text_color(TEXT).child(provider_name(p)))
                .child(div().flex_1().min_w_0().text_size(px(12.)).text_color(TEXT_3).child(status_source(p)))
        });
        let caption = div().px(px(14.)).pt(px(9.)).pb(px(7.)).text_size(px(12.)).text_color(TEXT_3).child("Status \u{b7} how Anywhere reads it");
        div().flex().flex_col().child(caption).children(rows)
    }

    fn providers_row(&mut self, cx: &mut Context<Self>) -> Div {
        let default = self.store.agents.default_agent();
        let rows = LaunchPick::PROVIDERS.into_iter().enumerate().map(|(i, p)| {
            let on = !self.store.agents.disabled.contains(p);
            let tile = div().size(px(28.)).flex().flex_none().items_center().justify_center().rounded(px(7.)).bg(FILL_2).child(provider_icon(p, 15., TEXT));
            let name = div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(TEXT)
                .child(provider_name(p))
                .when(p == default, |d| d.child(div().px(px(5.)).rounded(px(4.)).bg(FILL_3).text_size(px(10.5)).text_color(TEXT_2).child("Default")));
            let line = provider_line(self.settings.host.status.as_ref().and_then(|s| s.providers.get(p)).map(String::as_str));
            let text = div().flex_1().min_w_0().flex().flex_col().gap(px(1.)).child(name).child(div().truncate().font_family(MONO).text_size(px(11.5)).text_color(TEXT_3).child(line));
            let toggle = div().id(("provider-enabled", i)).flex_none().cursor_pointer().child(ui::toggle(on)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.store.agents.set_enabled(p, !on);
                this.save_soon(cx);
                cx.notify();
            }));
            div()
                .id(("provider", i))
                .px(px(14.))
                .py(px(9.))
                .flex()
                .items_center()
                .gap(px(12.))
                .cursor_pointer()
                .hover(|d| d.bg(FILL_1))
                .when(i > 0, |d| d.border_t(px(0.5)).border_color(SEPARATOR))
                .child(tile)
                .child(text)
                .child(toggle)
                .child(icon("chevron-right", 13., TEXT_4))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_provider(i, window, cx)))
        });
        div().flex().flex_col().children(rows)
    }

    fn starters_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let items = self.store.agents.starters.clone().into_iter().enumerate().map(|(i, text)| {
            let remove = ui::icon_button_sized(("starter-remove", i), "x", 20., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if i < this.store.agents.starters.len() {
                    this.store.agents.starters.remove(i);
                    this.save_soon(cx);
                    cx.notify();
                }
            }));
            div()
                .id(("starter", i))
                .h(px(30.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .border_b(px(0.5))
                .border_color(SEPARATOR)
                .cursor_grab()
                .on_drag(DragStarter(i), |_, _, _, cx| cx.new(|_| EmptyView))
                .drag_over::<DragStarter>(move |s, d, _, _| if d.0 == i { s } else { s.shadow(ui::drop_line(d.0 < i)) })
                .on_drop(cx.listener(move |this, d: &DragStarter, _, cx| {
                    this.store.agents.move_starter(d.0, i);
                    this.save_soon(cx);
                    cx.notify();
                }))
                .child(icon("grip", 12., TEXT_3))
                .child(div().flex_1().min_w_0().truncate().text_size(px(12.5)).text_color(TEXT).child(text))
                .child(remove)
        });
        let add = action("starter-add", "Add").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.add_starter(window, cx)));
        let input = div().h(px(32.)).px(px(10.)).flex().items_center().gap(px(8.)).child(div().flex_1().min_w_0().child(Input::new(&self.settings.host.starter).appearance(false).p_0().text_size(px(12.5)))).child(add);
        let list = div().mt(px(8.)).rounded(px(8.)).bg(FILL_1).border(px(0.5)).border_color(SEPARATOR).overflow_hidden().flex().flex_col().children(items).child(input);
        let head = row(setting.label, setting.hint, div());
        div().flex().flex_col().child(head.pb(px(4.))).child(div().px(px(14.)).pb(px(12.)).child(list))
    }

    pub(super) fn add_starter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.settings.host.starter.read(cx).value().to_string();
        if self.store.agents.add_starter(&text) {
            self.settings.host.starter.update(cx, |f, cx| f.set_value("", window, cx));
            self.save_soon(cx);
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{access_index, enabled_provider, provider_line, status_source};
    use store::Store;

    #[test]
    fn a_provider_picker_lists_only_enabled_providers() {
        let mut s = Store::default();
        assert_eq!([enabled_provider(&s, 0), enabled_provider(&s, 1)], [true, true]);
        s.agents.set_enabled("codex", false);
        assert_eq!([enabled_provider(&s, 0), enabled_provider(&s, 1)], [true, false]);
    }

    #[test]
    fn each_provider_says_how_its_status_is_read() {
        assert_eq!(status_source("claude"), "Built in \u{b7} uses Claude Code hooks for Needs you, Done and Failed");
        assert_eq!(status_source("codex"), "Built in \u{b7} reads Codex session events for Needs you, Done and Failed");
    }

    #[test]
    fn a_provider_says_where_it_was_found_or_that_it_was_not() {
        assert_eq!(provider_line(None), "Built in");
        assert_eq!(provider_line(Some("")), "Built in · not found on your login PATH");
        assert_eq!(provider_line(Some("/opt/homebrew/bin/claude")), "Built in · /opt/homebrew/bin/claude");
    }

    #[test]
    fn the_permission_row_starts_on_the_agent_s_own_settings() {
        assert_eq!((access_index("settings"), access_index("full")), (0, 4));
    }
}
