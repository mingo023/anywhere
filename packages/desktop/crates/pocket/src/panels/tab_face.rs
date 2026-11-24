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

    /// What a tab's face leads with: its icon, name and marks; the icon is dimmed unless `selected`.
    pub(super) fn tab_lead(&self, tab: &Tab, preview: bool, selected: bool, cx: &App) -> Lead {
        let text = || self.tab_text(tab, cx);
        let kind = match tab {
            Tab::Web(id) => Kind::Web { id: *id, loading: self.browsers.tabs.get(id).is_some_and(|b| b.loading), text: text() },
            Tab::Doc(Doc::Commit(_)) => Kind::Commit { text: text() },
            Tab::Doc(doc @ (Doc::File(path) | Doc::Diff(path) | Doc::CommitFile { path, .. } | Doc::PrFile { path, .. } | Doc::CompareFile { path, .. })) => {
                let totals = match doc {
                    Doc::Diff(p) => self.repo(cx).and_then(|r| r.files.iter().find(|f| f.path == *p)).map(|f| (f.added, f.removed)),
                    _ => None,
                };
                let unsaved = matches!(doc, Doc::File(p) if self.preview.read(cx).dirty(p));
                Kind::File { path: path.clone(), text: text(), totals, unsaved }
            }
            Tab::Term(term) => match self.summary(term) {
                Some(a) => Kind::Agent {
                    agent: a.id.clone(),
                    provider: a.provider.clone(),
                    name: self.agent_name(&a.provider),
                    state: Status::of(a).map(|s| state(s, 0, 0)),
                },
                None => {
                    let mark = match self.terminals.read(cx).sessions.get(term) {
                        Some(s) if s.failed() => Mark::Failed,
                        Some(s) if s.busy().is_some() => Mark::Busy,
                        _ => Mark::Idle,
                    };
                    Kind::Shell { text: text(), mark }
                }
            },
        };
        Lead { kind, preview, selected }
    }
}

/// What a tab's face draws, as plain data for the panels to draw from.
pub(crate) struct Lead {
    kind: Kind,
    preview: bool,
    selected: bool,
}

enum Kind {
    Web { id: u64, loading: bool, text: String },
    Commit { text: String },
    File { path: String, text: String, totals: Option<(usize, usize)>, unsaved: bool },
    Agent { agent: String, provider: String, name: String, state: Option<ui::State> },
    Shell { text: String, mark: Mark },
}

enum Mark {
    Failed,
    Busy,
    Idle,
}

impl Lead {
    pub(super) fn view(self) -> Div {
        let Self { kind, preview, selected } = self;
        let ink = if selected { TEXT } else { TEXT_3 };
        let row = div().flex().items_center().gap(px(6.));
        let lead = |el: AnyElement| div().flex_none().flex().when(!selected, |d| d.opacity(0.55)).child(el);
        let label = |text: String| div().flex_1().truncate().child(text);
        match kind {
            Kind::Web { id, loading, text } => {
                let icon =
                    if loading { spinner(format!("tab-loading:{id}"), 13., TEXT_3).into_any_element() } else { icon("globe", 13., TEXT_3).into_any_element() };
                row.child(lead(icon)).child(label(text))
            }
            Kind::Commit { text } => row.child(lead(icon("diff-multiple", 14., ink).into_any_element())).child(label(text).when(preview, |d| d.italic())),
            Kind::File { path, text, totals, unsaved } => row
                .child(lead(file_icon(&path, false, false, 14.).into_any_element()))
                .child(label(text).when(preview, |d| d.italic()))
                .children(totals.map(|(added, removed)| ui::meta_diff(added, removed, 11.)))
                .children(unsaved.then(|| dot(6., TEXT_2))),
            Kind::Agent { agent, provider, name, state } => {
                let mark = ui::indicator(id(format!("tab-mark:{agent}")), state);
                row.child(lead(provider_icon(&provider, 13., ink).into_any_element())).child(label(name)).children(mark)
            }
            Kind::Shell { text, mark } => {
                let mark = match mark {
                    Mark::Failed => icon("x", 12., FAILED).into_any_element(),
                    Mark::Busy => dot(6., ACCENT).into_any_element(),
                    Mark::Idle => dot(6., TEXT_5).into_any_element(),
                };
                row.child(lead(icon("prompt", 13., TEXT_3).into_any_element())).child(label(text)).child(mark)
            }
        }
    }
}
