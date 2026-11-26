use super::fold;
use super::reorder::{DragRow, List};
use super::row_menu::{MenuEvent, MenuFrame, menu_button, open_on_right_click};
use super::sections::{Section, emoji_mark, section_head};
use super::tree_card::{self, WAIT};
use super::{Kind, Sidebar, done_name, row_name, with_sub};
use crate::desktop::chrome::{Column, RowMenu, Screen, drag_area, id, seam_handle};
use crate::inbox::{Inbox, hung};
use crate::spaces::{Heading, SLIDE_MS};
use crate::star::Star;
use crate::updates::{Update, release_url};
use crate::util::{LOCAL_ICON, WORKTREE_ICON};
use git::github::Pr;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::{Duration, Instant};
use theme::*;
use ui::{self, icon_button_sized};
use workspace::tree::Axis;

#[derive(PartialEq)]
pub(crate) struct ProjectFrame {
    pub(crate) ix: usize,
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) mark: ui::Mark,
    pub(crate) git: bool,
    pub(crate) open: bool,
    pub(crate) setting_up: bool,
    pub(crate) selected: bool,
    pub(crate) state: Option<ui::State>,
    pub(crate) offline: bool,
    pub(crate) lit: bool,
    pub(crate) flashed: bool,
    pub(crate) section: Section,
    pub(crate) manual: bool,
    pub(crate) rows: Vec<TreeFrame>,
}

#[derive(PartialEq)]
pub(crate) enum RowMark {
    Failed,
    Setting,
    Status,
}

#[derive(PartialEq)]
pub(crate) struct CardFrame {
    pub(crate) pr: Option<Pr>,
    pub(crate) when: Option<String>,
}

#[derive(PartialEq)]
pub(crate) struct TreeFrame {
    pub(crate) project: String,
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) tip: String,
    pub(crate) kind: Kind,
    pub(crate) ix: usize,
    pub(crate) sub: Option<String>,
    pub(crate) selected: bool,
    pub(crate) state: Option<ui::State>,
    pub(crate) mark: RowMark,
    pub(crate) pr: Option<(&'static str, Token)>,
    pub(crate) diff: Option<(usize, usize)>,
    pub(crate) was: Option<(String, Instant)>,
    pub(crate) lit: bool,
    pub(crate) renames: bool,
    pub(crate) menus: bool,
    pub(crate) card: Option<CardFrame>,
}

pub(crate) enum PinnedItem {
    Project(ProjectFrame),
    Tree { key: String, row: Option<Box<TreeFrame>> },
}

impl PartialEq for PinnedItem {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Project(a), Self::Project(b)) => a == b,
            (Self::Tree { key: a, row: x }, Self::Tree { key: b, row: y }) => a == b && x == y,
            _ => false,
        }
    }
}

#[derive(PartialEq)]
pub(crate) struct PinnedFrame {
    pub(crate) open: bool,
    pub(crate) items: Vec<PinnedItem>,
}

#[derive(PartialEq)]
pub(crate) enum GroupBody {
    Hidden,
    Hint,
    Blocks(Vec<ProjectFrame>),
}

#[derive(PartialEq)]
pub(crate) struct GroupFrame {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) icon: Option<String>,
    pub(crate) open: bool,
    pub(crate) body: GroupBody,
}

#[derive(PartialEq)]
pub(crate) enum Tail {
    None,
    NoMatch(String),
    Other { blocks: Vec<ProjectFrame>, headed: bool },
}

#[derive(PartialEq)]
pub(crate) struct SectionsFrame {
    pub(crate) pinned_open: bool,
    pub(crate) pinned: Option<PinnedFrame>,
    pub(crate) groups: Vec<GroupFrame>,
    pub(crate) tail: Tail,
}

#[derive(PartialEq)]
pub(crate) enum Body {
    Empty(Option<String>),
    Sections(SectionsFrame),
}

#[derive(PartialEq)]
pub(crate) struct OfflineFrame {
    pub(crate) glyph: &'static str,
    pub(crate) title: String,
    pub(crate) note: String,
    pub(crate) reconnect: Option<String>,
}

#[derive(PartialEq)]
pub(crate) enum ScratchRow {
    Loose { path: String, title: String, folder: String, selected: bool, state: Option<ui::State>, was: Option<(String, Instant)> },
    Folder { ix: usize, path: String, name: String, selected: bool, state: Option<ui::State>, was: Option<(String, Instant)>, renames: bool },
}

