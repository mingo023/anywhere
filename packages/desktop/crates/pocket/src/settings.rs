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
mod provider;
mod search;
mod sidebar;
mod terminal;

pub(crate) use appearance::{forced, reduce_motion};

use crate::actions::OpenSettings;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, Screen, drag_area};
use catalog::{Control, Look, Setting};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use std::collections::HashMap;
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
    Automations,
    Phone,
    Projects,
    Sidebar,
    Terminal,
    Files,
    Browser,
    Git,
    Diff,
    /// A provider's own page, opened from Agents.
    Claude,
    Codex,
}

impl Section {
    pub const ALL: [Section; 14] = [
        Section::General,
        Section::Appearance,
        Section::Notifications,
        Section::Keyboard,
        Section::Agents,
        Section::Automations,
        Section::Phone,
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
            Section::Automations => "Automations",
            Section::Phone => "Phone & Remote",
            Section::Projects => "Projects & Worktrees",
            Section::Sidebar => "Sidebar & Inbox",
            Section::Terminal => "Terminal",
            Section::Files => "Files & Editor",
            Section::Browser => "Browser",
            Section::Git => "Git & GitHub",
            Section::Diff => "Diff & Graph",
            Section::Claude => "Claude Code",
            Section::Codex => "Codex",
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
            Section::Automations => "automations",
            Section::Phone => "phone",
            Section::Projects => "projects",
            Section::Sidebar => "sidebar",
            Section::Terminal => "terminal",
            Section::Files => "files",
            Section::Browser => "browser",
            Section::Git => "git",
            Section::Diff => "diff",
            Section::Claude => "claude",
            Section::Codex => "codex",
        }
    }

    fn group(self) -> &'static str {
        match self {
            Section::General | Section::Appearance | Section::Notifications | Section::Keyboard => "App",
            Section::Agents | Section::Automations | Section::Phone | Section::Claude | Section::Codex => "Agents",
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
            Section::Automations => "Defaults for new automations, and what happens to runs the Mac missed.",
            Section::Phone => "Pair the iPhone app and decide how much it can approve, and from where.",
            Section::Projects => "Where code lives on disk, per-project setup, and what happens when a worktree is deleted.",
            Section::Sidebar => "How sessions are listed in the sidebar, the right panel and the Inbox.",
            Section::Terminal => "The terminal that every shell and agent runs in.",
            Section::Files => "Tabs, the built-in editor and what the Explorer shows.",
            Section::Browser => "The in-app browser for docs, pull requests and your dev servers.",
            Section::Git => "Committing, pushing, AI commit messages and pull requests.",
            Section::Diff => "How changes and history are shown in Changes and the commit graph.",
            Section::Claude | Section::Codex => "",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Section::General => "sliders",
            Section::Appearance => "appearance",
            Section::Notifications => "bell",
            Section::Keyboard => "keyboard",
            Section::Agents | Section::Claude | Section::Codex => "sparkle",
            Section::Automations => "clock",
            Section::Phone => "phone",
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
            Section::Automations => automations::ROWS,
            Section::Phone => phone::ROWS,
            Section::Projects => projects::ROWS,
            Section::Sidebar => sidebar::ROWS,
            Section::Terminal => terminal::ROWS,
            Section::Files => files::ROWS,
            Section::Browser => browser::ROWS,
            Section::Git => git::ROWS,
            Section::Diff => diff::ROWS,
            Section::Claude => provider::CLAUDE_ROWS,
            Section::Codex => provider::CODEX_ROWS,
        }
    }

    /// The nav item that stays selected; a provider's page sits under Agents.
    fn nav(self) -> Section {
        match self {
            Section::Claude | Section::Codex => Section::Agents,
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

/// The section on show, the search, the row a result pointed at, and whether Keyboard's fixed keys are unfolded.
pub struct SettingsState {
    pub section: Section,
    back: Return,
    pub fixed_open: bool,
    pub(crate) search: Entity<InputState>,
    /// The row a search result opened, tinted until the section changes.
    pub(crate) found: Option<&'static str>,
    /// Every text row's field, by row id.
    pub(crate) fields: HashMap<&'static str, Entity<InputState>>,
    /// The dropdown row whose menu is open.
    pub(crate) menu: Option<&'static str>,
    pub(crate) host: host::Host,
    pub(crate) banners_denied: bool,
    /// Each project's dev server field, by project path.
    dev_urls: HashMap<String, (Entity<InputState>, Subscription)>,
    pub(crate) keyboard: keybindings::Keyboard,
    /// Installed font families, empty until they load.
    pub(crate) fonts: Vec<String>,
    /// The terminal colour being edited.
    swatch: Option<terminal::Swatch>,
    pub(crate) models: models::Models,
    launch: provider::LaunchFields,
    /// When Start service was clicked.
    pub(crate) starting: Option<std::time::Instant>,
}

impl SettingsState {
    pub fn new(store: &Store, window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let mut subs = vec![cx.subscribe_in(&search, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::PressEnter { .. } => this.open_first_result(window, cx),
            InputEvent::Change => cx.notify(),
            _ => {}
        })];
        let mut fields = HashMap::new();
        // The provider pages are reached from Agents, so ALL leaves them out.
        for setting in Section::ALL.iter().chain(&[Section::Claude, Section::Codex]).flat_map(|s| s.rows()) {
            let Control::Text { placeholder, get, set } = setting.control else { continue };
            let value = get(store);
            let field = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value));
            subs.push(cx.subscribe_in(&field, window, move |this, field, ev: &InputEvent, _, cx| {
                if let InputEvent::Change = ev {
                    set(&mut this.store, field.read(cx).value().to_string());
                    this.changed(setting, cx);
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
        appearance::load_fonts(cx);
        let state = Self { section: Section::General, back: Return::default(), fixed_open: false, search, found: None, fields, menu: None, host, banners_denied: false, dev_urls: HashMap::new(), keyboard, fonts: Vec::new(), swatch: None, models: models::Models::default(), launch, starting: None };
        (state, subs)
    }

    /// Back to General with nothing searched, as at launch.
    pub fn reset(&mut self, window: &mut Window, cx: &mut App) {
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

fn group(title: &str, rows: Vec<Div>) -> Div {
    div()
        .flex()
        .flex_col()
        .child(div().mb(px(7.)).ml(px(2.)).text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child(title.to_string()))
        .child(card(rows))
}

fn card(rows: Vec<Div>) -> Div {
    let rows = rows.into_iter().enumerate().map(|(i, r)| r.when(i > 0, |r| r.border_t(px(0.5)).border_color(SEPARATOR)));
    div().flex().flex_col().rounded(px(12.)).bg(SURFACE).shadow(vec![ui::ring(SEPARATOR, 0.5), ui::shadow(rgba(0x00000009), 1., 2.)]).overflow_hidden().children(rows)
}

fn action(id: impl Into<ElementId>, label: &'static str) -> Stateful<Div> {
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

fn dropdown_menu(setting: &'static Setting, options: &'static [&'static str], chosen: usize, store: &Store, cx: &mut Context<Desktop>) -> impl IntoElement {
    let offered = |i: &usize| *i == chosen || setting.offers.is_none_or(|offers| offers(store, *i));
    let rows = options.iter().enumerate().filter(|(i, _)| offered(i)).map(|(i, label)| {
        div()
            .id((setting.id, i))
            .h(px(26.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(5.))
            .cursor_pointer()
            .hover(|d| d.bg(FILL_2))
            .text_size(px(13.))
            .text_color(TEXT)
            .child(div().w(px(14.)).flex_none().when(i == chosen, |d| d.child(icon("check", 13., TEXT))))
            .child(*label)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.settings.menu = None;
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

impl Desktop {
    pub fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        if self.overlay.is_some() {
            self.close_overlay(window, cx);
        }
        self.settings.back.open(self.screen);
        self.screen = Screen::Settings;
        window.focus(&self.root, cx);
        if !self.agents.phone_max.is_empty() {
            self.store.phone.max_access = self.agents.phone_max.clone();
        }
        self.load_host(cx);
        self.load_dev_urls(window, cx);
        self.load_banner_permission(cx);
        self.load_section(cx);
        cx.notify();
    }

    pub(crate) fn show_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.settings.show(section);
        self.load_section(cx);
    }

    /// What the shown section reads from outside the app, fetched once.
    fn load_section(&mut self, cx: &mut Context<Self>) {
        if self.settings.section == Section::Git && self.prs.gh.is_none() {
            self.check_gh(cx);
        }
        if matches!(self.settings.section, Section::Git | Section::Codex) {
            self.load_models(cx);
        }
    }

    pub(crate) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = self.settings.back.0;
        if self.screen == Screen::Inbox {
            window.focus(&self.inbox.focus, cx);
        }
        cx.notify();
    }

    /// Esc clears a search before it closes Settings.
    pub(crate) fn escape_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.search.read(cx).value().is_empty() {
            return self.close_settings(window, cx);
        }
        self.settings.search.update(cx, |s, cx| s.set_value("", window, cx));
        window.focus(&self.root, cx);
        cx.notify();
    }

    pub(crate) fn focus_settings_search(&mut self, _: &crate::actions::FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.search.update(cx, |s, cx| s.focus(window, cx));
    }

    fn advanced_open(&self, section: Section) -> bool {
        self.store.advanced.contains(section.id())
    }

    fn set_advanced(&mut self, section: Section, open: bool, cx: &mut Context<Self>) {
        if open {
            self.store.advanced.insert(section.id().to_string());
        } else {
            self.store.advanced.remove(section.id());
        }
        self.save_soon(cx);
        cx.notify();
    }

    fn set_switch(&mut self, setting: &'static Setting, on: bool, cx: &mut Context<Self>) {
        if let Control::Switch { set, .. } = setting.control {
            set(&mut self.store, on);
            self.changed(setting, cx);
        }
    }

    pub(super) fn choose(&mut self, setting: &'static Setting, i: usize, cx: &mut Context<Self>) {
        if let Control::Choice { set, .. } = setting.control {
            set(&mut self.store, i);
            self.changed(setting, cx);
        }
    }

    fn set_number(&mut self, setting: &'static Setting, n: i32, cx: &mut Context<Self>) {
        if let Control::Stepper { set, .. } = setting.control {
            set(&mut self.store, n);
            self.changed(setting, cx);
        }
    }

    fn set_path(&mut self, setting: &'static Setting, path: String, cx: &mut Context<Self>) {
        if let Control::Path { set, .. } = setting.control {
            set(&mut self.store, path);
            self.changed(setting, cx);
        }
    }

    fn changed(&mut self, setting: &'static Setting, cx: &mut Context<Self>) {
        if let Some(apply) = setting.effect {
            apply(self, cx);
        }
        self.save_soon(cx);
        cx.notify();
    }

    pub(crate) fn reset_section(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        for setting in catalog::reset(section.rows(), &mut self.store) {
            if let Some(apply) = setting.effect {
                apply(self, cx);
            }
            if let (Control::Text { get, .. }, Some(field)) = (setting.control, self.settings.fields.get(setting.id)) {
                let value = get(&self.store);
                field.update(cx, |f, cx| f.set_value(value, window, cx));
            }
        }
        if section == Section::Browser {
            self.load_dev_urls(window, cx);
        }
        let time = self.store.automations.time.clone();
        if self.settings.host.time.read(cx).value() != time {
            self.settings.host.time.update(cx, |f, cx| f.set_value(time, window, cx));
        }
        self.settings.launch.sync(&self.store, window, cx);
        let port = host::port_field(self.store.phone.port);
        if self.settings.host.port.read(cx).value() != port {
            self.settings.host.port.update(cx, |f, cx| f.set_value(port, window, cx));
        }
        self.save_soon(cx);
        cx.notify();
    }

    fn control(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> AnyElement {
        match setting.control {
            Control::Switch { get, .. } => {
                let on = get(&self.store);
                div().id(setting.id).flex_none().cursor_pointer().child(ui::toggle(on)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_switch(setting, !on, cx))).into_any_element()
            }
            Control::Choice { options, look: Look::Segmented, get, .. } => {
                ui::segmented(segments(options), get(&self.store), true, false, move |this: &mut Self, i, cx| this.choose(setting, i, cx), cx).into_any_element()
            }
            Control::Choice { options, look: Look::Dropdown, get, .. } => {
                let chosen = get(&self.store);
                let open = self.settings.menu == Some(setting.id);
                let button = action(setting.id, options.get(chosen).copied().unwrap_or_default())
                    .gap(px(6.))
                    .child(icon("chevron-down", 12., TEXT_3))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.settings.menu = (!open).then_some(setting.id);
                        cx.notify();
                    }));
                div().relative().child(button).when(open, |d| d.child(ui::dropdown_right(30., dropdown_menu(setting, options, chosen, &self.store, cx)))).into_any_element()
            }
            Control::Choice { look: Look::Thumbnails, get, .. } => appearance::thumbnails(setting, get(&self.store), cx).into_any_element(),
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
                    .child(div().min_w(px(56.)).flex().justify_center().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(format!("{n}{unit}")))
                    .child(button("more", "+", step, n < max))
                    .into_any_element()
            }
            Control::Text { .. } => match self.settings.fields.get(setting.id) {
                Some(field) => field_box().child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(field).appearance(false).p_0().text_size(px(12.)))).into_any_element(),
                None => div().into_any_element(),
            },
            Control::Path { get, .. } => {
                let path = get(&self.store);
                let custom = path != get(catalog::defaults());
                let shown = field_box().child(icon("folder", 13., TEXT_3)).child(div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(12.)).text_color(TEXT).child(crate::util::tilde(&path)));
                let choose = action(SharedString::from(format!("{}-choose", setting.id)), "Choose…").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.pick_path(true, window, cx, move |d, paths, _, cx| {
                        if let Some(p) = paths.into_iter().next() {
                            d.set_path(setting, p.to_string_lossy().into_owned(), cx);
                        }
                    });
                }));
                let reset = custom.then(|| ui::link(SharedString::from(format!("{}-default", setting.id)), "Use default").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_path(setting, String::new(), cx))));
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
                    Some(_) => div().flex().items_center().gap(px(10.)).child(div().w(px(16.)).text_color(TEXT_4).child("↳")).child(setting.label).into_any_element(),
                    None => setting.label.into_any_element(),
                };
                let note = setting.note.and_then(|note| note(&self.store));
                row(title, note.as_deref().or(setting.hint), control)
            }
        };
        row.when(self.settings.found == Some(setting.id), |d| d.bg(ACCENT_TINT))
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
            let more = more_in_advanced(SharedString::from(format!("settings-more-{title}")), hidden).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_advanced(section, true, cx)));
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
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    (this.confirm, this.overlay) = (Some(Confirm::ResetSection(section)), Some(Overlay::Confirm));
                    cx.notify();
                }))
        });
        Some(div().pt(px(6.)).px(px(2.)).flex().items_center().justify_between().gap(px(12.)).child(div().children(toggle)).children(reset))
    }

    /// The sheet listing what Reset would put back.
    pub(crate) fn reset_sheet(&mut self, section: Section, cx: &mut Context<Self>) -> Div {
        let changes = catalog::changed(section.rows(), &self.store);
        let lead = match changes.len() {
            0 => "Every setting here is already at its default.".to_string(),
            1 => "Only this section changes. 1 setting you changed will go back:".to_string(),
            n => format!("Only this section changes. {n} settings you changed will go back:"),
        };
        let lines = changes.into_iter().map(|c| {
            div()
                .py(px(5.))
                .flex()
                .justify_between()
                .gap(px(12.))
                .border_b(px(0.5))
                .border_color(SEPARATOR)
                .text_size(px(12.5))
                .child(div().text_color(TEXT).child(c.label))
                .child(div().flex_none().text_color(TEXT_3).child(format!("{} → {}", c.from, c.to)))
                .into_any_element()
        });
        let body = std::iter::once(div().text_size(px(12.5)).line_height(px(18.)).text_color(TEXT_2).child(lead).into_any_element()).chain(lines);
        let cancel = ui::button("confirm-cancel", ui::Variant::Secondary, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let reset = ui::button("confirm-go", ui::Variant::Primary, None, "Reset").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.confirmed(window, cx)));
        ui::dialog(&format!("Reset {} to defaults?", section.label()), body, [cancel, reset])
    }

    pub(crate) fn settings_page(&mut self, cx: &mut Context<Self>) -> Div {
        let section = self.settings.section;
        let words = self.settings.query(cx);
        let (title, column) = if words.is_empty() {
            let body = match section {
                Section::Keyboard => self.keybinding_settings(cx),
                Section::Notifications => self.notification_settings(cx),
                Section::Automations => self.automations_settings(cx),
                Section::Claude | Section::Codex => self.provider_page(section, cx),
                _ => div().flex().flex_col().gap(px(22.)).children(self.setting_groups(section, cx)),
            };
            // Keyboard's Reset all sits above its list.
            let footer = (section != Section::Keyboard).then(|| self.settings_footer(section, cx)).flatten();
            let head = (section.nav() == section).then(|| header(section));
            (breadcrumb(section), div().children(head).child(body).children(footer))
        } else {
            ("Search results".into(), self.search_results(&words, cx))
        };
        let back = ui::icon_button_sized("settings-back", "back", 26., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_settings(window, cx)));
        let bar = drag_area(ui::page_bar())
            .px(px(16.))
            .gap(px(10.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .child(back)
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

#[cfg(test)]
mod tests {
    use super::{Return, Section, breadcrumb, nav_groups};
    use crate::desktop::chrome::Screen;

    #[test]
    fn sections_follow_the_spec_order() {
        assert_eq!(Section::ALL.map(Section::label)[..5], ["General", "Appearance", "Notifications & Sounds", "Keyboard", "Agents"]);
        assert_eq!(Section::ALL.len(), 14);
    }

    #[test]
    fn the_nav_groups_sections_by_what_they_set() {
        let groups: Vec<_> = nav_groups().into_iter().map(|(g, s)| (g, s.len())).collect();
        assert_eq!(groups, [("App", 4), ("Agents", 3), ("Projects", 5), ("Git", 2)]);
    }

    #[test]
    fn every_section_remembers_advanced_under_its_own_id() {
        let ids: std::collections::BTreeSet<_> = Section::ALL.map(Section::id).into();
        assert_eq!(ids.len(), Section::ALL.len());
    }

    #[test]
    fn a_provider_s_page_sits_under_agents() {
        assert_eq!((Section::Claude.nav(), Section::General.nav()), (Section::Agents, Section::General));
        assert_eq!((breadcrumb(Section::Codex), breadcrumb(Section::Git)), ("Agents \u{203a} Codex".to_string(), "Git & GitHub".to_string()));
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
