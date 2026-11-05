use gpui_kit::component::input::{InlineToken, InlineTokenContext, InputContent, Textarea, TextareaState};
use gpui_kit::*;
use theme::*;

/// The width a pill's × takes at its right end.
const PILL_REMOVE: f32 = 18.;

/// A command or skill picked into the prompt, as one piece; its × takes it out.
fn pill_token(t: &InlineTokenContext) -> Div {
    let ink = Hsla::from(COMMAND);
    div()
        .h(px(20.))
        .pl(px(6.))
        .flex()
        .items_center()
        .gap(px(4.))
        .rounded(px(6.))
        .bg(ink.opacity(0.1))
        .border_1()
        .border_color(ink.opacity(if t.is_selected() { 0.5 } else { 0.2 }))
        .text_size(px(12.5))
        .text_color(ink)
        .child(icon("sparkle", 11., ink))
        .child(t.token().label().clone())
        .child(div().w(px(PILL_REMOVE - 2.)).flex().justify_center().child(icon("x", 9., ink.opacity(0.7))))
}

/// The pill a picked `/name` shows as; the text stays the command.
pub(crate) fn command_token(name: &str) -> InlineToken {
    InlineToken::new(name.to_string(), format!("/{name}")).with_label(name.to_string())
}

/// `text` with the pills of `before` that its change left whole, moved to where they now sit, and `pill` at its offset.
pub(crate) fn kept_pills(before: &InputContent, text: String, pill: Option<(usize, InlineToken)>) -> InputContent {
    let old = before.text().as_ref();
    let start = shared_len(old.chars(), text.chars());
    let end = old.len() - shared_len(old[start..].chars().rev(), text[start..].chars().rev());
    let shift = text.len() as isize - old.len() as isize;
    let moved = before.tokens().iter().filter_map(|t| match t.range() {
        r if r.end <= start => Some((r, t.token().clone())),
        r if r.start >= end => Some((r.start.wrapping_add_signed(shift)..r.end.wrapping_add_signed(shift), t.token().clone())),
        _ => None,
    });
    let pill = pill.map(|(at, token)| (at..at + token.text().len(), token));
    moved.chain(pill).fold(InputContent::new(text), |content, (range, token)| content.clone().with_token(range, token).unwrap_or(content))
}

/// `textarea` showing picked commands as pills, a press on a pill's × taking it out. A press on a pill selects it first.
pub(crate) fn with_pills(textarea: Textarea, input: &Entity<TextareaState>) -> Textarea {
    let input = input.downgrade();
    textarea.token(|t, _, _| pill_token(t)).on_token_click(move |e, window, cx| {
        if e.click().position().x >= e.bounds().right() - px(PILL_REMOVE) {
            input.update(cx, |s, cx| s.replace_text_in_range(None, "", window, cx)).ok();
        }
    })
}

/// The bytes `a` and `b` share at their start.
fn shared_len(a: impl Iterator<Item = char>, b: impl Iterator<Item = char>) -> usize {
    a.zip(b).take_while(|(x, y)| x == y).map(|(x, _)| x.len_utf8()).sum()
}

#[cfg(test)]
mod tests {
    use super::{command_token, kept_pills};
    use gpui_kit::component::input::InputContent;

    fn pills(content: &InputContent) -> Vec<&str> {
        content.tokens().iter().map(|t| &content.text()[t.range()]).collect()
    }

    #[test]
    fn a_picked_command_turns_into_a_pill_and_the_others_keep_theirs() {
        let first = kept_pills(&InputContent::new("/pro"), "/prototype ".into(), Some((0, command_token("prototype"))));
        assert_eq!(pills(&first), ["/prototype"]);
        let second = kept_pills(&first, "/prototype /apple-design ".into(), Some((11, command_token("apple-design"))));
        assert_eq!(pills(&second), ["/prototype", "/apple-design"]);
        let mentioned = kept_pills(&InputContent::new("a /prototype").with_token(2..12, command_token("prototype")).unwrap(), "@x.rs a /prototype".into(), None);
        assert_eq!(pills(&mentioned), ["/prototype"]);
        assert_eq!(mentioned.tokens()[0].range(), 8..18);
    }

    #[test]
    fn a_pill_whose_text_changed_is_text_again() {
        let held = InputContent::new("/review now").with_token(0..7, command_token("review")).unwrap();
        assert!(kept_pills(&held, "/rev now".into(), None).tokens().is_empty());
    }
}
