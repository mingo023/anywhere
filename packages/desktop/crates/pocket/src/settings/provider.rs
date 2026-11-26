use super::agents::{make_default, status_line, status_source, toggle_enabled};
use super::catalog::Setting;
use super::dropdown::dropdown;
use super::dropdown::picks;
use super::fields::{self, Commit, Draft, TextField};
use super::models::{effort_options, model_rows};
use super::{Section, Settings, SettingsEvent, action, field_box, ring_around, row};
use crate::composer::model_menu::listed_name;
use crate::desktop::Desktop;
use crate::models::{effort_label, model_or_default, not_found};
use crate::util::tilde;
use ::agents::providers;
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

/// Each provider's page, in `providers::ALL` order, and its rows' ids: a row's focus, field, hover and open menu are
/// kept by row id, so each provider needs its own. `MENU_IDS` are the ids of the ⋯ menus on the Agents list.
macro_rules! pages {
    ($($i:literal $id:literal),*) => {
        pub(super) const PAGES: [&[Setting]; providers::COUNT] = [$(&rows::<$i>()),*];
        const ROW_IDS: [[&str; 9]; providers::COUNT] = [$([
            concat!($id, "-enabled"), concat!($id, "-label"), concat!($id, "-model"), concat!($id, "-effort"), concat!($id, "-command"),
            concat!($id, "-prompt-args"), concat!($id, "-resume-args"), concat!($id, "-fork-args"), concat!($id, "-status"),
        ]),*];
        pub(super) const MENU_IDS: [&str; providers::COUNT] = [$(concat!($id, "-menu")),*];
    };
}
pages!(0 "claude", 1 "codex", 2 "cursor", 3 "grok", 4 "opencode", 5 "pi", 6 "omp", 7 "fx", 8 "hermes", 9 "antigravity");

/// The id of the `i`th provider's model menu.
pub(crate) fn model_menu(i: usize) -> &'static str {
    ROW_IDS[i][2]
}

const fn rows<const P: usize>() -> [Setting; 9] {
    [
        Setting::custom(ROW_IDS[P][0], ENABLED, "Enabled", |d, s, cx| d.enabled_row(P, s, cx)).resets(
            |s| if s.agents.disabled.contains(LaunchPick::PROVIDERS[P]) { "Off" } else { "On" }.into(),
            |s, _| s.agents.enable(LaunchPick::PROVIDERS[P]),
        ),
        Setting::text(
            ROW_IDS[P][1],
            DEFAULTS,
            "Label",
            PROVIDER_NAMES[P],
            |s| s.agents.provider(LaunchPick::PROVIDERS[P]).label,
            |s, v| s.agents.provider_mut(LaunchPick::PROVIDERS[P]).label = v,
        )
        .hint("Replaces the built-in name everywhere in the app"),
        Setting::custom(ROW_IDS[P][2], DEFAULTS, "Model", |d, s, cx| d.model_row(P, s, cx))
            .resets(
                |s| model_or_default(&s.agents.provider(LaunchPick::PROVIDERS[P]).model),
                |s, _| s.agents.provider_mut(LaunchPick::PROVIDERS[P]).model.clear(),
            )
            .hint("The list comes from the CLI"),
        Setting::custom(ROW_IDS[P][3], DEFAULTS, "Effort", |d, s, cx| d.effort_row(P, s, cx)).resets(
            |s| effort_label(&s.agents.provider(LaunchPick::PROVIDERS[P]).effort),
            |s, _| s.agents.provider_mut(LaunchPick::PROVIDERS[P]).effort.clear(),
        ),
        Setting::custom(ROW_IDS[P][4], LAUNCH, "Command", |d, s, _| d.launch_row(P, Launch::Command, s))
            .resets(
                |s| command_label(&s.agents.provider(LaunchPick::PROVIDERS[P]).command),
                |s, _| s.agents.provider_mut(LaunchPick::PROVIDERS[P]).command.clear(),
            )
            .effect(|d, cx| d.send_launch(P, Launch::Command, cx))
            .hint("Binary name or path. Found on PATH by default"),
        args_row::<P, 0>().hint("Added only when launching with a prompt, e.g. --, --prompt, -i"),
        args_row::<P, 1>().hint("Used to restore a previous session; its id goes after these, e.g. --resume. Leave empty if the agent can't resume by id"),
        args_row::<P, 2>()
            .hint("Used to clone a previous session without changing it. Put {sessionId} where its id belongs, or leave empty if the agent can't fork")
            .under(|s| !Args::Fork.default_text(LaunchPick::PROVIDERS[P]).is_empty() || s.agents.provider(LaunchPick::PROVIDERS[P]).fork_args.is_some()),
        Setting::custom(ROW_IDS[P][8], STATUS, "How Anywhere reads status", |_, s, _| row(s.label, Some(&status_source(LaunchPick::PROVIDERS[P])), div())),
    ]
}

