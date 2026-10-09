use super::agents::status_source;
use super::catalog::Setting;
use super::dropdown::dropdown;
use super::models::{effort_label, effort_options, model_label, model_options};
use super::{Section, action, card, field_box, row};
use crate::desktop::Desktop;
use crate::util::tilde;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::{LaunchPick, Store};
use theme::*;

const DEFAULTS: &str = "Defaults";
const LAUNCH: &str = "Launch";
const STATUS: &str = "Status";
/// The Enabled card, drawn above the groups.
const ENABLED: &str = "Enabled";

/// pocketd's key for each provider's command, in `LaunchPick::PROVIDERS` order.
const COMMAND_KEYS: [&str; 2] = ["claude.command", "codex.command"];

pub(super) const CLAUDE_ROWS: &[Setting] = &rows::<0>();
pub(super) const CODEX_ROWS: &[Setting] = &rows::<1>();

/// Codex takes no effort from pocketd, so its page has no Effort row.
const fn rows<const P: usize>() -> [Setting; 5] {
    let effort = if P == 0 {
        Setting::custom("effort", DEFAULTS, "Effort", |d, s, cx| d.effort_row(P, s, cx)).resets(|s| effort_label(&s.agents.provider(LaunchPick::PROVIDERS[P]).effort), |s, _| {
            s.agents.provider_mut(LaunchPick::PROVIDERS[P]).effort.clear()
        })
    } else {
        Setting::custom("effort", DEFAULTS, "Effort", |_, _, _| div())
    };
    [
        Setting::custom("enabled", ENABLED, "Enabled", |d, _, cx| d.enabled_row(P, cx))
            .resets(|s| if s.agents.disabled.contains(LaunchPick::PROVIDERS[P]) { "Off" } else { "On" }.into(), |s, _| s.agents.set_enabled(LaunchPick::PROVIDERS[P], true)),
        Setting::custom("model", DEFAULTS, "Model", |d, s, cx| d.model_row(P, s, cx))
            .resets(|s| model_label(&s.agents.provider(LaunchPick::PROVIDERS[P]).model), |s, _| s.agents.provider_mut(LaunchPick::PROVIDERS[P]).model.clear())
            .hint("The list comes from the CLI; Agent default uses its own setting"),
        effort.under(|_| P == 0),
        Setting::custom("command", LAUNCH, "Command", |d, s, cx| d.command_row(P, s, cx))
            .resets(|s| command_label(&s.agents.provider(LaunchPick::PROVIDERS[P]).command), |s, _| s.agents.provider_mut(LaunchPick::PROVIDERS[P]).command.clear())
            .effect(|d, cx| d.send_command(P, cx))
            .hint("Binary name or path. Found on PATH by default"),
        Setting::custom("status", STATUS, "How Anywhere reads status", |_, s, _| row(s.label, Some(&status_source(LaunchPick::PROVIDERS[P])), div())),
    ]
}

fn command_label(command: &str) -> String {
    if command.is_empty() { "Found on PATH".into() } else { format!("\u{201c}{command}\u{201d}") }
}

/// "Built-in provider · ~/bin/claude", or that it isn't found.
fn provider_line(path: Option<&str>) -> String {
    match path {
        None | Some("") => "Built-in provider \u{b7} not found on your login PATH".into(),
        Some(p) => format!("Built-in provider \u{b7} {}", tilde(p)),
    }
}

/// Each provider's Command field, committed on Enter or leaving it.
pub(crate) struct Commands([Entity<InputState>; 2]);

impl Commands {
    pub fn new(store: &Store, window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let fields = LaunchPick::PROVIDERS.map(|p| cx.new(|cx| InputState::new(window, cx).placeholder(p).default_value(store.agents.provider(p).command)));
        let subs = fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                cx.subscribe_in(f, window, move |this, _, ev: &InputEvent, window, cx| {
                    if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        this.commit_command(i, window, cx);
                    }
                })
            })
            .collect();
        (Self(fields), subs)
    }

    /// Puts the store's commands back in the fields, as after a reset or pocketd's reply.
    pub fn sync(&self, store: &Store, window: &mut Window, cx: &mut App) {
        for (p, f) in LaunchPick::PROVIDERS.iter().zip(&self.0) {
            let command = store.agents.provider(p).command;
            if f.read(cx).value() != command {
                f.update(cx, |f, cx| f.set_value(command, window, cx));
            }
        }
    }
}

