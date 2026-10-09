use super::Menu;
use super::form::{PROVIDERS, TEMPLATES, Values};
use super::logic::{DEFAULT_EVERY, DEFAULT_TIME, Tab};
use super::parts::{card, crumbs, details, pill, section, segmented};
use crate::desktop::Desktop;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::prefs::automations::ACCESSES;
use theme::*;
use ui::{self, Variant};

pub struct Form {
    pub(super) title: Entity<InputState>,
    pub(super) prompt: Entity<TextareaState>,
    pub(super) time: Entity<InputState>,
    pub(super) every: Entity<InputState>,
    /// The choices; its texts only count as of the last `load`, see `values`.
    pub(super) v: Values,
    pub(crate) open: bool,
}

impl Form {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Untitled automation"));
        let prompt = cx.new(|cx| TextareaState::new(window, cx).placeholder("Tell the agent what to do when this automation runs…").auto_grow(5, 14));
        let time = cx.new(|cx| InputState::new(window, cx).placeholder(DEFAULT_TIME));
        let every = cx.new(|cx| InputState::new(window, cx).placeholder(DEFAULT_EVERY));
        let subs = vec![
            cx.subscribe(&title, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&prompt, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&time, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&every, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        (Self { title, prompt, time, every, v: Values::new(String::new()), open: false }, subs)
    }

    /// The choices with the texts as they stand in the inputs.
    pub fn values(&self, cx: &App) -> Values {
        Values {
            title: self.title.read(cx).value().to_string(),
            prompt: self.prompt.read(cx).value().to_string(),
            time: self.time.read(cx).value().to_string(),
            every: self.every.read(cx).value().to_string(),
            ..self.v.clone()
        }
    }

    fn load(&self, window: &mut Window, cx: &mut Context<Desktop>) {
        let v = self.v.clone();
        self.title.update(cx, |s, cx| s.set_value(v.title, window, cx));
        self.prompt.update(cx, |s, cx| s.set_value(v.prompt, window, cx));
        self.time.update(cx, |s, cx| s.set_value(v.time, window, cx));
        self.every.update(cx, |s, cx| s.set_value(v.every, window, cx));
    }
}

impl Desktop {
    /// Opens the editor on automation `id`, or on a new one.
    pub(crate) fn edit_automation(&mut self, id: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        let existing = id.and_then(|id| self.agents.automations.items.iter().find(|a| a.id == id));
        let folder = self.project.clone().or_else(|| self.projects().into_iter().next()).unwrap_or_default();
        let f = &mut self.automations.form;
        f.v = existing.map_or_else(|| Values::from_defaults(folder, &self.store.automations), Values::of);
        f.open = true;
        f.load(window, cx);
        f.title.update(cx, |s, cx| s.focus(window, cx));
        self.automations.tab = Tab::Automations;
        self.screen = crate::desktop::chrome::Screen::Automations;
        cx.notify();
    }

    fn save_automation(&mut self, cx: &mut Context<Self>) {
        let Ok(draft) = self.automations.form.values(cx).draft() else { return };
        self.outbox.automation_save(draft);
        self.automations.form.open = false;
        cx.notify();
    }

    fn close_editor(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        self.automations.form.open = false;
        cx.notify();
    }

    fn folder_choices(&self, current: &str) -> Vec<String> {
        let mut folders = self.projects();
        if !current.is_empty() && !folders.iter().any(|f| f == current) {
            folders.push(current.to_string());
        }
        folders
    }

