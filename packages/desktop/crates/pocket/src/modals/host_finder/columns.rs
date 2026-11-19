use super::nav::Row;
use crate::desktop::Desktop;
use crate::modals::add_project::host_folder::child;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::ops::Range;
use std::rc::Rc;
use theme::*;

const ROW: f32 = 26.;
/// Columns shown at once; earlier ones are a click away in the path bar.
const COLUMNS: usize = 3;

/// A column as drawn: its place in the chain, its folder, the name picked in it, and its rows once listed.
type Column = (usize, String, Option<String>, Option<Result<Rc<Vec<Row>>, String>>);

impl Desktop {
    /// The folder columns from the chain's tail; `projects` lists the keys of the projects on this host.
    pub(super) fn finder_columns(&mut self, projects: Rc<Vec<String>>, cx: &mut Context<Self>) -> Div {
        let Some(f) = self.host_finder.finder.as_ref() else { return div() };
        let chain = f.chain();
        let host = f.host.clone();
        let shown = if f.file.is_some() || f.is_repo(&f.dir) { COLUMNS - 1 } else { COLUMNS };
        let first = chain.len().saturating_sub(shown);
        let current = if f.file.is_some() { chain.len().saturating_sub(1) } else { chain.len().saturating_sub(2) };
        let columns: Vec<Column> = (first..chain.len())
            .map(|i| (i, chain[i].clone(), f.picked_in(&chain, i), f.listing(&chain[i]).map(|l| l.as_ref().map(|l| l.shown.clone()).map_err(Clone::clone))))
            .collect();
        let mut out = div().flex_none().flex();
        for (i, dir, picked, listing) in columns {
            let column = div().w(px(220.)).h_full().flex_none().border_r(px(0.5)).border_color(SEPARATOR);
            let rows = match listing {
                None => {
                    out = out.child(column.child(div().p(px(14.)).text_size(px(12.5)).text_color(TEXT_4).child("Listing\u{2026}")));
                    continue;
                }
                Some(Err(e)) => {
                    out = out.child(column.child(div().p(px(14.)).text_size(px(12.5)).text_color(FAILED).child(e)));
                    continue;
                }
                Some(Ok(rows)) if rows.is_empty() => {
                    out = out.child(column.child(div().p(px(14.)).text_size(px(12.5)).text_color(TEXT_4).child("Empty folder")));
                    continue;
                }
                Some(Ok(rows)) => rows,
            };
            let scroll = self.host_finder.scroll(&dir);
            let (projects, host) = (projects.clone(), host.clone());
            let here = i == current;
            let list = uniform_list(
                ("finder-col", i),
                rows.len(),
                cx.processor(move |_, range: Range<usize>, _, cx| {
                    range
                        .map(|r| {
                            let row = &rows[r];
                            let on = picked.as_deref() == Some(row.name.as_str());
                            let project = row.dir && projects.contains(&hosts::place(&host, &child(&dir, &row.name)));
                            let (column, name) = (dir.clone(), row.name.clone());
                            div()
                                .id(("finder-row", r))
                                .w_full()
                                .h(px(ROW))
                                .px(px(5.))
                                .child(
                                    // GPUI repaints a hover only on an element with an id.
                                    div()
                                        .id(("finder-row-fill", r))
                                        .size_full()
                                        .px(px(7.))
                                        .flex()
                                        .items_center()
                                        .gap(px(7.))
                                        .rounded(px(6.))
                                        .text_size(px(13.))
                                        .hover(|s| s.bg(FILL_2))
                                        .when(on, |d| d.bg(if here { ACCENT_BG } else { FILL_4 }))
                                        .when(row.ignored, |d| d.opacity(0.5))
                                        .child(icon(if row.dir { "folder" } else { "file" }, 14., TEXT_3))
                                        .child(div().flex_1().min_w_0().truncate().child(row.name.clone()))
                                        .when(project, |d| d.child(div().px(px(5.)).rounded(px(4.)).bg(FILL_3).text_size(px(10.5)).text_color(TEXT_3).child("Project")))
                                        .when(row.dir, |d| d.child(icon("chevron-right", 12., TEXT_4))),
                                )
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.finder_select(column.clone(), name.clone(), cx)))
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&scroll)
            .size_full()
            .py(px(4.));
            out = out.child(column.child(list));
        }
        out
    }
}
