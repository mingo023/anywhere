use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::Path;
use ui::{self, menu_row};
use workspace::{Doc, Place};

impl Desktop {
    pub(super) fn more_menu(&mut self, cx: &mut Context<Self>) -> Div {
        let at = self.panels.more_at.unwrap_or_default();
        let menu = ui::pop(div().absolute().left(at.x - px(216.)).top(at.y + px(18.)).w(px(230.)).p(px(6.)).flex().flex_col()).occlude();
        let path = match self.active_doc() {
            Some(Doc::File(p)) => Some(p),
            Some(Doc::Diff(p) | Doc::CommitFile { path: p, .. }) => self.cwd().map(|cwd| format!("{cwd}/{p}")),
            Some(Doc::Commit(_)) | None => None,
        };
        let Some(path) = path.filter(|_| self.screen == Screen::Sessions) else {
            return menu
                .child(menu_row("more-tab", "terminal", "New terminal tab", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.new_shell(Place::Pane(None), cx);
                    this.close_overlay(window, cx);
                })))
                .when(!self.agents.observe_only(), |d| {
                    d.child(menu_row("more-close", "x", "Close session", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.close_overlay(window, cx);
                        if let Some(id) = this.session.take() {
                            this.close_session(&id, cx);
                        }
                    })))
                });
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
}
