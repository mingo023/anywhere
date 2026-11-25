pub(crate) mod accounts;
mod agents;
mod appearance;
mod automations;
mod browser;
pub(crate) mod catalog;
mod diff;
pub(crate) mod dropdown;
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

pub(crate) use appearance::{reduce_motion, show_background};
pub(crate) use side::Side;

use crate::actions::OpenSettings;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, Screen, drag_area};
use ::agents::providers;
use catalog::{Control, Look, Setting};
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
        Section::Phone,
        Section::Hosts,
        Section::Projects,
        Section::Sidebar,
        Section::Terminal,
        Section::Files,
        Section::Browser,
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
            Section::Agents | Section::Accounts | Section::Automations | Section::Phone | Section::Hosts | Section::Provider(_) => "Agents",
            Section::Projects | Section::Sidebar | Section::Terminal | Section::Files | Section::Browser => "Projects",
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
            Section::Accounts => "Claude Code and Codex sign-ins on each machine, and the one new agents start on.",
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
            Section::Agents | Section::Provider(_) => "sparkle",
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
        match self {
            Section::Provider(_) => Section::Agents,
            s => s,
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
    Write { setting: Option<&'static Setting>, apply: Setter },
    Shown(Section),
    Close,
    Ask(Confirm),
    FocusRoot,
    Error(String),
    Port { port: u32, before: u32 },
    RenamePhone { id: String, name: String },
    RestartService,
    RestartToUpdate,
    CheckUpdates,
    StarOnGithub,
    FontsLoaded,
    ImportTheme,
    Launch { i: usize, f: provider::Launch, text: String, before: String },
    OpenAutomations,
    PairPhone,
    CheckGh,
    GhLogin,
    PickGhAccount { project: String, account: String },
    EditProject(String),
    AddProject,
    CloneProject,
    RemoveTheme(String),
    ChooseBackground(&'static Setting),
    Hosts(HostsAct),
    Accounts(AccountsAct),
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
    pub(crate) fields: HashMap<&'static str, Entity<InputState>>,
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
        let mut fields = HashMap::new();
        // The provider pages are reached from Agents, so ALL leaves them out.
        for setting in Section::ALL.into_iter().chain((0..providers::COUNT).map(Section::Provider)).flat_map(|s| s.rows()) {
            let Control::Text { placeholder, get, set } = setting.control else { continue };
            let value = get(store);
            let field = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value));
            subs.push(cx.subscribe_in(&field, window, move |this, field, ev: &InputEvent, _, cx| {
                if let InputEvent::Change = ev {
                    let text = field.read(cx).value().to_string();
                    this.set(setting, cx, move |s| set(s, text.clone()));
                }
            }));
            fields.insert(setting.id, field);
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
            _subs: subs,
        }
    }

    pub(crate) fn hand(&mut self, store: Option<Arc<Store>>, live: Option<Arc<::agents::Agents>>, side: Side, cx: &mut Context<Self>) {
        self.side = side;
        if store.is_none() && live.is_none() {
            return;
        }
        self.store = store.unwrap_or_else(|| self.store.clone());
        self.live = live.unwrap_or_else(|| self.live.clone());
        crate::region::notify_after_draw(cx);
    }

    pub(crate) fn holds(&self, store: &Store, live: &::agents::Agents) -> (bool, bool) {
        (store.settings_unchanged(&self.store), self.live.providers == live.providers)
    }

    pub(crate) fn write(&mut self, setting: Option<&'static Setting>, cx: &mut Context<Self>, apply: impl Fn(&mut Store, &::agents::Agents) + 'static) {
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

    pub(crate) fn sync_fields(&mut self, store: &Store, section: Section, moved: &[&'static Setting], window: &mut Window, cx: &mut Context<Self>) {
        for setting in moved {
            if let (Control::Text { get, .. }, Some(field)) = (setting.control, self.fields.get(setting.id)) {
                let value = get(store);
                field.update(cx, |f, cx| f.set_value(value, window, cx));
            }
        }
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

fn row(title: impl IntoElement, hint: Option<&str>, control: impl IntoElement) -> Div {
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
                .child(div().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(title))
                .children(hint.map(|h| div().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(h.to_string()))),
        )
        .child(div().flex_none().max_w(relative(0.6)).flex().justify_end().child(control))
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

fn action(id: impl Into<ElementId>, label: impl IntoElement) -> Stateful<Div> {
    div()
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
        .hover(|d| d.bg(FILL_3))
        .text_size(px(12.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(TEXT)
        .whitespace_nowrap()
        .child(label)
}

fn header(section: Section) -> Div {
    let tile = div()
        .size(px(40.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(10.))
        .bg(SURFACE)
        .shadow(vec![ui::ring(SEPARATOR, 0.5), ui::shadow(rgba(0x0000000d), 1., 1.5)])
        .child(icon(section.icon(), 20., TEXT));
    let text = div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(div().text_size(px(20.)).line_height(px(26.)).font_weight(FontWeight::BOLD).text_color(TEXT).child(section.label()))
        .child(div().text_size(px(12.5)).line_height(px(17.)).text_color(TEXT_2).child(section.subtitle()));
    div().mb(px(4.)).flex().items_center().gap(px(14.)).child(tile).child(text)
}

/// The box a text or folder value sits in.
fn field_box() -> Div {
    div().w(px(220.)).h(px(26.)).px(px(9.)).flex().items_center().gap(px(6.)).rounded(px(7.)).bg(FILL_2)
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
    ui::menu_in(SharedString::from(format!("{}-menu-in", setting.id)), ui::pop(div()).min_w(px(160.)).p(px(4.)).flex().flex_col().children(rows))
}

fn segments(options: &'static [&'static str]) -> Vec<Segment<usize>> {
    options.iter().enumerate().map(|(i, label)| Segment { icon: None, value: i, label: (*label).into(), badge: None }).collect()
}

/// "Agents › Claude Code" for a provider's page, else the section's name.
fn breadcrumb(section: Section) -> String {
    match section.nav() {
        nav if nav == section => section.label().into(),
        nav => format!("{} \u{203a} {}", nav.label(), section.label()),
    }
}

fn more_in_advanced(id: impl Into<ElementId>, n: usize) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(14.))
        .py(px(8.))
        .flex()
        .items_center()
        .gap(px(6.))
        .cursor_pointer()
        .hover(|d| d.bg(FILL_1))
        .text_size(px(12.))
        .text_color(TEXT_3)
        .child(format!("{n} more in Advanced"))
        .child(icon("chevron-down", 14., TEXT_3))
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

    fn control(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> AnyElement {
        match setting.control {
            Control::Switch { get, .. } => {
                let on = get(&self.store);
                div()
                    .id(setting.id)
                    .flex_none()
                    .cursor_pointer()
                    .child(ui::toggle("switch", on))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_switch(setting, !on, cx)))
                    .into_any_element()
            }
            Control::Choice { options, look: Look::Segmented, get, .. } => div()
                .id(setting.id)
                .child(ui::segmented(segments(options), get(&self.store), true, false, move |this: &mut Self, i, cx| this.choose(setting, i, cx), cx))
                .into_any_element(),
            Control::Choice { options, look: Look::Dropdown, .. } => {
                let chosen = setting.chosen(&self.store, &self.live);
                let open = self.menu == Some(setting.id);
                let button = action(setting.id, options.get(chosen).copied().unwrap_or_default())
                    .gap(px(6.))
                    .child(icon("chevron-down", 12., TEXT_3))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.menu = (!open).then_some(setting.id);
                        cx.notify();
                    }));
                div()
                    .relative()
                    .child(button)
                    .when(open, |d| d.child(ui::dropdown_right(30., dropdown_menu(setting, options, chosen, &self.store, &self.live, cx))))
                    .into_any_element()
            }
            Control::Stepper { min, max, step, unit, get, .. } => {
                let n = get(&self.store);
                let button = |id: &'static str, glyph: &'static str, by: i32, enabled: bool| {
                    div()
                        .id(SharedString::from(format!("{}-{id}", setting.id)))
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .text_size(px(14.))
                        .text_color(if enabled { TEXT_2 } else { TEXT_4 })
                        .when(enabled, |d| d.cursor_pointer().hover(|d| d.bg(FILL_3)))
                        .child(glyph)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if enabled {
                                this.set_number(setting, catalog::step(n, by, min, max), cx);
                            }
                        }))
                };
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
                            .min_w(px(56.))
                            .flex()
                            .justify_center()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(TEXT)
                            .child(format!("{n}{unit}")),
                    )
                    .child(button("more", "+", step, n < max))
                    .into_any_element()
            }
            Control::Text { .. } => match self.fields.get(setting.id) {
                Some(field) => field_box()
                    .child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(field).appearance(false).p_0().text_size(px(12.))))
                    .into_any_element(),
                None => div().into_any_element(),
            },
            Control::Path { get, .. } => {
                let path = get(&self.store);
                let custom = path != get(catalog::defaults());
                let shown = field_box()
                    .child(icon("folder", 13., TEXT_3))
                    .child(div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(12.)).text_color(TEXT).child(crate::util::tilde(&path)));
                let choose =
                    action(SharedString::from(format!("{}-choose", setting.id)), "Choose…").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.pick_paths(true, window, cx, move |this, paths, cx| {
                            if let Some(p) = paths.into_iter().next() {
                                this.set_path(setting, p.to_string_lossy().into_owned(), cx);
                            }
                        });
                    }));
                let reset = custom.then(|| {
                    ui::link(SharedString::from(format!("{}-default", setting.id)), "Use default")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_path(setting, String::new(), cx)))
                });
                div().flex().items_center().gap(px(6.)).child(shown).child(choose).children(reset).into_any_element()
            }
            Control::Custom(_) => div().into_any_element(),
        }
    }

    fn setting_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
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
        row.when(self.found == Some(setting.id), |d| d.bg(ACCENT_TINT))
    }

    /// A group's card: its rows the parent allows, Advanced ones only while open, else a line counting them.
    fn setting_group(&mut self, section: Section, title: &'static str, cx: &mut Context<Self>) -> Option<Div> {
        let open = self.advanced_open(section);
        let allowed: Vec<&'static Setting> = section.rows().iter().filter(|r| r.group == title && r.allowed(&self.store)).collect();
        let hidden = if open { 0 } else { allowed.iter().filter(|r| r.advanced).count() };
        let mut rows: Vec<Div> = allowed.into_iter().filter(|r| open || !r.advanced).map(|r| self.setting_row(r, cx)).collect();
        if rows.is_empty() {
            return None;
        }
        if hidden > 0 {
            let more = more_in_advanced(SharedString::from(format!("settings-more-{title}")), hidden)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_advanced(section, true, cx)));
            rows.push(div().child(more));
        }
        Some(group(title, rows))
    }

    /// Every group of the section's rows, in order.
    fn setting_groups(&mut self, section: Section, cx: &mut Context<Self>) -> Vec<Div> {
        catalog::groups(section.rows()).into_iter().filter_map(|g| self.setting_group(section, g, cx)).collect()
    }

    fn settings_footer(&mut self, section: Section, cx: &mut Context<Self>) -> Option<Div> {
        let rows = section.rows();
        let advanced = catalog::advanced_count(rows);
        let resettable = rows.iter().any(|r| !matches!(r.control, Control::Custom(_)) || r.resets.is_some());
        if advanced == 0 && !resettable {
            return None;
        }
        let open = self.advanced_open(section);
        let toggle = (advanced > 0).then(|| {
            div()
                .id("settings-advanced")
                .h(px(28.))
                .pl(px(8.))
                .pr(px(10.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(7.))
                .bg(FILL_2)
                .cursor_pointer()
                .hover(|d| d.bg(FILL_3))
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(TEXT_2)
                .child(icon(if open { "chevron-down" } else { "chevron-right" }, 13., TEXT_3))
                .child(if open { "Hide advanced settings" } else { "Show advanced settings" })
                .child(div().font_weight(FontWeight::NORMAL).text_color(TEXT_4).child(format!("({advanced})")))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_advanced(section, !open, cx)))
        });
        let reset = resettable.then(|| {
            div()
                .id("settings-reset")
                .h(px(28.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(7.))
                .cursor_pointer()
                .hover(|d| d.bg(FILL_2))
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(TEXT_2)
                .child(icon("discard", 13., TEXT_2))
                .child(format!("Reset {} to defaults…", section.label()))
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.emit(SettingsEvent::Ask(Confirm::ResetSection(section)));
                }))
        });
        Some(div().pt(px(6.)).px(px(2.)).flex().items_center().justify_between().gap(px(12.)).child(div().children(toggle)).children(reset))
    }

    fn settings_page(&mut self, cx: &mut Context<Self>) -> Div {
        let section = self.section;
        let words = self.query(cx);
        let (title, column) = if words.is_empty() {
            let body = match section {
                Section::Keyboard => self.keybinding_settings(cx),
                Section::Notifications => self.notification_settings(cx),
                Section::Automations => self.automations_settings(cx),
                Section::Accounts => self.accounts_settings(cx),
                Section::Provider(i) => self.provider_page(i, cx),
                _ => div().flex().flex_col().gap(px(22.)).children(self.setting_groups(section, cx)),
            };
            // Keyboard's Reset all sits above its list.
            let footer = (section != Section::Keyboard).then(|| self.settings_footer(section, cx)).flatten();
            let head = (section.nav() == section).then(|| header(section));
            (breadcrumb(section), div().children(head).child(body).children(footer))
        } else {
            ("Search results".into(), self.search_results(&words, cx))
        };
        let bar = drag_area(ui::page_bar())
            .px(px(16.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title));
        let column = column.w_full().max_w(px(704.)).px(px(32.)).pt(px(26.)).pb(px(40.)).flex().flex_col().gap(px(22.));
        ui::page(div())
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(bar)
            .child(div().id("settings-body").flex_1().min_h_0().overflow_y_scroll().child(div().w_full().flex().justify_center().child(column)))
    }
}

