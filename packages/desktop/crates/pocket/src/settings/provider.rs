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
use store::prefs::agents::{Agents, Args};
use store::{LaunchPick, Store};
use theme::*;

const DEFAULTS: &str = "Defaults";
const LAUNCH: &str = "Launch";
const STATUS: &str = "Status";
/// The Enabled card, drawn above the groups.
const ENABLED: &str = "Enabled";

pub(super) const CLAUDE_ROWS: &[Setting] = &rows::<0>();
pub(super) const CODEX_ROWS: &[Setting] = &rows::<1>();

/// Codex takes no effort from pocketd, so its page has no Effort row.
const fn rows<const P: usize>() -> [Setting; 9] {
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
        Setting::text(["claude-label", "codex-label"][P], DEFAULTS, "Label", ["Claude Code", "Codex"][P], |s| s.agents.provider(LaunchPick::PROVIDERS[P]).label, |s, v| {
            s.agents.provider_mut(LaunchPick::PROVIDERS[P]).label = v
        })
        .hint("Replaces the built-in name everywhere in the app"),
        Setting::custom("model", DEFAULTS, "Model", |d, s, cx| d.model_row(P, s, cx))
            .resets(|s| model_label(&s.agents.provider(LaunchPick::PROVIDERS[P]).model), |s, _| s.agents.provider_mut(LaunchPick::PROVIDERS[P]).model.clear())
            .hint("The list comes from the CLI; Agent default uses its own setting"),
        effort.under(|_| P == 0),
        Setting::custom("command", LAUNCH, "Command", |d, s, _| d.launch_row(P, Launch::Command, s))
            .resets(|s| command_label(&s.agents.provider(LaunchPick::PROVIDERS[P]).command), |s, _| s.agents.provider_mut(LaunchPick::PROVIDERS[P]).command.clear())
            .effect(|d, cx| d.send_launch(P, Launch::Command, cx))
            .hint("Binary name or path. Found on PATH by default"),
        args_row::<P, 0>().hint("Added only when launching with a prompt, e.g. --, --prompt, -i"),
        args_row::<P, 1>().hint("Used to restore a previous session; its id goes after these, e.g. --resume. Leave empty if the agent can't resume by id"),
        args_row::<P, 2>().hint("Used to clone a previous session without changing it. Put {sessionId} where its id belongs, or leave empty if the agent can't fork"),
        Setting::custom("status", STATUS, "How Anywhere reads status", |_, s, _| row(s.label, Some(&status_source(LaunchPick::PROVIDERS[P])), div())),
    ]
}

/// The `A`th of `Args::ALL` for the `P`th provider, sent to pocketd like Command.
const fn args_row<const P: usize, const A: usize>() -> Setting {
    let (id, label) = [("prompt-args", "Prompt-only args"), ("resume-args", "Resume args"), ("fork-args", "Fork args")][A];
    Setting::custom(id, LAUNCH, label, |d, s, _| d.launch_row(P, Launch::Args(Args::ALL[A]), s))
        .resets(|s| args_label(&s.agents.args(LaunchPick::PROVIDERS[P], Args::ALL[A])), |s, _| *s.agents.provider_mut(LaunchPick::PROVIDERS[P]).args_mut(Args::ALL[A]) = None)
        .effect(|d, cx| d.send_launch(P, Launch::Args(Args::ALL[A]), cx))
}

fn args_label(args: &str) -> String {
    if args.is_empty() { "None".into() } else { format!("\u{201c}{args}\u{201d}") }
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

/// A Launch field: what pocketd runs, or the args it puts around it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Launch {
    Command,
    Args(Args),
}

impl Launch {
    const ALL: [Self; 4] = [Self::Command, Self::Args(Args::Prompt), Self::Args(Args::Resume), Self::Args(Args::Fork)];

    fn index(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }

    /// pocketd's config key.
    fn key(self, provider: &str) -> String {
        match self {
            Self::Command => format!("{provider}.command"),
            Self::Args(a) => format!("{provider}.{}", a.key()),
        }
    }

    fn text(self, agents: &Agents, provider: &str) -> String {
        match self {
            Self::Command => agents.provider(provider).command,
            Self::Args(a) => agents.args(provider, a),
        }
    }

    /// Args equal to pocketd's default are stored as that default, so Reset has nothing to undo.
    fn set(self, agents: &mut Agents, provider: &str, text: String) {
        let own = agents.provider_mut(provider);
        match self {
            Self::Command => own.command = text,
            Self::Args(a) => *own.args_mut(a) = (text != a.default_text(provider)).then_some(text),
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Args(Args::Prompt) => "prompt-only args",
            Self::Args(Args::Resume) => "resume args",
            Self::Args(Args::Fork) => "fork args",
        }
    }

