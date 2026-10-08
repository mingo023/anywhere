use super::{Source, checkout_option, listed_branches, parse_pr};
use crate::desktop::Desktop;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Picker {
    Agent,
    Checkout,
    Branch,
    Pr,
}

const BRANCH_ROW: &str = "branch-row";

fn chip(id: &'static str, open: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(30.))
        .pl(px(10.))
        .pr(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(7.))
        .rounded(px(8.))
        .whitespace_nowrap()
        .cursor_pointer()
        .bg(if open { FILL_3 } else { FILL_2 })
        .hover(|s| s.bg(FILL_3))
}

fn pick_head(label: &str) -> Div {
    div().pt(px(8.)).px(px(8.)).pb(px(4.)).text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child(label.to_string())
}

fn pick_row(id: impl Into<ElementId>, selected: bool, lead: Option<impl IntoElement>, label: Div, meta: Option<String>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(34.))
        .px(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(9.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(13.))
        .when(selected, |d| d.bg(FILL_2))
        .when(!selected, |d| d.hover(|s| s.bg(FILL_2)))
        .children(lead)
        .child(label.min_w_0().truncate().font_weight(FontWeight::MEDIUM))
        .child(div().ml_auto().pl(px(10.)).flex_none().text_size(px(12.)).text_color(TEXT_4).children(meta))
        .child(div().w(px(16.)).flex().flex_none().justify_end().when(selected, |d| d.child(icon("check", 14., TEXT))))
}

fn picker_menu(id: &'static str, width: f32, rows: Vec<AnyElement>, cx: &mut Context<Desktop>) -> Stateful<Div> {
    menu_frame(id, width, cx).max_h(px(360.)).overflow_y_scroll().p(px(5.)).children(rows)
}

fn menu_frame(id: &'static str, width: f32, cx: &mut Context<Desktop>) -> Stateful<Div> {
    ui::pop(div().id(id))
        .w(px(width))
        .flex()
        .flex_col()
        // Menus hang past the sheet's edge; without this a click there also lands on the backdrop and closes the sheet.
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

pub(crate) fn access_row(id: impl Into<ElementId>, selected: bool, label: &str, hint: &str, color: Token) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(8.))
        .py(px(7.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(9.))
        .rounded(px(6.))
        .cursor_pointer()
        .when(selected, |d| d.bg(FILL_2))
        .when(!selected, |d| d.hover(|s| s.bg(FILL_2)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(color).child(label.to_string()))
                .child(div().text_size(px(11.5)).text_color(TEXT_3).child(hint.to_string())),
        )
        .child(div().w(px(16.)).flex().flex_none().justify_end().when(selected, |d| d.child(icon("check", 14., TEXT))))
}

impl Desktop {
    pub fn close_picker(&mut self) -> bool {
        self.new_form.draft.picker.take().is_some()
    }

    fn toggle_picker(&mut self, picker: Picker, cx: &mut Context<Self>) {
        let f = &mut self.new_form.draft;
        f.picker = (f.picker != Some(picker)).then_some(picker);
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

    fn agent_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let f = &self.new_form.draft;
        let mut rows = Vec::new();
        for provider in ["claude", "codex"] {
            rows.push(
                pick_row(provider, f.provider == provider, Some(provider_icon(provider, 13., TEXT)), div().child(provider_name(provider)), None)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.new_form.draft.pick_provider(provider);
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        picker_menu("agent-menu", 260., rows, cx)
    }

    fn checkout_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let f = &self.new_form.draft;
        let rows = [true, false].map(|worktree| {
            let (glyph, label, hint) = self.checkout_option(worktree);
            div()
                .id(("checkout", usize::from(worktree)))
                .px(px(8.))
                .py(px(7.))
                .flex()
                .flex_none()
                .items_start()
                .gap(px(9.))
                .rounded(px(6.))
                .cursor_pointer()
                .when(f.worktree == worktree, |d| d.bg(FILL_2))
                .when(f.worktree != worktree, |d| d.hover(|s| s.bg(FILL_2)))
                .child(div().pt(px(1.)).child(icon(glyph, 14., TEXT_2)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(label))
                        .child(div().text_size(px(11.5)).text_color(TEXT_3).child(hint)),
                )
                .child(div().w(px(16.)).flex().flex_none().justify_end().when(f.worktree == worktree, |d| d.child(icon("check", 14., TEXT))))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    (this.new_form.draft.worktree, this.new_form.draft.picker) = (worktree, None);
                    cx.notify();
                }))
                .into_any_element()
        });
        picker_menu("checkout-menu", 300., rows.into(), cx).gap(px(2.))
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
        for (n, (local, name)) in listed_branches(&f.branches, remote, &search).into_iter().enumerate() {
            let title = match (search.trim().is_empty(), local) {
                (true, Some(0)) => "Default",
                (true, _) => "Recent",
                (false, Some(_)) => "Local",
                (false, None) => "On origin",
            };
            if head != Some(title) {
                rows.push(pick_head(title).into_any_element());
                head = Some(title);
            }
            let label = if local.is_some() { name.to_string() } else { format!("origin/{name}") };
            let meta = local.and_then(|i| f.branches[i].1).map(|s| crate::util::ago_long(s * 1000, now));
            let branch = name.to_string();
            let open = div()
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
                .on_click(cx.listener({
                    let branch = branch.clone();
                    move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_branch(branch.clone(), window, cx);
                    }
                }));
            let row = pick_row(("branch", n), f.source == Source::New && local == Some(f.base), Some(icon("branch", 13., TEXT_3)), div().font_family(MONO).text_size(px(12.5)).child(label), meta)
                .group(BRANCH_ROW)
                .relative()
                .when(opens, |d| d.child(open))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| match local {
                    Some(i) => {
                        let f = &mut this.new_form.draft;
                        (f.base, f.source, f.picker) = (i, Source::New, None);
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
        let menu = (self.new_form.draft.picker == Some(Picker::Pr)).then(|| {
            div().absolute().top(px(38.)).right_0().child(deferred(anchored().anchor(Anchor::TopRight).snap_to_window_with_margin(px(8.)).child(ui::menu_in("pr-menu-in", self.pr_picker(cx)))).with_priority(1))
        });
        div().relative().child(button).children(menu)
    }

    pub(super) fn agent_select(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form.draft;
        let agent = chip("form-agent", f.picker == Some(Picker::Agent))
            .child(provider_icon(f.provider, 13., TEXT))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(provider_name(f.provider)))
            .child(icon("chevron-down", 12., TEXT_4))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.toggle_picker(Picker::Agent, cx);
            }));
        let agent_menu = (f.picker == Some(Picker::Agent)).then(|| ui::dropdown(36., ui::menu_in("agent-menu-in", self.agent_picker(cx))));
        div().relative().child(agent).children(agent_menu)
    }

    fn checkout_option(&self, worktree: bool) -> (&'static str, String, &'static str) {
        let main = self.project.as_deref().and_then(|p| self.tree_of(p));
        checkout_option(worktree, self.cwd().as_deref(), main.as_deref())
    }

    pub(super) fn checkout_select(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form.draft;
        let (glyph, label, _) = self.checkout_option(f.worktree);
        let checkout = chip("form-checkout", f.picker == Some(Picker::Checkout))
            .child(icon(glyph, 14., TEXT_3))
            .child(div().font_weight(FontWeight::MEDIUM).child(label))
            .child(icon("chevron-down", 12., TEXT_4));
        let checkout = if f.source == Source::Pr {
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
        let label = if f.source == Source::Branch { f.target.clone() } else { f.base_branch() };
        let branch = chip("form-branch", f.picker == Some(Picker::Branch))
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