    pub(super) fn automation_editor_page(&mut self, cx: &mut Context<Self>) -> Div {
        let v = self.automations.form.values(cx);
        let draft = v.draft();
        let editing = v.id.is_some();
        let crumb = if editing { format!("Edit · {}", if v.title.trim().is_empty() { "Untitled" } else { v.title.trim() }) } else { "New automation".to_string() };
        let ready = draft.is_ok();
        let right = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .children(draft.as_ref().err().map(|why| div().text_size(px(12.)).text_color(TEXT_3).child(*why)))
            .child(pill("editor-cancel", Variant::Secondary, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_editor(cx))))
            .child(
                pill("editor-save", Variant::Primary, None, if editing { "Save" } else { "Create" })
                    .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.save_automation(cx))))
                    .when(!ready, |d| d.bg(FILL_4).text_color(TEXT_3).shadow(vec![]).cursor_default()),
            );
        let bar = self.detail_bar(crumbs("Automations", crumb), right, cx);

        let project = self.repo_name(&v.folder);
        let f = &self.automations.form;
        // The input's line box is a fixed 1.25rem, so a larger size clips descenders.
        let title = div().font_weight(FontWeight::BOLD).child(Input::new(&f.title).appearance(false).p_0().text_size(px(20.)));
        let enabled = v.enabled;
        let status = div()
            .id("editor-enabled")
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .text_size(px(13.))
            .text_color(TEXT_2)
            .child(ui::toggle(enabled))
            .child(if enabled { "Active" } else { "Paused" })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.automations.form.v.enabled = !enabled;
                cx.notify();
            }));
        let meta = div().flex().items_center().gap(px(12.)).child(status).child(div().w(px(0.5)).h(px(16.)).bg(SEPARATOR)).child(self.project_pill(&v.folder, &project, cx));
        let templates = (!editing && v.prompt.trim().is_empty()).then(|| {
            div().flex().flex_wrap().gap(px(8.)).children(TEMPLATES.iter().enumerate().map(|(i, t)| {
                div()
                    .id(("editor-template", i))
                    .h(px(30.))
                    .px(px(11.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .rounded(px(9.))
                    .bg(SURFACE)
                    .shadow(ui::row_shadow())
                    .cursor_pointer()
                    .text_size(px(13.))
                    .text_color(TEXT_2)
                    .hover(|s| s.bg(FILL_2))
                    .child(icon(t.icon, 14., TEXT_3))
                    .child(t.title)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        let f = &mut this.automations.form;
                        f.v = f.values(cx);
                        f.v.fill(&TEMPLATES[i]);
                        f.load(window, cx);
                        cx.notify();
                    }))
            }))
        });
        let trigger = self.trigger_card(&v, cx);
        let runs = self.runs_card(&v, cx);
        let instructions = self.instructions_card(&v, cx);
        let body = [title.into_any_element(), meta.into_any_element()]
            .into_iter()
            .chain(templates.map(IntoElement::into_any_element))
            .chain([section("Trigger", trigger).into_any_element(), section("Runs", runs).into_any_element(), section("Instructions", instructions).into_any_element()]);
        div().flex_1().min_w_0().flex().flex_col().child(bar).child(self.detail_body("editor-body", 24., body))
    }

    fn project_pill(&self, folder: &str, name: &str, cx: &mut Context<Self>) -> Div {
        let open = self.automations.menu == Some(Menu::Project);
        let button = self.menu_toggle(
            div()
                .id("editor-project")
                .h(px(28.))
                .px(px(9.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(8.))
                .cursor_pointer()
                .text_size(px(14.))
                .font_weight(FontWeight(650.))
                .hover(|s| s.bg(FILL_2))
                .child(ui::swatch(self.repo_color(folder), 14., 4.))
                .child(if name.is_empty() { "Pick a project".to_string() } else { name.to_string() })
                .child(icon("chevron-down", 12., TEXT_3)),
            Menu::Project,
            cx,
        );
        let rows: Vec<_> = self.folder_choices(folder).into_iter().enumerate().map(|(i, p)| {
            let on = p == folder;
            let target = p.clone();
            ui::menu_row(("editor-project-row", i), "folder", &self.repo_name(&p), None).child(div().w(px(14.)).when(on, |d| d.child(icon("check", 14., TEXT))).into_any_element()).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.automations.form.v.folder = target.clone();
                this.close_menus();
                cx.notify();
            }))
        }).collect();
        let none = rows.is_empty().then(|| div().h(px(30.)).px(px(8.)).flex().items_center().text_size(px(13.)).text_color(TEXT_3).child("Add a project from the sidebar"));
        div().relative().child(button).when(open, |d| d.child(ui::dropdown(32., ui::menu_in("editor-project-in", self.popup("editor-project-menu", 240., cx).children(rows).children(none)))))
    }

    /// How each run starts; an automation saved without a mode shows none chosen and keeps the agent's own settings.
    fn runs_card(&self, v: &Values, cx: &mut Context<Self>) -> Div {
        let access = ACCESSES.iter().position(|a| *a == v.access).unwrap_or(usize::MAX);
        let mode = segmented("editor-access", &["Ask", "Edits", "Auto"], access, false, |this, i, cx| {
            this.automations.form.v.access = ACCESSES[i].into();
            cx.notify();
        }, cx);
        let on = v.new_worktree;
        let worktree = div()
            .id("editor-worktree")
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .text_color(TEXT_2)
            .child(ui::toggle(on))
            .child("Each run gets its own worktree and branch")
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.automations.form.v.new_worktree = !on;
                cx.notify();
            }));
        details(vec![("Permission mode", div().flex().child(mode).into_any_element()), ("New worktree", worktree.into_any_element())])
    }

    fn instructions_card(&self, v: &Values, cx: &mut Context<Self>) -> Div {
        let f = &self.automations.form;
        let open = self.automations.menu == Some(Menu::Provider);
        let provider = v.provider;
        let button = self.menu_toggle(
            div()
                .id("editor-provider")
                .h(px(28.))
                .px(px(10.))
                .pl(px(9.))
                .flex()
                .items_center()
                .gap(px(7.))
                .rounded(px(8.))
                .bg(FILL_3)
                .cursor_pointer()
                .text_size(px(13.))
                .font_weight(FontWeight(550.))
                .child(provider_icon(provider, 14., TEXT))
                .child(provider_name(provider))
                .child(icon("chevron-down", 12., TEXT_3)),
            Menu::Provider,
            cx,
        );
        let enabled = self.store.agents.enabled();
        let rows: Vec<_> = PROVIDERS.iter().filter(|p| enabled.contains(p) || **p == provider).enumerate().map(|(i, p)| {
            let p = *p;
            ui::menu_row(("editor-provider-row", i), if p == "codex" { "openai" } else { "claude" }, provider_name(p), None)
                .child(div().w(px(14.)).when(p == provider, |d| d.child(icon("check", 14., TEXT))))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.automations.form.v.provider = p;
                    this.close_menus();
                    cx.notify();
                }))
        }).collect();
        let menu = ui::menu_in("editor-provider-in", self.popup("editor-provider-menu", 200., cx).children(rows));
        // Opens upward: the card sits at the foot of the page.
        let up = div().absolute().top(px(-4.)).left_0().child(deferred(anchored().anchor(Anchor::BottomLeft).snap_to_window_with_margin(px(8.)).child(menu)).with_priority(1));
        card()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(div().min_h(px(120.)).mx(px(-10.)).mt(px(-5.)).text_size(px(15.)).line_height(px(23.25)).child(Textarea::new(&f.prompt).appearance(false).text_size(px(15.))))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(div().relative().child(button).when(open, |d| d.child(up)))
                    .child(div().text_size(px(12.)).text_color(TEXT_3).child("Skills, @file references and built-in commands work here.")),
            )
    }
}