impl Desktop {
    /// Opens a provider's page with its fields showing what the store holds.
    pub(crate) fn open_provider(&mut self, provider: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.commands.sync(&self.store, window, cx);
        self.show_section(if provider == 0 { Section::Claude } else { Section::Codex }, cx);
        cx.notify();
    }

    /// A trimmed command goes to pocketd, which checks it runs; a refusal puts the old one back.
    fn commit_command(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let p = LaunchPick::PROVIDERS[i];
        let field = self.settings.commands.0[i].clone();
        let text = field.read(cx).value().trim().to_string();
        let before = self.store.agents.provider(p).command;
        if text == before || self.capturing {
            return;
        }
        self.store.agents.provider_mut(p).command = text.clone();
        self.save_soon(cx);
        cx.notify();
        let sent = text.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move { daemon::request(&serde_json::json!({"op": "config-set", "key": COMMAND_KEYS[i], "text": sent})) }).await;
            this.update_in(cx, |d, window, cx| {
                if let Err(e) = result {
                    d.error = Some(format!("Anywhere kept the {} command: {e}", provider_name(p)));
                    // A newer edit has its own request; putting `before` back would undo it.
                    if d.store.agents.provider(p).command == text {
                        d.store.agents.provider_mut(p).command = before;
                        field.update(cx, |f, cx| f.set_value(d.store.agents.provider(p).command, window, cx));
                    }
                }
                if p == "codex" {
                    d.forget_models();
                }
                d.load_host(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Sends the store's command after Reset.
    fn send_command(&mut self, i: usize, cx: &mut Context<Self>) {
        let command = self.store.agents.provider(LaunchPick::PROVIDERS[i]).command;
        self.set_host(COMMAND_KEYS[i], command, cx);
        self.forget_models();
    }

    fn enabled_row(&mut self, i: usize, cx: &mut Context<Self>) -> Div {
        let p = LaunchPick::PROVIDERS[i];
        let on = !self.store.agents.disabled.contains(p);
        let default = self.store.agents.default_agent() == p;
        let tile = div().size(px(32.)).flex().flex_none().items_center().justify_center().rounded(px(8.)).bg(FILL_2).child(provider_icon(p, 17., TEXT));
        let badge = default.then(|| div().px(px(6.)).py(px(1.)).rounded(px(4.)).bg(FILL_3).text_size(px(11.)).text_color(TEXT_2).child("Default"));
        let set_default = (!default).then(|| {
            action("provider-set-default", "Set as default").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.store.agents.default = p.into();
                this.store.agents.set_enabled(p, true);
                this.save_soon(cx);
                cx.notify();
            }))
        });
        let toggle = div().id("provider-enabled").flex_none().cursor_pointer().child(ui::toggle(on)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.store.agents.set_enabled(p, !on);
            this.save_soon(cx);
            cx.notify();
        }));
        let text = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(ENABLED))
            .child(div().text_size(px(12.)).text_color(TEXT_3).child("Shows up in Default agent, New session, Automations and AI commit messages"));
        div().px(px(14.)).py(px(10.)).flex().items_center().gap(px(12.)).child(tile).child(text).children(badge).children(set_default).child(toggle)
    }

    fn model_row(&mut self, i: usize, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let p = LaunchPick::PROVIDERS[i];
        let model = self.store.agents.provider(p).model;
        let options = model_options(p, self.codex_models(), &model, "Agent default");
        let id = ["claude-model", "codex-model"][i];
        let trigger = action(id, "").child(model_label_of(&options, &model)).gap(px(6.)).child(icon("chevron-down", 12., TEXT_3));
        let menu = dropdown(id, trigger, options, &model, self, move |d, v, _| d.store.agents.provider_mut(p).model = v, cx);
        row(setting.label, setting.hint, menu)
    }

    fn effort_row(&mut self, i: usize, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let p = LaunchPick::PROVIDERS[i];
        let effort = self.store.agents.provider(p).effort;
        let trigger = action("claude-effort", "").child(effort_label(&effort)).gap(px(6.)).child(icon("chevron-down", 12., TEXT_3));
        let menu = dropdown("claude-effort", trigger, effort_options(p, "Agent default"), &effort, self, move |d, v, _| d.store.agents.provider_mut(p).effort = v, cx);
        row(setting.label, setting.hint, menu)
    }

    fn command_row(&mut self, i: usize, setting: &'static Setting, _: &mut Context<Self>) -> Div {
        let field = field_box().child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(&self.settings.commands.0[i]).appearance(false).p_0().text_size(px(12.))));
        row(setting.label, setting.hint, field)
    }

    /// The page for one provider: its header, the Enabled card, then its groups.
    pub(super) fn provider_page(&mut self, section: Section, cx: &mut Context<Self>) -> Div {
        let i = usize::from(section == Section::Codex);
        let p = LaunchPick::PROVIDERS[i];
        let path = self.settings.host.status.as_ref().and_then(|s| s.providers.get(p)).cloned();
        let tile = div()
            .size(px(40.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(10.))
            .bg(SURFACE)
            .shadow(vec![ui::ring(SEPARATOR, 0.5), ui::shadow(rgba(0x0000000d), 1., 1.5)])
            .child(icon("sparkle", 20., TEXT));
        let line = provider_line(path.as_deref());
        let (lead, path) = line.split_once(" \u{b7} ").map_or((line.clone(), String::new()), |(a, b)| (format!("{a} \u{b7} "), b.to_string()));
        let mono = path.starts_with('/') || path.starts_with('~');
        let text = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(div().text_size(px(20.)).line_height(px(26.)).font_weight(FontWeight::BOLD).text_color(TEXT).child(provider_name(p)))
            .child(div().flex().text_size(px(12.5)).line_height(px(17.)).text_color(TEXT_2).child(lead).child(div().when(mono, |d| d.font_family(MONO).text_size(px(12.))).child(path)));
        let header = div().mb(px(4.)).flex().items_center().gap(px(14.)).child(tile).child(text);
        let enabled = card(vec![self.enabled_row(i, cx)]);
        let groups: Vec<Div> = [DEFAULTS, LAUNCH, STATUS].into_iter().filter_map(|g| self.setting_group(section, g, cx)).collect();
        div().flex().flex_col().gap(px(22.)).child(header).child(enabled).children(groups)
    }
}

