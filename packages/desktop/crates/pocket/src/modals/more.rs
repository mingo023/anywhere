use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::region::notify_after_draw;
use gpui_kit::*;
use std::path::Path;
use ui::{self, menu_row};
use workspace::Doc;

#[derive(Default)]
pub(crate) struct MoreMenu {
    at: Point<Pixels>,
    path: Option<String>,
}

pub(crate) enum MoreEvent {
    Done,
}

impl EventEmitter<MoreEvent> for MoreMenu {}

impl MoreMenu {
    pub(crate) fn open_at(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        if self.at != at {
            self.at = at;
            cx.notify();
        }
    }

    pub(crate) fn show(&mut self, path: Option<String>, cx: &mut Context<Self>) {
        if self.path != path {
            self.path = path;
            notify_after_draw(cx);
        }
    }
}

impl Render for MoreMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let menu = ui::pop(div().absolute().left(self.at.x - px(216.)).top(self.at.y + px(18.)).w(px(230.)).p(px(6.)).flex().flex_col()).occlude();
        let Some(path) = self.path.clone() else { return menu };
        let reveal = path.clone();
        menu.child(menu_row("more-open", "external", "Open in editor", None).on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
            cx.open_with_system(Path::new(&path));
            cx.emit(MoreEvent::Done);
        })))
        .child(menu_row("more-reveal", "folder", "Reveal in Finder", None).on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
            cx.reveal_path(Path::new(&reveal));
            cx.emit(MoreEvent::Done);
        })))
    }
}

impl Desktop {
    pub(super) fn more_view(&mut self, cx: &mut Context<Self>) -> Div {
        let path = match self.active_doc(cx) {
            Some(Doc::File(p)) => Some(p),
            Some(Doc::Diff(p) | Doc::CommitFile { path: p, .. } | Doc::PrFile { path: p, .. } | Doc::CompareFile { path: p, .. }) => {
                self.cwd(cx).map(|cwd| format!("{cwd}/{p}"))
            }
            Some(Doc::Commit(_)) | None => None,
        };
        let more = self.panels.read(cx).more.clone();
        more.update(cx, |m, cx| m.show(path.filter(|_| self.screen == Screen::Sessions), cx));
        div().absolute().inset_0().child(more)
    }

    pub(crate) fn on_more_event(&mut self, _: &Entity<MoreMenu>, ev: &MoreEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            MoreEvent::Done => self.close_overlay(window, cx),
        }
    }
}
