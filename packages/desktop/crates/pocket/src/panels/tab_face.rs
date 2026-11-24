use crate::browser::Browser;
use crate::desktop::Desktop;
use crate::desktop::chrome::{id, state};
use crate::status::Status;
use crate::util::basename;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, dot};
use workspace::{Doc, Tab};

impl Desktop {
    /// What a tab is called: a doc's file name, a page's title, an agent's conversation title else its name, a terminal's running command else its label.
    pub(super) fn tab_text(&self, tab: &Tab, cx: &App) -> String {
        match tab {
            Tab::Web(id) => self.browsers.tabs.get(id).map(Browser::label).unwrap_or_default().to_string(),
            Tab::Doc(Doc::File(p) | Doc::Diff(p)) => basename(p),
            Tab::Doc(Doc::CommitFile { sha, path }) => format!("{} ({})", basename(path), git::short_sha(sha)),
            Tab::Doc(Doc::PrFile { path, .. }) => format!("{} (PR)", basename(path)),
            Tab::Doc(Doc::CompareFile { against, path }) => format!("{} (vs {against})", basename(path)),
            Tab::Doc(Doc::Commit(sha)) => git::short_sha(sha).to_string(),
            Tab::Term(term) => match self.summary(term) {
                Some(a) if !a.title.is_empty() => a.title.clone(),
                Some(a) => self.agent_name(&a.provider),
                None => self.terminals.read(cx).sessions.get(term).and_then(|s| s.busy()).map_or_else(|| self.pane_label(term, cx), str::to_string),
            },
        }
    }

    /// A tab's icon, dimmed unless `selected`, then its name and marks.
    pub(super) fn tab_lead(&self, tab: &Tab, preview: bool, selected: bool, cx: &App) -> Div {
        let ink = if selected { TEXT } else { TEXT_3 };
        let row = div().flex().items_center().gap(px(6.));
        let lead = |el: AnyElement| div().flex_none().flex().when(!selected, |d| d.opacity(0.55)).child(el);
        let label = |text: String| div().flex_1().truncate().child(text);
        let term = match tab {
            Tab::Term(term) => term,
            Tab::Web(id) => {
                let icon = match self.browsers.tabs.get(id) {
                    Some(b) if b.loading => spinner(format!("tab-loading:{id}"), 13., TEXT_3).into_any_element(),
                    _ => icon("globe", 13., TEXT_3).into_any_element(),
                };
                return row.child(lead(icon)).child(label(self.tab_text(tab, cx)));
            }
            Tab::Doc(doc) => {
                let path = match doc {
                    Doc::File(p) | Doc::Diff(p) | Doc::CommitFile { path: p, .. } | Doc::PrFile { path: p, .. } | Doc::CompareFile { path: p, .. } => p,
                    Doc::Commit(_) => {
                        return row
                            .child(lead(icon("diff-multiple", 14., ink).into_any_element()))
                            .child(label(self.tab_text(tab, cx)).when(preview, |d| d.italic()));
                    }
                };
                let text = self.tab_text(tab, cx);
                let totals = match doc {
                    Doc::Diff(p) => self.repo(cx).and_then(|r| r.files.iter().find(|f| f.path == *p)).map(|f| ui::meta_diff(f.added, f.removed, 11.)),
                    _ => None,
                };
                let unsaved = matches!(doc, Doc::File(p) if self.preview.read(cx).dirty(p)).then(|| dot(6., TEXT_2));
                return row
                    .child(lead(file_icon(path, false, false, 14.).into_any_element()))
                    .child(label(text).when(preview, |d| d.italic()))
                    .children(totals)
                    .children(unsaved);
            }
        };
        if let Some(a) = self.summary(term) {
            let mark = ui::indicator(id(format!("tab-mark:{}", a.id)), Status::of(a).map(|s| state(s, 0, 0)));
            return row.child(lead(provider_icon(&a.provider, 13., ink).into_any_element())).child(label(self.agent_name(&a.provider))).children(mark);
        }
        let mark = match self.terminals.read(cx).sessions.get(term) {
            Some(s) if s.failed() => icon("x", 12., FAILED).into_any_element(),
            Some(s) if s.busy().is_some() => dot(6., ACCENT).into_any_element(),
            _ => dot(6., TEXT_5).into_any_element(),
        };
        row.child(lead(icon("prompt", 13., TEXT_3).into_any_element())).child(label(self.tab_text(tab, cx))).child(mark)
    }
}
