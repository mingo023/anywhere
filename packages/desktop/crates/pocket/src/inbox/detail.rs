use crate::desktop::Desktop;
use crate::desktop::chrome::drag_area;
use crate::status::Status;
use crate::terminal_view::surface;
use crate::util::now_ms;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, State, Variant, kbd};

impl Desktop {
    pub fn inbox_detail(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(n) = self.shown_notes().into_iter().nth(self.inbox.selected) else {
            return drag_area(div()).flex_1().flex().items_center().justify_center().text_size(px(14.)).text_color(TEXT_3).child("You're all caught up.");
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
            .child(div().text_color(TEXT_2).child(project))
            .child(div().text_color(TEXT_6).child("/"))
            .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(title))
            .when(n.status == Status::NeedsYou, |d| {
                d.child(ui::status("needs-you", State::NeedsYou)).child(div().font_family(MONO).text_size(px(11.5)).text_color(WAITING_TEXT).child(format!("{}:{:02}", secs / 60, secs % 60)))
            })
            .child(div().flex_1())
            .child(ui::button("open-session", Variant::Secondary, None, "Open session").child(icon("forward", 14., TEXT)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.focus_agent(&agent, window, cx)
            })));
        let pane = self.pane(&n.terminal, &surface::Metrics::of(&self.store.terminal), cx);
        let hints = div()
            .h(px(36.))
            .flex_none()
            .px(px(20.))
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(TEXT_2)
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
            .child(div().flex_1().min_h_0().px(px(10.)).pt(px(10.)).flex().key_context(keys::CONTEXT).track_focus(&self.terminal.focus).on_key_down(cx.listener(Self::on_term_key)).child(pane))
            .child(hints)
    }
}
