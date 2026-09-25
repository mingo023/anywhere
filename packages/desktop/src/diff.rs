use crate::git::{Kind, Line};
use crate::theme::*;
use crate::view::{button, segmented};
use crate::Desktop;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

const NUM: f32 = 48.;
const SIGN: f32 = 18.;
const ROW: f32 = 22.;

fn stat(added: usize, removed: usize) -> Div {
    div()
        .flex()
        .flex_none()
        .gap(px(6.))
        .font_family(MONO)
        .text_size(px(12.5))
        .child(div().text_color(rgb(GREEN)).child(format!("+{added}")))
        .child(div().text_color(rgb(RED)).child(format!("−{removed}")))
}

fn checkbox(on: bool) -> Div {
    div()
        .size(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.))
        .border_1()
        .border_color(rgb(if on { INK } else { 0xb9b6af }))
        .bg(rgb(if on { INK } else { WHITE }))
        .when(on, |d| d.child(icon("check", 10., WHITE)))
}

fn colors(kind: Kind) -> (Option<u32>, u32, &'static str) {
    match kind {
        Kind::Hunk => (Some(HUNK), MUTED, ""),
        Kind::Add => (Some(ADD_BG), ADD_TEXT, "+"),
        Kind::Del => (Some(DEL_BG), DEL_TEXT, "-"),
        Kind::Context => (None, TEXT, ""),
    }
}

fn number(n: Option<usize>) -> Div {
    div().w(px(NUM)).flex_none().pr(px(8.)).flex().justify_end().text_color(rgb(FAINT)).child(n.map(|n| n.to_string()).unwrap_or_default())
}

fn code(l: &Line, numbers: Vec<Option<usize>>) -> Div {
    let (bg, fg, sign) = colors(l.kind);
    div()
        .h(px(ROW))
        .flex()
        .items_center()
        .whitespace_nowrap()
        .when_some(bg, |d, bg| d.bg(rgb(bg)))
        .children(numbers.into_iter().map(number))
        .child(div().w(px(SIGN)).flex_none().flex().justify_center().text_color(rgb(fg)).child(sign))
        .child(div().pl(px(8.)).text_color(rgb(fg)).child(l.text.clone()))
}

