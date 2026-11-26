pub(crate) mod accounts;
mod agents;
mod appearance;
mod automations;
mod browser;
pub(crate) mod catalog;
mod diff;
pub(crate) mod dropdown;
pub(crate) mod fields;
mod files;
mod general;
mod git;
mod host;
mod keybindings;
pub(crate) mod models;
mod nav;
mod notifications;
mod phone;
mod projects;
pub(crate) mod provider;
mod search;
mod side;
mod sidebar;
pub(crate) mod ssh_hosts;
mod terminal;
mod themes;
pub(crate) mod toasts;

pub(crate) use appearance::{reduce_motion, show_background};
pub(crate) use side::Side;

use crate::actions::OpenSettings;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, Screen, drag_area};
use ::agents::providers;
use catalog::{Control, Look, Setting};
use fields::{Bounds, Commit, TextField};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::slider::SliderState;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use store::Store;
use theme::*;
use toasts::{Saves, Toast, Toasts, Undo};
use ui::Segment;

pub(crate) const CONTEXT: &str = "Settings";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Section {
    General,
    Appearance,
    Notifications,
    Keyboard,
    Agents,
    Accounts,
    Automations,
    Phone,
    Hosts,
    Projects,
    Sidebar,
    Terminal,
    Files,
    Browser,
    Git,
    Diff,
    /// The page of `providers::ALL[i]`, opened from Agents.
    Provider(usize),
}

impl Section {
    pub const ALL: [Section; 16] = [
        Section::General,
        Section::Appearance,
        Section::Notifications,
        Section::Keyboard,
        Section::Agents,
        Section::Accounts,
        Section::Automations,
        Section::Hosts,
        Section::Phone,
        Section::Projects,
        Section::Terminal,
        Section::Files,
        Section::Browser,
        Section::Sidebar,
        Section::Git,
        Section::Diff,
    ];

    fn label(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Appearance => "Appearance",
            Section::Notifications => "Notifications & Sounds",
            Section::Keyboard => "Keyboard",
            Section::Agents => "Agents",
            Section::Accounts => "Accounts",
            Section::Automations => "Automations",
            Section::Phone => "Phone & Remote",
            Section::Hosts => "Hosts",
            Section::Projects => "Projects",
            Section::Sidebar => "Sidebar & Inbox",
            Section::Terminal => "Terminal",
            Section::Files => "Files & Editor",
            Section::Browser => "Browser",
            Section::Git => "Git & GitHub",
            Section::Diff => "Diff & Graph",
            Section::Provider(i) => PROVIDER_NAMES[i],
        }
    }

    /// Where Advanced is remembered, in `desktop.json`.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Section::General => "general",
            Section::Appearance => "appearance",
            Section::Notifications => "notifications",
            Section::Keyboard => "keyboard",
            Section::Agents => "agents",
            Section::Accounts => "accounts",
            Section::Automations => "automations",
            Section::Phone => "phone",
            Section::Hosts => "hosts",
            Section::Projects => "projects",
            Section::Sidebar => "sidebar",
            Section::Terminal => "terminal",
            Section::Files => "files",
            Section::Browser => "browser",
            Section::Git => "git",
            Section::Diff => "diff",
            Section::Provider(i) => providers::IDS[i],
        }
    }

    fn group(self) -> &'static str {
        match self {
            Section::General | Section::Appearance | Section::Notifications | Section::Keyboard => "App",
            Section::Agents | Section::Accounts | Section::Automations | Section::Provider(_) => "Agents",
            Section::Hosts | Section::Phone => "Machines",
            Section::Projects | Section::Terminal | Section::Files | Section::Browser | Section::Sidebar => "Projects",
            Section::Git | Section::Diff => "Git",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Section::General => "Updates, startup, quitting and how hard the app works in the background.",
            Section::Appearance => "Theme, color and type for the app. The terminal has its own font and colors.",
            Section::Notifications => "How agents reach you, by urgency: Needs you, Failed, Done.",
            Section::Keyboard => "Every shortcut, grouped by what it does. Click one to change it.",
            Section::Agents => "The agent providers you use, and what a new session starts with.",
            Section::Accounts => "Agent sign-ins on each machine, and the one new agents start on.",
            Section::Automations => "Defaults for new automations, and what happens to runs the Mac missed.",
            Section::Phone => "Pair the iPhone app and decide how much it can approve, and from where.",
            Section::Hosts => "Machines your agents run on: this Mac, and any machine you can sign in to over SSH. Each one is a space in the sidebar.",
            Section::Projects => "Where code lives on disk, per-project setup, and what happens when a worktree is deleted.",
            Section::Sidebar => "How sessions are listed in the sidebar, the right panel and the Inbox.",
            Section::Terminal => "The terminal that every shell and agent runs in.",
            Section::Files => "Tabs, the built-in editor and what the Explorer shows.",
            Section::Browser => "The in-app browser for docs, pull requests and your dev servers.",
            Section::Git => "Committing, pushing, AI commit messages and pull requests.",
            Section::Diff => "How changes and history are shown in Changes and the commit graph.",
            Section::Provider(_) => "",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Section::General => "sliders",
            Section::Appearance => "appearance",
            Section::Notifications => "bell",
            Section::Keyboard => "keyboard",
            Section::Agents | Section::Provider(_) => "bot",
            Section::Accounts => "shield",
            Section::Automations => "clock",
            Section::Phone => "phone",
            Section::Hosts => "server",
            Section::Projects => "folder",
            Section::Sidebar => "sidebar",
            Section::Terminal => "terminal",
            Section::Files => "file",
            Section::Browser => "globe",
            Section::Git => "branch",
            Section::Diff => "diff-split",
        }
    }

    pub(crate) fn rows(self) -> &'static [Setting] {
        match self {
            Section::General => general::ROWS,
            Section::Appearance => appearance::ROWS,
            Section::Notifications => notifications::ROWS,
            Section::Keyboard => keybindings::ROWS,
            Section::Agents => agents::ROWS,
            Section::Accounts => accounts::ROWS,
            Section::Automations => automations::ROWS,
            Section::Phone => phone::ROWS,
            Section::Hosts => ssh_hosts::ROWS,
            Section::Projects => projects::ROWS,
            Section::Sidebar => sidebar::ROWS,
            Section::Terminal => terminal::ROWS,
            Section::Files => files::ROWS,
            Section::Browser => browser::ROWS,
            Section::Git => git::ROWS,
            Section::Diff => diff::ROWS,
            Section::Provider(i) => provider::PAGES[i],
        }
    }

    /// The nav item that stays selected; a provider's page sits under Agents.
    fn nav(self) -> Section {
        self.parent().unwrap_or(self)
    }

    /// The page one level up, which the breadcrumb links and ⌘[ opens.
    pub(crate) fn parent(self) -> Option<Section> {
        match self {
            Section::Provider(_) => Some(Section::Agents),
            _ => None,
        }
    }
}

/// The screen closing Settings returns to.
#[derive(Clone, Copy)]
struct Return(Screen);

impl Default for Return {
    fn default() -> Self {
        Self(Screen::Sessions)
    }
}

impl Return {
    /// Remembers `from`; reopening from Settings keeps the first screen.
    fn open(&mut self, from: Screen) {
        if from != Screen::Settings {
            self.0 = from;
        }
    }
}

pub(crate) type Setter = Rc<dyn Fn(&mut Store, &::agents::Agents)>;

