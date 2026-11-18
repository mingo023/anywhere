use super::machine::{Blocker, blocker, unreachable_notice};
use super::{Checkout, NO_PROJECT, Source, checkout_choice, listed_branches, parse_pr};
use crate::composer::launch::access_row;
use crate::desktop::Desktop;
use crate::modals::host_finder::Opener;
use crate::settings::ssh_hosts::link_color;
use agents::hosts::Link;
use crate::spaces::header::glyph;
use crate::spaces::{Reach, Space, in_space, on_host};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Picker {
    Machine,
    Project,
    Checkout,
    Branch,
    Pr,
}

const BRANCH_ROW: &str = "branch-row";

pub(crate) fn pick_head(label: &str) -> Div {
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

/// A machine's menu row: its link's dot, and why it can't be picked while it can't be reached.
fn machine_row(id: impl Into<ElementId>, space: &Space, name: &str, link: Option<&Link>, reach: Reach, chosen: bool) -> Stateful<Div> {
    let unusable = reach != Reach::Online;
    let note = match reach {
        Reach::Online => "",
        Reach::Connecting => "Connecting",
        Reach::Offline => "Offline",
    };
    ui::menu_pick(id, icon(glyph(space), 14., TEXT_3), name, false)
        .when(unusable, |d| d.cursor_default().child(div().text_size(px(11.5)).text_color(TEXT_3).child(note)))
        .when(space.host().is_some(), |d| d.child(ui::dot(6., link_color(link))))
        .when(chosen, |d| d.child(icon("check", 13., TEXT_2)))
}

/// A row per space in the switcher's order, `chosen` checked. Only a reachable one takes a click, which calls `pick`.
pub(crate) fn machine_rows(desktop: &Desktop, id: &'static str, chosen: &Space, pick: fn(&mut Desktop, Space, &mut Window, &mut Context<Desktop>), cx: &mut Context<Desktop>) -> Vec<AnyElement> {
    desktop
        .space_order()
        .into_iter()
        .enumerate()
        .map(|(i, space)| {
            let link = space.host().and_then(|id| desktop.fleet.hosts.links.get(id));
            let reach = desktop.space_reach(&space);
            let row = machine_row((id, i), &space, &desktop.space_name(&space), link, reach, space == *chosen).when(reach != Reach::Online, |d| d.opacity(0.5));
            row.when(reach == Reach::Online, |d| d.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| pick(this, space.clone(), window, cx)))).into_any_element()
        })
        .collect()
}