impl Desktop {
    pub fn changes_list(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let list = div().id("changes").flex_1().overflow_y_scroll().flex().flex_col();
        let Some(repo) = self.repo().cloned() else {
            return list.child(div().p(px(16.)).text_size(px(13.5)).text_color(rgb(MUTED)).child("Not a git repository."));
        };
        let (added, removed) = repo.totals();
        let staged = repo.files.iter().filter(|f| f.staged).count();
        let label = if staged == repo.files.len() && staged > 0 { "Staged" } else { "Changes" };
        let branch = div()
            .h(px(40.))
            .mt(px(6.))
            .px(px(16.))
            .flex()
            .items_center()
            .gap(px(6.))
            .font_family(MONO)
            .text_size(px(12.5))
            .child(icon("branch", 12., MUTED))
            .child(div().truncate().text_color(rgb(INK)).child(repo.branch.clone()))
            .when_some(repo.base.clone(), |d, base| d.child(icon("arrow-right", 12., MUTED)).child(div().text_color(rgb(MUTED)).child(base)))
            .child(div().flex_1())
            .child(div().font_family(SANS).text_color(rgb(MUTED)).child(format!("{} ahead", repo.ahead)));
        let heading = div()
            .px(px(16.))
            .pt(px(4.))
            .pb(px(6.))
            .flex()
            .items_center()
            .text_size(px(12.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(MUTED))
            .child(div().flex_1().child(format!("{label} · {} file{}", repo.files.len(), if repo.files.len() == 1 { "" } else { "s" })))
            .child(stat(added, removed).font_weight(FontWeight::NORMAL));
        let files = repo.files.iter().enumerate().map(|(i, f)| {
            let selected = self.diff_file.as_ref() == Some(&f.path);
            let (path, on) = (f.path.clone(), f.staged);
            let open = f.path.clone();
            div()
                .id(("file", i))
                .mx(px(12.))
                .h(px(36.))
                .px(px(4.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(8.))
                .rounded(px(8.))
                .when(selected, |d| d.bg(rgb(SELECTED)))
                .when(!selected, |d| d.hover(|s| s.bg(rgb(HOVER))))
                .child(
                    div().id(("stage", i)).size(px(24.)).flex().items_center().justify_center().child(checkbox(on)).on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.stage(path.clone(), !on, cx);
                        },
                    )),
                )
                .child(icon("file", 14., MUTED))
                .child(div().flex_1().truncate().font_family(MONO).text_size(px(12.5)).text_color(rgb(INK)).child(f.path.clone()))
                .child(stat(f.added, f.removed).pr(px(4.)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_changes(Some(open.clone()), cx)))
        });
        let commits = repo.commits.iter().map(|c| {
            div()
                .px(px(16.))
                .h(px(26.))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(13.))
                .child(div().font_family(MONO).text_size(px(12.5)).text_color(rgb(GREEN)).child(c.sha.clone()))
                .child(div().truncate().text_color(rgb(TEXT)).child(c.subject.clone()))
        });
        list.child(branch)
            .child(heading)
            .children(files)
            .child(div().mx(px(12.)).my(px(10.)).h(px(1.)).bg(rgb(HAIR)))
            .child(div().px(px(16.)).pb(px(4.)).text_size(px(12.5)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(MUTED)).child("Commits on branch"))
            .children(commits)
    }

    pub fn diff_view(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(path) = self.diff_file.clone() else {
            return div().flex_1().flex().items_center().justify_center().text_size(px(14.)).text_color(rgb(MUTED)).child("No changes.");
        };
        let file = self.repo().and_then(|r| r.files.iter().find(|f| f.path == path)).cloned();
        let open = self.cwd().map(|c| std::path::Path::new(&c).join(&path));
        let header = div()
            .h(px(48.))
            .flex_none()
            .px(px(20.))
            .flex()
            .items_center()
            .gap(px(10.))
            .border_b_1()
            .border_color(rgb(HAIR))
            .child(icon("file", 15., MUTED))
            .child(div().truncate().font_family(MONO).text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child(path.clone()))
            .children(file.map(|f| stat(f.added, f.removed)))
            .child(div().flex_1())
            .child(segmented(vec![(false, "Unified".into()), (true, "Split".into())], self.diff_split, 26., 12.5, |this, v, cx| {
                this.diff_split = v;
                cx.notify();
            }, cx))
            .child(button("open-editor").child(icon("external", 13., INK)).child("Open in editor").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                if let Some(p) = &open {
                    cx.open_with_system(p);
                }
            })));
        let mut rows: Vec<AnyElement> = Vec::new();
        let lines = std::mem::take(&mut self.diff);
        if self.diff_split {
            for (i, (l, r)) in crate::git::split(&lines).into_iter().enumerate() {
                let side = |l: Option<&Line>, n: fn(&Line) -> Option<usize>| match l {
                    Some(l) => code(l, vec![n(l)]).flex_1().min_w_0().overflow_hidden(),
                    None => div().flex_1().h(px(ROW)).bg(rgb(0xf3f2ee)),
                };
                let target = r.or(l).and_then(|l| l.new.or(l.old)).filter(|_| r.or(l).is_some_and(|l| l.kind != Kind::Hunk));
                rows.push(self.line_row(i, &path, target, div().flex().child(side(l, |l| l.old)).child(side(r, |l| l.new)), cx));
                self.after_line(&path, target, &mut rows, cx);
            }
        } else {
            for (i, l) in lines.iter().enumerate() {
                let target = (l.kind != Kind::Hunk).then(|| l.new.or(l.old)).flatten();
                rows.push(self.line_row(i, &path, target, code(l, vec![l.old, l.new]), cx));
                self.after_line(&path, target, &mut rows, cx);
            }
        }
        self.diff = lines;
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(WHITE))
            .child(header)
            .child(div().id("diff").flex_1().overflow_scroll().py(px(8.)).font_family(MONO).text_size(px(12.5)).line_height(px(ROW)).children(rows))
    }

    fn line_row(&self, i: usize, path: &str, target: Option<usize>, row: Div, cx: &mut Context<Self>) -> AnyElement {
        let path = path.to_string();
        row.id(("line", i))
            .when_some(target, |d, line| {
                d.cursor_pointer().on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_comment(path.clone(), line, window, cx)))
            })
            .into_any_element()
    }

    fn after_line(&self, path: &str, line: Option<usize>, rows: &mut Vec<AnyElement>, cx: &mut Context<Self>) {
        let Some(line) = line else { return };
        for (i, c) in self.comments.iter().enumerate().filter(|(_, c)| c.path == path && c.line == line) {
            rows.push(self.comment_card(i, &c.text, line, cx).into_any_element());
        }
        if self.commenting.as_ref().is_some_and(|(p, l)| p == path && *l == line) {
            rows.push(
                card()
                    .font_family(SANS)
                    .child(author(&self.initials, line))
                    .child(Input::new(&self.comment_input).text_size(px(14.)))
                    .into_any_element(),
            );
        }
    }

    fn comment_card(&self, i: usize, text: &str, line: usize, cx: &mut Context<Self>) -> Div {
        let provider = self.session.as_deref().and_then(|s| self.summary(s)).map(|a| provider_name(&a.provider)).unwrap_or("agent");
        let provider = provider.strip_suffix(" Code").unwrap_or(provider);
        card()
            .font_family(SANS)
            .child(author(&self.initials, line))
            .child(div().text_size(px(14.)).line_height(px(21.)).text_color(rgb(INK)).child(text.to_string()))
            .child(
                div()
                    .mt(px(2.))
                    .flex()
                    .gap(px(8.))
                    .child(
                        button(("send-comment", i))
                            .bg(rgb(INK))
                            .border_color(rgb(INK))
                            .text_color(rgb(WHITE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .hover(|s| s.bg(rgb(0x33312e)))
                            .child(icon("send", 13., WHITE))
                            .child(format!("Send to {provider}"))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_comment(i, cx))),
                    )
                    .child(button(("resolve", i)).child("Resolve").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.comments.remove(i);
                        cx.notify();
                    }))),
            )
    }
}

fn card() -> Div {
    div()
        .mt(px(6.))
        .mb(px(6.))
        .mr(px(16.))
        .ml(px(106.))
        .px(px(12.))
        .py(px(10.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .rounded(px(10.))
        .border_1()
        .border_color(rgb(CARD))
        .bg(rgb(WHITE))
        .whitespace_normal()
}

fn author(initials: &str, line: usize) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(13.))
        .child(
            div()
                .size(px(22.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(11.))
                .bg(rgb(INK))
                .text_size(px(10.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(WHITE))
                .child(initials.to_string()),
        )
        .child(div().font_weight(FontWeight::SEMIBOLD).child("You"))
        .child(div().text_color(rgb(MUTED)).child(format!("on line {line}")))
}
