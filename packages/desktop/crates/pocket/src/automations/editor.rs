use super::Menu;
use super::form::{TEMPLATES, Values};
use super::logic::{DEFAULT_EVERY, DEFAULT_TIME, Tab};
use super::parts::{crumbs, pill, section};
use crate::chat::composer::{INPUT_LINE_HEIGHT, INPUT_TEXT_SIZE, frame};
use crate::composer::Composer;
use crate::composer::launch::access_row;
use crate::desktop::Desktop;
use crate::modals::new_session::picker::{menu_input, pick_head};
use crate::modals::new_session::{default_first, listed_branches};
use crate::util::{LOCAL_ICON, WORKTREE_ICON, ago_long, now_ms};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::LaunchPick;
use theme::*;
use ui::{self, Variant};

/// Where each run starts: a new worktree of the project, or its checkout as it stands.
const CHECKOUTS: [(bool, &str, &str, &str); 2] = [
    (true, WORKTREE_ICON, "Worktree", "Each run gets its own branch and copy of the files"),
    (false, LOCAL_ICON, "Local", "Runs in the project's checkout: files and branch are shared"),
];

/// Local branches with their last commit's unix seconds, as `git::dated_branches` lists them.
type Branches = Vec<(String, Option<i64>)>;

pub struct Form {
    pub(super) title: Entity<InputState>,
    pub(super) time: Entity<InputState>,
    pub(super) every: Entity<InputState>,
    /// The instructions, with the agent, model and access chips a session's prompt box has.
    pub(crate) composer: Composer,
    pub(super) branch_search: Entity<InputState>,
    /// A folder's local branches, its default first, as last read.
    pub(super) branches: Option<(String, Branches)>,
    /// The choices; its texts and chips only count as of the last `load`, see `values`.
    pub(super) v: Values,
    pub(crate) open: bool,
}

fn slot(d: &mut Desktop) -> &mut Composer {
    &mut d.automations.form.composer
}

/// The key the composer remembers its pick under.
fn project_key(folder: &str) -> Option<String> {
    Some(folder.to_string()).filter(|f| !f.is_empty())
}