    fn placeholder(self, provider: &'static str) -> &'static str {
        match self {
            Self::Command => provider,
            Self::Args(Args::Prompt) => "Nothing before the prompt",
            Self::Args(_) => "Not supported",
        }
    }
}

/// Each provider's Launch fields, committed on Enter or leaving one.
pub(crate) struct LaunchFields([[Entity<InputState>; 4]; 2]);

impl LaunchFields {
    pub fn new(store: &Store, window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let mut subs = vec![];
        let fields = LaunchPick::PROVIDERS.map(|p| {
            let i = usize::from(p == "codex");
            Launch::ALL.map(|f| {
                let field = cx.new(|cx| InputState::new(window, cx).placeholder(f.placeholder(p)).default_value(f.text(&store.agents, p)));
                subs.push(cx.subscribe_in(&field, window, move |this, _, ev: &InputEvent, window, cx| {
                    if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        this.commit_launch(i, f, window, cx);
                    }
                }));
                field
            })
        });
        (Self(fields), subs)
    }

    /// Puts the store's values back in the fields, as after a reset or pocketd's reply.
    pub fn sync(&self, store: &Store, window: &mut Window, cx: &mut App) {
        for (p, fields) in LaunchPick::PROVIDERS.iter().zip(&self.0) {
            for (f, field) in Launch::ALL.iter().zip(fields) {
                let text = f.text(&store.agents, p);
                if field.read(cx).value() != text {
                    field.update(cx, |field, cx| field.set_value(text, window, cx));
                }
            }
        }
    }
}

impl Desktop {
    /// Opens a provider's page with its fields showing what the store holds.
    pub(crate) fn open_provider(&mut self, provider: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.launch.sync(&self.store, window, cx);
        self.show_section(if provider == 0 { Section::Claude } else { Section::Codex }, cx);
        cx.notify();
    }

    /// The trimmed text goes to pocketd, which checks it; a refusal puts the old one back.
    fn commit_launch(&mut self, i: usize, f: Launch, window: &mut Window, cx: &mut Context<Self>) {
        let p = LaunchPick::PROVIDERS[i];
        let field = self.settings.launch.0[i][f.index()].clone();
        let text = field.read(cx).value().trim().to_string();
        let before = f.text(&self.store.agents, p);
        if text == before || self.capturing {
            return;
        }
        f.set(&mut self.store.agents, p, text.clone());
        self.save_soon(cx);
        cx.notify();
        let (key, sent) = (f.key(p), text.clone());
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move { daemon::request(&serde_json::json!({"op": "config-set", "key": key, "text": sent})) }).await;
            this.update_in(cx, |d, window, cx| {
                if let Err(e) = result {
                    d.error = Some(format!("Anywhere kept the {} {}: {e}", d.agent_name(p), f.noun()));
                    // A newer edit has its own request; putting `before` back would undo it.
                    if f.text(&d.store.agents, p) == text {
                        f.set(&mut d.store.agents, p, before);
                        field.update(cx, |field, cx| field.set_value(f.text(&d.store.agents, p), window, cx));
                    }
                }
                if p == "codex" && f == Launch::Command {
                    d.forget_models();
                }
                d.load_host(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Sends the store's value after Reset.
    fn send_launch(&mut self, i: usize, f: Launch, cx: &mut Context<Self>) {
        let p = LaunchPick::PROVIDERS[i];
        let (key, text) = (f.key(p), f.text(&self.store.agents, p));
        self.set_host(key, text, cx);
        if f == Launch::Command {
            self.forget_models();
        }
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

    /// The title and hint over a field as wide as the card, as typed args run long.
    fn launch_row(&mut self, i: usize, f: Launch, setting: &'static Setting) -> Div {
        let input = Input::new(&self.settings.launch.0[i][f.index()]).appearance(false).p_0().text_size(px(12.));
        let field = field_box().w_full().h(px(30.)).child(div().flex_1().min_w_0().font_family(MONO).child(input));
        div().flex().flex_col().child(row(setting.label, setting.hint, div()).pb(px(6.))).child(div().px(px(14.)).pb(px(12.)).child(field))
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
            .child(div().text_size(px(20.)).line_height(px(26.)).font_weight(FontWeight::BOLD).text_color(TEXT).child(self.agent_name(p)))
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
            (own.model, own.effort, own.command, own.label, own.fork_args) = ("opus".into(), "high".into(), "/opt/agent".into(), "Mine".into(), Some(String::new()));
        }
        let labels: Vec<_> = changed(CLAUDE_ROWS, &store).into_iter().map(|c| (c.label, c.from, c.to)).collect();
        assert_eq!(
            labels,
            [
                ("Enabled", "Off".into(), "On".into()),
                ("Label", "\u{201c}Mine\u{201d}".into(), "Empty".into()),
                ("Model", "Opus".into(), "Agent default".into()),
                ("Effort", "High".into(), "Agent default".into()),
                ("Command", command_label("/opt/agent"), command_label("")),
                ("Fork args", "None".into(), "\u{201c}--resume {sessionId} --fork-session\u{201d}".into()),
            ]
        );
        assert_eq!(reset(CLAUDE_ROWS, &mut store).len(), 6);
        assert!(changed(CLAUDE_ROWS, &store).is_empty());
        assert_eq!(store.agents.provider("codex").model, "opus");
        assert_eq!(changed(CODEX_ROWS, &store).len(), 4);
    }
}
