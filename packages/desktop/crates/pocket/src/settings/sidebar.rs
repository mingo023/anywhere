use super::catalog::{Look, Setting};
use super::row;
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::Store;
use store::prefs::sidebar::{Details, InboxFilter, InboxSort, SessionSort};
use theme::*;

const SORTS: [SessionSort; 4] = [SessionSort::Newest, SessionSort::Status, SessionSort::Activity, SessionSort::Name];
const INBOX_SORTS: [InboxSort; 3] = [InboxSort::Urgent, InboxSort::Newest, InboxSort::Oldest];
const INBOX_FILTERS: [InboxFilter; 4] = [InboxFilter::All, InboxFilter::NeedsYou, InboxFilter::Failed, InboxFilter::Done];

type Detail = (&'static str, fn(&mut Details) -> &mut bool);

const DETAILS: [Detail; 4] = [("Model", |d| &mut d.model), ("Time", |d| &mut d.time), ("Branch", |d| &mut d.branch), ("Diff stats", |d| &mut d.diff)];

/// The details shown, as the reset sheet names them.
fn details(s: &Store) -> String {
    let mut shown = s.sidebar.details;
    let on: Vec<_> = DETAILS.iter().filter(|(_, field)| *field(&mut shown)).map(|(label, _)| *label).collect();
    if on.is_empty() { "None".into() } else { on.join(", ") }
}

pub(super) const ROWS: &[Setting] = &[
    Setting::choice("session-sort", "Session list", "Sort sessions by", &["Newest", "Status", "Last activity", "Name"], Look::Dropdown, |s| s.sidebar.sort as usize, |s, i| s.sidebar.sort = SORTS[i])
        .hint("Status puts Needs you and Failed first"),
    Setting::custom("session-details", "Session list", "Details on each row", |d, s, cx| d.details_row(s, cx)).resets(details, |s, d| s.sidebar.details = d.sidebar.details),
    Setting::switch("external-worktrees", "Session list", "Show worktrees not created by Anywhere", |s| s.sidebar.external_worktrees, |s, on| s.sidebar.external_worktrees = on)
        .hint("Worktrees you made with git worktree add")
        .advanced(),
    Setting::switch("usage-card", "Context bars", "Show context bars on the rail", |s| s.sidebar.usage_card, |s, on| s.sidebar.usage_card = on).hint("Context left in each agent's newest session"),
    Setting::stepper("usage-warn", "Context bars", "Warn at", (50, 95, 5), "%", |s| s.sidebar.warn_at as i32, |s, n| s.sidebar.set_warn(n as u32)).hint("The bars turn amber").under(|s| s.sidebar.usage_card),
    Setting::stepper("usage-critical", "Context bars", "Critical at", (60, 99, 1), "%", |s| s.sidebar.critical_at as i32, |s, n| s.sidebar.set_critical(n as u32))
        .hint("The bars turn red")
        .under(|s| s.sidebar.usage_card),
    Setting::switch("jump-hints", "Jump hints", "Show jump hints while holding Control", |s| s.sidebar.jump_hints, |s, on| s.sidebar.jump_hints = on).hint("Numbers appear on sessions; press one with ⌃ to jump"),
    Setting::stepper("jump-delay", "Jump hints", "Delay before hints appear", (0, 1000, 20), " ms", |s| s.sidebar.hint_delay_ms as i32, |s, n| s.sidebar.hint_delay_ms = n as u32)
        .under(|s| s.sidebar.jump_hints)
        .advanced(),
    Setting::choice("inbox-sort", "Inbox", "Sort inbox by", &["Most urgent first", "Newest", "Oldest"], Look::Dropdown, |s| s.sidebar.inbox_sort as usize, |s, i| s.sidebar.inbox_sort = INBOX_SORTS[i]),
    Setting::choice("inbox-filter", "Inbox", "Open the inbox showing", &["All", "Needs you", "Failed", "Done"], Look::Segmented, |s| s.sidebar.inbox_filter as usize, |s, i| s.sidebar.inbox_filter = INBOX_FILTERS[i]),
];

impl Desktop {
    fn details_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let chips = DETAILS.into_iter().map(|(label, field)| {
            let on = *field(&mut self.store.sidebar.details);
            div()
                .id(label)
                .h(px(24.))
                .px(px(9.))
                .flex()
                .items_center()
                .gap(px(5.))
                .rounded(px(12.))
                .cursor_pointer()
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .map(|d| if on { d.bg(TEXT).text_color(ON_TEXT).child(icon("check", 11., ON_TEXT)) } else { d.bg(FILL_2).text_color(TEXT_2).hover(|d| d.bg(FILL_3)) })
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    let on = field(&mut this.store.sidebar.details);
                    *on = !*on;
                    this.changed(setting, cx);
                }))
        });
        row(setting.label, None, div().flex().gap(px(6.)).children(chips))
    }
}

#[cfg(test)]
mod tests {
    use super::{DETAILS, ROWS};
    use store::Store;
    use store::prefs::sidebar::Details;

    #[test]
    fn each_detail_chip_toggles_its_own_part_of_the_row() {
        let mut d = Details::default();
        for (_, field) in DETAILS {
            *field(&mut d) = false;
        }
        assert_eq!(d, Details { model: false, time: false, branch: false, diff: false });
    }

    #[test]
    fn the_sidebar_rows_read_back_what_they_write() {
        let mut store = Store::default();
        for r in ROWS {
            if let super::super::catalog::Control::Choice { options, get, set, .. } = r.control {
                set(&mut store, options.len() - 1);
                assert_eq!(get(&store), options.len() - 1, "{}", r.id);
            }
        }
    }
}
