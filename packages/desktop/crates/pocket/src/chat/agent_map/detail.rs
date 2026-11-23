use super::{AgentMap, Of};
use crate::chat::agent_detail::{self, Detail};
use crate::desktop::chrome::id;
use chat::Spawned;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::sync::Arc;
use theme::*;

fn button(key: String, label: &str) -> Stateful<Div> {
    div()
        .id(id(key))
        .h(px(30.))
        .px(px(12.))
        .flex()
        .items_center()
        .rounded(px(9.))
        .bg(FILL_3)
        .hover(|s| s.bg(FILL_4))
        .cursor_pointer()
        .text_size(px(12.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(TEXT)
        .child(label.to_string())
}

impl AgentMap {
    /// The details of `map[ix]` in the Facts layout the dock also draws, with a way back to the graph and to its place
    /// in the chat. `live` takes clicks, as the details do while they're the page showing.
    pub(super) fn detail(&self, map: &[Spawned], ix: usize, live: bool, now: i64, of: &Of, cx: &mut Context<Self>) -> Div {
        let agent = of.agent;
        let s = &map[ix];
        let key = format!("chat-map-detail-{agent}");
        let back = cx.listener(move |this, _: &ClickEvent, _, cx| {
            if live {
                this.nav.back();
                cx.notify();
            }
        });
        let close = cx.listener(move |this, _: &ClickEvent, _, cx| {
            if live {
                this.close(cx);
            }
        });
        let on_brief = cx.listener(move |this, _: &ClickEvent, _, cx| {
            if live {
                this.nav.brief_open ^= true;
                cx.notify();
            }
        });
        let on_more = cx.listener(move |this, _: &ClickEvent, _, cx| {
            if live {
                this.nav.all_steps ^= true;
                cx.notify();
            }
        });
        let links = self.links(agent, cx);
        let detail = Detail { key, call: s.call, agent: s.agent, now, all_steps: self.nav.all_steps, brief_open: self.nav.brief_open };
        let body = agent_detail::view(
            detail,
            Some(agent_detail::action(format!("chat-map-back:{agent}"), "back", back)),
            vec![agent_detail::action(format!("chat-map-close2:{agent}"), "x", close)],
            on_brief,
            on_more,
            of.tools,
            &links,
            cx,
        );
        let foot = div().flex_none().flex().gap(px(8.)).pt(px(10.)).pb(px(14.)).px(px(16.)).border_t(px(0.5)).border_color(HAIRLINE).child(
            button(format!("chat-map-reveal:{agent}"), "Show in chat").when(live, |d| {
                let agent = agent.to_string();
                let t = Arc::clone(of.transcript);
                d.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.reveal(&agent, &t, ix, cx)))
            }),
        );
        div().absolute().top_0().bottom_0().w_full().flex().flex_col().child(div().flex_1().min_h_0().child(body)).child(foot)
    }
}