/// The `A`th of `Args::ALL` for the `P`th provider, sent to pocketd like Command.
const fn args_row<const P: usize, const A: usize>() -> Setting {
    let label = ["Prompt-only args", "Resume args", "Fork args"][A];
    Setting::custom(ROW_IDS[P][5 + A], LAUNCH, label, |d, s, _| d.launch_row(P, Launch::Args(Args::ALL[A]), s))
        .resets(
            |s| args_label(&s.agents.args(LaunchPick::PROVIDERS[P], Args::ALL[A])),
            |s, _| *s.agents.provider_mut(LaunchPick::PROVIDERS[P]).args_mut(Args::ALL[A]) = None,
        )
        .effect(|d, cx| d.send_launch(P, Launch::Args(Args::ALL[A]), cx))
}

fn args_label(args: &str) -> String {
    if args.is_empty() { "None".into() } else { format!("\u{201c}{args}\u{201d}") }
}

fn command_label(command: &str) -> String {
    if command.is_empty() { "Found on PATH".into() } else { format!("\u{201c}{command}\u{201d}") }
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

/// Each provider's Launch fields, committed through a `Draft` on Enter or leaving one.
pub(crate) struct LaunchFields([[TextField; 4]; providers::COUNT]);

impl LaunchFields {
    pub fn new(store: &Store, window: &mut Window, cx: &mut Context<Settings>) -> (Self, Vec<Subscription>) {
        let mut subs = vec![];
        let fields = std::array::from_fn(|i| {
            let p = LaunchPick::PROVIDERS[i];
            Launch::ALL.map(|f| {
                let text = f.text(&store.agents, p);
                let input = cx.new(|cx| InputState::new(window, cx).placeholder(f.placeholder(p)).default_value(text.clone()));
                subs.push(cx.subscribe_in(&input, window, move |this, _, ev: &InputEvent, window, cx| {
                    if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        this.commit_launch(i, f, window, cx);
                    }
                }));
                TextField::new(input, text)
            })
        });
        (Self(fields), subs)
    }

    /// Puts the store's values back in the fields, as after a reset or pocketd's reply, except one being typed in.
    pub fn sync(&mut self, store: &Store, window: &mut Window, cx: &mut App) {
        for (p, fields) in LaunchPick::PROVIDERS.iter().zip(&mut self.0) {
            for (f, field) in Launch::ALL.iter().zip(fields) {
                field.sync(&f.text(&store.agents, p), window, cx);
            }
        }
    }

    fn input(&self, i: usize, f: Launch) -> Entity<InputState> {
        self.0[i][f.index()].input.clone()
    }

    /// Takes `text` as what the field holds, as after pocketd refused what was typed.
    fn reverted(&mut self, i: usize, f: Launch, text: String) {
        let field = &mut self.0[i][f.index()];
        (field.draft, field.saved) = (Draft::new(text), None);
    }
}

impl Settings {
    /// Opens a provider's page with its fields showing what the store holds.
    pub(crate) fn open_provider(&mut self, provider: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.launch.sync(&self.store, window, cx);
        self.show_section(Section::Provider(provider), cx);
        cx.notify();
    }

    /// Opens a provider's page with its Command field ready to type in, as "Set command…" does.
    pub(crate) fn open_provider_command(&mut self, provider: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.open_provider(provider, window, cx);
        self.launch.input(provider, Launch::Command).update(cx, |field, cx| field.focus(window, cx));
    }

    fn commit_launch(&mut self, i: usize, f: Launch, window: &mut Window, cx: &mut Context<Self>) {
        let stored = f.text(&self.store.agents, LaunchPick::PROVIDERS[i]);
        let input = self.launch.input(i, f);
        let (typed, trimmed) = (input.read(cx).value().to_string(), input.read(cx).value().trim().to_string());
        if typed != trimmed {
            input.update(cx, |field, cx| field.set_value(trimmed, window, cx));
        }
        if let Commit::Save(text) = self.launch.0[i][f.index()].commit(&stored, fields::any, cx) {
            cx.emit(SettingsEvent::Launch { i, f, text, before: stored });
            self.tick_saved(cx);
        }
        cx.notify();
    }

    fn launch_efforts(&self, provider: &str, model: &str) -> Vec<String> {
        let settings = self.store.agents.provider(provider).model;
        let model = if model.is_empty() { &settings } else { model };
        match self.models_of(provider).iter().find(|m| m.value == model || m.resolved == model) {
            Some(m) => m.efforts.clone(),
            None => self.live.provider_info(provider).efforts.clone(),
        }
    }
}

