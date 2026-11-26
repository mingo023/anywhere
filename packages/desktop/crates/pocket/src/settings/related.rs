use super::{Section, Settings, hint_link};
use gpui_kit::*;
use theme::*;

/// A pointer from the foot of a group to the page that sets what the group leaves out.
pub(super) struct Related {
    pub lead: &'static str,
    pub to: Section,
    pub label: &'static str,
}

/// The pages and groups that carry one: (page, group, pointer).
const LINKS: &[(Section, &str, Related)] = &[
    (Section::Files, "Tabs", Related { lead: "Whether a new tab starts a shell or an agent is set in", to: Section::Agents, label: "Agents \u{203a} Tabs" }),
    (Section::Terminal, "Keyboard & mouse", Related { lead: "Terminal shortcuts are in", to: Section::Keyboard, label: "Keyboard" }),
    (Section::Agents, "Tabs", Related { lead: "Where tabs sit and how they preview is set in", to: Section::Files, label: "Files & Editor \u{203a} Tabs" }),
    (Section::Git, "GitHub", Related { lead: "The accounts your agents sign in with are in", to: Section::Accounts, label: "Accounts" }),
];

/// The pointer under `group` of `section`'s page.
pub(super) fn of(section: Section, group: &str) -> Option<&'static Related> {
    LINKS.iter().find(|(s, g, _)| *s == section && *g == group).map(|(_, _, r)| r)
}

impl Settings {
    /// "Terminal shortcuts are in Keyboard", the last word a link to that page.
    pub(super) fn related_line(&self, related: &'static Related, cx: &mut Context<Self>) -> Div {
        let to = related.to;
        let link = hint_link(SharedString::from(format!("related-{}", related.label)), related.label)
            .text_color(TEXT)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.show_section(to, cx)));
        div()
            .mt(px(8.))
            .mx(px(4.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(TEXT_3)
            .child(icon("info", 12., TEXT_3))
            .child(related.lead)
            .child(link)
    }
}

#[cfg(test)]
mod tests {
    use super::{LINKS, of};
    use crate::settings::Section;

    #[test]
    fn a_pointer_sits_under_a_group_its_page_has_and_leads_to_another_page() {
        for (section, group, related) in LINKS {
            assert!(section.rows().iter().any(|r| r.group == *group), "{section:?} has no group {group}");
            assert_ne!(related.to, *section, "{group} points at its own page");
        }
    }

    #[test]
    fn only_the_groups_that_name_a_related_page_get_a_pointer() {
        assert_eq!(of(Section::Files, "Tabs").map(|r| r.to), Some(Section::Agents));
        assert_eq!(of(Section::Agents, "Tabs").map(|r| r.to), Some(Section::Files));
        assert_eq!(of(Section::Git, "GitHub").map(|r| r.to), Some(Section::Accounts));
        assert!(of(Section::Files, "Editor").is_none());
        assert!(of(Section::Terminal, "Tabs").is_none());
    }
}
