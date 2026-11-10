use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, RowMenu, Screen, id};
use crate::modals::form::home;
use crate::removal::dispose;
use crate::sidebar::fold::{self, Fold};
use crate::sidebar::sections::{matches, section_head};
use crate::sidebar::{done_name, rolled_state, row_name};
use crate::util::now_ms;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::Path;
use theme::*;
use ui::{self, icon_button_sized};

const HEAD: &str = "aside-scratch-head";
/// Past this the section scrolls, so a long list can't squeeze Projects out.
const MAX_ROWS: f32 = 8.5;

#[derive(Clone)]
struct DragScratch {
    path: String,
    ix: usize,
}

impl Desktop {
    /// The Sessions section above Projects; `None` when the sidebar's filter leaves nothing in it.
    pub(super) fn scratch_section(&self, cx: &mut Context<Self>) -> Option<Div> {
        let query = self.sidebar.filter.query(cx);
        let filtering = !query.is_empty();
        let rows: Vec<String> = self.store.scratches().into_iter().filter(|p| !filtering || matches(&self.place_name(p), &query)).collect();
        if filtering && rows.is_empty() {
            return None;
        }
        let open = filtering || !self.store.scratch_folded;
        let roll = (!open).then(|| ui::indicator(id("aside-scratch-roll".to_string()), rolled_state(&rows.iter().flat_map(|p| self.cards(p)).collect::<Vec<_>>()))).flatten();
        let chevron = div().when(open, |d| d.invisible().group_hover(HEAD, |s| s.visible())).child(icon(if open { "chevron-down" } else { "chevron-right" }, 11., TEXT_4));
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
            .child("SESSIONS")
            .child(chevron)
            .children(roll)
            .when(!open && !rows.is_empty(), |d| d.child(ui::group_count(rows.len())))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.store.scratch_folded = !this.store.scratch_folded;
                if this.store.scratch_folded {
                    this.sidebar.closing.start(Fold::Scratch);
                }
                this.store.save();
                cx.notify();
            }));
        let new = icon_button_sized("aside-scratch-new", "plus", 22., TEXT_3).rounded(px(6.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.new_session_without_project(window, cx)));
        let head = section_head().pl(px(6.)).child(toggle).child(new);
        let height = fold::stacked(rows.iter().map(|_| ui::SIDEBAR_ROW_H), 1.).min(ui::SIDEBAR_ROW_H * MAX_ROWS);
        let current = self.place().filter(|_| self.screen == Screen::Sessions && self.project.as_deref().is_some_and(|p| self.store.is_scratch(p)));
        let drawn: Vec<AnyElement> = if self.sidebar.closing.drawn(&Fold::Scratch, open) { rows.iter().enumerate().map(|(ix, p)| self.scratch_row(ix, p, current.as_deref(), cx)).collect() } else { vec![] };
        let list = div().id("aside-scratch-rows").max_h(px(height)).overflow_y_scroll().flex().flex_col().gap(px(1.)).children(drawn);
        Some(div().flex().flex_col().flex_none().child(head).child(fold::springing("aside-scratch-fold", open, height, div().child(list))))
    }

    fn scratch_row(&self, ix: usize, path: &str, current: Option<&str>, cx: &mut Context<Self>) -> AnyElement {
        let name = self.place_name(path);
        let selected = current == Some(path);
        let state = rolled_state(&self.cards(path));
        let mark = ui::indicator(id(format!("aside-spin:{path}")), state);
        let label = match self.sidebar.rename.as_ref().filter(|r| r.tree == path) {
            Some(r) => Input::new(&r.input).appearance(false).p_0().text_size(px(13.)).into_any_element(),
            None => done_name(row_name(&self.renames, path, name, state, selected, cx.reduce_motion()), state),
        };
        let menu = RowMenu::Scratch(path.to_string());
        let trail = ui::row_trail(None, vec![self.row_menu_button(path, menu.clone(), cx)], self.row_menu.as_ref() == Some(&menu));
        let (key, to) = (path.to_string(), path.to_string());
        ui::worktree_row(id(format!("aside-tree:{path}")), "folder", label, selected, mark, None)
            .pl(px(6.))
            .child(trail)
            .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                // The rename input sits inside the row; its clicks must not restart or leave it.
                if this.sidebar.renaming(&key) {
                    return;
                }
                if ev.click_count() > 1 && this.agents.names_offered() {
                    return this.start_rename(key.clone(), window, cx);
                }
                this.select_tree(key.clone(), None, cx);
            }))
            .on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx))
            .on_drag(DragScratch { path: path.to_string(), ix }, |_, _, _, cx| cx.new(|_| EmptyView))
            .drag_over::<DragScratch>(move |s, d, _, _| if d.ix == ix { s } else { s.shadow(ui::drop_line(d.ix < ix)) })
            .on_drop(cx.listener(move |this, d: &DragScratch, _, cx| {
                this.store.move_project(&d.path, &to);
                this.store.save();
                cx.notify();
            }))
            .into_any_element()
    }

    pub(crate) fn make_scratch(&self, cx: &mut Context<Self>) -> Task<Result<String, String>> {
        let root = project::scratch::root(&home());
        let seed = now_ms() as usize;
        cx.background_executor().spawn(async move { project::scratch::create(&root, seed) })
    }

    pub(crate) fn adopt_scratch(&mut self, path: &str) {
        self.store.add_scratch(path);
        self.store.scratch_folded = false;
        // pocketd reads this file to know the folder before the first session starts in it.
        self.store.save();
        // Its row needs this now; `refresh_git` lists it two seconds from now.
        self.worktrees.insert(path.to_string(), vec![git::Worktree { path: path.to_string(), branch: git::INIT_BRANCH.into(), main: true }]);
    }

    /// Deletes a folder with nothing to lose straight away, and asks about any other.
    pub(super) fn ask_delete_scratch(&mut self, path: String, cx: &mut Context<Self>) {
        let job = path.clone();
        let task = cx.background_executor().spawn(async move { project::scratch::work(&job) });
        cx.spawn(async move |this, cx| {
            let work = task.await;
            this.update(cx, |d, cx| {
                if project::scratch::nothing_to_lose(work, d.tree_terminals(&path).len()) {
                    return d.delete_scratch(&path, cx);
                }
                let (dirty, lost) = work.unwrap_or_default();
                d.confirm = Some(Confirm::DeleteScratch { path, dirty, lost });
                d.overlay = Some(Overlay::Confirm);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn delete_scratch(&mut self, path: &str, cx: &mut Context<Self>) {
        let root = project::scratch::root(&home());
        let trash = self.store.worktree.trash;
        self.remove_project(path, cx);
        self.workspaces.remove(path);
        self.store.forget_scratch(path);
        self.store.save();
        // desktop.json can be edited by hand, and this removes whatever path it names.
        if project::scratch::holds(&root, path) {
            let folder = path.to_string();
            cx.background_executor().spawn(async move { dispose(Path::new(&folder), trash) }).detach();
        }
        cx.notify();
    }
}
