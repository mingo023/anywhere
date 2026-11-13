//! What the page's picker script posts when the user picks an element.

use serde::Deserialize;

/// An element picked on a page, as the agent reads it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Element {
    pub url: String,
    pub selector: String,
    /// Its tag and id, else its first class: `div.Artwork__HkJNWQ`.
    pub name: String,
    /// What it says, on one line; empty when it says nothing.
    pub text: String,
    /// Its outerHTML, cut to about 2KB.
    pub html: String,
    pub width: u32,
    pub height: u32,
}

#[cfg(any(test, not(target_os = "linux")))]
pub(crate) const SCRIPT: &str = include_str!("pick.js");

#[cfg(any(test, not(target_os = "linux")))]
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Message {
    Picked(Element),
    Ended,
}

/// The event an ipc `body` from the picker means, if it is one.
#[cfg(any(test, not(target_os = "linux")))]
pub(crate) fn event(body: &str) -> Option<crate::Event> {
    match serde_json::from_str(body).ok()? {
        Message::Picked(e) => Some(crate::Event::Picked(e)),
        Message::Ended => Some(crate::Event::PickEnded),
    }
}

#[cfg(test)]
mod tests {
    use super::{Element, event};
    use crate::Event;

    #[test]
    fn a_picked_message_reads_as_the_element_it_names() {
        let body = r#"{"kind":"picked","url":"https://github.com/","selector":"form > input:nth-of-type(1)","name":"input.Primer_Brand__TextInput","text":"Enter your email","html":"<input type=\"email\">","width":420,"height":41}"#;
        let Some(Event::Picked(e)) = event(body) else { panic!("not a pick") };
        assert_eq!(
            e,
            Element {
                url: "https://github.com/".into(),
                selector: "form > input:nth-of-type(1)".into(),
                name: "input.Primer_Brand__TextInput".into(),
                text: "Enter your email".into(),
                html: "<input type=\"email\">".into(),
                width: 420,
                height: 41,
            }
        );
    }

    #[test]
    fn an_ended_message_says_the_picker_turned_off() {
        assert!(matches!(event(r#"{"kind":"ended"}"#), Some(Event::PickEnded)));
    }

    #[test]
    fn anything_else_a_page_posts_is_ignored() {
        for body in ["moved", "", "{}", r#"{"kind":"picked"}"#, r#"{"kind":"other"}"#] {
            assert!(event(body).is_none(), "{body}");
        }
    }
}
