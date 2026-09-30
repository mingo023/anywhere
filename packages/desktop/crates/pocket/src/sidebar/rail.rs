use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, drag_area, state};
use crate::status::{Card, Status};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, State};

/// The rail's sessions: the busy ones and the selected one, most urgent first.
fn live(cards: Vec<Card>, selected: Option<&str>) -> Vec<Card> {
    let mut live: Vec<Card> = cards.into_iter().filter(|c| c.status != Status::Idle || selected == Some(c.id.as_str())).collect();
    live.sort_by_key(|c| c.status);
    live
}

impl Desktop {
    /// The compact layout's rail: the project's non-idle sessions and the selected one, other non-idle repositories, and new session.
    pub(crate) fn nav(&self, cx: &mut Context<Self>) -> Div {
        let rule = || div().w(px(28.)).h(px(0.5)).my(px(4.)).flex_none().bg(rgba(SEPARATOR_STRONG));
        let mark = |name: &str, selected: bool, state: Option<State>| {
            ui::repo_mark(name, selected, state).size(px(24.)).text_size(px(12.))
        };
        let toggle = div()
            .id("nav-panel")
            .w(px(36.))
            .h(px(32.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .cursor_pointer()
            .when(self.panel, |d| d.bg(rgba(FILL_3)))
            .hover(|s| s.bg(rgba(FILL_3)))
            .child(icon("sidebar", 18., TEXT_2))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_rail(&crate::actions::ToggleRail, window, cx)));
        let project = self.project.clone().unwrap_or_default();
        let badge = |d: Div, color: u32| d.absolute().right(px(3.)).size(px(8.)).rounded(px(4.)).bg(rgba(color)).shadow(vec![ui::ring(SURFACE_SUNKEN, 2.)]);
        let sessions = live(self.cards(&project), self.session.as_deref()).into_iter().enumerate().map(|(i, c)| {
            let selected = self.session.as_ref() == Some(&c.id);
            let id = c.id.clone();
            div()
                .id(("nav-session", i))
                .relative()
                .size(px(36.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(9.))
                .cursor_pointer()
                .when(selected, |d| d.bg(rgba(WHITE)).shadow(ui::row_shadow()))
                .when(!selected, |d| d.hover(|s| s.bg(rgba(FILL_2))))
                .child(icon("terminal", 17., provider_color(&c.provider)))
                .children(ui::alert_color(state(c.status, 0, 0)).map(|color| badge(div().top(px(3.)), color)))
                .when(c.status == Status::Working, |d| d.child(badge(div().bottom(px(3.)), RUNNING)))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.focus_agent(&id, window, cx)))
        });
        let repos = self.projects().into_iter().filter(|p| *p != project).filter_map(|p| self.project_state(&p).map(|st| (p, st))).enumerate().map(|(i, (p, st))| {
            div()
                .id(("nav-repo", i))
                .w(px(36.))
                .h(px(34.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(9.))
                .cursor_pointer()
                .hover(|s| s.bg(rgba(FILL_2)))
                .child(mark(&self.repo_name(&p), false, Some(st)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.panel = true;
                    this.select_project(p.clone(), cx);
                }))
        });
        let compose = div()
            .id("nav-compose")
            .size(px(36.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(9.))
            .cursor_pointer()
            .child(icon("compose", 17., WHITE))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::NewSession, window, cx)));
        let bars = self.usage().into_iter().map(|(p, left)| {
            div().w(px(24.)).h(px(3.)).flex().rounded(px(2.)).bg(rgba(SEPARATOR_STRONG)).child(div().w(relative(left as f32 / 100.)).rounded(px(2.)).bg(rgba(provider_color(p))))
        });
        let me = if self.initials.is_empty() { "ME".to_string() } else { self.initials.clone() };
        drag_area(ui::side(div()))
            .w(px(56.))
            .flex_none()
            .h_full()
            .pt(px(37.))
            .pb(px(12.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.))
            .child(toggle)
            .child(rule())
            .child(div().mb(px(2.)).child(mark(&self.repo_name(&project), true, None)))
            .children(sessions)
            .child(rule())
            .children(repos)
            .child(
                div()
                    .mt_auto()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.))
                    .child(ui::primary(compose))
                    .child(div().py(px(4.)).flex().flex_col().items_center().gap(px(3.)).children(bars))
                    .child(ui::avatar(&me, 30.).text_size(px(10.5))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::live;
    use crate::status::{self, Card};
    use agents::Summary;

    fn card(id: &str, status: &str) -> Card {
        status::card(&Summary { id: id.into(), status: status.into(), attached: true, ..Default::default() }, "/p")
    }

    #[test]
    fn the_rail_shows_busy_sessions_and_the_selected_one_most_urgent_first() {
        let cards = vec![card("idle", "idle"), card("work", "working"), card("picked", "idle"), card("ask", "needsYou"), card("done", "done")];
        let ids: Vec<String> = live(cards, Some("picked")).into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["ask", "done", "work", "picked"]);
    }
}