impl Desktop {
    pub(crate) fn set_launch(&mut self, i: usize, f: Launch, text: String, before: String, window: &mut Window, cx: &mut Context<Self>) {
        let p = LaunchPick::PROVIDERS[i];
        if self.capturing {
            return;
        }
        let field = self.settings.read(cx).launch.input(i, f);
        f.set(&mut self.store.agents, p, text.clone());
        self.save_soon(cx);
        cx.notify();
        let (key, sent) = (f.key(p), text.clone());
        cx.spawn_in(window, async move |this, cx| {
            let result =
                cx.background_executor().spawn(async move { daemon::request(&serde_json::json!({"op": "config-set", "key": key, "text": sent})) }).await;
            this.update_in(cx, |d, window, cx| {
                if let Err(e) = result {
                    d.fail(format!("Anywhere kept the {} {}: {e}", d.agent_name(p), f.noun()), cx);
                    // A newer edit has its own request; putting `before` back would undo it.
                    if f.text(&d.store.agents, p) == text {
                        f.set(&mut d.store.agents, p, before);
                        let shown = f.text(&d.store.agents, p);
                        d.settings.update(cx, |s, _| s.launch.reverted(i, f, shown.clone()));
                        field.update(cx, |field, cx| field.set_value(shown, window, cx));
                    }
                }
                if f == Launch::Command {
                    d.models.forget(p);
                }
                d.load_host(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Sends the store's value after Reset.
    pub(super) fn send_launch(&mut self, i: usize, f: Launch, cx: &mut Context<Self>) {
        let p = LaunchPick::PROVIDERS[i];
        let (key, text) = (f.key(p), f.text(&self.store.agents, p));
        self.set_host(key, text, cx);
        if f == Launch::Command {
            self.models.forget(p);
        }
    }
}

impl Settings {
    /// Whether the agent is on; one pocketd didn't find says how to install it instead.
    fn enabled_row(&mut self, i: usize, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let p = LaunchPick::PROVIDERS[i];
        if !self.installed(p) {
            return row("Not installed", Some(&not_found(p)), div());
        }
        let on = !self.store.agents.disabled.contains(p);
        let focus = self.control_focus(setting.id, cx);
        let switch = div()
            .id(setting.id)
            .track_focus(&focus)
            .flex_none()
            .cursor_pointer()
            .child(ui::toggle("switch", on))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.write(None, cx, move |s, live| toggle_enabled(&mut s.agents, p, live))));
        row(setting.label, Some("Off hides it from Default agent, New session, Automations and AI commit messages"), ring_around(switch, 9.))
    }

    fn model_row(&mut self, i: usize, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let p = LaunchPick::PROVIDERS[i];
        let model = self.store.agents.provider(p).model;
        let (id, offered) = (setting.id, self.offered_models(p));
        let trigger = action(id, "").child(listed_name(&model, offered)).gap(px(6.)).child(icon("chevron-down", 12., TEXT_3));
        let menu = dropdown(
            id,
            trigger,
            (self.side.models.revision, self.installed(p), &model),
            || model_rows(p, offered, &model),
            &model,
            self,
            move |d, v, cx| d.write(None, cx, move |s, _| s.agents.provider_mut(p).model = v.clone()),
            cx,
        );
        row(setting.label, setting.hint, menu)
    }

    fn effort_row(&mut self, i: usize, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let p = LaunchPick::PROVIDERS[i];
        let levels = self.launch_efforts(p, "");
        if levels.is_empty() {
            return div();
        }
        let effort = self.store.agents.provider(p).effort;
        let id = setting.id;
        let trigger = action(id, "").child(effort_label(&effort)).gap(px(6.)).child(icon("chevron-down", 12., TEXT_3));
        let menu = dropdown(
            id,
            trigger,
            &levels,
            || picks(effort_options(&levels, "Agent default")),
            &effort,
            self,
            move |d, v, cx| d.write(None, cx, move |s, _| s.agents.provider_mut(p).effort = v.clone()),
            cx,
        );
        row(providers::known(p).effort_word, setting.hint, menu)
    }

    /// The title and hint over a field as wide as the card, as typed args run long.
    fn launch_row(&mut self, i: usize, f: Launch, setting: &'static Setting) -> Div {
        let field = &self.launch.0[i][f.index()];
        let input = Input::new(&field.input).appearance(false).p_0().text_size(px(12.));
        let bad = field.draft.error.is_some();
        let tick = field.just_saved(std::time::Instant::now()).then(|| {
            let saved = div().flex().items_center().gap(px(3.)).text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(SUCCESS);
            saved.child(icon("check", 11., SUCCESS)).child("Saved")
        });
        let title = div().flex().items_center().gap(px(7.)).child(setting.label).children(tick);
        let field =
            field_box().w_full().h(px(30.)).when(bad, |d| d.shadow(vec![ui::ring(FAILED, 1.)])).child(div().flex_1().min_w_0().font_family(MONO).child(input));
        div().flex().flex_col().child(row(title, setting.hint, div()).pb(px(6.))).child(div().px(px(14.)).pb(px(12.)).child(field))
    }

    /// The line under the toolbar's breadcrumb: how its status is read, and where pocketd found it.
    pub(super) fn provider_subtitle(&self, i: usize) -> String {
        let p = LaunchPick::PROVIDERS[i];
        if !self.installed(p) {
            return "Not found on your login PATH".into();
        }
        let found = self.host.status.as_ref().and_then(|s| s.providers.get(p)).map(|path| tilde(path));
        status_line(providers::known(p).status_source, self.live.provider_info(p).never_asks(), found.as_deref())
    }

    /// The toolbar's Make default button, or a Default chip on the one that is.
    pub(super) fn provider_actions(&mut self, i: usize, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = LaunchPick::PROVIDERS[i];
        if !self.installed(p) {
            return Vec::new();
        }
        if self.store.agents.default_agent(&self.live) == p {
            let chip =
                div().px(px(6.)).py(px(2.)).rounded(px(5.)).bg(FILL_3).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child("Default");
            return vec![chip.into_any_element()];
        }
        let button = action("provider-set-default", "Make default")
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.write(None, cx, move |s, _| make_default(&mut s.agents, p))));
        vec![button.into_any_element()]
    }

    /// The page for one provider: its groups, with its name and status in the toolbar.
    pub(super) fn provider_page(&mut self, i: usize, cx: &mut Context<Self>) -> Div {
        let groups: Vec<Div> = [ENABLED, DEFAULTS, LAUNCH, STATUS].into_iter().filter_map(|g| self.setting_group(Section::Provider(i), g, cx)).collect();
        div().flex().flex_col().gap(px(22.)).children(groups)
    }
}

#[cfg(test)]
mod tests {
    use super::{MENU_IDS, PAGES, command_label};
    use crate::settings::catalog::{changed, reset};
    use store::Store;

    #[test]
    fn each_provider_s_rows_are_kept_under_its_own_ids() {
        for (page, provider) in PAGES.iter().zip(agents::providers::IDS) {
            assert!(page.iter().all(|row| row.id.starts_with(&format!("{provider}-"))), "{provider}");
        }
        let ids: std::collections::HashSet<_> = PAGES.iter().flat_map(|page| page.iter().map(|row| row.id)).collect();
        assert_eq!(ids.len(), PAGES.iter().map(|page| page.len()).sum::<usize>(), "no two rows share a focus or hover group");
    }

    #[test]
    fn each_provider_s_agents_list_menu_has_its_own_id() {
        for (id, provider) in MENU_IDS.iter().zip(agents::providers::IDS) {
            assert_eq!(*id, format!("{provider}-menu"));
        }
    }

    #[test]
    fn an_agent_that_cannot_fork_has_no_fork_args_row_until_some_are_set() {
        let mut store = Store::default();
        let fork = |page: usize, s: &Store| PAGES[page].iter().find(|r| r.id.ends_with("-fork-args")).unwrap().allowed(s);
        assert_eq!([0, 1, 3, 5].map(|page| fork(page, &store)), [true, true, false, true], "claude, codex, grok, pi");
        store.agents.provider_mut("grok").fork_args = Some("--fork {sessionId}".into());
        store.agents.provider_mut("claude").fork_args = Some(String::new());
        assert!(fork(3, &store) && fork(0, &store), "one that forks keeps its row when cleared");
    }

    #[test]
    fn reset_puts_one_provider_back_and_leaves_the_other() {
        let mut store = Store::default();
        store.agents.disable("claude", &agents::Agents::default());
        for p in ["claude", "codex"] {
            let own = store.agents.provider_mut(p);
            (own.model, own.effort, own.command, own.label, own.fork_args) =
                ("opus".into(), "high".into(), "/opt/agent".into(), "Mine".into(), Some(String::new()));
        }
        let labels: Vec<_> = changed(PAGES[0], &store).into_iter().map(|c| (c.label, c.from, c.to)).collect();
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
        assert_eq!(reset(PAGES[0], &mut store).len(), 6);
        assert!(changed(PAGES[0], &store).is_empty());
        assert_eq!(store.agents.provider("codex").model, "opus");
        assert_eq!(changed(PAGES[1], &store).len(), 5);
    }
}