#[derive(PartialEq)]
pub(crate) struct ScratchFrame {
    pub(crate) open: bool,
    pub(crate) roll: Option<ui::State>,
    pub(crate) count: usize,
    pub(crate) rows: Vec<ScratchRow>,
}

#[derive(PartialEq)]
pub(crate) struct AsideFrame {
    pub(crate) width: f32,
    pub(crate) seam: bool,
    pub(crate) reduce_motion: bool,
    pub(crate) unseen: usize,
    pub(crate) inbox: Option<Entity<Inbox>>,
    pub(crate) screen: Screen,
    pub(crate) pages: Vec<(&'static str, &'static str, Screen)>,
    pub(crate) scratch: Option<ScratchFrame>,
    pub(crate) offline: Option<OfflineFrame>,
    pub(crate) body: Body,
    pub(crate) slide: Option<(u64, Heading)>,
    pub(crate) update: Update,
    pub(crate) star: Option<Entity<Star>>,
    pub(crate) menu: Option<MenuFrame>,
}

pub(crate) enum SidebarEvent {
    Bell,
    Collapse,
    Palette,
    Page(Screen),
    SelectTree { project: String, tree: Option<String>, progress: bool },
    NewPlace(String),
    ToggleProject(String),
    OpenChanges,
    Rename(String),
    Renamed { tree: String, title: String },
    GroupRenamed { group: u32, name: String },
    DropTree { project: String, from: String, to: String },
    DropProject { path: String, onto: String, section: Section },
    DropInto { path: String, section: Section },
    TogglePins,
    ToggleGroup { id: u32, clicks: usize },
    ToggleScratch,
    NewScratch,
    DropScratch { from: String, to: String },
    Menu(MenuEvent),
    Swipe(ScrollEvent),
    RestartToUpdate,
    DismissWhatsNew,
    Reconnect(String),
    OpenFolder,
    CloneProject,
    AddHost,
    Settings,
    Drag(Column, f32),
    ResetWidth(Column),
}

impl EventEmitter<SidebarEvent> for Sidebar {}

const HEAD: &str = "aside-scratch-head";
/// Past this the section scrolls, so a long list can't squeeze Projects out.
const MAX_ROWS: f32 = 8.5;

impl Sidebar {
    pub(crate) fn set_frame(&mut self, frame: AsideFrame, cx: &mut Context<Self>) {
        if self.frame.as_ref() != Some(&frame) {
            self.frame = Some(frame);
            crate::region::notify_after_draw(cx);
        }
    }

    fn send(&mut self, ev: MenuEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(SidebarEvent::Menu(ev));
    }

    fn hover_tree(&mut self, tree: &str, over: bool, y: f32, cx: &mut Context<Self>) {
        let now = Instant::now();
        // Rows reserve their trail by `Hover::on`, so redraw even when no card shows or hides.
        cx.notify();
        if !over {
            self.hover.leave(tree, now);
            return;
        }
        if self.hover.enter(tree, now, y) {
            return;
        }
        let tree = tree.to_string();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(WAIT).await;
            this.update(cx, |s, cx| {
                if s.hover.waited(&tree, Instant::now()) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// `label`, reading `text`, scrolling through its cut-off end while `tree`'s card shows.
    fn scrolling(&self, tree: &str, text: String, label: AnyElement, reduce_motion: bool, cx: &mut Context<Self>) -> AnyElement {
        if reduce_motion || self.hover.shows(tree).is_none() {
            return label;
        }
        let (this, row) = (cx.weak_entity(), tree.to_string());
        ui::marquee(id(format!("aside-marquee:{tree}")), text.into(), self.hover.label(tree), label, move |width, overflow, cx| {
            this.update(cx, |this, cx| {
                if this.hover.measured(&row, width, overflow) {
                    cx.notify();
                }
            })
            .ok();
        })
        .into_any_element()
    }

    /// Shows `card` off `tree`'s row once the pointer rests there.
    fn with_card(row: Stateful<Div>, tree: &str, card: Option<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        let key = tree.to_string();
        row.relative()
            .on_hover(cx.listener(move |this, over: &bool, window, cx| this.hover_tree(&key, *over, window.mouse_position().y.into(), cx)))
            .on_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                if this.hover.press() {
                    cx.notify();
                }
            }))
            .children(card)
    }

    fn card_for(&self, f: &TreeFrame, reduce_motion: bool) -> Option<Div> {
        self.hover.shows(&f.key).zip(f.card.as_ref()).map(|(fades, card)| {
            let slide = self.hover.slide().filter(|_| !reduce_motion);
            tree_card::hung(f.label.clone(), f.tip.clone(), card.pr.as_ref(), card.when.clone(), fades && !reduce_motion, slide)
        })
    }

