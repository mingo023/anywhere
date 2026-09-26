use crate::theme::*;
use crate::view::{ago, basename, button, drag_area, icon_button, kbd, now_ms, sidebar_frame, square};
use crate::{Desktop, Screen, termview};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cmp::Reverse;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Ask,
    Failed,
    Done,
}

pub struct Note {
    pub key: String,
    pub agent: String,
    pub kind: Kind,
    pub title: String,
    pub subtitle: String,
    pub at: i64,
}

fn first_line(s: &str) -> String {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().to_string()
}

impl Desktop {
    /// Open permission asks first, then today's finished turns that haven't been marked read.
    pub fn notes(&self) -> Vec<Note> {
        let title = |id: &str| self.agents.get(id).map(|a| a.title.clone()).unwrap_or_default();
        let mut asks: Vec<Note> = self
            .agents
            .pending
            .iter()
            .map(|p| Note { key: p.request_id.clone(), agent: p.agent_id.clone(), kind: Kind::Ask, title: p.ask(), subtitle: title(&p.agent_id), at: p.at })
            .collect();
        asks.sort_by_key(|n| Reverse(n.at));
        let today = chrono::Local::now().date_naive();
        let mut done: Vec<Note> = self
            .agents
            .list
            .iter()
            .filter_map(|a| {
                let r = self.agents.last_result(&a.id)?;
                let key = format!("{}:{}", a.id, r.id);
                let day = chrono::DateTime::from_timestamp_millis(r.ts)?.with_timezone(&chrono::Local).date_naive();
                if day != today || self.read.contains(&key) {
                    return None;
                }
                let (kind, title, subtitle) = if !r.failed() {
                    (Kind::Done, a.title.clone(), first_line(self.agents.last_text(&a.id).unwrap_or_default()))
                } else {
                    (Kind::Failed, format!("{} failed", a.title), first_line(&r.error))
                };
                Some(Note { key, agent: a.id.clone(), kind, title, subtitle, at: r.ts })
            })
            .collect();
        done.sort_by_key(|n| Reverse(n.at));
        asks.extend(done);
        asks
    }