pub(crate) enum SettingsEvent {
    Write {
        setting: Option<&'static Setting>,
        apply: Setter,
    },
    Shown(Section),
    Close,
    Ask(Confirm),
    FocusRoot,
    Error(String),
    Port {
        port: u32,
        before: u32,
    },
    RenamePhone {
        id: String,
        name: String,
    },
    RestartService,
    RestartToUpdate,
    CheckUpdates,
    StarOnGithub,
    FontsLoaded,
    ImportTheme,
    Launch {
        i: usize,
        f: provider::Launch,
        text: String,
        before: String,
    },
    OpenAutomations,
    PairPhone,
    CheckGh,
    GhLogin,
    PickGhAccount {
        project: String,
        account: String,
    },
    EditProject(String),
    AddProject,
    CloneProject,
    RemoveTheme(String),
    ChooseBackground(&'static Setting),
    Hosts(HostsAct),
    Accounts(AccountsAct),
    /// Retry after `desktop.json` couldn't be written.
    SaveAgain,
    Undo(Undo),
}

pub(crate) enum HostsAct {
    Open(crate::spaces::Space),
    Install { id: String, update: bool },
    Retry(String),
    OpenPassword(String),
    ClosePassword,
    SignInWithPassword,
    OpenAdd,
    Suggest(String),
    ClearKeyFile,
    PickKeyFile,
    CloseAdd,
    Add,
    DismissInstall(String),
}

pub(crate) enum AccountsAct {
    Reload,
    SignIn(crate::accounts::Key),
    SetAuto { host: crate::accounts::Host, on: bool },
    Menu(Option<crate::accounts::Key>),
    Op { host: crate::accounts::Host, msg: serde_json::Value },
    Rename { key: crate::accounts::Key, name: String },
    Cancel { host: crate::accounts::Host, provider: String },
    ToggleOutput { host: crate::accounts::Host, provider: String },
}

/// The section on show, the search, the row a result pointed at, and whether Keyboard's fixed keys are unfolded.
pub struct Settings {
    section: Section,
    back: Return,
    pub fixed_open: bool,
    pub(crate) search: Entity<InputState>,
    /// The row a search result opened, tinted until the section changes.
    pub(crate) found: Option<&'static str>,
    /// Every text row's field, by row id.
    pub(crate) fields: HashMap<&'static str, TextField>,
    /// Every stepper's typed value, by row id.
    numbers: HashMap<&'static str, TextField>,
    /// The dropdown row whose menu is open.
    menu: Option<&'static str>,
    /// The project whose GitHub account menu is open, while `menu` is `git::ACCOUNT_MENU`.
    pub(crate) account_menu: Option<String>,
    pub(crate) host: host::Host,
    pub(crate) banners_denied: bool,
    /// Each project's dev server field, by project path.
    dev_urls: HashMap<String, (Entity<InputState>, Subscription)>,
    pub(crate) keyboard: keybindings::Keyboard,
    /// Installed font families, empty until they load.
    pub(crate) fonts: Vec<String>,
    /// The terminal colour being edited.
    swatch: Option<terminal::Swatch>,
    /// The open model or effort menu's rows, and where it's scrolled to.
    pub(crate) menu_rows: crate::pick_menu::Memo<Vec<dropdown::Row>>,
    pub(crate) menu_scroll: UniformListScrollHandle,
    launch: provider::LaunchFields,
    /// When Start service was clicked.
    pub(crate) starting: Option<std::time::Instant>,
    pub(crate) glass: Entity<SliderState>,
    pub(crate) store: Arc<Store>,
    pub(crate) live: Arc<::agents::Agents>,
    pub(crate) side: Side,
    pub(crate) toasts: Toasts,
    pub(crate) saves: Saves,
    /// How many rows differ from their default, per `Section::ALL`; counted when the store changes, not per frame.
    changed: Vec<usize>,
    /// The store changed since the fields last followed it.
    resync: bool,
    /// Each nav item's focus, per `Section::ALL`, so arrows can move it.
    nav_focus: Vec<FocusHandle>,
    /// Each row control's focus, by row id, so a search result can focus it.
    focus: HashMap<&'static str, FocusHandle>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<SettingsEvent> for Settings {}

impl Settings {
    pub fn new(store: &Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let mut subs = vec![cx.subscribe_in(&search, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::PressEnter { .. } => this.open_first_result(window, cx),
            InputEvent::Change => cx.notify(),
            _ => {}
        })];
        let (mut fields, mut numbers) = (HashMap::new(), HashMap::new());
        // The provider pages are reached from Agents, so ALL leaves them out.
        for setting in Section::ALL.into_iter().chain((0..providers::COUNT).map(Section::Provider)).flat_map(|s| s.rows()) {
            let (value, placeholder) = match setting.control {
                Control::Text { placeholder, get, .. } => (get(store), placeholder),
                Control::Stepper { get, .. } => (get(store).to_string(), ""),
                _ => continue,
            };
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value.clone()));
            subs.push(cx.subscribe_in(&input, window, move |this, _, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    this.commit_field(setting, window, cx);
                }
            }));
            let field = TextField::new(input, value);
            match setting.control {
                Control::Stepper { .. } => numbers.insert(setting.id, field),
                _ => fields.insert(setting.id, field),
            };
        }
        let (host, host_subs) = host::Host::new(store, window, cx);
        subs.extend(host_subs);
        let (keyboard, keyboard_subs) = keybindings::Keyboard::new(window, cx);
        subs.extend(keyboard_subs);
        let (launch, launch_subs) = provider::LaunchFields::new(store, window, cx);
        subs.extend(launch_subs);
        let (glass, glass_sub) = appearance::glass_slider(store, window, cx);
        subs.push(glass_sub);
        appearance::load_fonts(cx);
        Self {
            section: Section::General,
            back: Return::default(),
            fixed_open: false,
            search,
            found: None,
            fields,
            numbers,
            menu: None,
            account_menu: None,
            host,
            banners_denied: false,
            dev_urls: HashMap::new(),
            keyboard,
            fonts: Vec::new(),
            swatch: None,
            menu_rows: Default::default(),
            menu_scroll: Default::default(),
            launch,
            starting: None,
            glass,
            store: Arc::new(store.settings_view()),
            live: Arc::default(),
            side: Side::default(),
            toasts: Toasts::default(),
            saves: Saves::default(),
            changed: Section::ALL.iter().map(|s| catalog::changed_count(s.rows(), store)).collect(),
            resync: false,
            nav_focus: Section::ALL.iter().map(|_| cx.focus_handle().tab_stop(true)).collect(),
            focus: HashMap::new(),
            _subs: subs,
        }
    }

    pub(crate) fn hand(&mut self, store: Option<Arc<Store>>, live: Option<Arc<::agents::Agents>>, side: Side, cx: &mut Context<Self>) {
        self.side = side;
        if store.is_none() && live.is_none() {
            return;
        }
        if let Some(store) = store {
            self.changed = Section::ALL.iter().map(|s| catalog::changed_count(s.rows(), &store)).collect();
            (self.store, self.resync) = (store, true);
        }
        self.live = live.unwrap_or_else(|| self.live.clone());
        crate::region::notify_after_draw(cx);
    }

    pub(crate) fn holds(&self, store: &Store, live: &::agents::Agents) -> (bool, bool) {
        (store.settings_unchanged(&self.store), self.live.providers == live.providers)
    }

    pub(crate) fn write(&mut self, setting: Option<&'static Setting>, cx: &mut Context<Self>, apply: impl Fn(&mut Store, &::agents::Agents) + 'static) {
        if let Some(setting) = setting {
            self.saves.wrote(setting);
            self.toasts.forget(setting.id);
        }
        cx.emit(SettingsEvent::Write { setting, apply: Rc::new(apply) });
    }

    pub(crate) fn set(&mut self, setting: &'static Setting, cx: &mut Context<Self>, apply: impl Fn(&mut Store) + 'static) {
        self.write(Some(setting), cx, move |s, _| apply(s));
    }

    /// Back to General with nothing searched, as at launch.
    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        (self.section, self.back, self.fixed_open, self.found) = (Section::General, Return::default(), false, None);
        self.keyboard.stop();
        self.search.update(cx, |s, cx| s.set_value("", window, cx));
    }

    pub(crate) fn query(&self, cx: &App) -> Vec<String> {
        catalog::words(&self.search.read(cx).value())
    }

    pub fn show(&mut self, section: Section) {
        (self.section, self.found, self.menu, self.swatch) = (section, None, None, None);
        self.keyboard.stop();
    }

    pub(crate) fn show_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.show(section);
        cx.emit(SettingsEvent::Shown(section));
        cx.notify();
    }

    pub(crate) fn section(&self) -> Section {
        self.section
    }

    pub(crate) fn set_section(&mut self, section: Section, cx: &mut Context<Self>) {
        if self.section != section {
            self.section = section;
            cx.notify();
        }
    }

    pub(crate) fn set_menu(&mut self, menu: Option<&'static str>, cx: &mut Context<Self>) {
        if self.menu != menu {
            self.menu = menu;
            cx.notify();
        }
    }

    pub(crate) fn sync_fields(&mut self, store: &Store, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        if section == Section::Browser {
            self.load_dev_urls(window, cx);
        }
        let time = store.automations.time.clone();
        if self.host.time.read(cx).value() != time {
            self.host.time.update(cx, |f, cx| f.set_value(time, window, cx));
        }
        self.launch.sync(store, window, cx);
        let glass = store.appearance.glass() as f32;
        self.glass.update(cx, |s, cx| s.set_value(glass, window, cx));
        let port = host::port_field(store.phone.port);
        if self.host.port.read(cx).value() != port {
            self.host.port.update(cx, |f, cx| f.set_value(port, window, cx));
        }
    }

    /// Commits a text row's draft, or a stepper's typed value, on Enter or blur.
    fn commit_field(&mut self, setting: &'static Setting, window: &mut Window, cx: &mut Context<Self>) {
        match setting.control {
            Control::Text { get, set, .. } => {
                let (stored, valid) = (get(&self.store), setting.valid.unwrap_or(fields::any));
                let Some(field) = self.fields.get_mut(setting.id) else { return };
                if let Commit::Save(text) = field.commit(&stored, valid, cx) {
                    self.set(setting, cx, move |s| set(s, text.clone()));
                    self.tick_saved(cx);
                }
            }
            Control::Stepper { min, max, step, get, set, .. } => {
                let Some(field) = self.numbers.get_mut(setting.id) else { return };
                let current = get(&self.store);
                let typed = field.input.read(cx).value().to_string();
                let bounds = Bounds { min: min.into(), max: max.into(), step: step.into() };
                let n = fields::typed_step(&typed, current.into(), bounds) as i32;
                // Written back, since the typed text may have been clamped or rounded.
                field.draft = fields::Draft::new(n.to_string());
                field.input.update(cx, |f, cx| f.set_value(n.to_string(), window, cx));
                if n != current {
                    self.set(setting, cx, move |s| set(s, n));
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// Redraws once the "Saved" tick's time is up, so it goes.
    fn tick_saved(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async |this, cx| {
            cx.background_executor().timer(fields::SAVED_FOR).await;
            this.update(cx, |_, cx| cx.notify()).ok();
        })
        .detach();
    }

    /// Every text field and typed stepper follows the store, except one being typed in or holding an invalid draft. The one way
    /// a store change reaches the fields, whatever made it: a stepper, a reset, Undo, Retry or another device.
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.resync = false;
        let store = self.store.clone();
        for (id, field) in self.fields.iter_mut().chain(self.numbers.iter_mut()) {
            let stored = match catalog::find(id).map(|s| s.control) {
                Some(Control::Text { get, .. }) => get(&store),
                Some(Control::Stepper { get, .. }) => get(&store).to_string(),
                _ => continue,
            };
            field.sync(&stored, window, cx);
        }
    }

    /// The focus a row's control takes, made the first time it's asked for.
    pub(crate) fn control_focus(&mut self, id: &'static str, cx: &mut Context<Self>) -> FocusHandle {
        self.focus.entry(id).or_insert_with(|| cx.focus_handle().tab_stop(true)).clone()
    }

    /// Moves focus to the control of row `id`, as a search result opening it does.
    #[expect(dead_code, reason = "the Search slice focuses the row a result opens")]
    pub(crate) fn focus_row(&mut self, id: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        match self.fields.get(id).or(self.numbers.get(id)) {
            Some(field) => field.input.update(cx, |f, cx| f.focus(window, cx)),
            None => window.focus(&self.control_focus(id, cx), cx),
        }
    }

    /// How many rows of `section` differ from their default.
    pub(crate) fn changed_in(&self, section: Section) -> usize {
        match Section::ALL.iter().position(|s| *s == section) {
            Some(i) => self.changed.get(i).copied().unwrap_or(0),
            None => catalog::changed_count(section.rows(), &self.store),
        }
    }

    /// Opens the page one level up; false on a page with none.
    pub(crate) fn up(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(parent) = self.section.parent() else { return false };
        self.show_section(parent, cx);
        true
    }

    /// Puts one row back to its default, with a toast naming the value.
    fn reset_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) {
        if let Some(field) = self.fields.get_mut(setting.id) {
            field.draft.error = None;
        }
        let to = setting.default_value().unwrap_or_default();
        self.write(Some(setting), cx, move |s, _| setting.reset_to_default(s));
        self.toast(Toast::tell(format!("{} reset to {to}", setting.label)), cx);
    }

    /// The menu, then a recording, then the search close before Settings does.
    pub(crate) fn innermost(&self, cx: &App) -> Layer {
        innermost(self.menu.is_some() || self.swatch.is_some(), self.keyboard.recording.is_some(), !self.search.read(cx).value().is_empty())
    }

    pub(crate) fn installed(&self, provider: &str) -> bool {
        self.live.provider_info(provider).available
    }

    pub(crate) fn agent_name(&self, provider: &str) -> String {
        self.store.agents.label(provider).unwrap_or_else(|| theme::provider_name(provider).into())
    }

    pub(crate) fn models_of(&self, provider: &str) -> &[::agents::ListedModel] {
        self.side.models.listed(provider)
    }

    pub(crate) fn offered_models(&self, provider: &str) -> &[::agents::ListedModel] {
        self.side.models.offered(provider, self.installed(provider))
    }

    pub(crate) fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |s, cx| s.set_value("", window, cx));
    }

    pub(crate) fn back_to(&self) -> Screen {
        self.back.0
    }

    pub(crate) fn opened_from(&mut self, from: Screen) {
        self.back.open(from);
    }
}

