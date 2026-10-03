use super::{Item, Section, toggle_fold};
use crate::desktop::Desktop;
use git::FileStat;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::icon_button_sized;

const ROW_GROUP: &str = "change-row";
const SECTION_GROUP: &str = "change-section";

fn action(id: impl Into<ElementId>, name: &str) -> Stateful<Div> {
    icon_button_sized(id, name, 22., TEXT_2).rounded(px(6.))
}

fn count_pill(n: usize) -> Div {
    div()
        .min_w(px(18.))
        .h(px(18.))
        .px(px(5.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(9.))
        .bg(FILL_3)
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(TEXT_2)
        .child(n.to_string())
}

fn row(id: impl Into<ElementId>, depth: usize) -> Stateful<Div> {
    div().id(id).w_full().h(px(28.)).pl(px(8. + depth as f32 * 14.)).pr(px(6.)).flex().flex_none().items_center().gap(px(6.)).rounded(px(8.)).cursor_pointer()
}

impl Desktop {
    pub(super) fn section(&self, s: Section, paths: &[String], cx: &mut Context<Self>) -> Stateful<Div> {
        let key = s.key();
        let open = !self.changes.folded.contains(key);
        let count = paths.len();
        let paths = paths.to_vec();
        let actions = match s {
            Section::Staged => vec![action("unstage-all", "minus").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.stage(paths.clone(), false, cx);
            }))],
            Section::Changes => {
                let discard = paths.clone();
                vec![
                    action("discard-all", "discard").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.ask_discard(discard.clone(), cx);
                    })),
                    action("stage-all", "plus").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.stage(paths.clone(), true, cx);
                    })),
                ]
            }
        };
        div()
            .id(key)
            .group(SECTION_GROUP)
            .w_full()
            .h(px(28.))
            .pl(px(6.))
            .pr(px(6.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .cursor_pointer()
            .hover(|st| st.bg(FILL_1))
            .child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_4))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(TEXT_3)
                    .child(if s == Section::Staged { "Staged Changes" } else { "Changes" }),
            )
            .child(div().flex().gap(px(2.)).opacity(0.).group_hover(SECTION_GROUP, |st| st.opacity(1.)).children(actions))
            .child(count_pill(count))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                toggle_fold(&mut this.changes.folded, key);
                cx.notify();
            }))
    }

    pub(super) fn tree_item(&self, s: Section, item: &Item, cx: &mut Context<Self>) -> AnyElement {
        match item {
            Item::File { file, depth } => self.change_row(s, file, *depth, self.changes.tree, cx).into_any_element(),
            Item::Dir { key, label, depth, open } => {
                let (key, open) = (key.clone(), *open);
                row(ElementId::Name(key.clone().into()), *depth)
                    .hover(|st| st.bg(FILL_1))
                    .text_size(px(13.5))
                    .font_weight(FontWeight(450.))
                    .child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_4))
                    .child(file_icon(label, true, open, 16.))
                    .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        toggle_fold(&mut this.changes.folded, &key);
                        cx.notify();
                    }))
                    .into_any_element()
            }
        }
    }

    /// In the tree a file sits under its folder, so only its name shows, indented past the folders' chevrons.
    fn change_row(&self, s: Section, f: &FileStat, depth: usize, in_tree: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.diff.view(self.focused_pane()).is_some_and(|v| v.shows(&f.path, None));
        let (dir, name) = f.path.rsplit_once('/').unwrap_or(("", &f.path));
        let comments = self.diff.comments.iter().filter(|c| c.path == f.path).count();
        let id = |kind: &str| ElementId::Name(format!("{kind}:{}:{}", s.key(), f.path).into());
        let path = f.path.clone();
        let actions = match s {
            Section::Staged => vec![action(id("unstage"), "minus").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.stage(vec![path.clone()], false, cx);
            }))],
            Section::Changes => {
                let discard = path.clone();
                vec![
                    action(id("discard"), "discard").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.ask_discard(vec![discard.clone()], cx);
                    })),
                    action(id("stage"), "plus").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.stage(vec![path.clone()], true, cx);
                    })),
                ]
            }
        };
        let open = f.path.clone();
        row(id("file"), depth)
            .group(ROW_GROUP)
            .when(selected, |d| d.bg(FILL_3))
            .when(!selected, |d| d.hover(|st| st.bg(FILL_1)))
            .when(in_tree, |d| d.child(div().w(px(12.)).flex_none()))
            .child(file_icon(name, false, false, 16.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_baseline()
                    .gap(px(6.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(
                        div()
                            .flex_none()
                            .max_w_full()
                            .truncate()
                            .text_size(px(13.5))
                            .font_weight(if selected { FontWeight::SEMIBOLD } else { FontWeight(450.) })
                            .when(f.status == 'D', |d| d.line_through())
                            .child(name.to_string()),
                    )
                    .when(!in_tree && !dir.is_empty(), |d| d.child(div().min_w_0().truncate().text_size(px(12.)).text_color(TEXT_4).child(dir.to_string()))),
            )
            .when(comments > 0, |d| {
                d.child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(3.))
                        .text_size(px(11.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(WAITING_TEXT)
                        .child(icon("comment", 11., WAITING_TEXT))
                        .child(comments.to_string()),
                )
            })
            .child(div().flex().flex_none().gap(px(2.)).opacity(0.).group_hover(ROW_GROUP, |st| st.opacity(1.)).children(actions))
            .child(
                div()
                    .w(px(14.))
                    .flex()
                    .flex_none()
                    .justify_center()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui::git_color(f.status))
                    .child(f.status.to_string()),
            )
            .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| this.open_changes(Some(open.clone()), ev.click_count() > 1, cx)))
    }
}
