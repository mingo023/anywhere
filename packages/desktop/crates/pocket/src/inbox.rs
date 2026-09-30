use theme::*;
use ui::{self, State, Variant, icon_button, kbd};
use crate::status::Status;
use crate::view::{ago, basename, column, drag_area, empty, now_ms};
use crate::{Desktop, Screen, termview};
use agents::{Agents, Summary};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cmp::Reverse;

pub struct Note {
    pub agent: String,
    pub terminal: String,
    pub status: Status,
    pub title: String,
    pub subtitle: String,
    pub at: i64,
}

fn first_line(s: &str) -> String {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().to_string()
}

fn noted(a: &Summary) -> Option<Status> {
    Status::of(a).filter(Status::alerting)
}

/// Agents that need you, then the failed and done ones nobody has seen yet.
pub fn notes(agents: &Agents) -> Vec<Note> {
    let mut out: Vec<Note> = agents
        .list
        .iter()
        .filter_map(|a| {
            let status = noted(a)?;
            let (title, subtitle) = match (status, agents.pending.iter().find(|p| p.agent_id == a.id)) {
                (Status::NeedsYou, Some(p)) => (p.ask(), a.title.clone()),
                (Status::Failed, _) => (format!("{} failed", a.title), first_line(agents.last_result(&a.id).map_or("", |r| r.error.as_str()))),
                _ => (a.title.clone(), first_line(agents.last_text(&a.id).unwrap_or_default())),
            };
            Some(Note { agent: a.id.clone(), terminal: a.terminal_id.clone(), status, title, subtitle, at: a.updated_at })
        })
        .collect();
    out.sort_by_key(|n| (n.status, Reverse(n.at)));
    out
}

/// Keeps the selection on the focused note as the list changes. If that note left, drops focus
/// so keystrokes never reach a terminal the user didn't pick.
pub fn reselect(notes: &[Note], focused: Option<&str>, i: usize) -> (usize, Option<String>) {
    match notes.iter().position(|n| focused == Some(n.terminal.as_str())) {
        Some(j) => (j, Some(notes[j].terminal.clone())),
        None => (i.min(notes.len().saturating_sub(1)), None),
    }
}

