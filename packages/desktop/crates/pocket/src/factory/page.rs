use super::station::{COLUMNS, Column, column};
use super::tabs::{OFFERED, Tab, shown};
use crate::board::logic::priority_name;
use crate::board::parts::{bar_button, label_dot, priority_glyph, prop_chip, station_glyph};
use crate::board::ticket_page::prop_row;
use crate::desktop::Desktop;
use crate::team::plural;
use crate::util::{ago_long, now_ms};
use agents::tickets::{Gate, Station, Status, Ticket};
use gpui_kit::*;
use theme::*;
use ui::{Segment, Variant, agent_avatar};

/// The open Factory ticket page. Plain data, so it's tested without a window.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Page {
    pub(crate) ticket: String,
    /// The tab you picked; `None` follows the ticket's station.
    pub(crate) tab: Option<Tab>,
    /// Request changes was clicked: the bar shows its text field.
    pub(crate) changes: bool,
}

impl Page {
    pub(crate) fn of(ticket: &str) -> Self {
        Self { ticket: ticket.into(), ..Self::default() }
    }

    /// Whether the page has nothing left to show: its ticket went away, or isn't a Factory ticket.
    pub(crate) fn stale(&self, ticket: Option<&Ticket>) -> bool {
        ticket.is_none_or(|t| t.station == Station::None)
    }

    pub(crate) fn shows(&self, ticket: &str) -> bool {
        self.ticket == ticket
    }
}