impl Render for Settings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.settings_page(cx)
    }
}

/// The sheet listing what Reset would put back.
fn reset_sheet(section: Section, store: &Store) -> crate::modals::confirm::Sheet {
    let changes = catalog::changed(section.rows(), store);
    let lead = match changes.len() {
        0 => "Every setting here is already at its default.".to_string(),
        1 => "Only this section changes. 1 setting you changed will go back:".to_string(),
        n => format!("Only this section changes. {n} settings you changed will go back:"),
    };
    let lines = changes.into_iter().map(|c| (c.label.to_string(), format!("{} → {}", c.from, c.to))).collect();
    crate::modals::confirm::Sheet::Reset { title: format!("Reset {} to defaults?", section.label()), lead, lines }
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

    /// Esc clears a search before it closes Settings.
    pub(crate) fn escape_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.fleet.form.open {
            return self.close_add_host(window, cx);
        }
        if self.settings.read(cx).search.read(cx).value().is_empty() {
            return self.close_settings(window, cx);
        }
        self.settings.update(cx, |s, cx| s.clear_search(window, cx));
        window.focus(&self.root, cx);
        cx.notify();
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
            SettingsEvent::Error(e) => {
                self.fail(e.clone(), cx);
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

    pub(crate) fn reset_section(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        let moved = catalog::reset(section.rows(), &mut self.store);
        for setting in &moved {
            if let Some(apply) = setting.effect {
                apply(self, cx);
            }
        }
        self.sync_settings(cx);
        let store = &self.store;
        self.settings.update(cx, |s, cx| s.sync_fields(store, section, &moved, window, cx));
        self.save_soon(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Return, Section, breadcrumb, nav_groups};
    use crate::desktop::chrome::Screen;

    #[test]
    fn sections_follow_the_spec_order() {
        assert_eq!(Section::ALL.map(Section::label)[..5], ["General", "Appearance", "Notifications & Sounds", "Keyboard", "Agents"]);
        assert_eq!(Section::ALL.len(), 16);
    }

    #[test]
    fn the_nav_groups_sections_by_what_they_set() {
        let groups: Vec<_> = nav_groups().into_iter().map(|(g, s)| (g, s.len())).collect();
        assert_eq!(groups, [("App", 4), ("Agents", 5), ("Projects", 5), ("Git", 2)]);
    }

    #[test]
    fn every_section_remembers_advanced_under_its_own_id() {
        let ids: std::collections::BTreeSet<_> = Section::ALL.map(Section::id).into();
        assert_eq!(ids.len(), Section::ALL.len());
    }

    #[test]
    fn a_provider_s_page_sits_under_agents() {
        assert_eq!((Section::Provider(0).nav(), Section::General.nav()), (Section::Agents, Section::General));
        assert_eq!((breadcrumb(Section::Provider(1)), breadcrumb(Section::Git)), ("Agents \u{203a} Codex".to_string(), "Git & GitHub".to_string()));
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
