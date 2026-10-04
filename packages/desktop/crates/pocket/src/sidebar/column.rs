use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, HEADER, LIGHTS, Layout, Overlay, RAIL, Screen, Side, column, drag_area};
use crate::util::basename;
use git::Repo;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, icon_button_sized};

/// The Changes toggle's added and removed counts, while the worktree has any changed file.
pub(crate) fn changes_badge(repo: Option<&Repo>) -> Option<(String, String)> {
    let (added, removed) = repo.filter(|r| !r.files.is_empty())?.totals();
    Some((short(added), short(removed)))
}

fn short(n: usize) -> String {
    match n {
        ..1_000 => n.to_string(),
        1_000..10_000 => format!("{:.1}k", n as f32 / 1000.).replace(".0k", "k"),
        _ => format!("{}k", n / 1000),
    }
}

/// Marks a sidebar toggle while the hidden Changes list has something in it.
pub(crate) fn changes_dot() -> Div {
    ui::dot(6., ACCENT).absolute().top(px(5.)).right(px(5.)).shadow(vec![ui::ring(Token::new(0xfafafaff, 0x171717ff), 1.5)])
}

impl Desktop {
    pub(crate) fn column_view(&mut self, cx: &mut Context<Self>) -> Div {
        let body = match (self.screen, self.side) {
            (Screen::Inbox, _) => {
                let list = self.inbox_list(cx).w(px(self.width(Column::Sessions, 348.)));
                return self.resizable(list, Column::Sessions, cx);
            }
            (_, Side::Sessions) => self.session_list(cx).into_any_element(),
            (_, Side::Explorer) => self.explorer(cx).into_any_element(),
            (_, Side::Changes) => self.changes_list(cx).into_any_element(),
        };
        let totals = changes_badge(self.repo());
        let tabs = [(Side::Sessions, "Sessions"), (Side::Explorer, "Explorer"), (Side::Changes, "Changes")].into_iter().enumerate().map(|(i, (side, label))| {
            let selected = self.side == side;
            let badge = totals.clone().filter(|_| side == Side::Changes).map(|(added, removed)| {
                div()
                    .h(px(17.))
                    .px(px(5.))
                    .flex()
                    .items_center()
                    .gap(px(6.6))
                    .rounded(px(8.))
                    .bg(HAIRLINE)
                    .font_family(MONO)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(11.))
                    .child(div().text_color(SUCCESS_TEXT).child(format!("+{added}")))
                    .child(div().text_color(FAILED).child(format!("−{removed}")))
            });
            div()
                .id(("column-tab", i))
                .flex_1()
                .h(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .rounded(px(8.))
                .cursor_pointer()
                .whitespace_nowrap()
                .text_size(px(13.))
                .when(selected, |d| d.bg(FILL_4).text_color(TEXT).font_weight(FontWeight::SEMIBOLD))
                .when(!selected, |d| d.text_color(TEXT_2).font_weight(FontWeight::MEDIUM).hover(|s| s.bg(FILL_2)))
                .when(badge.is_none(), |d| d.child(label))
                .children(badge)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.side = side;
                    this.refresh_graph(cx);
                    cx.notify();
                }))
        });
        let tabs = div().p(px(8.)).flex().flex_none().gap(px(4.)).border_b(px(0.5)).border_color(SEPARATOR).children(tabs);
        let column = column()
            .w(px(self.width(Column::Sessions, 334.)))
            .child(drag_area(self.column_header(cx)).h(px(HEADER)).flex_none().border_b(px(0.5)).border_color(SEPARATOR))
            .when(self.layout != Layout::Compact, |d| d.child(tabs))
            .child(body);
        self.resizable(column, Column::Sessions, cx)
    }

    fn column_header(&self, cx: &mut Context<Self>) -> Div {
        let title = self.cwd().map(|t| basename(&t)).or_else(|| self.project.as_deref().map(|p| self.repo_name(p))).unwrap_or_default();
        let add = icon_button_sized("column-add", "plus", 28., TEXT_2)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::NewSession, window, cx)));
        div()
            .pl(px(if self.layout == Layout::Compact { LIGHTS - RAIL } else { 18. }))
            .pr(px(12.))
            .flex()
            .items_center()
            .gap(px(4.))
            .child(div().flex_1().text_size(px(16.)).font_weight(FontWeight::BOLD).truncate().child(title))
            .child(add)
    }
}

#[cfg(test)]
mod tests {
    use super::{changes_badge, short};
    use git::{FileStat, Repo};

    fn changed(added: usize, removed: usize) -> FileStat {
        FileStat { path: "a.rs".into(), added, removed, staged: false, unstaged: true, status: 'M' }
    }

    #[test]
    fn a_clean_worktree_has_no_changes_badge() {
        assert_eq!(changes_badge(Some(&Repo::default())), None);
        assert_eq!(changes_badge(None), None);
    }

    #[test]
    fn the_changes_badge_totals_every_changed_file() {
        let repo = Repo { files: vec![changed(3, 1), changed(4, 0)], ..Default::default() };
        assert_eq!(changes_badge(Some(&repo)), Some(("7".into(), "1".into())));
    }

    #[test]
    fn binary_changes_alone_still_badge_the_worktree() {
        let repo = Repo { files: vec![changed(0, 0)], ..Default::default() };
        assert_eq!(changes_badge(Some(&repo)), Some(("0".into(), "0".into())));
    }

    #[test]
    fn counts_past_a_thousand_are_shortened_to_fit_the_toggle() {
        assert_eq!([987, 1_000, 1_234, 9_960, 12_345].map(short), ["987", "1k", "1.2k", "10k", "12k"]);
    }
}