/// The words after the station's name on the status line.
pub(crate) fn status_text(t: &Ticket) -> &'static str {
    match (t.gate, t.status, t.station) {
        (Gate::Ask | Gate::Plan | Gate::Pr, ..) => "needs you",
        (_, Status::Failed, _) => "failed",
        (_, Status::Starting, _) => "starting",
        (_, _, Station::Spec) => "drafting the spec",
        (_, _, Station::Plan) => "planning",
        _ => "working",
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Dot {
    Done,
    At,
    Ahead,
}

/// Where the ticket is on the seven stations.
pub(crate) fn track(at: Column) -> [Dot; 7] {
    let i = COLUMNS.iter().position(|c| *c == at).unwrap_or(0);
    std::array::from_fn(|j| if j < i { Dot::Done } else if j == i { Dot::At } else { Dot::Ahead })
}

pub(crate) fn branch_text(branch: &str) -> String {
    if branch.is_empty() { "Made when the lead starts".into() } else { branch.into() }
}

pub(crate) fn pr_text(number: u32) -> String {
    if number == 0 { "Opens after Checks".into() } else { format!("#{number}") }
}

pub(crate) fn tasks_text(n: usize) -> String {
    if n == 0 { "Planned at Plan".into() } else { plural(n, "task", "tasks") }
}

impl Desktop {
    pub(crate) fn factory_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if let Some(page) = self.factory.page.as_mut() {
            page.tab = Some(tab);
        }
        cx.notify();
    }

    pub(crate) fn factory_page(&mut self, page: &Page, cx: &mut Context<Self>) -> Div {
        let Some(t) = self.agents.board.get(&page.ticket).cloned() else { return div() };
        let col = column(&t, self.agents.team(&t.agent_id), self.prs.get(&t.worktree));
        let tab = shown(page.tab, col);
        let head = div().flex().flex_col().gap(px(10.)).child(div().text_size(px(24.)).line_height(px(28.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(t.title.clone())).child(self.factory_status(&t, col)).child(self.factory_tabs(tab, cx));
        let body = match tab {
            Tab::Activity => self.factory_activity_tab(&t, cx).into_any_element(),
            Tab::Workers => div().id("factory-doc").flex_1().min_h_0().overflow_y_scroll().pb(px(72.)).child(self.factory_workers_tab(&t)).into_any_element(),
            _ => div().id("factory-doc").flex_1().min_h_0().overflow_y_scroll().pb(px(72.)).child(self.factory_spec_tab(&t, cx)).into_any_element(),
        };
        let doc = div().w_full().max_w(px(760.)).flex_1().min_h_0().flex().flex_col().child(head).child(body);
        let pane = div().relative().flex_1().min_w_0().min_h_0().px(px(24.)).pt(px(18.)).flex().flex_col().items_center().child(doc).children(self.factory_bar(&t, page, cx));
        let main = div().flex_1().min_h_0().flex().child(pane).child(self.factory_properties(&t, col));
        div().relative().flex_1().min_w_0().min_h_0().flex().flex_col().bg(CANVAS).child(self.factory_page_bar(&t, cx)).child(main)
    }

    fn factory_page_bar(&self, t: &Ticket, cx: &mut Context<Self>) -> Div {
        let key = self.ticket_key(t).unwrap_or_default();
        let name = div().min_w_0().truncate().child(format!("{key}  {}", t.title));
        let back = ui::icon_button_sized("factory-back", "back", 24., TEXT_2).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.leave_factory_ticket(window, cx)));
        let board = div().id("factory-crumb").px(px(4.)).h(px(24.)).flex().flex_none().items_center().gap(px(6.)).rounded(px(6.)).cursor_pointer().font_weight(FontWeight::MEDIUM).text_color(TEXT_2).hover(|s| s.bg(FILL_2).text_color(TEXT)).child(icon("blocks", 13., TEXT_3)).child("Board").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.leave_factory_ticket(window, cx)));
        let crumb = div().flex_1().min_w_0().flex().items_center().gap(px(4.)).child(back).child(board).child(icon("chevron-right", 11., TEXT_4)).child(name);
        let lead = t.agent_id.clone();
        let open = self.agents.get(&lead).is_some().then(|| bar_button("factory-open-session", Variant::Secondary, None, "Open session").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.focus_agent(&lead, window, cx))));
        self.detail_bar(crumb.ml(px(2.)), div().flex().items_center().gap(px(6.)).children(open), cx).h(px(44.))
    }

    fn factory_status(&self, t: &Ticket, col: Column) -> Div {
        let you = matches!(t.gate, Gate::Plan | Gate::Pr | Gate::Ask);
        let failed = t.status == Status::Failed;
        let dots = track(col).map(|d| {
            let (w, color) = match d {
                Dot::Done => (4., TEXT_3),
                Dot::At => (10., if you { WAITING_DOT } else { TEXT }),
                Dot::Ahead => (4., FILL_4),
            };
            div().w(px(w)).h(px(4.)).rounded(px(2.)).bg(color)
        });
        div()
            .flex()
            .items_center()
            .gap(px(7.))
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .child(station_glyph(col, t.gate, failed))
            .child(div().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(col.name()))
            .child(div().text_color(if you { WAITING_TEXT } else if failed { FAILED_TEXT } else { TEXT_2 }).child(status_text(t)))
            .child(div().ml(px(4.)).flex().items_center().gap(px(3.)).children(dots))
    }

    fn factory_tabs(&self, tab: Tab, cx: &mut Context<Self>) -> Div {
        let items = OFFERED.iter().map(|t| Segment { icon: None, value: *t, label: t.label().into(), badge: None }).collect();
        div().mt(px(6.)).flex().child(ui::segmented(items, tab, true, false, |this: &mut Desktop, t, cx| this.factory_tab(t, cx), cx))
    }

    fn factory_properties(&self, t: &Ticket, col: Column) -> Div {
        let chip = |text: String| prop_chip().child(text).into_any_element();
        let failed = t.status == Status::Failed;
        let status = prop_chip().child(station_glyph(col, t.gate, failed)).child(col.name()).into_any_element();
        let priority = prop_chip().child(priority_glyph(t.priority)).child(priority_name(t.priority)).into_any_element();
        let labels = if t.labels.is_empty() { chip("No labels".into()) } else { prop_chip().children(t.labels.first().map(|l| label_dot(l))).child(t.labels.join(", ")).into_any_element() };
        let agent = prop_chip().child(agent_avatar("factory-prop-agent", &t.provider, self.agent_name(&t.provider))).child(self.agent_name(&t.provider)).into_any_element();
        let project = prop_chip().child(icon("folder", 13., TEXT_3)).child(self.repo_name(&t.folder)).into_any_element();
        let branch = self.agents.get(&t.agent_id).and_then(|a| self.worktree_of(&a.cwd)).map(|w| w.branch.clone()).unwrap_or_default();
        let tasks = self.agents.team(&t.agent_id).map_or(0, |team| team.tasks.len());
        let quiet = |text: String| div().text_color(TEXT_3).child(text).into_any_element();
        div()
            .w(px(280.))
            .flex_none()
            .px(px(16.))
            .py(px(14.))
            .border_l_1()
            .border_color(SEPARATOR)
            .flex()
            .flex_col()
            .child(prop_row("Status", status))
            .child(prop_row("Priority", priority))
            .child(prop_row("Labels", labels))
            .child(div().h(px(1.)).my(px(8.)).bg(HAIRLINE))
            .child(prop_row("Agent", agent))
            .child(prop_row("Project", project))
            .child(prop_row("Branch", if branch.is_empty() { quiet(branch_text(&branch)) } else { prop_chip().child(icon("branch", 12., TEXT_3)).child(div().min_w_0().truncate().font_family(MONO).child(branch)).into_any_element() }))
            .child(prop_row("PR", if t.pr == 0 { quiet(pr_text(0)) } else { chip(pr_text(t.pr)) }))
            .child(prop_row("Sub-issues", if tasks == 0 { quiet(tasks_text(0)) } else { chip(tasks_text(tasks)) }))
            .child(prop_row("Updated", chip(ago_long(t.changed_at, now_ms()))))
    }
}