pub(crate) fn menu_input(glyph: &str, input: &Entity<InputState>) -> Div {
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
        let draft = &mut self.new_form.composer.draft;
        let menu = draft.escape_model() || draft.menu.take().is_some();
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

    /// Picking a local branch makes it the base; Open makes it the branch to start a worktree on, the only use of a branch only origin has.
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
                    this.new_form.draft.open_branch(branch.clone());
                    this.focus_prompt(window, cx);
                }))
            });
            let row = ui::pick_row(("branch", n), f.is_picked(local_ix, name), Some(icon("branch", 13., TEXT_3)), div().font_family(MONO).text_size(px(12.5)).child(label), meta)
                .group(BRANCH_ROW)
                .relative()
                .children(open)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    match local_ix {
                        Some(i) => this.new_form.draft.pick_base(i),
                        None => this.new_form.draft.open_branch(branch.clone()),
                    }
                    this.focus_prompt(window, cx);
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

    /// What stops the draft starting in the space on screen.
    pub(super) fn draft_blocker(&self) -> Option<Blocker> {
        let space = self.space();
        blocker(self.new_form.draft.repo.as_deref(), &space, self.space_reach(&space))
    }

    /// Run on: This Mac and the SSH hosts; one that can't be reached is dimmed and can't be picked.
    fn machine_menu(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let rows = machine_rows(self, "machine", &self.space(), Self::pick_machine, cx);
        let add = ui::menu_pick("machine-add", icon("plus", 14., TEXT_3), "Add a host\u{2026}", false).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            this.new_form.draft.picker = None;
            this.add_host_from_sidebar(window, cx);
        }));
        let rows: Vec<AnyElement> = std::iter::once(ui::menu_label("Run on").into_any_element()).chain(rows).chain([ui::menu_divider().into_any_element(), add.into_any_element()]).collect();
        picker_menu("machine-menu", 260., rows, cx).gap(px(2.))
    }

    /// The machine pill: the space on screen, whose menu switches it.
    pub(super) fn machine_select(&self, cx: &mut Context<Self>) -> Div {
        let (space, f) = (self.space(), &self.new_form.draft);
        let link = space.host().and_then(|id| self.fleet.hosts.links.get(id));
        let chip = ui::menu_chip("form-machine", f.picker == Some(Picker::Machine))
            .child(icon(glyph(&space), 14., TEXT_3))
            .child(div().font_weight(FontWeight::MEDIUM).child(self.space_name(&space)))
            .when(space.host().is_some(), |d| d.child(ui::dot(6., link_color(link))))
            .child(icon("chevron-down", 12., TEXT_4))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.toggle_picker(Picker::Machine, cx);
            }));
        let menu = (f.picker == Some(Picker::Machine)).then(|| ui::dropdown(36., ui::menu_in("machine-menu-in", self.machine_menu(cx))));
        div().relative().child(chip).children(menu)
    }

    /// Under the footer: why an unreachable machine starts nothing, and Reconnect.
    pub(super) fn machine_notice(&self, cx: &mut Context<Self>) -> Option<Div> {
        let space = self.space();
        let Some(Blocker::Unreachable(reach)) = self.draft_blocker() else { return None };
        let id = space.host()?.to_string();
        let reconnect = (reach == Reach::Offline).then(|| {
            ui::link("form-reconnect", "Reconnect").text_color(TEXT).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.retry_host(&id);
                cx.notify();
            }))
        });
        Some(
            div()
                .w_full()
                .max_w(px(600.))
                .mt(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(12.5))
                .text_color(WAITING_TEXT)
                .child(icon("warning", 13., WAITING_TEXT))
                .child(unreachable_notice(reach, &self.space_name(&space)))
                .children(reconnect),
        )
    }

    /// The space's projects, led by No project on This Mac, then a folder to open there and Clone from URL.
    fn form_project_menu(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let f = &self.new_form.draft;
        let (space, repo) = (self.space(), f.repo.as_deref());
        let none = (space == Space::Local).then(|| {
            ui::menu_pick("project-none", icon("folder", 14., TEXT_3), NO_PROJECT, f.no_project()).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.new_form.draft.pick_no_project();
                this.focus_prompt(window, cx);
            }))
        });
        let rows = in_space(&self.form_projects(), &space).into_iter().enumerate().map(|(i, p)| {
            let (name, tile) = (self.repo_name(&p), self.repo_tile_for(&p, 14.));
            let chosen = repo == Some(p.as_str());
            ui::menu_pick(("project", i), tile, &name, chosen)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.new_form.draft.picker = None;
                    this.pick_repo(p.clone(), window, cx);
                    cx.notify();
                }))
                .into_any_element()
        });
        let host = self.space_host_name(&space);
        let open = ui::menu_pick("project-open", icon("folder-plus", 14., TEXT_3), &on_host("Open folder", host.as_deref()), false).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_folder(Opener::NewSession, window, cx)));
        let clone = ui::menu_pick("project-clone", icon("branch", 14., TEXT_3), "Clone from URL\u{2026}", false).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            this.new_form.draft.picker = None;
            this.clone_project(window, cx);
        }));
        let rows: Vec<AnyElement> = none.map(IntoElement::into_any_element).into_iter().chain(rows).chain([ui::menu_divider().into_any_element(), open.into_any_element(), clone.into_any_element()]).collect();
        picker_menu("project-menu", 260., rows, cx).gap(px(2.))
    }

    pub(crate) fn repo_tile_for(&self, repo: &str, size: f32) -> Div {
        ui::repo_tile(&self.project_mark(repo, crate::util::initials(&self.repo_name(repo))), size, false, None)
    }

    pub(super) fn project_select(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form.draft;
        let name = f.repo.as_ref().map_or_else(|| NO_PROJECT.to_string(), |repo| self.repo_name(repo));
        let tile = f.repo.as_ref().map(|repo| self.repo_tile_for(repo, 16.));
        let missing = (self.draft_blocker() == Some(Blocker::Elsewhere)).then(|| format!("Not on {}", self.space_name(&self.space())));
        let chip = ui::menu_chip("form-project", f.picker == Some(Picker::Project))
            .children(tile)
            .when(f.repo.is_none(), |d| d.child(icon("folder", 14., TEXT_3)))
            .child(div().font_weight(FontWeight::MEDIUM).when(missing.is_some(), |d| d.text_color(TEXT_3)).child(name))
            .when_some(missing, |d, tip| d.child(icon("warning", 13., WAITING_TEXT)).tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx)))
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
        let checkout = if f.linked_pr().is_some() || self.draft_blocker().is_some() {
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
            .child(icon("chevron-down", 12., TEXT_4));
        let branch = if self.draft_blocker().is_some() {
            branch.opacity(0.5).cursor_default()
        } else {
            branch.capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.toggle_search(Picker::Branch, this.new_form.search.clone(), window, cx);
            }))
        };
        let branch_menu = (f.picker == Some(Picker::Branch)).then(|| ui::dropdown(36., ui::menu_in("branch-menu-in", self.branch_picker(cx))));
        div().relative().child(branch).children(branch_menu)
    }
}
