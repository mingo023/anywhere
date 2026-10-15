use super::{Checkout, Source, checkout_choice, listed_branches, parse_pr};
use crate::composer::launch::access_row;
use crate::desktop::Desktop;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Picker {
    Project,
    Checkout,
    Branch,
    Pr,
}

const BRANCH_ROW: &str = "branch-row";

fn pick_head(label: &str) -> Div {
    div().pt(px(8.)).px(px(8.)).pb(px(4.)).text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child(label.to_string())
}

fn picker_menu(id: &'static str, width: f32, rows: Vec<AnyElement>, cx: &mut Context<Desktop>) -> Stateful<Div> {
    menu_frame(id, width, cx).max_h(px(360.)).overflow_y_scroll().p(px(5.)).children(rows)
}

fn menu_frame(id: &'static str, width: f32, cx: &mut Context<Desktop>) -> Stateful<Div> {
    ui::pop(div().id(id))
        .w(px(width))
        .flex()
        .flex_col()
        // Menus hang over the page; without this a click on one also lands on what it covers.
        .occlude()
        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.new_form.draft.picker = None;
            cx.notify();
        }))
}

fn menu_input(glyph: &str, input: &Entity<InputState>) -> Div {
    div()
        .h(px(44.))
        .px(px(14.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .border_b(px(0.5))
        .border_color(SEPARATOR)
        .child(icon(glyph, 15., TEXT_3))
        .child(div().flex_1().text_size(px(13.)).child(Input::new(input).appearance(false).p_0().text_size(px(13.))))
}

impl Desktop {
    pub fn close_picker(&mut self) -> bool {
        let picker = self.new_form.draft.picker.take().is_some();
        let menu = self.new_form.composer.draft.menu.take().is_some();
        picker || menu
    }

    /// One menu at a time, the composer's included.
    pub(crate) fn toggle_picker(&mut self, picker: Picker, cx: &mut Context<Self>) {
        let f = &mut self.new_form.draft;
        f.picker = (f.picker != Some(picker)).then_some(picker);
        self.new_form.composer.draft.menu = None;
        cx.notify();
    }

    /// Opens a menu led by `input`, emptied and focused.
    fn toggle_search(&mut self, picker: Picker, input: Entity<InputState>, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_picker(picker, cx);
        if self.new_form.draft.picker == Some(picker) {
            input.update(cx, |s, cx| {
                s.set_value("", window, cx);
                s.focus(window, cx);
            });
        }
    }

    fn checkout_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let f = &self.new_form.draft;
        let rows: Vec<AnyElement> = f.targets().into_iter().enumerate().map(|(i, target)| {
            let Checkout { glyph, label, hint } = checkout_choice(target);
            access_row(("checkout", i), f.target() == target, Some(glyph), label, hint, TEXT)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.new_form.draft.pick_checkout(target);
                    cx.notify();
                }))
                .into_any_element()
        }).collect();
        picker_menu("checkout-menu", 300., rows, cx).gap(px(2.))
    }

    /// Picking a local branch makes it the base; Open starts a worktree on it at once, the only use of a branch only origin has.
    fn branch_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let f = &self.new_form.draft;
        let opens = self.agents.opens();
        let search = self.new_form.search.read(cx).value().to_string();
        let remote: &[String] = if opens { &f.remote_branches } else { &[] };
        let now = crate::util::now_ms();
        let mut rows = Vec::new();
        let mut head = None;
        for (n, (local_ix, name)) in listed_branches(&f.branches, remote, &search).into_iter().enumerate() {
            let title = match (search.trim().is_empty(), local_ix) {
                (true, Some(0)) => "Default",
                (true, _) => "Recent",
                (false, Some(_)) => "Local",
                (false, None) => "On origin",
            };
            if head != Some(title) {
                rows.push(pick_head(title).into_any_element());
                head = Some(title);
            }
            let label = if local_ix.is_some() { name.to_string() } else { format!("origin/{name}") };
            let meta = local_ix.and_then(|i| f.branches[i].1).map(|s| crate::util::ago_long(s * 1000, now));
            let branch = name.to_string();
            let open = opens.then(|| {
                let branch = branch.clone();
                div()
                .id(("branch-open", n))
                .absolute()
                .top(px(5.))
                .right(px(6.))
                .h(px(24.))
                .px(px(9.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .bg(POPOVER)
                .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .active(|s| s.opacity(0.6))
                .invisible()
                .group_hover(BRANCH_ROW, |s| s.visible())
                .child("Open worktree")
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_branch(branch.clone(), window, cx);
                }))
            });
            let row = ui::pick_row(("branch", n), f.source == Source::New && local_ix == Some(f.base), Some(icon("branch", 13., TEXT_3)), div().font_family(MONO).text_size(px(12.5)).child(label), meta)
                .group(BRANCH_ROW)
                .relative()
                .children(open)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| match local_ix {
                    Some(i) => {
                        this.new_form.draft.pick_base(i);
                        cx.notify();
                    }
                    None => this.open_branch(branch.clone(), window, cx),
                }));
            rows.push(row.into_any_element());
        }
        let empty = div().h(px(36.)).px(px(10.)).flex().items_center().text_size(px(13.)).text_color(TEXT_3).child("No branches found");
        let list = div().id("branch-list").max_h(px(316.)).overflow_y_scroll().p(px(5.)).flex().flex_col().when(rows.is_empty(), |d| d.child(empty)).children(rows);
        menu_frame("branch-menu", 340., cx).overflow_hidden().child(menu_input("search", &self.new_form.search)).child(list)
    }

    fn pr_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let typed = self.new_form.pr.read(cx).value().trim().to_string();
        let bad = !typed.is_empty() && parse_pr(&typed).is_none();
        let note = div().px(px(14.)).py(px(10.)).text_size(px(12.)).text_color(if bad { FAILED } else { TEXT_4 }).child(if bad { "Use 123, #123 or a PR URL" } else { "Press ↵ to link the pull request" });
        menu_frame("pr-menu", 300., cx).overflow_hidden().child(menu_input("pull-request", &self.new_form.pr)).child(note)
    }

    pub(super) fn pr_select(&self, cx: &mut Context<Self>) -> Div {
        let button = ui::icon_button_sized("form-pr", "pull-request", 32., TEXT_3).capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            this.toggle_search(Picker::Pr, this.new_form.pr.clone(), window, cx);
        }));
        let menu = (self.new_form.draft.picker == Some(Picker::Pr)).then(|| ui::dropdown_right(38., ui::menu_in("pr-menu-in", self.pr_picker(cx))));
        div().relative().child(button).children(menu)
    }

    /// Projects a session can start in: those with a git checkout.
    fn form_project_menu(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let repo = self.new_form.draft.repo.as_deref();
        let mut rows: Vec<AnyElement> = self.projects().into_iter().filter(|p| self.listed_trees(p).is_some_and(|t| !t.is_empty())).enumerate().map(|(i, p)| {
            let name = self.repo_name(&p);
            let tile = ui::repo_tile(&self.project_mark(&p, crate::util::initials(&name)), 14., false, None);
            let chosen = repo == Some(p.as_str());
            ui::menu_pick(("project", i), tile, &name, chosen)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.new_form.draft.picker = None;
                    this.pick_repo(p.clone(), window, cx);
                    cx.notify();
                }))
                .into_any_element()
        }).collect();
        rows.push(ui::menu_divider().into_any_element());
        rows.push(ui::menu_row("project-add", "plus", "Add project…", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            this.new_form.draft.picker = None;
            this.add_local_project(window, cx);
        })).into_any_element());
        picker_menu("project-menu", 260., rows, cx).gap(px(2.))
    }

    pub(super) fn project_select(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form.draft;
        let lead = f.repo.as_ref().map(|repo| {
            let name = self.repo_name(repo);
            (ui::repo_tile(&self.project_mark(repo, crate::util::initials(&name)), 16., false, None), name)
        });
        let (tile, name) = lead.map_or((None, "Select project".to_string()), |(tile, name)| (Some(tile), name));
        let chip = ui::menu_chip("form-project", f.picker == Some(Picker::Project))
            .children(tile)
            .child(div().font_weight(FontWeight::MEDIUM).child(name))
            .child(icon("chevron-down", 12., TEXT_4))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.toggle_picker(Picker::Project, cx);
            }));
        let menu = (f.picker == Some(Picker::Project)).then(|| ui::dropdown(36., ui::menu_in("project-menu-in", self.form_project_menu(cx))));
        div().relative().child(chip).children(menu)
    }

    pub(super) fn checkout_select(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form.draft;
        let Checkout { glyph, label, .. } = checkout_choice(f.target());
        let checkout = ui::menu_chip("form-checkout", f.picker == Some(Picker::Checkout))
            .child(icon(glyph, 14., TEXT_3))
            .child(div().font_weight(FontWeight::MEDIUM).child(label))
            .child(icon("chevron-down", 12., TEXT_4));
        let checkout = if f.linked_pr().is_some() {
            checkout.opacity(0.5).cursor_default()
        } else {
            checkout.capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.toggle_picker(Picker::Checkout, cx);
            }))
        };
        let menu = (f.picker == Some(Picker::Checkout)).then(|| ui::dropdown(36., ui::menu_in("checkout-menu-in", self.checkout_picker(cx))));
        div().relative().child(checkout).children(menu)
    }

    /// The base to branch from, or the branch to open.
    pub(super) fn branch_select(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form.draft;
        let label = match &f.source {
            Source::Branch(branch) => branch.clone(),
            _ => f.base_branch(),
        };
        let branch = ui::menu_chip("form-branch", f.picker == Some(Picker::Branch))
            .child(icon("branch", 14., TEXT_3))
            .child(div().font_family(MONO).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(label))
            .child(icon("chevron-down", 12., TEXT_4))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.toggle_search(Picker::Branch, this.new_form.search.clone(), window, cx);
            }));
        let branch_menu = (f.picker == Some(Picker::Branch)).then(|| ui::dropdown(36., ui::menu_in("branch-menu-in", self.branch_picker(cx))));
        div().relative().child(branch).children(branch_menu)
    }
}
