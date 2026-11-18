use super::nav::Preview;
use crate::desktop::Desktop;
use crate::explorer::preview::size;
use crate::util::folder;
use gpui_kit::*;
use std::ops::Range;
use std::rc::Rc;
use theme::*;

const LINE: f32 = 19.;

impl Desktop {
    /// What's picked, beside the columns: a file's text, a repository's branch and remote, or a folder's count.
    pub(super) fn finder_detail(&self) -> Div {
        let Some(f) = self.host_finder.finder.as_ref() else { return div() };
        let pane = div().flex_1().min_w(px(240.)).p(px(16.)).flex().flex_col().gap(px(12.)).overflow_hidden();
        let head = |glyph: &str, title: String, sub: String| {
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(div().size(px(36.)).flex_none().flex().items_center().justify_center().rounded(px(9.)).bg(FILL_3).child(icon(glyph, 18., TEXT_2)))
                .child(div().min_w_0().flex().flex_col().child(div().truncate().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title)).child(div().truncate().text_size(px(12.)).text_color(TEXT_3).child(sub)))
        };
        if let Some(file) = &f.file {
            let row_size = f.listing(&f.dir).and_then(|l| l.as_ref().ok()).and_then(|l| l.shown.iter().find(|r| &r.name == file)).map_or(0, |r| r.size);
            let body = match f.preview.as_ref().map(|(_, p)| p) {
                Some(Preview::Text(lines)) => {
                    let list = uniform_list("finder-preview", lines.len(), preview_rows(lines.clone()))
                        .track_scroll(&self.host_finder.preview_scroll)
                        .size_full()
                        .py(px(8.));
                    div().flex_1().min_h_0().rounded(px(9.)).bg(FILL_1).border(px(0.5)).border_color(SEPARATOR).child(list)
                }
                Some(Preview::TooBig(_)) => note("Too big to preview."),
                Some(Preview::Binary) => note("No preview for this file."),
                Some(Preview::Failed(e)) => note(e),
                Some(Preview::Loading) | None => note("Reading\u{2026}"),
            };
            return pane.child(head("file", file.clone(), size(row_size as usize))).child(body);
        }
        if f.dir.is_empty() {
            return pane;
        }
        let title = folder(&f.dir).to_string();
        let title = if title.is_empty() { f.dir.clone() } else { title };
        if f.is_repo(&f.dir) {
            let repo = f.repo.as_ref().filter(|(d, _)| *d == f.dir).and_then(|(_, r)| r.clone());
            let fact = |k: &str, v: String| div().flex().gap(px(14.)).text_size(px(12.5)).child(div().w(px(56.)).flex_none().text_color(TEXT_3).child(k.to_string())).child(div().min_w_0().truncate().font_family(MONO).child(v));
            let (branch, remote) = match repo {
                Some(r) => (r.branch.unwrap_or_else(|| "detached".into()), r.remote.unwrap_or_else(|| "none".into())),
                None => ("\u{2026}".into(), "\u{2026}".into()),
            };
            return pane.child(head("branch", title, "Git repository".into())).child(div().flex().flex_col().gap(px(6.)).child(fact("Branch", branch)).child(fact("Remote", remote)).child(fact("Path", f.pretty(&f.dir))));
        }
        let count = match f.listing(&f.dir) {
            Some(Ok(l)) => format!("{} items", l.shown.len()),
            Some(Err(_)) => "Couldn\u{2019}t list".into(),
            None => "Listing\u{2026}".into(),
        };
        pane.child(head("folder", title, count))
    }
}

fn note(text: impl Into<SharedString>) -> Div {
    div().text_size(px(12.5)).text_color(TEXT_3).child(text.into())
}

/// A preview's lines as `uniform_list` draws them: only those in view.
fn preview_rows(lines: Rc<Vec<String>>) -> impl Fn(Range<usize>, &mut Window, &mut App) -> Vec<Div> + 'static {
    move |range, _, _| {
        range
            .map(|i| {
                div()
                    .w_full()
                    .h(px(LINE))
                    .flex()
                    .font_family(MONO)
                    .text_size(px(12.))
                    .child(div().w(px(40.)).flex_none().pr(px(12.)).text_right().text_color(TEXT_5).child((i + 1).to_string()))
                    .child(div().flex_1().min_w_0().whitespace_nowrap().overflow_hidden().text_color(TEXT_2).child(lines[i].clone()))
            })
            .collect()
    }
}
