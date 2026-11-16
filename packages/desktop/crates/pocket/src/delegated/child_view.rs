use crate::automations::logic::clock;
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::status;
use agents::Summary;
use chrono::{Local, TimeZone};
use std::fmt::Display;
use gpui_kit::*;
use theme::*;

/// The chat that delegated `id`, while it's listed.
pub(super) fn parent_of<'a>(list: &'a [Summary], id: &str) -> Option<&'a Summary> {
    let child = list.iter().find(|a| a.id == id).filter(|a| status::is_child(a))?;
    list.iter().find(|a| a.id == child.parent)
}

/// "Delegated by Claude · 15:34": who delegated it, and when it was delegated, if that is known.
fn by_label<Tz: TimeZone>(by: &str, at: i64, tz: &Tz) -> String
where
    Tz::Offset: Display,
{
    if at > 0 { format!("Delegated by {by} \u{b7} {}", clock(at, tz)) } else { format!("Delegated by {by}") }
}

impl Desktop {
    fn delegated_parent(&self, agent: &str) -> Option<&Summary> {
        parent_of(&self.agents.list, agent)
    }

    pub(crate) fn delegated_by(&self, agent: &str) -> Option<(String, String)> {
        let parent = self.delegated_parent(agent)?;
        let at = self.agents.delegations.of(&parent.id).iter().find(|d| d.child_id == agent).map_or_else(|| self.agents.get(agent).map_or(0, |a| a.created_at), |d| d.created_at);
        Some((parent.provider.clone(), by_label(provider_name(&parent.provider), at, &Local)))
    }

    /// `‹ <chat> / <session>` over a delegated session's chat; Back is the chat that delegated it.
    pub(crate) fn delegated_crumb(&self, agent: &str, cx: &mut Context<Self>) -> Option<Div> {
        let parent = self.delegated_parent(agent)?;
        let child = self.agents.get(agent)?;
        let to = parent.id.clone();
        let back = div()
            .id("delegated-back")
            .h(px(28.))
            .pl(px(6.))
            .pr(px(9.))
            .flex()
            .items_center()
            .gap(px(6.))
            .flex_shrink(1.)
            .min_w_0()
            .rounded(px(8.))
            .text_size(px(13.))
            .text_color(TEXT_3)
            .cursor_pointer()
            .hover(|s| s.bg(FILL_3).text_color(TEXT))
            .on_click(cx.listener(move |this, _, window, cx| this.delegated_open(&to, window, cx)))
            .child(icon("back", 14., TEXT_3))
            .child(div().min_w_0().truncate().child(status::title(parent)));
        Some(
            div()
                .flex_none()
                .h(px(48.))
                .px(px(14.))
                .flex()
                .items_center()
                .gap(px(4.))
                .child(back)
                .child(div().text_size(px(13.)).text_color(TEXT_6).child("/"))
                .child(
                    div()
                        .min_w_0()
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(7.))
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(TEXT)
                        .child(provider_icon(&child.provider, 13., TEXT))
                        .child(div().min_w_0().truncate().child(status::title(child))),
                )
                .child(div().flex_1())
                .child(ui::kbd("esc")),
        )
    }

    pub(crate) fn delegated_back(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(parent) = self.session().and_then(|id| self.delegated_parent(id)).map(|p| p.id.clone()) else { return false };
        self.delegated_open(&parent, window, cx);
        true
    }

    /// Esc on a working child stops it, as on any chat, so only an idle child goes back.
    pub(crate) fn delegated_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(agent) = self.session().filter(|_| self.screen == Screen::Sessions && self.overlay.is_none() && !self.new_form.shown) else { return false };
        let busy = self.agents.get(agent).is_some_and(|a| self.chat_busy(a));
        self.delegated_parent(agent).is_some() && !busy && self.delegated_back(window, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::{by_label, parent_of};
    use agents::Summary;
    use chrono::FixedOffset;

    fn session(id: &str, parent: &str, relationship: &str) -> Summary {
        Summary { id: id.into(), parent: parent.into(), relationship: relationship.into(), ..Default::default() }
    }

    #[test]
    fn a_delegated_session_goes_back_to_the_chat_that_delegated_it_while_that_is_listed() {
        let list = [session("p", "", ""), session("c", "p", "subagent"), session("f", "p", "fork"), session("o", "gone", "subagent")];
        assert_eq!(parent_of(&list, "c").map(|p| p.id.as_str()), Some("p"));
        assert_eq!([parent_of(&list, "p").is_none(), parent_of(&list, "f").is_none(), parent_of(&list, "o").is_none()], [true, true, true], "a fork, an orphan and a plain chat have none");
    }

    #[test]
    fn a_delegated_session_says_who_delegated_it_and_when_while_the_time_is_known() {
        let utc = FixedOffset::east_opt(0).unwrap();
        assert_eq!(by_label("Claude", 1_791_450_840_000, &utc), "Delegated by Claude \u{b7} 09:14");
        assert_eq!(by_label("Claude", 0, &utc), "Delegated by Claude");
    }
}