    pub fn open_inbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = Screen::Inbox;
        self.select_note(0, cx);
        window.focus(&self.inbox_focus, cx);
    }

    fn select_note(&mut self, i: usize, cx: &mut Context<Self>) {
        let notes = self.notes();
        self.inbox = i.min(notes.len().saturating_sub(1));
        self.focused = notes.get(self.inbox).map(|n| n.agent.clone());
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

    fn project_badge(&self, agent: &str) -> (String, u32) {
        let projects = self.projects();
        let cwd = self.cwd_of(agent).unwrap_or_default();
        let i = self.project_of(&cwd, &projects).and_then(|p| projects.iter().position(|x| x == p));
        (i.map(|i| basename(&projects[i])).unwrap_or_else(|| basename(&cwd)), project_color(i.unwrap_or(0)))
    }

    pub fn inbox_list(&mut self, cx: &mut Context<Self>) -> Div {
        let notes = self.notes();
        let total = notes.len();
        let asks = notes.iter().filter(|n| n.kind == Kind::Ask).count();
        let now = now_ms();
        let header = drag_area(div())
            .h(px(52.))
            .pl(px(16.))
            .pr(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .child(div().flex_1().text_size(px(16.)).font_weight(FontWeight::BOLD).child("Inbox"))
            .child(icon_button("inbox-filter", "filter", 28., 15.))
            .child(
                div()
                    .id("mark-read")
                    .h(px(28.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .rounded(px(7.))
                    .text_size(px(13.5))
                    .text_color(rgb(MUTED))
                    .hover(|s| s.bg(rgb(HOVER)))
                    .child("Mark all read")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        let keys: Vec<String> = this.notes().into_iter().filter(|n| n.kind != Kind::Ask).map(|n| n.key).collect();
                        this.read.extend(keys);
                        this.select_note(this.inbox, cx);
                    })),
            );
        let section = |label: &str, count: usize| {
            div()
                .pt(px(14.))
                .pb(px(6.))
                .px(px(20.))
                .flex()
                .text_size(px(12.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(MUTED))
                .child(div().flex_1().child(label.to_string()))
                .child(div().font_family(MONO).font_weight(FontWeight::MEDIUM).child(count.to_string()))
        };
        let mut list = div()
            .id("notes")
            .flex_1()
            .overflow_y_scroll()
            .track_focus(&self.inbox_focus)
            .on_key_down(cx.listener(Self::on_inbox_key))
            .flex()
            .flex_col();
        for (i, n) in notes.into_iter().enumerate() {
            if i == 0 && asks > 0 {
                list = list.child(section("NEEDS YOU", asks));
            }
            if i == asks {
                list = list.child(section("TODAY", total - asks));
            }
            list = list.child(self.note_row(i, n, now, cx));
        }
        if total == 0 {
            list = list.child(div().p(px(20.)).text_size(px(13.5)).text_color(rgb(MUTED)).child("Nothing needs you."));
        }
        sidebar_frame().child(header).child(list)
    }

    fn note_row(&self, i: usize, n: Note, now: i64, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = i == self.inbox;
        let (glyph, color) = match n.kind {
            Kind::Ask => ("shield", AMBER_TEXT),
            Kind::Failed => ("x", RED),
            Kind::Done => ("check", GREEN),
        };
        let (project, pcolor) = self.project_badge(&n.agent);
        let provider = self.agents.get(&n.agent).map(|a| provider_name(&a.provider)).unwrap_or("Shell");
        div()
            .id(("note", i))
            .mx(px(8.))
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_none()
            .gap(px(12.))
            .rounded(px(10.))
            .when(selected, |d| d.bg(rgb(SELECTED)))
            .when(!selected, |d| d.hover(|s| s.bg(rgb(0xefede9))))
            .child(
                div()
                    .size(px(26.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .border_1()
                    .border_color(rgb(CARD))
                    .bg(rgb(WHITE))
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
                            .text_size(px(12.5))
                            .child(square(8., pcolor))
                            .child(div().font_weight(FontWeight::SEMIBOLD).text_color(rgb(TEXT)).child(project))
                            .child(div().flex_1().truncate().text_color(rgb(MUTED)).child(format!("· {provider}")))
                            .child(div().text_color(rgb(MUTED)).child(ago(n.at, now))),
                    )
                    .child(div().truncate().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(INK)).child(n.title))
                    .child(div().truncate().text_size(px(12.5)).text_color(rgb(MUTED)).child(n.subtitle)),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.select_note(i, cx);
                window.focus(&this.inbox_focus, cx);
            }))
    }

    pub fn inbox_detail(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(n) = self.notes().into_iter().nth(self.inbox) else {
            return drag_area(div()).flex_1().flex().items_center().justify_center().text_size(px(14.)).text_color(rgb(MUTED)).child("You're all caught up.");
        };
        let (project, pcolor) = self.project_badge(&n.agent);
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
            .text_size(px(14.5))
            .child(square(10., pcolor))
            .child(div().text_color(rgb(MUTED)).child(project))
            .child(div().text_color(rgb(FAINT)).child("/"))
            .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(title))
            .when(n.kind == Kind::Ask, |d| {
                d.child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(AMBER_TEXT))
                        .child(crate::view::dot(7., AMBER))
                        .child(format!("Waiting for input · {}:{:02}", secs / 60, secs % 60)),
                )
            })
            .child(div().flex_1())
            .child(button("open-session").child("Open session").child(icon("forward", 13., MUTED)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.select_session(agent.clone(), window, cx)
            })));
        let pane = self.pane(&n.agent, None, &termview::MAIN, cx);
        let hints = div()
            .h(px(36.))
            .flex_none()
            .px(px(20.))
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.5))
            .text_color(rgb(MUTED))
            .child("Answer in the terminal ·")
            .child(kbd("J"))
            .child(kbd("K"))
            .child("next / previous ·")
            .child(kbd("⌘↵"))
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
    use super::first_line;

    #[test]
    fn first_line_skips_blank_lines() {
        assert_eq!(first_line("\n  Tagged, changelog updated\nmore"), "Tagged, changelog updated");
        assert_eq!(first_line(""), "");
    }
}
