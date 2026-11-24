use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, RowMenu, Screen, id};
use crate::modals::form::home;
use crate::removal::dispose;
use crate::sidebar::fold::{self, Fold};
use crate::sidebar::sections::{matches, section_head};
use crate::sidebar::{done_name, rolled_state, row_name, with_sub};
use crate::spaces::space_of;
use crate::status::Card;
use crate::util::{now_ms, tilde};
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
    /// The Threads section above Projects; `None` when the sidebar's filter leaves nothing in it.
    pub(super) fn scratch_section(&self, cx: &mut Context<Self>) -> Option<Div> {
        let query = self.sidebar.filter.query(cx);
        let filtering = !query.is_empty();
        let space = self.space();
        let rows: Vec<String> = self.store.scratches().into_iter().filter(|p| space_of(p) == space && (!filtering || matches(&self.place_name(p), &query))).collect();
        let loose: Vec<String> = self.loose_folders(&space).into_iter().filter(|p| !filtering || matches(&thread_title(&self.cards(p)), &query) || matches(p, &query)).collect();
        let rows: Vec<String> = rows.into_iter().chain(loose.iter().cloned()).collect();
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
            .child("THREADS")
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
        let current = self.place().filter(|_| self.screen == Screen::Sessions && self.project.as_deref().is_some_and(|p| rows.iter().any(|r| r == p)));
        let drawn: Vec<AnyElement> = if self.sidebar.closing.drawn(&Fold::Scratch, open) {
            rows.iter().enumerate().map(|(ix, p)| if loose.contains(p) { self.loose_row(p, current.as_deref(), cx) } else { self.scratch_row(ix, p, current.as_deref(), cx) }).collect()
        } else {
            vec![]
        };
        let list = div().id("aside-scratch-rows").max_h(px(height)).overflow_y_scroll().flex().flex_col().gap(px(1.)).children(drawn);
        Some(div().flex().flex_col().flex_none().child(head).child(fold::springing("aside-scratch-fold", open, height, div().child(list))))
    }

    fn loose_row(&self, path: &str, current: Option<&str>, cx: &mut Context<Self>) -> AnyElement {
        let cards = self.cards(path);
        let (selected, state) = (current == Some(path), rolled_state(&cards));
        let mark = ui::indicator(id(format!("aside-spin:{path}")), state);
        let label = with_sub(done_name(row_name(&self.renames, path, thread_title(&cards), state, selected, cx.reduce_motion()), state), Some(short_folder(&tilde(path))), false);
        let key = path.to_string();
        ui::worktree_row(id(format!("aside-tree:{path}")), "terminal", label, selected, mark)
            .pl(px(6.))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_tree(key.clone(), None, cx)))
            .into_any_element()
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
        ui::worktree_row(id(format!("aside-tree:{path}")), "folder", label, selected, mark)
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

    /// Makes a Scratch folder on the space on screen and returns its key.
    pub(crate) fn make_scratch(&self, cx: &mut Context<Self>) -> Task<Result<String, String>> {
        let seed = now_ms() as usize;
        let Some(id) = self.space().host().map(str::to_string) else {
            let root = project::scratch::root(&home());
            return cx.background_executor().spawn(async move { project::scratch::create(&root, seed) });
        };
        cx.background_executor().spawn(async move {
            let host = hosts::remote::connected(&id).ok_or_else(|| git::NotConnected::error().to_string())?;
            let root = project::scratch::root(&host.home().map_err(|e| e.to_string())?);
            let path = project::scratch::create_on(&git::Runner::Remote(host.clone()), &root, seed, |p, parents| host.mkdir(p, parents), |p| _ = host.remove(p, true))?;
            Ok(host.key(&path))
        })
    }

    pub(crate) fn adopt_scratch(&mut self, path: &str) {
        self.store.add_scratch(path);
        self.store.scratch_folded = false;
        self.store.save();
        // pocketd starts sessions only in folders it lists.
        self.publish_registry();
        // Its row needs this now; `refresh_git` lists it two seconds from now.
        self.worktrees.insert(path.to_string(), vec![git::Worktree { path: path.to_string(), branch: git::INIT_BRANCH.into(), main: true, made: false, created: None }]);
    }

    /// Deletes a folder with nothing to lose straight away, and asks about any other.
    pub(super) fn ask_delete_scratch(&mut self, path: String, cx: &mut Context<Self>) {
        if let Some(host) = hosts::split(&path).0.filter(|h| hosts::remote::connected(h).is_none()) {
            self.error = Some(format!("{} isn't connected", self.host_name(host)));
            return cx.notify();
        }
        let job = path.clone();
        let task = cx.background_executor().spawn(async move { project::scratch::work(&hosts::runner_for(&job), &job) });
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
        let trash = self.store.worktree.trash;
        self.remove_project(path, cx);
        self.workspaces.remove(path);
        self.store.forget_scratch(path);
        self.store.save();
        let folder = path.to_string();
        cx.background_executor().spawn(async move { dispose_scratch(&folder, trash) }).detach();
        cx.notify();
    }
}

/// Removes the Scratch folder `key` from the machine holding it.
/// Only one directly in that machine's Scratch root goes: desktop.json can be edited by hand, and this removes whatever path it names.
fn dispose_scratch(key: &str, trash: bool) {
    match hosts::split(key) {
        (None, path) => {
            if project::scratch::holds(&project::scratch::root(&home()), path) {
                dispose(Path::new(path), trash);
            }
        }
        (Some(id), path) => {
            let Some(host) = hosts::remote::connected(id) else { return };
            let Ok(home) = host.home() else { return };
            if project::scratch::holds(&project::scratch::root(&home), path) && !(trash && host.trash(path).is_ok()) {
                _ = host.remove(path, true);
            }
        }
    }
}

/// `~/Downloads` stays whole; a deeper folder keeps only its own name, so the title beside it isn't squeezed out.
fn short_folder(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((parent, name)) if !matches!(parent, "~" | "") => format!("…/{name}"),
        _ => path.to_string(),
    }
}

fn thread_title(cards: &[Card]) -> String {
    cards.iter().max_by_key(|c| c.at).map(|c| c.title.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| "Terminal".into())
}

#[cfg(test)]
mod tests {
    use super::{short_folder, thread_title};
    use crate::status::Card;

    fn card(title: &str, at: i64) -> Card {
        Card { title: title.into(), at, ..crate::status::card(&agents::Summary::default(), "/w") }
    }

    #[test]
    fn a_thread_is_named_by_its_newest_session_else_as_a_terminal() {
        assert_eq!(thread_title(&[card("Old", 1), card("Fix login redirect", 9)]), "Fix login redirect");
        assert_eq!(thread_title(&[card("", 9)]), "Terminal");
        assert_eq!(thread_title(&[]), "Terminal");
    }

    #[test]
    fn a_thread_shows_a_folder_under_home_whole_and_a_deeper_one_by_its_name() {
        assert_eq!(short_folder("~/Downloads"), "~/Downloads");
        assert_eq!(short_folder("~/code/clients/acme/api"), "…/api");
        assert_eq!(short_folder("/tmp/x"), "…/x");
        assert_eq!(short_folder("/opt"), "/opt");
    }
}
