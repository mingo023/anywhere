//! The strip over a child session's chat: its parent, which it focuses, then the session.

use crate::desktop::Desktop;
use crate::status;
use agents::Agents;
use gpui_kit::*;
use theme::*;

/// What the breadcrumb over a chat names.
#[derive(Debug, PartialEq)]
pub(crate) struct Trail {
    pub(crate) parent: String,
    pub(crate) parent_title: String,
    pub(crate) title: String,
}

/// The breadcrumb over `agent`'s chat; none unless the session it was started under is listed.
pub(crate) fn trail(agents: &Agents, agent: &str) -> Option<Trail> {
    let a = agents.get(agent)?;
    let parent = agents.get(&a.parent)?;
    Some(Trail { parent: parent.id.clone(), parent_title: status::title(parent), title: status::title(a) })
}

impl Desktop {
    pub(super) fn chat_breadcrumb(&self, agent: &str, cx: &mut Context<Self>) -> Option<Div> {
        let Trail { parent, parent_title, title } = trail(&self.agents, agent)?;
        let back = div()
            .id("chat-parent")
            .h(px(24.))
            .px(px(6.))
            .rounded(px(6.))
            .max_w(px(220.))
            .truncate()
            .line_height(px(24.))
            .text_color(TEXT_3)
            .cursor_pointer()
            .hover(|s| s.bg(FILL_3).text_color(TEXT))
            .child(parent_title)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.focus_agent(&parent, window, cx)));
        Some(
            div()
                .h(px(36.))
                .flex_none()
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .border_b(px(0.5))
                .border_color(SEPARATOR)
                .child(back)
                .child(div().text_color(TEXT_5).child("›"))
                .child(div().min_w_0().truncate().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Trail, trail};
    use agents::{Agents, Summary};

    fn listing(list: Vec<Summary>) -> Agents {
        Agents { list, ..Default::default() }
    }

    fn session(id: &str, title: &str, parent: &str) -> Summary {
        Summary { id: id.into(), title: title.into(), parent: parent.into(), ..Default::default() }
    }

    #[test]
    fn a_childs_chat_leads_back_to_its_parent() {
        let agents = listing(vec![session("lead", "Refactor the fold", ""), session("w1", "Split fold.rs", "lead")]);
        assert_eq!(trail(&agents, "w1"), Some(Trail { parent: "lead".into(), parent_title: "Refactor the fold".into(), title: "Split fold.rs".into() }));
    }

    #[test]
    fn a_session_without_a_listed_parent_has_no_breadcrumb() {
        let agents = listing(vec![session("lead", "Refactor the fold", ""), session("w1", "Split fold.rs", "gone")]);
        assert_eq!((trail(&agents, "lead"), trail(&agents, "w1")), (None, None));
    }
}