impl Desktop {
    pub fn open_inbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = Screen::Inbox;
        self.select_note(0, cx);
        window.focus(&self.inbox_focus, cx);
    }

    fn select_note(&mut self, i: usize, cx: &mut Context<Self>) {
        let notes = notes(&self.agents);
        self.inbox = i.min(notes.len().saturating_sub(1));
        self.focused = notes.get(self.inbox).map(|n| n.terminal.clone());
        cx.notify();
    }

    fn on_inbox_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        match ev.keystroke.key.as_str() {
            "j" | "down" => self.select_note(self.inbox + 1, cx),
            "k" | "up" => self.select_note(self.inbox.saturating_sub(1), cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn project_name(&self, agent: &str) -> String {
        let projects = self.projects();
        let cwd = self.cwd_of(agent).unwrap_or_default();
        basename(self.project_of(&cwd, &projects).unwrap_or(&cwd))
    }

    pub fn inbox_list(&mut self, cx: &mut Context<Self>) -> Div {
        let notes = notes(&self.agents);
        let total = notes.len();
        let asks = notes.iter().filter(|n| n.status == Status::NeedsYou).count();
        let now = now_ms();
        let header = drag_area(div())
            .h(px(52.))
            .pl(px(16.))
            .pr(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .child(div().flex_1().text_size(px(17.)).font_weight(FontWeight::BOLD).child("Inbox"))
            .child(icon_button("inbox-filter", "filter"))
            .child(
                ui::button("mark-read", Variant::Ghost, None, "Mark all read").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        let ids: Vec<String> = crate::inbox::notes(&this.agents).into_iter().filter(|n| n.status != Status::NeedsYou).map(|n| n.agent).collect();
                        if !ids.is_empty() {
                            this.outbox.seen(&ids);
                        }
                        this.select_note(0, cx);
                    })),
            );
        let section = |label: &str, count: usize| {
            div()
                .pt(px(12.))
                .pb(px(4.))
                .px(px(12.))
                .flex()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgba(TEXT_3))
                .child(div().flex_1().child(label.to_string()))
                .child(div().font_family(MONO).font_weight(FontWeight::MEDIUM).child(count.to_string()))
        };
        let mut list = div()
            .id("notes")
            .flex_1()
            .overflow_y_scroll()
            .track_focus(&self.inbox_focus)
            .on_key_down(cx.listener(Self::on_inbox_key))
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(2.));
        for (i, n) in notes.into_iter().enumerate() {
            if i == 0 && asks > 0 {
                list = list.child(section("NEEDS YOU", asks));
            }
            if i == asks {
                list = list.child(section("DONE", total - asks));
            }
            list = list.child(self.note_row(i, n, now, cx));
        }
        if total == 0 {
            list = list.child(empty("Nothing needs you."));
        }
        column().child(header).child(list)
    }

    fn note_row(&self, i: usize, n: Note, now: i64, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = i == self.inbox;
        let (glyph, color) = match n.status {
            Status::NeedsYou => ("shield", WAITING_TEXT),
            Status::Failed => ("x", FAILED),
            _ => ("check", ACCENT),
        };
        let project = self.project_name(&n.agent);
        let provider = self.agents.get(&n.agent).map(|a| provider_name(&a.provider)).unwrap_or("Shell");
        div()
            .id(("note", i))
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_none()
            .gap(px(12.))
            .rounded(px(12.))
            .cursor_pointer()
            .when(selected, |d| d.bg(rgba(FILL_3)))
            .when(!selected, |d| d.hover(|s| s.bg(rgba(FILL_1))))
            .child(
                div()
                    .size(px(26.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .bg(rgba(SURFACE))
                    .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
                    .child(icon(glyph, 13., color)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(rgba(TEXT_2))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(project))
                            .child(div().text_color(rgba(TEXT_6)).child("·"))
                            .child(div().flex_1().truncate().child(provider))
                            .child(div().text_color(rgba(TEXT_4)).child(ago(n.at, now))),
                    )
                    .child(div().truncate().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(n.title))
                    .child(div().truncate().text_size(px(12.)).text_color(rgba(TEXT_2)).child(n.subtitle)),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.select_note(i, cx);
                window.focus(&this.inbox_focus, cx);
            }))
    }

    pub fn inbox_detail(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(n) = notes(&self.agents).into_iter().nth(self.inbox) else {
            return drag_area(div()).flex_1().flex().items_center().justify_center().text_size(px(14.)).text_color(rgba(TEXT_3)).child("You're all caught up.");
        };
        let project = self.project_name(&n.agent);
        let title = self.agents.get(&n.agent).map(|a| a.title.clone()).unwrap_or_default();
        let secs = (now_ms() - n.at).max(0) / 1000;
        let agent = n.agent.clone();
        let header = drag_area(div())
            .h(px(52.))
            .flex_none()
            .pl(px(20.))
            .pr(px(20.))
            .flex()
            .items_center()
            .gap(px(10.))
            .text_size(px(14.))
            .child(ui::repo_mark(&project, false, None))
            .child(div().text_color(rgba(TEXT_2)).child(project))
            .child(div().text_color(rgba(TEXT_6)).child("/"))
            .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(title))
            .when(n.status == Status::NeedsYou, |d| {
                d.child(ui::status("needs-you", State::NeedsYou)).child(div().font_family(MONO).text_size(px(11.5)).text_color(rgba(WAITING_TEXT)).child(format!("{}:{:02}", secs / 60, secs % 60)))
            })
            .child(div().flex_1())
            .child(ui::button("open-session", Variant::Secondary, None, "Open session").child(icon("forward", 14., TEXT)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.focus_agent(&agent, window, cx)
            })));
        let pane = self.pane(&n.terminal, None, &termview::MAIN, cx);
        let hints = div()
            .h(px(36.))
            .flex_none()
            .px(px(20.))
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(rgba(TEXT_2))
            .child("Answer in the terminal ·")
            .child(kbd("J"))
            .child(kbd("K"))
            .child("next / previous ·")
            .child(kbd("⌘ ↵"))
            .child("open session");
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(div().flex_1().min_h_0().px(px(10.)).pt(px(10.)).flex().track_focus(&self.term_focus).on_key_down(cx.listener(Self::on_term_key)).child(pane))
            .child(hints)
    }
}