    fn name(&self, f: &TreeFrame, rf: &AsideFrame, cx: &mut Context<Self>) -> AnyElement {
        match self.rename.as_ref().filter(|r| r.tree == f.key) {
            Some(r) => Input::new(&r.input).appearance(false).p_0().text_size(px(13.)).into_any_element(),
            None => done_name(
                self.scrolling(
                    &f.key,
                    f.label.clone(),
                    row_name(f.was.as_ref(), &f.key, f.label.clone(), f.state, f.selected, rf.reduce_motion),
                    rf.reduce_motion,
                    cx,
                ),
                f.state,
            ),
        }
    }

    /// `f`'s row; `at` its place among its project's rows, where it can be dragged to another, except while renamed so its field's text stays selectable.
    fn tree_row(&self, f: &TreeFrame, at: Option<usize>, rf: &AsideFrame, cx: &mut Context<Self>) -> AnyElement {
        let worktree = f.kind == Kind::Worktree;
        let menu = if worktree {
            RowMenu::Tree { project: f.project.clone(), tree: f.key.clone() }
        } else {
            RowMenu::Local { project: f.project.clone(), local: f.key.clone() }
        };
        let key = f.key.as_str();
        let mark = match f.mark {
            RowMark::Failed => ui::indicator(id(format!("aside-failed:{key}")), Some(ui::State::Failed)),
            RowMark::Setting => Some(dot_spinner(id(format!("aside-setup:{key}")), 11., TEXT_4).into_any_element()),
            RowMark::Status => {
                ui::indicator(id(format!("aside-spin:{key}")), f.state).or_else(|| f.pr.map(|(name, color)| icon(name, 13., color).into_any_element()))
            }
        };
        let label = with_sub(self.name(f, rf, cx), f.sub.clone(), self.renaming(key));
        let open = rf.menu.as_ref().is_some_and(|m| m.menu == menu) || self.hover.on(key);
        let button = menu_button(key, menu.clone(), rf.menu.as_ref(), Self::send, cx);
        let trail = if worktree {
            let diff = f.diff.map(|(added, removed)| {
                ui::meta_diff(added, removed, 11.)
                    .id(id(format!("aside-diff:{key}")))
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::OpenChanges)))
                    .into_any_element()
            });
            Some(ui::row_trail(diff, vec![button], open))
        } else {
            f.menus.then(|| ui::row_trail(None, vec![button], open))
        };
        let glyph = if worktree {
            WORKTREE_ICON
        } else if hosts::split(&f.project).0.is_some() {
            "globe"
        } else {
            LOCAL_ICON
        };
        let (target, path, renames, kind) = (f.project.clone(), f.key.clone(), f.renames, f.kind);
        let row = ui::worktree_row(id(format!("aside-tree:{key}")), glyph, label, f.selected, mark)
            .children(trail)
            .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                // The rename input sits inside the row; its clicks must not restart or leave it.
                if this.renaming(&path) {
                    return;
                }
                if ev.click_count() > 1 && renames {
                    return cx.emit(SidebarEvent::Rename(path.clone()));
                }
                let tree = (kind != Kind::Own).then(|| path.clone());
                cx.emit(SidebarEvent::SelectTree { project: target.clone(), tree, progress: kind == Kind::Worktree });
            }))
            .when(f.sub.is_some(), |row| row.pl(px(6.)))
            .when(f.lit, |row| row.bg(FILL_3))
            .when(f.menus, |row| row.on_mouse_down(MouseButton::Right, open_on_right_click(menu, Self::send, cx)));
        let row = match at.filter(|_| f.sub.is_none() && !self.renaming(key)) {
            Some(ix) => self.holds(row, List::Trees(f.project.clone()), ix, f.key.clone(), true, cx),
            None => row,
        };
        Self::with_card(row, key, self.card_for(f, rf.reduce_motion), cx).into_any_element()
    }

    /// `p`'s block, item `at` of its section's.
    fn project_block(&self, p: &ProjectFrame, at: usize, rf: &AsideFrame, cx: &mut Context<Self>) -> (AnyElement, f32) {
        let (i, path) = (p.ix, p.path.as_str());
        let chevron = p.git.then(|| ui::chevron(("aside-chevron", i), p.open));
        let menu = RowMenu::Project(path.to_string());
        let mut buttons = Vec::new();
        if p.git {
            let target = path.to_string();
            buttons.push(
                icon_button_sized(id(format!("aside-plus:{path}")), "plus", 22., TEXT_3)
                    .rounded(px(6.))
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        cx.emit(SidebarEvent::NewPlace(target.clone()));
                    }))
                    .into_any_element(),
            );
        }
        buttons.push(menu_button(path, menu.clone(), rf.menu.as_ref(), Self::send, cx));
        let setup = p.setting_up.then(|| ui::busy(id(format!("aside-setup:{path}")), "Setting up…").into_any_element());
        let trail = ui::row_trail(setup, buttons, rf.menu.as_ref().is_some_and(|m| m.menu == menu) || self.hover.on_project(path));
        let (target, pointed, git) = (path.to_string(), path.to_string(), p.git);
        let row = ui::repo_row(("aside-repo", i), chevron, &p.name, &p.mark, p.selected, p.state)
            .child(trail)
            .when(p.offline, |row| row.opacity(0.5))
            .when(p.lit, |row| row.bg(FILL_3))
            .when(p.flashed, |row| row.bg(ACCENT_BG))
            .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                this.hover.point_project(&pointed, *over);
                cx.notify();
            }))
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                if git {
                    cx.emit(SidebarEvent::ToggleProject(target.clone()));
                } else {
                    cx.emit(SidebarEvent::SelectTree { project: target.clone(), tree: None, progress: false });
                }
            }))
            .on_mouse_down(MouseButton::Right, open_on_right_click(menu, Self::send, cx));
        let (section, manual) = (p.section, p.manual);
        let row = self.holds(row, List::Projects(section), at, path.to_string(), manual && section != Section::Pinned, cx);
        let trees = List::Trees(path.to_string());
        let out: Vec<AnyElement> = p.rows.iter().enumerate().map(|(ix, r)| self.placed(self.tree_row(r, Some(ix), rf, cx), &trees, ix)).collect();
        // The rows are fixed-height, so the open height is known without measuring: top pad, rows, gaps between them, bottom pad.
        let height = if out.is_empty() { 0. } else { 1. + fold::stacked(vec![ui::SIDEBAR_ROW_H; out.len()], 1.) + 6. };
        let rows = fold::springing(
            id(format!("aside-fold:{path}")),
            p.open,
            height,
            div().child(self.tracks(trees, div().id("rows")).flex().flex_col().gap(px(1.)).when(!out.is_empty(), |d| d.pt(px(1.)).pb(px(6.))).children(out)),
        );
        let to = path.to_string();
        let block = div().flex().flex_col().child(row).child(rows).on_drop(cx.listener(move |_, d: &DragRow, _, cx| {
            if moves_into(d, section) {
                cx.emit(SidebarEvent::DropProject { path: d.key.clone(), onto: to.clone(), section });
            }
        }));
        (self.placed(block.into_any_element(), &List::Projects(section), at), ui::SIDEBAR_ROW_H + if p.open { height } else { 0. })
    }

    /// Takes a project from another section dropped anywhere on `el` into `to`.
    fn drops_into<E: InteractiveElement + StatefulInteractiveElement>(el: E, to: Section, cx: &mut Context<Self>) -> E {
        el.drag_over::<DragRow>(move |s, d, _, _| if moves_into(d, to) { s.bg(FILL_2) } else { s }).on_drop(cx.listener(move |_, d: &DragRow, _, cx| {
            if moves_into(d, to) {
                cx.emit(SidebarEvent::DropInto { path: d.key.clone(), section: to });
            }
        }))
    }

    /// A section's rows under its head, springing to their height as it opens and shuts; `group_box`'s gap moves inside, so a shut section keeps its height.
    fn section_body(&self, id: impl Into<ElementId>, open: bool, rows: Vec<(AnyElement, f32)>, section: Section) -> impl IntoElement {
        let height = if rows.is_empty() { 0. } else { 2. + fold::stacked(rows.iter().map(|r| r.1), 2.) };
        let body = self.tracks(List::Projects(section), div().id("rows"));
        let body = body.flex().flex_col().gap(px(2.)).when(!rows.is_empty(), |d| d.pt(px(2.))).children(rows.into_iter().map(|r| r.0));
        fold::springing(id, open, height, div().child(body))
    }

    fn sections(&self, s: &SectionsFrame, rf: &AsideFrame, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        if let Some(pinned) = &s.pinned {
            let open = pinned.open;
            let head = ui::section_head("aside-pinned", icon("pin", 14., TEXT), "Pinned", open, None, false)
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::TogglePins)));
            let rows: Vec<(AnyElement, f32)> = pinned
                .items
                .iter()
                .enumerate()
                .map(|(at, item)| match item {
                    PinnedItem::Project(b) => self.project_block(b, at, rf, cx),
                    PinnedItem::Tree { key, row } => {
                        let row = row.as_ref().map(|r| self.tree_row(r, None, rf, cx));
                        let open = row.is_some();
                        let height = if open { ui::SIDEBAR_ROW_H } else { 0. };
                        (fold::springing(id(format!("aside-pin-fold:{key}")), open, ui::SIDEBAR_ROW_H, div().children(row)).into_any_element(), height)
                    }
                })
                .collect();
            let body = self.section_body("aside-pinned-fold", open, rows, Section::Pinned);
            out.push(Self::drops_into(ui::group_box().id("aside-pinned-box").gap_0().child(head).child(body), Section::Pinned, cx).into_any_element());
        }
        for g in &s.groups {
            let (gid, open) = (g.id, g.open);
            let name = match self.organize.rename.as_ref().filter(|r| r.group == gid) {
                Some(r) => Input::new(&r.input).appearance(false).p_0().text_size(px(14.)).into_any_element(),
                None => g.name.clone().into_any_element(),
            };
            let menu = RowMenu::Group(gid);
            let more = menu_button(&format!("group:{gid}"), menu.clone(), rf.menu.as_ref(), Self::send, cx);
            let head = ui::section_head(
                ("aside-group", gid as usize),
                div().children(g.icon.clone().map(emoji_mark)),
                name,
                open,
                Some(more),
                rf.menu.as_ref().is_some_and(|m| m.menu == menu),
            )
            .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                if this.organize.rename.as_ref().is_some_and(|r| r.group == gid) {
                    return;
                }
                cx.emit(SidebarEvent::ToggleGroup { id: gid, clicks: ev.click_count() });
            }))
            .on_mouse_down(MouseButton::Right, open_on_right_click(menu, Self::send, cx));
            let rows: Vec<(AnyElement, f32)> = match &g.body {
                GroupBody::Hidden => vec![],
                GroupBody::Hint => vec![(
                    div()
                        .h(px(ui::SIDEBAR_ROW_H))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .text_size(px(12.5))
                        .text_color(TEXT_4)
                        .child("Drag projects here")
                        .into_any_element(),
                    ui::SIDEBAR_ROW_H,
                )],
                GroupBody::Blocks(blocks) => blocks.iter().enumerate().map(|(at, b)| self.project_block(b, at, rf, cx)).collect(),
            };
            let body = self.section_body(("aside-group-fold", gid as usize), open, rows, Section::Group(gid));
            out.push(
                Self::drops_into(ui::group_box().id(("aside-group-box", gid as usize)).gap_0().child(head).child(body), Section::Group(gid), cx)
                    .into_any_element(),
            );
        }
        let blocks = match &s.tail {
            Tail::None => return out,
            Tail::NoMatch(text) => {
                out.push(
                    div()
                        .px(px(10.))
                        .py(px(12.))
                        .text_size(px(12.5))
                        .text_color(TEXT_3)
                        .child(format!("No projects or worktrees match “{text}”."))
                        .into_any_element(),
                );
                return out;
            }
            Tail::Other { blocks, headed } => (blocks, *headed),
        };
        let (blocks, headed) = blocks;
        let rows: Vec<AnyElement> = blocks.iter().enumerate().map(|(at, b)| self.project_block(b, at, rf, cx).0).collect();
        let other = div().id("aside-other").flex().flex_col().gap(px(1.)).when(headed && (!rows.is_empty() || cx.has_active_drag()), |d| {
            d.child(
                div()
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(TEXT_3)
                    .child("Other")
                    .child(ui::group_count(rows.len())),
            )
        });
        let rows = (!rows.is_empty()).then(|| self.tracks(List::Projects(Section::Other), div().id("rows")).flex().flex_col().gap(px(1.)).children(rows));
        out.push(Self::drops_into(other.children(rows), Section::Other, cx).into_any_element());
        out
    }

    fn scratch_section(&self, s: &ScratchFrame, rf: &AsideFrame, cx: &mut Context<Self>) -> Div {
        let open = s.open;
        let roll = (!open).then(|| ui::indicator(id("aside-scratch-roll".to_string()), s.roll)).flatten();
        let chevron = div().when(open, |d| d.invisible().group_hover(HEAD, |s| s.visible())).child(icon(
            if open { "chevron-down" } else { "chevron-right" },
            11.,
            TEXT_4,
        ));
        let toggle = div()
            .id(HEAD)
            .group(HEAD)
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .gap(px(4.))
            .cursor_pointer()
            .text_size(px(11.))
            .font_weight(FontWeight::BOLD)
            .text_color(TEXT_3)
            .child("THREADS")
            .child(chevron)
            .children(roll)
            .when(!open && s.count > 0, |d| d.child(ui::group_count(s.count)))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::ToggleScratch)));
        let new = icon_button_sized("aside-scratch-new", "plus", 22., TEXT_3)
            .rounded(px(6.))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::NewScratch)));
        let head = section_head().pl(px(6.)).child(toggle).child(new);
        let height = fold::stacked((0..s.count).map(|_| ui::SIDEBAR_ROW_H), 1.).min(ui::SIDEBAR_ROW_H * MAX_ROWS);
        let (mut folders, mut loose) = (Vec::new(), Vec::new());
        for r in &s.rows {
            match r {
                ScratchRow::Loose { path, title, folder, selected, state, was } => {
                    loose.push(self.loose_row(path, title, folder, *selected, *state, was.as_ref(), rf, cx))
                }
                ScratchRow::Folder { ix, path, name, selected, state, was, renames } => {
                    let row = self.scratch_row(*ix, path, name, *selected, *state, was.as_ref(), *renames, rf, cx);
                    folders.push(self.placed(row, &List::Threads, *ix));
                }
            }
        }
        // Folders come before loose threads, and only folders move.
        let folders = (!folders.is_empty()).then(|| self.tracks(List::Threads, div().id("folders")).flex().flex_col().gap(px(1.)).children(folders));
        let list = div()
            .id("aside-scratch-rows")
            .track_scroll(&self.reorder.threads)
            .max_h(px(height))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(1.))
            .children(folders)
            .children(loose);
        div().flex().flex_col().flex_none().child(head).child(fold::springing("aside-scratch-fold", open, height, div().child(list)))
    }

    #[expect(clippy::too_many_arguments, reason = "a view helper takes each part it draws")]
    fn loose_row(
        &self,
        path: &str,
        title: &str,
        folder: &str,
        selected: bool,
        state: Option<ui::State>,
        was: Option<&(String, Instant)>,
        rf: &AsideFrame,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mark = ui::indicator(id(format!("aside-spin:{path}")), state);
        let label = with_sub(done_name(row_name(was, path, title.to_string(), state, selected, rf.reduce_motion), state), Some(folder.to_string()), false);
        let key = path.to_string();
        ui::worktree_row(id(format!("aside-tree:{path}")), "terminal", label, selected, mark)
            .pl(px(6.))
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::SelectTree { project: key.clone(), tree: None, progress: false })))
            .into_any_element()
    }

    #[expect(clippy::too_many_arguments, reason = "a view helper takes each part it draws")]
    fn scratch_row(
        &self,
        ix: usize,
        path: &str,
        name: &str,
        selected: bool,
        state: Option<ui::State>,
        was: Option<&(String, Instant)>,
        renames: bool,
        rf: &AsideFrame,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mark = ui::indicator(id(format!("aside-spin:{path}")), state);
        let label = match self.rename.as_ref().filter(|r| r.tree == path) {
            Some(r) => Input::new(&r.input).appearance(false).p_0().text_size(px(13.)).into_any_element(),
            None => done_name(row_name(was, path, name.to_string(), state, selected, rf.reduce_motion), state),
        };
        let menu = RowMenu::Scratch(path.to_string());
        let open = rf.menu.as_ref().is_some_and(|m| m.menu == menu);
        let trail = ui::row_trail(None, vec![menu_button(path, menu.clone(), rf.menu.as_ref(), Self::send, cx)], open);
        let key = path.to_string();
        let row = ui::worktree_row(id(format!("aside-tree:{path}")), "folder", label, selected, mark)
            .pl(px(6.))
            .child(trail)
            .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                // The rename input sits inside the row; its clicks must not restart or leave it.
                if this.renaming(&key) {
                    return;
                }
                if ev.click_count() > 1 && renames {
                    return cx.emit(SidebarEvent::Rename(key.clone()));
                }
                cx.emit(SidebarEvent::SelectTree { project: key.clone(), tree: None, progress: false });
            }))
            .on_mouse_down(MouseButton::Right, open_on_right_click(menu, Self::send, cx));
        self.holds(row, List::Threads, ix, path.to_string(), true, cx).into_any_element()
    }

    fn offline_card(&self, o: &OfflineFrame, cx: &mut Context<Self>) -> AnyElement {
        let reconnect = o.reconnect.clone().map(|target| {
            div().flex().child(
                ui::button("space-offline-reconnect", ui::Variant::Secondary, Some("reload"), "Reconnect")
                    .h(px(26.))
                    .text_size(px(12.5))
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::Reconnect(target.clone())))),
            )
        });
        div()
            .mt(px(4.))
            .mb(px(10.))
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(px(10.))
            .bg(FILL_2)
            .text_size(px(12.))
            .text_color(TEXT_3)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(TEXT)
                    .child(icon(o.glyph, 13., TEXT_3))
                    .child(o.title.clone()),
            )
            .child(o.note.clone())
            .children(reconnect)
            .into_any_element()
    }

    fn empty_space(&self, name: &str, cx: &mut Context<Self>) -> AnyElement {
        let open = ui::button("space-empty-open", ui::Variant::Secondary, None, "Open folder\u{2026}")
            .h(px(26.))
            .text_size(px(12.5))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::OpenFolder)));
        let clone = ui::button("space-empty-clone", ui::Variant::Secondary, None, "Clone\u{2026}")
            .h(px(26.))
            .text_size(px(12.5))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::CloneProject)));
        div()
            .mx(px(8.))
            .my(px(20.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .text_center()
            .text_size(px(12.5))
            .text_color(TEXT_3)
            .child(icon("folder", 22., TEXT_4))
            .child(div().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(format!("No projects on {name} yet")))
            .child("Open a folder that's already there, or clone a repository onto it.")
            .child(div().flex().gap(px(6.)).child(open).child(clone))
            .into_any_element()
    }

    fn update_card(&self, update: &Update, cx: &mut Context<Self>) -> Option<AnyElement> {
        match update {
            Update::Idle => None,
            Update::Ready { version } => Some(
                ui::button("aside-restart-update", ui::Variant::Accent, Some("reload"), format!("Restart to update · v{version}"))
                    .mt(px(8.))
                    .mx(px(2.))
                    .h(px(34.))
                    .rounded(px(17.))
                    .justify_center()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::RestartToUpdate)))
                    .into_any_element(),
            ),
            Update::WhatsNew { version } => {
                let url = release_url(version);
                let dismiss = div()
                    .id("aside-whats-new-dismiss")
                    .size(px(20.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .hover(|s| s.bg(FILL_3))
                    .child(icon("x", 12., TEXT_3))
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        cx.emit(SidebarEvent::DismissWhatsNew);
                    }));
                Some(
                    div()
                        .id("aside-whats-new")
                        .mt(px(8.))
                        .mx(px(2.))
                        .py(px(8.))
                        .pl(px(10.))
                        .pr(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded(px(12.))
                        .bg(SIDE_NOTICE)
                        .shadow(vec![ui::ring(FILL_3, 0.5)])
                        .text_size(px(12.))
                        .text_color(TEXT)
                        .cursor_pointer()
                        .child(icon("sparkle", 12., TEXT_2))
                        .child(div().flex_1().min_w_0().truncate().child(format!("What's new in {version}")))
                        .child(dismiss)
                        .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| cx.open_url(&url))
                        .into_any_element(),
                )
            }
        }
    }

    /// The aside's foot: the space buttons, add a host, and Settings at the right.
    fn space_switcher(&self, cx: &mut Context<Self>) -> Div {
        let add = crate::sidebar::rail::rail_button("aside-add-host", "plus", false)
            .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new("Add a host").build(window, cx))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::AddHost)));
        let settings = crate::sidebar::rail::rail_button("aside-settings", "settings", false)
            .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new("Settings").build(window, cx))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::Settings)));
        div()
            .mt(px(6.))
            .mx(px(-8.))
            .px(px(8.))
            .pt(px(6.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.))
            .border_t(px(0.5))
            .border_color(SEPARATOR)
            .child(
                // With many hosts the buttons scroll, so Add and Settings stay in reach.
                div().id("aside-spaces").flex().items_center().gap(px(2.)).min_w_0().p(px(1.)).overflow_x_scroll().child(self.aside_spaces.clone()),
            )
            .child(add)
            .child(div().flex_1())
            .child(settings)
    }

    fn resizable(&self, d: Div, seam: bool, cx: &mut Context<Self>) -> Div {
        if !seam {
            return d;
        }
        let col = Column::Projects;
        let handle = seam_handle(("resize-column", col as usize), Axis::Row, move |_: &mut Self, cx| cx.emit(SidebarEvent::ResetWidth(col)), cx)
            .right(px(-crate::desktop::chrome::SEAM / 2.))
            .on_drag(col, |_, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| EmptyView)
            });
        // Deferred so the handle straddles the edge: `column()` clips its children.
        d.relative().child(deferred(handle)).on_drag_move(cx.listener(move |_, e: &DragMoveEvent<Column>, _, cx| {
            if *e.drag(cx) == col {
                cx.emit(SidebarEvent::Drag(col, f32::from(e.event.position.x - e.bounds.left())));
            }
        }))
    }
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.slide_rows(window, cx);
        let root = div().size_full().flex().flex_col();
        let Some(f) = self.frame.as_ref() else { return root };
        let top = drag_area(div())
            .h(px(42.))
            .flex()
            .flex_none()
            .items_center()
            .justify_end()
            .child(
                div()
                    .relative()
                    .child(ui::popover_trigger(
                        icon_button_sized("aside-bell", "bell", 28., if f.inbox.is_some() { TEXT } else { TEXT_3 })
                            .when(f.inbox.is_some(), |d| d.bg(FILL_3))
                            .when(f.unseen > 0, |d| d.child(ui::count_badge(f.unseen).top(px(-3.)).right(px(-3.)))),
                        f.inbox.is_some(),
                        cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(SidebarEvent::Bell)),
                    ))
                    .children(f.inbox.clone().map(|inbox| hung(34., -6., inbox))),
            )
            .child(
                icon_button_sized("aside-collapse", "sidebar-collapse", 28., TEXT_3)
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::Collapse))),
            );
        let search =
            ui::trigger_field("aside-search", "search", "Search all machines", &crate::settings::hint(&crate::actions::OpenPalette, cx).unwrap_or_default())
                .mx(px(4.))
                .mb(px(6.))
                .h(px(32.))
                .rounded(px(10.))
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::Palette)));
        let pages: Vec<_> = f
            .pages
            .iter()
            .map(|&(name, icon, screen)| {
                ui::nav_row(SharedString::from(format!("aside-{}", name.to_lowercase())), icon, name, f.screen == screen)
                    .mb(px(2.))
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::Page(screen))))
            })
            .collect();
        let sections = match &f.body {
            Body::Empty(name) => name.iter().map(|n| self.empty_space(n, cx)).collect(),
            Body::Sections(s) => self.sections(s, f, cx),
        };
        let list = div().flex().flex_col().gap(px(1.)).children(f.offline.as_ref().map(|o| self.offline_card(o, cx))).children(sections);
        let list = match f.slide {
            Some((n, heading)) => list
                .with_animation(
                    ("aside-space-slide", n as usize),
                    Animation::new(Duration::from_millis(SLIDE_MS)).with_easing(ease_out_quint()),
                    move |d, t| {
                        let off = 24. * heading.sign() * (1. - t);
                        d.opacity(t).ml(px(off)).mr(px(-off))
                    },
                )
                .into_any_element(),
            None => list.into_any_element(),
        };
        let body = div().id("aside-repos").track_scroll(&self.reorder.projects).flex_1().min_h_0().overflow_y_scroll().flex().flex_col().child(list);
        let scratch = f.scratch.as_ref().map(|s| self.scratch_section(s, f, cx));
        let aside = ui::side(div())
            .bg(nav_fill())
            .w(px(f.width))
            .flex_none()
            .h_full()
            .px(px(8.))
            .pb(px(10.))
            .flex()
            .flex_col()
            .child(top)
            .child(search)
            .children(pages)
            .child(self.space_header.clone())
            .children(scratch)
            .child(self.header.clone())
            .child(body)
            .children(self.update_card(&f.update, cx).or_else(|| f.star.clone().map(|s| s.into_any_element())))
            .child(self.space_switcher(cx))
            .on_scroll(cx.listener(|_, e: &ScrollEvent, _, cx| cx.emit(SidebarEvent::Swipe(e.clone()))))
            .on_drag_move(cx.listener(|this, e: &DragMoveEvent<DragRow>, _, cx| this.move_row(e, cx)))
            .capture_any_mouse_up(cx.listener(|this, e: &MouseUpEvent, _, cx| {
                if e.button == MouseButton::Left {
                    this.release_row(cx);
                }
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| this.release_row(cx)));
        root.child(self.resizable(aside, f.seam, cx))
    }
}

/// `d` carries a project from another section than `to`.
fn moves_into(d: &DragRow, to: Section) -> bool {
    matches!(d.list, List::Projects(from) if from != to)
}