/// What Esc closes first, innermost out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Layer {
    Menu,
    Recording,
    Search,
    Settings,
}

fn innermost(menu: bool, recording: bool, searching: bool) -> Layer {
    match () {
        _ if menu => Layer::Menu,
        _ if recording => Layer::Recording,
        _ if searching => Layer::Search,
        _ => Layer::Settings,
    }
}

fn nav_groups() -> Vec<(&'static str, Vec<Section>)> {
    let mut groups: Vec<(&'static str, Vec<Section>)> = Vec::new();
    for s in Section::ALL {
        match groups.last_mut() {
            Some((g, list)) if *g == s.group() => list.push(s),
            _ => groups.push((s.group(), vec![s])),
        }
    }
    groups
}

/// What `setting_row` adds to the row a page draws: tags after the title, a hint in place of its own, a reset button before
/// the control, and whether the control is off. It travels through a thread-local so that every page's `row(…)` call,
/// custom rows included, carries it without each draw passing it along.
#[derive(Default)]
struct Decor {
    tags: Vec<AnyElement>,
    hint: Option<AnyElement>,
    reset: Option<AnyElement>,
    off: bool,
}

thread_local! {
    static DECOR: std::cell::RefCell<Option<Decor>> = const { std::cell::RefCell::new(None) };
}

fn row(title: impl IntoElement, hint: Option<&str>, control: impl IntoElement) -> Div {
    let decor = DECOR.with_borrow_mut(Option::take).unwrap_or_default();
    let hint = decor.hint.or_else(|| hint.map(|h| h.to_string().into_any_element()));
    let control = if decor.off {
        // Dimmed, with a cover that takes its presses, since a control can't be told it's off.
        div().relative().opacity(0.45).child(control).child(div().absolute().inset_0().occlude()).into_any_element()
    } else {
        control.into_any_element()
    };
    div()
        .px(px(14.))
        .py(px(10.))
        .flex()
        .items_center()
        .gap(px(16.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(7.))
                        .text_size(px(13.))
                        .line_height(px(18.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(TEXT)
                        .child(title)
                        .children(decor.tags),
                )
                .children(hint.map(|h| div().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(h))),
        )
        .child(div().flex_none().max_w(relative(0.6)).flex().items_center().justify_end().gap(px(8.)).children(decor.reset).child(control))
}

/// The ring a focused control shows when focus came from the keyboard.
/// Inset, since a drop shadow fills the whole box under a control without a background of its own.
pub(crate) fn focus_ring<E: InteractiveElement>(e: E) -> E {
    e.focus_visible(|s| s.shadow(vec![ring()]))
}

fn ring() -> BoxShadow {
    ui::ring(ACCENT_GLOW, 2.).inset()
}

/// `focus_ring` around a control its child fills edge to edge: the 2 px the ring takes come out of the margin, so the
/// ring lands just outside the child and nothing moves.
fn ring_around<E: InteractiveElement + Styled>(e: E, radius: f32) -> E {
    focus_ring(e.p(px(2.)).m(px(-2.)).rounded(px(radius + 2.)))
}

/// The title of a row that belongs to the one above it.
fn sub_title(label: &'static str) -> Div {
    div().flex().items_center().gap(px(10.)).child(div().w(px(16.)).flex().child(icon("corner-down-right", 14., TEXT_4))).child(label)
}

fn group(title: &str, rows: Vec<Div>) -> Div {
    div()
        .flex()
        .flex_col()
        .child(
            div().mb(px(7.)).ml(px(2.)).text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child(title.to_string()),
        )
        .child(card(rows))
}

fn card(rows: Vec<Div>) -> Div {
    let rows = rows.into_iter().enumerate().map(|(i, r)| r.when(i > 0, |r| r.border_t(px(0.5)).border_color(SEPARATOR)));
    div()
        .flex()
        .flex_col()
        .rounded(px(12.))
        .bg(SURFACE)
        .shadow(vec![ui::ring(SEPARATOR, 0.5), ui::shadow(rgba(0x00000009), 1., 2.)])
        .overflow_hidden()
        .children(rows)
}

/// A button; it takes focus, and Space or Enter presses it.
fn action(id: impl Into<ElementId>, label: impl IntoElement) -> Stateful<Div> {
    let button = div()
        .id(id)
        .h(px(26.))
        .px(px(10.))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(7.))
        .bg(SURFACE)
        .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5), ui::shadow(rgba(0x0000000d), 1., 1.5)])
        .cursor_pointer()
        .tab_index(0)
        .hover(|d| d.bg(FILL_3))
        .text_size(px(12.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(TEXT)
        .whitespace_nowrap()
        .child(label);
    focus_ring(button)
}

/// A link inside a hint: a fix for a row that can't apply, or Retry.
fn hint_link(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    ring_around(ui::link(id, label).text_size(px(12.)).tab_index(0), 3.)
}

/// A small grey tag after a row's title: "Advanced".
fn tag(text: &'static str) -> Div {
    div()
        .px(px(6.))
        .py(px(1.))
        .rounded(px(5.))
        .bg(FILL_3)
        .text_size(px(10.5))
        .line_height(px(14.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(TEXT_3)
        .child(text)
}

/// The box a text or folder value sits in.
fn field_box() -> Div {
    div().w(px(220.)).h(px(26.)).px(px(9.)).flex().items_center().gap(px(6.)).rounded(px(7.)).bg(FILL_2)
}

/// One choice of a few, side by side: a single stop for Tab, where ← and → move the choice.
pub(crate) fn segmented_control(
    id: impl Into<ElementId>,
    labels: Vec<SharedString>,
    chosen: usize,
    pick: impl Fn(&mut Settings, usize, &mut Context<Settings>) + 'static,
    cx: &mut Context<Settings>,
) -> Stateful<Div> {
    let (pick, last) = (Rc::new(pick), labels.len().saturating_sub(1));
    let items = labels.into_iter().enumerate().map(|(i, label)| Segment { icon: None, value: i, label, badge: None }).collect();
    let on = pick.clone();
    let group = div()
        .id(id)
        .tab_index(0)
        .on_key_down(cx.listener(move |this, ev: &KeyDownEvent, _, cx| {
            if let Some(i) = segment_step(&ev.keystroke.key, chosen, last) {
                pick(this, i, cx);
                cx.stop_propagation();
            }
        }))
        .child(ui::segmented(items, chosen, true, false, move |this: &mut Settings, i, cx| on(this, i, cx), cx));
    ring_around(group, 11.)
}

/// Where ← or → moves a segmented choice from `chosen`; `None` for another key or at an end.
fn segment_step(key: &str, chosen: usize, last: usize) -> Option<usize> {
    match key {
        "left" if chosen > 0 => Some(chosen - 1),
        "right" if chosen < last => Some(chosen + 1),
        _ => None,
    }
}

fn dropdown_menu(
    setting: &'static Setting,
    options: &'static [&'static str],
    chosen: usize,
    store: &Store,
    live: &::agents::Agents,
    cx: &mut Context<Settings>,
) -> impl IntoElement {
    let rows = options.iter().enumerate().filter(|(i, _)| setting.lists(store, live, *i)).map(|(i, label)| {
        dropdown::pick_row((setting.id, i), i == chosen).child(*label).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.menu = None;
            this.choose(setting, i, cx);
        }))
    });
    let menu = ui::pop(div()).min_w(px(160.)).p(px(4.)).flex().flex_col().children(rows);
    ui::menu_in(SharedString::from(format!("{}-menu-in", setting.id)), dropdown::shuts(menu, setting.id, cx))
}

