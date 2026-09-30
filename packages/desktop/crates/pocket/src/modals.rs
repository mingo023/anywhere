pub(crate) mod add_project;
pub(crate) mod form;
pub(crate) mod new_session;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, Screen};
use crate::util::{basename, tilde};
use gpui_kit::*;
use std::path::Path;
use std::time::Duration;
use theme::*;
use ui::{self, Variant, menu_row};
use workspace::Doc;

fn closes(n: usize) -> Option<String> {
    match n {
        0 => None,
        1 => Some("Closes 1 terminal".into()),
        n => Some(format!("Closes {n} terminals")),
    }
}

impl Desktop {
    pub fn open(&mut self, o: Overlay, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.overlay = Some(o);
        match o {
            Overlay::Palette => self.open_palette(window, cx),
            Overlay::NewSession => self.reset_new_form(None, false, window, cx),
            Overlay::AddRepo => self.reset_repo_form(None, window, cx),
            Overlay::More | Overlay::Confirm => {}
        }
        cx.notify();
    }

    pub fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = None;
        window.focus(&self.root, cx);
        cx.notify();
    }

    fn more_menu(&mut self, cx: &mut Context<Self>) -> Div {
        let menu = ui::pop(div().absolute().right(px(22.)).top(px(58.)).w(px(230.)).p(px(6.)).rounded(px(16.)).flex().flex_col()).occlude();
        let path = match self.active_doc() {
            Some(Doc::File(p)) => Some(p),
            Some(Doc::Diff(p)) => self.cwd().map(|cwd| format!("{cwd}/{p}")),
            None => None,
        };
        let Some(path) = path.filter(|_| self.screen == Screen::Sessions) else {
            return menu
                .child(menu_row("more-tab", "terminal", "New terminal tab", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.new_shell(None, cx);
                    this.close_overlay(window, cx);
                })))
                .child(menu_row("more-close", "x", "Close session", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    if let Some(id) = this.session.take() {
                        this.close_session(&id, cx);
                    }
                    this.close_overlay(window, cx);
                })));
        };
        let reveal = path.clone();
        menu.child(menu_row("more-open", "external", "Open in editor", None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            cx.open_with_system(Path::new(&path));
            this.close_overlay(window, cx);
        })))
        .child(menu_row("more-reveal", "folder", "Reveal in Finder", None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            cx.reveal_path(Path::new(&reveal));
            this.close_overlay(window, cx);
        })))
    }

    fn confirm_view(&mut self, cx: &mut Context<Self>) -> Div {
        let (title, action, facts, dirty): (String, &str, Vec<String>, usize) = match &self.confirm {
            Some(Confirm::RemoveProject(p)) => (
                format!("Remove {}?", self.repo_name(p)),
                "Remove",
                closes(self.project_terminals(p).len()).into_iter().chain(["The repository stays on disk".to_string()]).collect(),
                0,
            ),
            Some(Confirm::DeleteWorktree { tree, branch, dirty, .. }) => (
                format!("Delete {}?", basename(tree)),
                "Delete",
                closes(self.tree_terminals(tree).len()).into_iter().chain([format!("Deletes the folder {}", tilde(tree)), format!("Keeps the branch {branch}")]).collect(),
                *dirty,
            ),
            Some(Confirm::Discard(paths)) => {
                let untracked = self.repo().map_or(0, |r| r.files.iter().filter(|f| f.status == 'A' && !f.staged && paths.contains(&f.path)).count());
                let title = match paths.as_slice() {
                    [one] => format!("Discard changes to {}?", basename(one)),
                    many => format!("Discard changes to {} files?", many.len()),
                };
                let deletes = (untracked > 0).then(|| format!("Deletes {untracked} untracked {}", if untracked == 1 { "file" } else { "files" }));
                (title, "Discard", std::iter::once("Unstaged edits can't be restored".to_string()).chain(deletes).collect(), 0)
            }
            None => return div(),
        };
        let bullet = |text: String| div().flex().gap(px(8.)).text_size(px(13.5)).text_color(rgba(TEXT_2)).child("•").child(text);
        let mut body = vec![div().flex().flex_col().gap(px(6.)).children(facts.into_iter().map(bullet)).into_any_element()];
        if dirty > 0 {
            let files = if dirty == 1 { "file" } else { "files" };
            body.push(
                div().px(px(12.)).py(px(10.)).rounded(px(10.)).bg(rgba(FAILED_BG)).text_size(px(13.)).text_color(rgba(FAILED)).child(format!("{dirty} uncommitted {files} will be lost.")).into_any_element(),
            );
        }
        let cancel = ui::large(ui::button("confirm-cancel", Variant::Ghost, None, "Cancel").text_color(rgba(TEXT)))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let submit = ui::large(ui::button("confirm-go", Variant::Danger, None, action)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.confirmed(window, cx)));
        body.push(crate::modals::form::footer("", cancel, submit).into_any_element());
        let close = ui::icon_button("confirm-close", "x").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        ui::modal(&title, 440., 160., close, body)
    }

    fn confirmed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.confirm.take() {
            Some(Confirm::RemoveProject(p)) => self.remove_project(&p, cx),
            Some(Confirm::DeleteWorktree { project, tree, .. }) => self.delete_worktree(project, tree, cx),
            Some(Confirm::Discard(paths)) => self.discard(paths, cx),
            None => {}
        }
        self.close_overlay(window, cx);
    }

    pub fn overlay_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let o = self.overlay?;
        // Entrance as (ms, rise). The palette and new-session sheet open from shortcuts many times a day; motion would only slow them.
        let (body, alpha, entrance) = match o {
            Overlay::Palette => (self.palette(cx), 0x1f, None),
            Overlay::NewSession => (self.new_session_view(window, cx), 0x2e, None),
            Overlay::AddRepo => (self.repo_view(window, cx), 0x40, Some((200, 8.))),
            Overlay::More => (self.more_menu(cx), 0, Some((150, -4.))),
            Overlay::Confirm => (self.confirm_view(cx), 0x2e, Some((200, 8.))),
        };
        let backdrop = ui::backdrop("backdrop", alpha).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let layer = div().absolute().inset_0();
        let Some((ms, rise)) = entrance else {
            return Some(layer.child(backdrop).child(body).into_any_element());
        };
        let enter = Animation::new(Duration::from_millis(ms)).with_easing(ease_out_quint());
        Some(
            layer
                .child(backdrop.with_animation("backdrop-in", enter.clone(), |d, t| d.opacity(t)))
                .child(body.with_animation("overlay-in", enter, move |d, t| d.opacity(t).mt(px(rise * (1. - t)))))
                .into_any_element(),
        )
    }
}
