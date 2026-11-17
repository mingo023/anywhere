use crate::desktop::Desktop;
use crate::fleet::Links;
use agents::hosts::Link;
use gpui_kit::*;
use hosts::Host;
use theme::*;

/// Where the host an agent's card asks for stands, from the hosts this Mac links now.
#[derive(Debug, PartialEq)]
pub(super) enum Linked {
    No,
    Connecting(String),
    Online(String),
    /// Listed, but its link failed or is in a state this app doesn't know.
    Trouble(String),
}

/// What `hint` names, an id, a name or an address, matched against the current host list: so a card settles by what the list says, whatever happened to the link attempt it asked for.
pub(super) fn linked(hint: &str, list: &[Host], links: &Links) -> Linked {
    let hint = hint.trim();
    let Some(host) = list.iter().find(|h| !hint.is_empty() && [h.id.as_str(), h.name.as_str(), h.address().as_str()].iter().any(|n| n.eq_ignore_ascii_case(hint))) else { return Linked::No };
    let name = host.name.clone();
    match links.get(&host.id) {
        Some(Link::Online { .. }) => Linked::Online(name),
        Some(Link::Failed(_) | Link::Unknown) => Linked::Trouble(name),
        Some(Link::Connecting | Link::Retrying(_)) | None => Linked::Connecting(name),
    }
}

/// An agent asks for a host this Mac doesn't link yet: why, and a button that opens the add form with the host it named. Once that host is listed the card says how its link stands instead.
pub(super) fn card(id: &str, hint: &str, reason: &str, linked: Linked, cx: &mut Context<Desktop>) -> Div {
    let tile = div().size(px(28.)).flex().flex_none().items_center().justify_center().rounded(px(7.)).bg(FILL_2).child(icon("server", 14., TEXT_2));
    let title = match (&linked, hint) {
        (Linked::Online(name), _) => format!("{name} is linked"),
        (Linked::Connecting(name), _) => format!("Linking {name}\u{2026}"),
        (Linked::Trouble(name), _) => format!("Couldn\u{2019}t link {name}"),
        (Linked::No, "") => "Add a host".to_string(),
        (Linked::No, name) => format!("Add {name} as a host"),
    };
    let text = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(1.))
        .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(title))
        .children((!reason.is_empty()).then(|| div().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(reason.to_string())));
    let hint = hint.to_string();
    let action = match linked {
        Linked::No => Some(ui::button(SharedString::from(format!("host-link-{id}")), ui::Variant::Secondary, None, "Add host\u{2026}").h(px(26.)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.link_host(hint.clone(), window, cx)))),
        Linked::Trouble(_) => Some(ui::button(SharedString::from(format!("host-link-{id}")), ui::Variant::Secondary, None, "Hosts\u{2026}").h(px(26.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.show_hosts(window, cx)))),
        Linked::Connecting(_) | Linked::Online(_) => None,
    };
    div().w_full().px(px(12.)).py(px(10.)).flex().items_center().gap(px(12.)).rounded(px(10.)).border_1().border_color(SEPARATOR).child(tile).child(text).children(action)
}

#[cfg(test)]
mod tests {
    use super::{Linked, linked};
    use crate::fleet::Links;
    use agents::hosts::{Failure, Link, Target};
    use hosts::Host;

    fn host(id: &str, name: &str, target: Target) -> Host {
        Host { id: id.into(), name: name.into(), target, windows: false, socket: Default::default() }
    }

    fn alias(name: &str) -> Target {
        Target::Alias { name: name.into() }
    }

    fn links(of: &[(&str, Link)]) -> Links {
        Links(of.iter().map(|(id, l)| (id.to_string(), l.clone())).collect())
    }

    #[test]
    fn a_card_waits_while_no_listed_host_matches_what_it_asks_for() {
        let list = [host("devbox", "devbox", alias("devbox"))];
        let up = links(&[("devbox", Link::Online { version: String::new(), uptime: 0 })]);
        assert_eq!([linked("vps", &list, &up), linked("", &list, &up), linked("vps", &[], &Links::default())], [Linked::No, Linked::No, Linked::No]);
    }

    #[test]
    fn a_card_settles_by_the_host_list_after_a_reconnect_with_no_link_attempt_to_report() {
        let list = [host("gpu-1", "GPU box", Target::Address { addr: "10.0.4.12".into(), port: 22, user: "dev".into(), key_file: None })];
        let state = |link: Option<Link>, hint: &str| linked(hint, &list, &links(&link.map(|l| ("gpu-1", l)).into_iter().collect::<Vec<_>>()));
        let online = || Some(Link::Online { version: "1.4.2".into(), uptime: 5 });
        for hint in ["gpu-1", "gpu box", "dev@10.0.4.12", " GPU box "] {
            assert_eq!(state(online(), hint), Linked::Online("GPU box".into()), "{hint}");
        }
        assert_eq!(state(Some(Link::Retrying(Failure::Dropped)), "gpu-1"), Linked::Connecting("GPU box".into()));
        assert_eq!(state(None, "gpu-1"), Linked::Connecting("GPU box".into()));
        assert_eq!(state(Some(Link::Failed(Failure::Auth)), "gpu-1"), Linked::Trouble("GPU box".into()));
        assert_eq!(state(Some(Link::Unknown), "gpu-1"), Linked::Trouble("GPU box".into()));
    }
}