impl Settings {
    pub(crate) fn pick_paths(
        &mut self,
        dirs: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, Vec<PathBuf>, &mut Context<Self>) + 'static,
    ) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: !dirs, directories: dirs, multiple: !dirs, prompt: None });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            this.update(cx, |this, cx| {
                then(this, paths, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn advanced_open(&self, section: Section) -> bool {
        self.store.advanced.contains(section.id())
    }

    fn set_advanced(&mut self, section: Section, open: bool, cx: &mut Context<Self>) {
        self.write(None, cx, move |s, _| {
            if open {
                s.advanced.insert(section.id().to_string());
            } else {
                s.advanced.remove(section.id());
            }
        });
    }

    fn set_switch(&mut self, setting: &'static Setting, on: bool, cx: &mut Context<Self>) {
        if let Control::Switch { set, .. } = setting.control {
            self.set(setting, cx, move |s| set(s, on));
        }
    }

    pub(crate) fn choose(&mut self, setting: &'static Setting, i: usize, cx: &mut Context<Self>) {
        if let Control::Choice { set, .. } = setting.control {
            self.set(setting, cx, move |s| set(s, i));
        }
    }

    fn set_number(&mut self, setting: &'static Setting, n: i32, cx: &mut Context<Self>) {
        if let Control::Stepper { set, .. } = setting.control {
            self.set(setting, cx, move |s| set(s, n));
        }
    }

    fn set_path(&mut self, setting: &'static Setting, path: String, cx: &mut Context<Self>) {
        if let Control::Path { set, .. } = setting.control {
            self.set(setting, cx, move |s| set(s, path.clone()));
        }
    }

    /// An off row's control is drawn but takes neither presses (see `row`) nor focus.
    fn control(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> AnyElement {
        let off = setting.off_in(&self.store).is_some();
        let focus = self.control_focus(setting.id, cx).tab_stop(!off);
        if let Some(field) = self.fields.get(setting.id).or(self.numbers.get(setting.id)) {
            field.input.update(cx, |input, cx| {
                input.set_disabled(off, cx);
                input.focus_handle(cx).tab_stop(!off);
            });
        }
        match setting.control {
            Control::Switch { get, .. } => {
                let on = get(&self.store);
                let switch = div()
                    .id(setting.id)
                    .track_focus(&focus)
                    .flex_none()
                    .cursor_pointer()
                    .child(ui::toggle("switch", on))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_switch(setting, !on, cx)));
                ring_around(switch, 9.).into_any_element()
            }
            Control::Choice { options, look: Look::Segmented, get, .. } => {
                let labels = options.iter().map(|o| SharedString::from(*o)).collect();
                segmented_control(setting.id, labels, get(&self.store), move |this, i, cx| this.choose(setting, i, cx), cx)
                    .track_focus(&focus)
                    .into_any_element()
            }
            Control::Choice { options, look: Look::Dropdown, .. } => {
                let chosen = setting.chosen(&self.store, &self.live);
                let open = self.menu == Some(setting.id);
                let button = action(setting.id, options.get(chosen).copied().unwrap_or_default())
                    .track_focus(&focus)
                    .gap(px(6.))
                    .when(open, |d| d.bg(FILL_3))
                    .child(icon("chevron-down", 12., TEXT_3));
                div()
                    .relative()
                    .child(dropdown::trigger(button, setting.id, open, cx))
                    .when(open, |d| d.child(ui::dropdown_right(30., dropdown_menu(setting, options, chosen, &self.store, &self.live, cx))))
                    .into_any_element()
            }
            Control::Stepper { min, max, step, unit, get, .. } => {
                let n = get(&self.store);
                let button = |id: &'static str, glyph: &'static str, by: i32, enabled: bool| {
                    let b = div()
                        .id(SharedString::from(format!("{}-{id}", setting.id)))
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .text_size(px(14.))
                        .text_color(if enabled { TEXT_2 } else { TEXT_4 })
                        .when(enabled, |d| d.cursor_pointer().hover(|d| d.bg(FILL_3)))
                        .when(enabled && !off, |d| d.tab_index(0))
                        .child(glyph)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if enabled {
                                this.set_number(setting, catalog::step(n, by, min, max), cx);
                            }
                        }));
                    focus_ring(b)
                };
                let value = self.numbers.get(setting.id).map(|f| Input::new(&f.input).appearance(false).p_0().text_size(px(12.5)));
                div()
                    .h(px(26.))
                    .px(px(2.))
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .rounded(px(7.))
                    .bg(FILL_2)
                    .child(button("less", "\u{2212}", -step, n > min))
                    .child(
                        div()
                            .w(px(56.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(TEXT)
                            .child(div().w(px(30.)).children(value))
                            .child(div().text_color(TEXT_2).child(unit.trim().to_string())),
                    )
                    .child(button("more", "+", step, n < max))
                    .into_any_element()
            }
            Control::Text { .. } => match self.fields.get(setting.id) {
                Some(field) => field_box()
                    .when(field.draft.error.is_some(), |d| d.shadow(vec![ui::ring(FAILED, 1.)]))
                    .child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(&field.input).appearance(false).p_0().text_size(px(12.))))
                    .into_any_element(),
                None => div().into_any_element(),
            },
            Control::Path { get, .. } => {
                let path = get(&self.store);
                let custom = path != get(catalog::defaults());
                let shown = field_box()
                    .child(icon("folder", 13., TEXT_3))
                    .child(div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(12.)).text_color(TEXT).child(crate::util::tilde(&path)));
                let choose = action(SharedString::from(format!("{}-choose", setting.id)), "Choose…").track_focus(&focus).on_click(cx.listener(
                    move |this, _: &ClickEvent, window, cx| {
                        this.pick_paths(true, window, cx, move |this, paths, cx| {
                            if let Some(p) = paths.into_iter().next() {
                                this.set_path(setting, p.to_string_lossy().into_owned(), cx);
                            }
                        });
                    },
                ));
                let reset = custom.then(|| {
                    hint_link(SharedString::from(format!("{}-default", setting.id)), "Use default")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_path(setting, String::new(), cx)))
                });
                div().flex().items_center().gap(px(6.)).child(shown).child(choose).children(reset).into_any_element()
            }
            Control::Custom(_) => div().into_any_element(),
        }
    }

    /// What the row adds around the page's own drawing of it: the Advanced tag and Saved tick, a hint that says why it
    /// failed, is invalid or can't apply, and a reset while it's changed.
    fn decor(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Decor {
        let mut decor = Decor::default();
        if setting.advanced {
            decor.tags.push(tag("Advanced").into_any_element());
        }
        let field = self.fields.get(setting.id);
        if field.is_some_and(|f| f.just_saved(std::time::Instant::now())) {
            let saved = div().flex().items_center().gap(px(3.)).text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(SUCCESS);
            decor.tags.push(saved.child(icon("check", 11., SUCCESS)).child("Saved").into_any_element());
        }
        let invalid = field.and_then(|f| f.draft.error.clone());
        let off = setting.off_in(&self.store);
        decor.off = off.is_some();
        decor.hint = if let Some(error) = self.saves.error(setting.id) {
            let retry = error.retry.clone();
            let link = hint_link(SharedString::from(format!("{}-retry", setting.id)), "Retry")
                .text_color(TEXT)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.retry(&retry, cx)));
            Some(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4.))
                    .text_color(FAILED_TEXT)
                    .child(format!("{}.", error.message.trim_end_matches('.')))
                    .child(link)
                    .into_any_element(),
            )
        } else if let Some(error) = invalid {
            Some(div().text_size(px(11.5)).text_color(FAILED_TEXT).child(error).into_any_element())
        } else {
            off.map(|off| {
                let fix = off.fix.map(|(label, apply)| {
                    hint_link(SharedString::from(format!("{}-fix", setting.id)), label)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.write(Some(setting), cx, move |s, _| apply(s))))
                });
                div().flex().flex_wrap().gap(px(4.)).child(off.reason).children(fix).into_any_element()
            })
        };
        if setting.is_changed(&self.store) {
            let to = setting.default_value().unwrap_or_default();
            let tip = format!("Reset to default ({to})");
            let group = SharedString::from(format!("row-{}", setting.id));
            let reset = div()
                .id(SharedString::from(format!("{}-reset", setting.id)))
                .size(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .tab_index(0)
                .opacity(0.)
                .group_hover(group, |s| s.opacity(1.))
                .hover(|s| s.bg(FILL_3))
                .focus_visible(|s| s.opacity(1.).shadow(vec![ring()]))
                .child(icon("discard", 13., TEXT_3))
                .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.reset_row(setting, cx)));
            decor.reset = Some(reset.into_any_element());
        }
        decor
    }

    fn setting_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let decor = self.decor(setting, cx);
        let changed = setting.is_changed(&self.store);
        DECOR.with_borrow_mut(|d| *d = Some(decor));
        let row = match setting.control {
            Control::Custom(draw) => draw(self, setting, cx),
            _ => {
                let control = self.control(setting, cx);
                let title: AnyElement = match setting.parent {
                    Some(_) => sub_title(setting.label).into_any_element(),
                    None => setting.label.into_any_element(),
                };
                let note = setting.note.and_then(|note| note(&self.store));
                row(title, note.as_deref().or(setting.hint), control)
            }
        };
        // A custom draw that drew no `row` leaves its decor unused.
        DECOR.with_borrow_mut(|d| *d = None);
        row.relative()
            .group(SharedString::from(format!("row-{}", setting.id)))
            .when(changed, |d| d.child(div().absolute().left(px(5.)).top_0().bottom_0().flex().items_center().child(ui::dot(5., TEXT))))
            .when(self.found == Some(setting.id), |d| d.bg(ACCENT_TINT))
    }

    /// A group's card: its rows the parent allows, Advanced ones only while the page shows them.
    fn setting_group(&mut self, section: Section, title: &'static str, cx: &mut Context<Self>) -> Option<Div> {
        let open = self.advanced_open(section);
        let shown: Vec<&'static Setting> = section.rows().iter().filter(|r| r.group == title && r.allowed(&self.store) && (open || !r.advanced)).collect();
        let rows: Vec<Div> = shown.into_iter().map(|r| self.setting_row(r, cx)).collect();
        (!rows.is_empty()).then(|| group(title, rows))
    }

    /// Every group of the section's rows, in order.
    fn setting_groups(&mut self, section: Section, cx: &mut Context<Self>) -> Vec<Div> {
        catalog::groups(section.rows()).into_iter().filter_map(|g| self.setting_group(section, g, cx)).collect()
    }

    /// The line under the toolbar's title.
    fn subtitle(&self, section: Section) -> Option<SharedString> {
        match section {
            Section::Provider(_) => None,
            s => Some(s.subtitle().into()),
        }
    }

    /// Buttons a page puts in the toolbar, before Advanced.
    #[expect(clippy::match_single_binding, reason = "each page that has toolbar buttons adds its arm")]
    fn page_actions(&mut self, section: Section, _cx: &mut Context<Self>) -> Vec<AnyElement> {
        match section {
            _ => Vec::new(),
        }
    }

    /// "Advanced 4" and its switch, for a page that has Advanced rows.
    fn advanced_toggle(&mut self, section: Section, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let n = catalog::advanced_count(section.rows());
        if n == 0 {
            return None;
        }
        let open = self.advanced_open(section);
        let toggle = div()
            .id("settings-advanced")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .tab_index(0)
            .text_size(px(12.))
            .text_color(TEXT_2)
            .child("Show advanced")
            .child(div().text_color(TEXT_4).child(n.to_string()))
            .child(ui::toggle("settings-advanced-switch", open))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_advanced(section, !open, cx)));
        Some(ring_around(toggle, 9.))
    }

    /// The breadcrumb, with the page one level up as a link, then the page's subtitle on one line.
    fn crumbs(&mut self, section: Section, cx: &mut Context<Self>) -> Div {
        let up = section.parent().map(|parent| {
            let link = div()
                .id("settings-crumb-up")
                .px(px(4.))
                .py(px(2.))
                .rounded(px(5.))
                .cursor_pointer()
                .tab_index(0)
                .text_color(TEXT_3)
                .font_weight(FontWeight::MEDIUM)
                .hover(|d| d.bg(FILL_2).text_color(TEXT))
                .child(parent.label())
                .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(format!("Back to {} (\u{2318}[)", parent.label())).build(window, cx))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.show_section(parent, cx)));
            div().flex().items_center().gap(px(4.)).child(focus_ring(link)).child(div().text_color(TEXT_4).child("\u{203a}"))
        });
        let title =
            div().flex().items_center().gap(px(4.)).text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).children(up).child(section.label());
        let subtitle = self.subtitle(section).filter(|s| !s.is_empty()).map(|s| div().mt(px(1.)).truncate().text_size(px(12.)).text_color(TEXT_3).child(s));
        div().flex_1().min_w_0().flex().flex_col().child(title).children(subtitle)
    }

    /// "3 changes from default in General · Review & reset…", floated over the page's foot while anything differs.
    fn change_bar(&mut self, section: Section, cx: &mut Context<Self>) -> Option<Div> {
        let n = self.changed_in(section);
        if n == 0 || section == Section::Keyboard {
            return None;
        }
        let what = if n == 1 { "change" } else { "changes" };
        let review = action("settings-reset", "Review & reset\u{2026}")
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Ask(Confirm::ResetSection(section)))));
        let bar = ui::pop(div())
            .id("settings-change-bar")
            .occlude()
            .pl(px(14.))
            .pr(px(6.))
            .py(px(6.))
            .flex()
            .items_center()
            .gap(px(12.))
            .rounded(px(12.))
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .whitespace_nowrap()
            .child(div().flex().items_center().gap(px(6.)).child(ui::dot(6., TEXT)).child(format!("{n} {what} from default in {}", section.label())))
            .child(review);
        Some(div().absolute().left_0().right_0().bottom(px(16.)).flex().justify_center().child(bar))
    }

    fn settings_page(&mut self, cx: &mut Context<Self>) -> Div {
        let section = self.section;
        let words = self.query(cx);
        let searching = !words.is_empty();
        let column = if !searching {
            match section {
                Section::Keyboard => self.keybinding_settings(cx),
                Section::Notifications => self.notification_settings(cx),
                Section::Automations => self.automations_settings(cx),
                Section::Accounts => self.accounts_settings(cx),
                Section::Provider(i) => self.provider_page(i, cx),
                _ => div().flex().flex_col().gap(px(22.)).children(self.setting_groups(section, cx)),
            }
        } else {
            self.search_results(&words, cx)
        };
        let head = if searching {
            div().flex_1().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("Search results")
        } else {
            self.crumbs(section, cx)
        };
        let actions = if searching { Vec::new() } else { self.page_actions(section, cx) };
        let advanced = (!searching).then(|| self.advanced_toggle(section, cx)).flatten();
        let bar = drag_area(ui::page_bar())
            .h(px(56.))
            .px(px(22.))
            .gap(px(12.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .child(head)
            .children(actions)
            .children(advanced);
        let changes = (!searching).then(|| self.change_bar(section, cx)).flatten();
        let column = column.w_full().max_w(px(704.)).px(px(32.)).pt(px(22.)).pb(px(96.)).flex().flex_col().gap(px(22.));
        ui::page(div())
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(bar)
            .child(div().id("settings-body").flex_1().min_h_0().overflow_y_scroll().child(div().w_full().flex().justify_center().child(column)))
            .children(changes)
            .children(self.toasts_view(cx))
    }
}

impl Render for Settings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.resync {
            self.sync_inputs(window, cx);
        }
        self.settings_page(cx)
    }
}