impl Form {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Untitled automation"));
        let prompt = cx.new(|cx| TextareaState::new(window, cx).placeholder("Tell the agent what to do when this automation runs…").auto_grow(5, 14));
        let time = cx.new(|cx| InputState::new(window, cx).placeholder(DEFAULT_TIME));
        let every = cx.new(|cx| InputState::new(window, cx).placeholder(DEFAULT_EVERY));
        let models = cx.new(|cx| InputState::new(window, cx).placeholder("Search models"));
        let branch_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search branches…"));
        let mut subs: Vec<_> = [&title, &time, &every, &models, &branch_search].into_iter().map(|input| cx.subscribe(input, |_, _, _: &InputEvent, cx| cx.notify())).collect();
        subs.push(cx.subscribe(&prompt, |_, _, _: &InputEvent, cx| cx.notify()));
        let composer = Composer::new(prompt, models, slot, |d| d.automations.menu = None, |_, _| {});
        (Self { title, time, every, composer, branch_search, branches: None, v: Values::new(String::new()), open: false }, subs)
    }

    /// The choices with the texts and chips as they stand.
    pub fn values(&self, cx: &App) -> Values {
        let mut v = Values {
            title: self.title.read(cx).value().to_string(),
            prompt: self.composer.prompt.read(cx).value().to_string(),
            time: self.time.read(cx).value().to_string(),
            every: self.every.read(cx).value().to_string(),
            ..self.v.clone()
        };
        v.set_pick(&self.pick());
        v
    }

    /// The agent, model, effort and access the chips show: picked there, else as loaded.
    fn pick(&self) -> LaunchPick {
        self.composer.draft.pick(project_key(&self.v.folder).as_deref(), || self.v.pick())
    }

    /// The folder's branches once read, its default first.
    fn branches(&self) -> &[(String, Option<i64>)] {
        self.branches.as_ref().filter(|(folder, _)| *folder == self.v.folder).map_or(&[], |(_, b)| b.as_slice())
    }

    /// The branch each new worktree starts from: the one picked, else the folder's default.
    fn base_label(&self) -> Option<String> {
        Some(self.v.base.clone()).filter(|b| !b.is_empty()).or_else(|| self.branches().first().map(|(b, _)| b.clone()))
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Desktop>) {
        let v = self.v.clone();
        self.title.update(cx, |s, cx| s.set_value(v.title.clone(), window, cx));
        self.composer.prompt.update(cx, |s, cx| s.set_value(v.prompt.clone(), window, cx));
        self.time.update(cx, |s, cx| s.set_value(v.time.clone(), window, cx));
        self.every.update(cx, |s, cx| s.set_value(v.every.clone(), window, cx));
        self.composer.draft.clear();
        self.composer.draft.choose(project_key(&v.folder), v.pick());
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
        let folder = f.v.folder.clone();
        self.automations.tab = Tab::Automations;
        self.screen = crate::desktop::chrome::Screen::Automations;
        self.load_branches(folder, cx);
        cx.notify();
    }

    /// Reads `folder`'s local branches off the UI thread, for the branch menu while the editor is still on it.
    fn load_branches(&mut self, folder: String, cx: &mut Context<Self>) {
        if folder.is_empty() {
            return;
        }
        let (runner, dir) = (hosts::runner_for(&folder), folder.clone());
        let task = cx.background_executor().spawn(async move {
            let current = git::read(&runner, &dir, false).map(|r| r.branch).unwrap_or_default();
            let mut branches = git::dated_branches(&runner, &dir);
            default_first(&mut branches, "", &current);
            branches
        });
        cx.spawn(async move |this, cx| {
            let branches = task.await;
            this.update(cx, |d, cx| {
                if d.automations.form.v.folder == folder {
                    d.automations.form.branches = Some((folder, branches));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Moves the draft to `folder`, keeping what its chips picked; its branch is the new folder's default.
    fn set_folder(&mut self, folder: String, cx: &mut Context<Self>) {
        let f = &mut self.automations.form;
        let pick = f.pick();
        (f.v.folder, f.v.base) = (folder.clone(), String::new());
        f.composer.draft.choose(project_key(&folder), pick);
        self.close_menus();
        self.load_branches(folder, cx);
        cx.notify();
    }

    fn save_automation(&mut self, cx: &mut Context<Self>) {
        let v = self.automations.form.values(cx);
        let (own, efforts) = (self.store.agents.provider(v.provider), self.agents.provider_info(v.provider).efforts.clone());
        let v = v.settled(self.store.agents.access(), &own.model, &own.effort, &efforts);
        let Ok(draft) = v.draft() else { return };
        self.outbox.local.automation_save(draft);
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
            .child(ui::toggle("switch", enabled))
            .child(if enabled { "Active" } else { "Paused" })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.automations.form.v.enabled = !enabled;
                cx.notify();
            }));
        let meta = div().flex().items_center().child(status);
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
        let instructions = div().flex().flex_col().gap(px(10.)).child(self.instructions_box(&v, cx)).child(self.runs_in(&v, cx));
        let body = [title.into_any_element(), meta.into_any_element()]
            .into_iter()
            .chain(templates.map(IntoElement::into_any_element))
            .chain([section("Trigger", trigger).into_any_element(), section("Instructions", instructions).into_any_element()]);
        div().flex_1().min_w_0().flex().flex_col().child(bar).child(self.detail_body("editor-body", 24., body))
    }

    /// The instructions in a session's prompt box: access on the left, agent and model on the right.
    fn instructions_box(&self, v: &Values, cx: &mut Context<Self>) -> Div {
        let c = &self.automations.form.composer;
        let (project, pick) = (project_key(&v.folder), v.pick());
        let textarea = Textarea::new(&c.prompt).appearance(false).text_size(px(INPUT_TEXT_SIZE)).line_height(px(INPUT_LINE_HEIGHT));
        let bar = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .pt(px(4.))
            .px(px(7.))
            .pb(px(7.))
            .child(self.access_chip(c, &pick, project.clone(), cx))
            .child(div().flex_1())
            .child(self.model_pill(c, &pick, project, cx));
        let prompt = c.prompt.clone();
        frame(!v.prompt.trim().is_empty())
            .on_mouse_down(MouseButton::Left, move |_, window, cx| prompt.update(cx, |s, cx| s.focus(window, cx)))
            .child(div().pt(px(5.)).px(px(5.)).child(textarea))
            .child(bar)
    }

    /// Under the prompt box, as under a session's: the project, then where each run starts.
    fn runs_in(&self, v: &Values, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(2.))
            .text_size(px(13.))
            .child(self.project_chip(&v.folder, cx))
            .child(div().px(px(4.)).text_color(TEXT_4).child("·"))
            .child(self.checkout_chip(v.new_worktree, cx))
            .children(v.new_worktree.then(|| self.branch_chip(cx)))
    }

    fn project_chip(&self, folder: &str, cx: &mut Context<Self>) -> Div {
        let open = self.automations.menu == Some(Menu::Project);
        let name = if folder.is_empty() { "Pick a project".to_string() } else { self.repo_name(folder) };
        let chip = ui::menu_chip("editor-project", open)
            .children((!folder.is_empty()).then(|| self.repo_tile_for(folder, 16.)))
            .child(div().font_weight(FontWeight::MEDIUM).child(name))
            .child(icon("chevron-down", 12., TEXT_4));
        let rows: Vec<_> = self.folder_choices(folder).into_iter().enumerate().map(|(i, p)| {
            let chosen = p == folder;
            ui::menu_pick(("editor-project-row", i), self.repo_tile_for(&p, 14.), &self.repo_name(&p), chosen).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_folder(p.clone(), cx)))
        }).collect();
        let none = rows.is_empty().then(|| div().h(px(30.)).px(px(8.)).flex().items_center().text_size(px(13.)).text_color(TEXT_3).child("Add a project from the sidebar"));
        let menu = open.then(|| ui::dropdown(36., ui::menu_in("editor-project-in", self.popup("editor-project-menu", 260., cx).gap(px(2.)).children(rows).children(none))));
        div().relative().child(self.menu_toggle(chip, Menu::Project, cx)).children(menu)
    }

    fn checkout_chip(&self, worktree: bool, cx: &mut Context<Self>) -> Div {
        let open = self.automations.menu == Some(Menu::Checkout);
        let (_, glyph, label, _) = CHECKOUTS[!worktree as usize];
        let chip = ui::menu_chip("editor-checkout", open).child(icon(glyph, 14., TEXT_3)).child(div().font_weight(FontWeight::MEDIUM).child(label)).child(icon("chevron-down", 12., TEXT_4));
        let rows: Vec<_> = CHECKOUTS.iter().enumerate().map(|(i, &(on, glyph, label, hint))| {
            access_row(("editor-checkout-row", i), on == worktree, Some(glyph), label, hint, TEXT).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.automations.form.v.new_worktree = on;
                this.close_menus();
                cx.notify();
            }))
        }).collect();
        let menu = open.then(|| ui::dropdown(36., ui::menu_in("editor-checkout-in", self.popup("editor-checkout-menu", 300., cx).gap(px(2.)).children(rows))));
        div().relative().child(self.menu_toggle(chip, Menu::Checkout, cx)).children(menu)
    }

    /// The branch each run's new worktree starts from; the menu lists the folder's local branches, as a session's base.
    fn branch_chip(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.automations.form;
        let open = self.automations.menu == Some(Menu::Branch);
        let label = f.base_label();
        let search_input = f.branch_search.clone();
        let chip = ui::menu_chip("editor-branch", open)
            .child(icon("branch", 14., TEXT_3))
            .child(div().font_family(MONO).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(label.clone().unwrap_or_else(|| "Default branch".into())))
            .child(icon("chevron-down", 12., TEXT_4))
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.toggle_menu(Menu::Branch, cx);
                if this.automations.menu == Some(Menu::Branch) {
                    search_input.update(cx, |s, cx| {
                        s.set_value("", window, cx);
                        s.focus(window, cx);
                    });
                }
            }));
        let menu = open.then(|| {
            let search = f.branch_search.read(cx).value().to_string();
            let now = now_ms();
            let branches = f.branches();
            let mut rows = Vec::new();
            let mut head = None;
            for (n, (local, name)) in listed_branches(branches, &[], &search).into_iter().enumerate() {
                let title = match (search.trim().is_empty(), local) {
                    (true, Some(0)) => "Default",
                    (true, _) => "Recent",
                    (false, _) => "Local",
                };
                if head != Some(title) {
                    rows.push(pick_head(title).into_any_element());
                    head = Some(title);
                }
                let meta = local.and_then(|i| branches[i].1).map(|s| ago_long(s * 1000, now));
                let branch = name.to_string();
                let row = ui::pick_row(("editor-branch-row", n), label.as_deref() == Some(name), Some(icon("branch", 13., TEXT_3)), div().font_family(MONO).text_size(px(12.5)).child(branch.clone()), meta)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.automations.form.v.base = branch.clone();
                        this.close_menus();
                        cx.notify();
                    }));
                rows.push(row.into_any_element());
            }
            let empty = rows.is_empty().then(|| {
                let why = if f.branches.as_ref().is_some_and(|(folder, _)| *folder == f.v.folder) { "No branches found" } else { "Reading branches…" };
                div().h(px(36.)).px(px(10.)).flex().items_center().text_size(px(13.)).text_color(TEXT_3).child(why)
            });
            let note = label.as_ref().map(|b| div().px(px(12.)).py(px(8.)).border_t(px(0.5)).border_color(SEPARATOR).text_size(px(12.)).text_color(TEXT_3).child(format!("Each run starts a new branch off {b}.")));
            let list = div().id("editor-branch-list").max_h(px(316.)).overflow_y_scroll().p(px(5.)).flex().flex_col().children(empty).children(rows);
            ui::dropdown(36., ui::menu_in("editor-branch-in", self.popup("editor-branch-menu", 340., cx).p_0().overflow_hidden().child(menu_input("search", &f.branch_search)).child(list).children(note)))
        });
        div().relative().child(chip).children(menu)
    }
}
