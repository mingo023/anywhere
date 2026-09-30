use super::{language, size};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, doc_bar};
use crate::explorer::status_word;
use crate::util::{ago_long, now_ms};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::Path;
use std::time::Duration;
use theme::*;
use ui::{self, Segment, Variant, dot};

/// `path` relative to `root`, or whole when it lies outside.
pub fn relative(path: &str, root: &str) -> String {
    path.strip_prefix(root).map(|r| r.trim_start_matches('/').to_string()).unwrap_or_else(|| path.to_string())
}

impl Desktop {
    fn copy_path(&mut self, path: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(path.to_string()));
        let reset = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            this.update(cx, |this, cx| {
                this.preview.path_copied = None;
                cx.notify();
            })
            .ok();
        });
        self.preview.path_copied = Some((path.to_string(), reset));
        cx.notify();
    }

    /// The bar over a file: its breadcrumbs, language, size and git status, and what can be done with it.
    pub(super) fn file_header(&mut self, path: &str, cx: &mut Context<Self>) -> Div {
        let root = self.explore_root().unwrap_or_default();
        let rel = relative(path, &root);
        let project = self.project.as_deref().map(|p| self.repo_name(p)).unwrap_or_default();
        let crumbs = std::iter::once(project).chain(rel.split('/').map(str::to_string)).collect();
        let prompt = format!("About {rel}: ");
        let copy = rel.clone();
        let opened = path.to_string();
        let text = self.preview.text();
        let right = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .when(self.preview.markdown(), |d| {
                d.child(div().id("md-mode").child(ui::segmented(
                    vec![Segment { icon: None, value: false, label: "Preview".into(), badge: None }, Segment { icon: None, value: true, label: "Source".into(), badge: None }],
                    self.preview.md_source,
                    true,
                    false,
                    |this, v, cx| {
                        this.preview.md_source = v;
                        cx.notify();
                    },
                    cx,
                )))
            })
            .child(
                ui::button("ask-file", Variant::Ghost, Some("sparkle"), "Ask about this file").text_color(TEXT).on_click(cx.listener(
                    move |this, _: &ClickEvent, window, cx| {
                        this.open(Overlay::NewSession, window, cx);
                        this.reset_new_form(Some(prompt.clone()), false, window, cx);
                    },
                )),
            )
            .child(ui::icon_group([
                ui::group_button("open-editor", "external").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_with_system(Path::new(&opened)))),
                match self.preview.path_copied.as_ref().is_some_and(|(p, _)| *p == copy) {
                    true => ui::group_button_swapped("copy-path", "check"),
                    false => ui::group_button("copy-path", "copy"),
                }
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.copy_path(&copy, cx))),
                ui::group_button("file-more", "more").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::More, window, cx))),
            ]));
        let (status_color, status_label) = status_word(self.file_status(path));
        let mut meta = vec![ui::meta_item().child(icon("file", 13., TEXT_2)).child(language(path)).into_any_element()];
        if let Some(t) = text {
            meta.push(ui::meta_item().child(ui::meta_value(format!("{} lines · {}", t.lines().count(), size(t.len())))).into_any_element());
        }
        let status = ui::meta_item().id("meta-status").child(dot(7., status_color)).child(ui::meta_value(status_label));
        meta.push(match self.agents.last_edit(path) {
            Some((a, ts)) => status
                .cursor_pointer()
                .child(format!("by {} · {}", provider_name(&a.provider), ago_long(ts, now_ms())))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_changes(None, cx)))
                .into_any_element(),
            None => status.into_any_element(),
        });
        doc_bar(crumbs, meta, right)
    }
}

#[cfg(test)]
mod tests {
    use super::relative;

    #[test]
    fn paths_show_relative_to_the_root_unless_outside_it() {
        assert_eq!(relative("/r/src/a.rs", "/r"), "src/a.rs");
        assert_eq!(relative("/elsewhere/a.rs", "/r"), "/elsewhere/a.rs");
    }
}