/// The sheet listing what Reset would put back.
fn reset_sheet(section: Section, store: &Store) -> crate::modals::confirm::Sheet {
    let changes = catalog::changed(section.rows(), store);
    let lead = reset_lead(changes.len());
    let lines = changes.into_iter().map(|c| (c.label.to_string(), c.from, c.to)).collect();
    crate::modals::confirm::Sheet::Reset { title: format!("Reset {} to defaults?", section.label()), lead, lines }
}

fn reset_lead(n: usize) -> String {
    match n {
        0 => "Every setting here is already at its default.".to_string(),
        1 => "1 setting goes back. Everything else here is already at its default.".to_string(),
        n => format!("{n} settings go back. Everything else here is already at its default."),
    }
}

impl Desktop {
    pub fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus(cx);
        if self.overlay.is_some() {
            self.close_overlay(window, cx);
        }
        let from = self.screen;
        self.settings.update(cx, |s, _| s.opened_from(from));
        self.screen = Screen::Settings;
        window.focus(&self.root, cx);
        self.sync_settings(cx);
        self.load_host(cx);
        self.settings.update(cx, |s, cx| {
            s.load_dev_urls(window, cx);
            s.load_banner_permission(cx);
        });
        let section = self.settings.read(cx).section();
        self.load_section(section, cx);
        cx.notify();
    }

    pub(crate) fn show_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.settings.update(cx, |s, cx| s.show_section(section, cx));
    }

    pub(crate) fn shortcut_record_hook(cx: &mut Context<Self>) -> Subscription {
        let desktop = cx.entity().downgrade();
        // Ahead of the keymap, so ⌘K becomes the new keys instead of opening the palette.
        cx.intercept_keystrokes(move |ev, _, cx| {
            let taken = desktop.update(cx, |d, cx| {
                let shown = d.screen == Screen::Settings;
                d.settings.update(cx, |s, cx| s.record_shortcut(&ev.keystroke, shown, cx))
            });
            if taken.unwrap_or(false) {
                cx.stop_propagation();
            }
        })
    }

    pub(crate) fn sync_settings(&mut self, cx: &mut Context<Self>) {
        let (store_held, live_held) = self.settings.read(cx).holds(&self.store, &self.agents);
        let store = (!store_held).then(|| Arc::new(self.store.settings_view()));
        let live = (!live_held).then(|| Arc::new(::agents::Agents { providers: self.agents.providers.clone(), ..Default::default() }));
        let side = self.settings_side(cx);
        self.settings.update(cx, |s, cx| s.hand(store, live, side, cx));
    }

    /// What the shown section reads from outside the app, fetched once.
    fn load_section(&mut self, section: Section, cx: &mut Context<Self>) {
        if section == Section::Git && self.prs.gh.is_none() {
            self.check_gh(cx);
        }
        self.models.retry();
        match section {
            // Agents counts every installed agent's models.
            Section::Agents => self.load_models(cx),
            Section::Git => self.load_models_of(&self.store.git.agent.clone(), cx),
            Section::Provider(i) => self.load_models_of(providers::IDS[i], cx),
            _ => {}
        }
        if section == Section::Accounts {
            self.refresh_accounts(cx);
        }
    }

    pub(crate) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = self.settings.read(cx).back_to();
        if self.screen == Screen::Inbox {
            let focus = self.inbox.read(cx).focus.clone();
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// Esc closes the innermost layer: a menu, then a recording, then the search, then Settings.
    pub(crate) fn escape_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.fleet.form.open {
            return self.close_add_host(window, cx);
        }
        match self.settings.read(cx).innermost(cx) {
            Layer::Menu => self.settings.update(cx, |s, cx| {
                (s.menu, s.account_menu, s.swatch) = (None, None, None);
                cx.notify();
            }),
            Layer::Recording => self.settings.update(cx, |s, cx| {
                s.keyboard.stop();
                cx.notify();
            }),
            Layer::Search => {
                self.settings.update(cx, |s, cx| s.clear_search(window, cx));
                window.focus(&self.root, cx);
            }
            Layer::Settings => return self.close_settings(window, cx),
        }
        cx.notify();
    }

    pub(crate) fn settings_up(&mut self, _: &crate::actions::SettingsUp, _: &mut Window, cx: &mut Context<Self>) {
        if self.screen == Screen::Settings && !self.settings.update(cx, |s, cx| s.up(cx)) {
            cx.propagate();
        }
    }

    /// Shows `toast` inside Settings, where the main view's toasts can't be seen.
    pub(crate) fn settings_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.settings.update(cx, |s, cx| s.toast(toast, cx));
    }

    pub(crate) fn focus_settings_search(&mut self, _: &crate::actions::FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        let search = self.settings.read(cx).search.clone();
        search.update(cx, |s, cx| s.focus(window, cx));
    }

    pub(crate) fn reset_sheet(&self, section: Section) -> crate::modals::confirm::Sheet {
        reset_sheet(section, &self.store)
    }

    pub(crate) fn on_settings_event(&mut self, _: &Entity<Settings>, ev: &SettingsEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            SettingsEvent::Write { setting, apply } => {
                apply(&mut self.store, &self.agents);
                self.changed(*setting, cx);
            }
            SettingsEvent::EditProject(p) => self.edit_project(Some(p.clone()), window, cx),
            SettingsEvent::AddProject => self.add_local_project(window, cx),
            SettingsEvent::CloneProject => self.clone_project(window, cx),
            SettingsEvent::CheckGh => self.check_gh(cx),
            SettingsEvent::GhLogin => {
                let argv = ["gh", "auth", "login"].map(String::from);
                self.close_settings(window, cx);
                self.run_in_tree(workspace::Place::Pane(None), &argv, None, |cwd| daemon::agent_op(&argv, cwd), cx);
            }
            SettingsEvent::PickGhAccount { project, account } => {
                self.pick_gh_account(project, account.clone(), cx);
                cx.notify();
            }
            SettingsEvent::PairPhone => self.open(Overlay::PairPhone, window, cx),
            SettingsEvent::OpenAutomations => self.open_automations(window, cx),
            SettingsEvent::Launch { i, f, text, before } => self.set_launch(*i, *f, text.clone(), before.clone(), window, cx),
            SettingsEvent::ImportTheme => self.pick_theme_file(window, cx),
            SettingsEvent::RemoveTheme(slug) => self.remove_theme(slug, cx),
            SettingsEvent::FontsLoaded => appearance::apply_fonts(self, cx),
            SettingsEvent::ChooseBackground(setting) => appearance::choose_background(setting, cx),
            SettingsEvent::Shown(section) => self.load_section(*section, cx),
            SettingsEvent::Close => self.close_settings(window, cx),
            SettingsEvent::FocusRoot => window.focus(&self.root, cx),
            SettingsEvent::Error(e) => self.settings_toast(Toast::failed(e.clone(), None), cx),
            SettingsEvent::SaveAgain => self.save_soon(cx),
            SettingsEvent::Undo(undo) => {
                (undo.0)(self, window, cx);
                cx.notify();
            }
            SettingsEvent::Port { port, before } => self.set_port(*port, *before, window, cx),
            SettingsEvent::RenamePhone { id, name } => self.rename_phone(id.clone(), name.clone(), cx),
            SettingsEvent::RestartService => self.restart_service(cx),
            SettingsEvent::RestartToUpdate => self.restart_to_update(cx),
            SettingsEvent::CheckUpdates => self.updates.check(),
            SettingsEvent::StarOnGithub => self.star_on_github(cx),
            SettingsEvent::Ask(confirm) => {
                self.ask(confirm.clone(), cx);
                cx.notify();
            }
            SettingsEvent::Hosts(act) => self.on_hosts_act(act, window, cx),
            SettingsEvent::Accounts(act) => self.on_accounts_act(act, window, cx),
        }
    }

    fn on_hosts_act(&mut self, act: &HostsAct, window: &mut Window, cx: &mut Context<Self>) {
        match act {
            HostsAct::Open(space) => self.switch_space(space.clone(), window, cx),
            HostsAct::Install { id, update } => self.install_host(id, *update, cx),
            HostsAct::Retry(id) => self.retry_host(id),
            HostsAct::OpenPassword(id) => self.open_password(id.clone(), window, cx),
            HostsAct::ClosePassword => self.close_password(window, cx),
            HostsAct::SignInWithPassword => self.sign_in_with_password(window, cx),
            HostsAct::OpenAdd => self.open_add_host(None, window, cx),
            HostsAct::Suggest(alias) => self.suggest_host(alias.clone(), window, cx),
            HostsAct::ClearKeyFile => {
                self.fleet.form.key_file = None;
                cx.notify();
            }
            HostsAct::PickKeyFile => self.pick_key_file(window, cx),
            HostsAct::CloseAdd => self.close_add_host(window, cx),
            HostsAct::Add => self.add_host(cx),
            HostsAct::DismissInstall(id) => {
                self.fleet.installs.remove(id);
                cx.notify();
            }
        }
    }

    fn on_accounts_act(&mut self, act: &AccountsAct, window: &mut Window, cx: &mut Context<Self>) {
        match act {
            AccountsAct::Reload => self.refresh_accounts(cx),
            AccountsAct::SignIn(key) => self.sign_in_account(key.clone(), window, cx),
            AccountsAct::SetAuto { host, on } => self.set_auto_switch(host.clone(), *on, cx),
            AccountsAct::Menu(key) => {
                self.accounts.menu = key.clone();
                cx.notify();
            }
            AccountsAct::Op { host, msg } => self.account_op(host.clone(), msg.clone(), cx),
            AccountsAct::Rename { key, name } => self.start_account_rename(key.clone(), name.clone(), window, cx),
            AccountsAct::Cancel { host, provider } => self.cancel_sign_in(host, provider, cx),
            AccountsAct::ToggleOutput { host, provider } => {
                if let Some(s) = self.accounts.signins.iter_mut().find(|s| s.on(host, provider)) {
                    s.output_open = !s.output_open;
                }
                cx.notify();
            }
        }
    }

    pub(crate) fn changed(&mut self, setting: Option<&'static Setting>, cx: &mut Context<Self>) {
        if let Some(effect) = setting.and_then(|s| s.effect) {
            effect(self, cx);
        }
        self.save_soon(cx);
        cx.notify();
    }

    /// Puts the section back to its defaults, with a toast whose Undo restores each row still at its default.
    pub(crate) fn reset_section(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        let before = self.store.settings_view();
        let moved = catalog::reset(section.rows(), &mut self.store);
        self.apply_moved(section, &moved, window, cx);
        if moved.is_empty() {
            return;
        }
        let n = moved.len();
        let undo = Undo::new(move |d, window, cx| {
            let back: Vec<&'static Setting> = moved.iter().copied().filter(|r| !r.is_changed(&d.store)).collect();
            for r in &back {
                r.restore(&mut d.store, &before);
            }
            d.apply_moved(section, &back, window, cx);
        });
        let text = if n == 1 { "Reset 1 setting".to_string() } else { format!("Reset {n} settings") };
        self.settings_toast(Toast::undo(text, undo), cx);
    }

    /// Reruns the effects of the rows a reset or its Undo moved, and saves.
    fn apply_moved(&mut self, section: Section, moved: &[&'static Setting], window: &mut Window, cx: &mut Context<Self>) {
        for setting in moved {
            if let Some(apply) = setting.effect {
                apply(self, cx);
            }
        }
        self.sync_settings(cx);
        let store = &self.store;
        self.settings.update(cx, |s, cx| s.sync_fields(store, section, window, cx));
        self.save_soon(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Layer, Return, Section, innermost, nav_groups, reset_lead, segment_step};
    use crate::desktop::chrome::Screen;

    #[test]
    fn sections_follow_the_spec_order() {
        assert_eq!(Section::ALL.map(Section::label)[..5], ["General", "Appearance", "Notifications & Sounds", "Keyboard", "Agents"]);
        assert_eq!(Section::ALL.len(), 16);
    }

    #[test]
    fn the_nav_groups_sections_by_what_they_set() {
        let groups: Vec<_> = nav_groups().into_iter().map(|(g, s)| (g, s.len())).collect();
        assert_eq!(groups, [("App", 4), ("Agents", 3), ("Machines", 2), ("Projects", 5), ("Git", 2)]);
        let workspace = nav_groups().into_iter().find(|(g, _)| *g == "Projects").map(|(_, s)| s.into_iter().map(Section::label).collect::<Vec<_>>());
        assert_eq!(workspace.unwrap(), ["Projects", "Terminal", "Files & Editor", "Browser", "Sidebar & Inbox"]);
    }

    #[test]
    fn every_section_remembers_advanced_under_its_own_id() {
        let ids: std::collections::BTreeSet<_> = Section::ALL.map(Section::id).into();
        assert_eq!(ids.len(), Section::ALL.len());
    }

    #[test]
    fn a_provider_s_page_sits_one_level_under_agents() {
        assert_eq!((Section::Provider(0).nav(), Section::General.nav()), (Section::Agents, Section::General));
        assert_eq!((Section::Provider(1).parent(), Section::Agents.parent(), Section::Git.parent()), (Some(Section::Agents), None, None));
    }

    #[test]
    fn esc_closes_a_menu_then_a_recording_then_the_search_then_settings() {
        assert_eq!(innermost(true, true, true), Layer::Menu);
        assert_eq!(innermost(false, true, true), Layer::Recording);
        assert_eq!(innermost(false, false, true), Layer::Search);
        assert_eq!(innermost(false, false, false), Layer::Settings);
    }

    #[test]
    fn left_and_right_move_a_segmented_choice_and_stop_at_its_ends() {
        assert_eq!(
            [segment_step("right", 0, 2), segment_step("left", 1, 2), segment_step("left", 0, 2), segment_step("right", 2, 2)],
            [Some(1), Some(0), None, None]
        );
        assert_eq!(segment_step("space", 1, 2), None);
    }

    #[test]
    fn the_reset_sheet_counts_what_goes_back() {
        assert_eq!(reset_lead(3), "3 settings go back. Everything else here is already at its default.");
        assert_eq!(reset_lead(1), "1 setting goes back. Everything else here is already at its default.");
    }

    #[test]
    fn closing_settings_returns_to_the_screen_it_came_from() {
        let mut back = Return::default();
        assert_eq!(back.0, Screen::Sessions);
        back.open(Screen::Inbox);
        back.open(Screen::Settings);
        assert_eq!(back.0, Screen::Inbox);
        back.open(Screen::Sessions);
        assert_eq!(back.0, Screen::Sessions);
    }
}