#[cfg(test)]
mod tests {
    use super::{Note, first_line, notes, reselect};
    use crate::status::Status;
    use agents::{Agents, Item, Permission, Summary};

    fn agent(id: &str, status: &str, at: i64) -> Summary {
        Summary { id: id.into(), terminal_id: format!("t-{id}"), title: id.to_uppercase(), status: status.into(), attached: true, updated_at: at, ..Default::default() }
    }

    #[test]
    fn selection_follows_the_focused_note_else_clamps_and_drops_focus() {
        let note = |t: &str| Note { agent: t.into(), terminal: t.into(), status: Status::NeedsYou, title: String::new(), subtitle: String::new(), at: 0 };
        assert_eq!(reselect(&[note("new"), note("a"), note("b")], Some("a"), 0), (1, Some("a".to_string())));
        assert_eq!(reselect(&[note("a"), note("b")], Some("c"), 2), (1, None));
        assert_eq!(reselect(&[note("a")], None, 4), (0, None));
    }

    #[test]
    fn lists_agents_that_need_you_then_failed_then_done() {
        let mut agents = Agents::default();
        agents.list = vec![agent("done", "done", 3), Summary { failed: true, ..agent("fail", "done", 1) }, agent("ask", "needsYou", 0), agent("busy", "working", 9), Summary { attached: false, ..agent("blind", "done", 9) }];
        agents.pending = vec![Permission { agent_id: "ask".into(), tool_name: "Bash".into(), ..Default::default() }];
        agents.timelines.insert("done".into(), vec![Item { kind: "assistant".into(), text: "\nTagged v2\nmore".into(), ..Default::default() }]);
        agents.timelines.insert("fail".into(), vec![Item { kind: "result".into(), error: "exit 1".into(), ..Default::default() }]);
        let got: Vec<(String, Status, String, String)> = notes(&agents).into_iter().map(|n| (n.terminal, n.status, n.title, n.subtitle)).collect();
        let want = [("t-ask", Status::NeedsYou, "Wants to use Bash", "ASK"), ("t-fail", Status::Failed, "FAIL failed", "exit 1"), ("t-done", Status::Done, "DONE", "Tagged v2")];
        assert_eq!(got, want.map(|(t, s, a, b)| (t.to_string(), s, a.to_string(), b.to_string())));
    }

    #[test]
    fn lists_the_newest_first_within_a_section() {
        let mut agents = Agents::default();
        agents.list = vec![agent("old", "done", 1), agent("ask", "needsYou", 2), agent("new", "done", 5), agent("ask2", "needsYou", 7)];
        let got: Vec<String> = notes(&agents).into_iter().map(|n| n.agent).collect();
        assert_eq!(got, vec!["ask2", "ask", "new", "old"]);
    }

    #[test]
    fn a_needs_you_agent_without_an_open_ask_reads_its_title_and_last_text() {
        let mut agents = Agents::default();
        agents.list = vec![agent("ask", "needsYou", 0)];
        agents.timelines.insert("ask".into(), vec![Item { kind: "assistant".into(), text: "Which branch?".into(), ..Default::default() }]);
        let n = &notes(&agents)[0];
        assert_eq!((n.title.as_str(), n.subtitle.as_str()), ("ASK", "Which branch?"));
    }

    #[test]
    fn first_line_skips_blank_lines() {
        assert_eq!(first_line("\n  Tagged, changelog updated\nmore"), "Tagged, changelog updated");
        assert_eq!(first_line(""), "");
    }
}