#[cfg(test)]
mod tests {
    use super::{Dot, Page, branch_text, pr_text, status_text, tasks_text, track};
    use crate::factory::station::Column;
    use agents::tickets::{Gate, State, Station, Status, Ticket};

    fn live(station: Station, gate: Gate, status: Status) -> Ticket {
        Ticket { state: State::Live, station, gate, status, ..Ticket::sample() }
    }

    #[test]
    fn the_status_line_says_what_the_ticket_needs_of_you_or_is_at() {
        assert_eq!(status_text(&live(Station::Plan, Gate::Plan, Status::NeedsYou)), "needs you");
        assert_eq!(status_text(&live(Station::Spec, Gate::Ask, Status::NeedsYou)), "needs you");
        assert_eq!(status_text(&live(Station::Spec, Gate::None, Status::Failed)), "failed");
        assert_eq!(status_text(&live(Station::Spec, Gate::None, Status::Working)), "drafting the spec");
        assert_eq!(status_text(&live(Station::Plan, Gate::None, Status::Working)), "planning");
        assert_eq!(status_text(&live(Station::Implement, Gate::None, Status::Working)), "working");
        assert_eq!(status_text(&live(Station::Spec, Gate::None, Status::Starting)), "starting");
    }

    #[test]
    fn the_track_marks_the_stations_behind_and_the_one_it_is_at() {
        assert_eq!(track(Column::Plan), [Dot::Done, Dot::Done, Dot::At, Dot::Ahead, Dot::Ahead, Dot::Ahead, Dot::Ahead]);
        assert_eq!(track(Column::Todo)[0], Dot::At);
    }

    #[test]
    fn the_properties_say_when_an_empty_value_comes() {
        assert_eq!(branch_text(""), "Made when the lead starts");
        assert_eq!(branch_text("ticket-3f9a0c1b"), "ticket-3f9a0c1b");
        assert_eq!(pr_text(0), "Opens after Checks");
        assert_eq!(pr_text(12), "#12");
        assert_eq!(tasks_text(0), "Planned at Plan");
        assert_eq!(tasks_text(3), "3 tasks");
    }

    #[test]
    fn a_page_is_stale_once_its_ticket_is_gone_or_has_no_station() {
        let page = Page::of("t1");
        let at_plan = Ticket { id: "t1".into(), station: Station::Plan, ..Ticket::sample() };
        assert!(!page.stale(Some(&at_plan)));
        assert!(page.stale(Some(&Ticket { id: "t1".into(), ..Ticket::sample() })));
        assert!(page.stale(None));
    }

    #[test]
    fn a_page_asks_again_only_for_the_ticket_it_shows() {
        let page = Page::of("t1");
        assert!(page.shows("t1"));
        assert!(!page.shows("t2"));
    }
}
