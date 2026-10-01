use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use gpui_kit::*;
use std::path::Path;
use ui::{self, menu_row};
use workspace::Doc;

impl Desktop {
    pub(super) fn more_menu(&mut self, cx: &mut Context<Self>) -> Div {
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
                    this.close_overlay(window, cx);
                    if let Some(id) = this.session.take() {
                        this.close_session(&id, cx);
                    }
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
}