/// The label `options` give `value`.
fn model_label_of(options: &[(String, String)], value: &str) -> String {
    options.iter().find(|(v, _)| v == value).map_or_else(|| model_label(value), |(_, l)| l.clone())
}

#[cfg(test)]
mod tests {
    use super::{CLAUDE_ROWS, CODEX_ROWS, command_label, provider_line};
    use crate::settings::catalog::{changed, reset};
    use store::Store;

    #[test]
    fn a_provider_says_where_pocketd_found_it() {
        assert_eq!(provider_line(Some("/opt/homebrew/bin/claude")), "Built-in provider \u{b7} /opt/homebrew/bin/claude");
        assert_eq!(provider_line(Some("")), "Built-in provider \u{b7} not found on your login PATH");
    }

    #[test]
    fn reset_puts_one_provider_back_and_leaves_the_other() {
        let mut store = Store::default();
        store.agents.set_enabled("claude", false);
        for p in ["claude", "codex"] {
            let own = store.agents.provider_mut(p);
            (own.model, own.effort, own.command) = ("opus".into(), "high".into(), "/opt/agent".into());
        }
        let labels: Vec<_> = changed(CLAUDE_ROWS, &store).into_iter().map(|c| (c.label, c.from, c.to)).collect();
        assert_eq!(labels, [("Enabled", "Off".into(), "On".into()), ("Model", "Opus".into(), "Agent default".into()), ("Effort", "High".into(), "Agent default".into()), ("Command", command_label("/opt/agent"), command_label(""))]);
        assert_eq!(reset(CLAUDE_ROWS, &mut store).len(), 4);
        assert!(changed(CLAUDE_ROWS, &store).is_empty());
        assert_eq!(store.agents.provider("codex").model, "opus");
        assert_eq!(changed(CODEX_ROWS, &store).len(), 2);
    }
}
