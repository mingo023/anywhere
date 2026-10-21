use super::shimmer::shimmer;
use super::working::format_elapsed;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// What a live fold says the agent is doing.
#[derive(Debug, PartialEq)]
pub(crate) enum Live {
    Stopping,
    Thinking,
    /// `background`: sub-agents and commands still running after the turn that began them ended.
    Working { secs: i64, background: usize },
    Approval,
    Answer,
}

/// The agent as the fold names it: "Claude", "Codex".
pub(crate) fn who(provider: &str) -> &'static str {
    brand(provider).map_or("Agent", |b| b.short)
}

/// "Opus 5.5 working for 7s · in the background"; the agent's name stands in for an unknown model.
pub(crate) fn live_label(who: &str, model: &str, live: &Live) -> String {
    match live {
        Live::Stopping => "Stopping…".into(),
        Live::Thinking => "Thinking…".into(),
        Live::Approval => "Needs your approval".into(),
        Live::Answer => "Needs your answer".into(),
        Live::Working { secs, background } => {
            let name = if model.is_empty() { who } else { model };
            let rest = match background {
                0 => String::new(),
                1 => " · in the background".into(),
                n => format!(" · {n} in the background"),
            };
            format!("{name} working for {}{rest}", format_elapsed(*secs))
        }
    }
}

/// "Claude worked for 1m 4s".
pub(crate) fn settled_label(who: &str, ms: u64) -> String {
    format!("{who} worked for {}", format_elapsed((ms / 1000) as i64))
}

fn glyph(provider: &str) -> Svg {
    provider_icon(provider, 14., AGENT_CODEX)
}

fn line() -> Div {
    div().py(px(4.)).flex().items_center().gap(px(7.)).text_size(px(14.)).line_height(px(20.))
}

fn slot(child: impl IntoElement) -> Div {
    div().size(px(16.)).flex_none().flex().items_center().justify_center().child(child)
}

/// The line heading a live reply: the agent's mark and what it's doing, light sweeping across it.
pub(super) fn live(id: &str, provider: &str, label: String) -> Div {
    line().child(slot(glyph(provider))).child(div().min_w_0().flex_1().child(shimmer(SharedString::from(format!("fold-{id}")), label, TEXT_3)))
}

/// The line heading a settled reply; it opens the work behind the answer.
pub(super) fn settled(id: &str, provider: &str, label: String, open: bool, on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Div {
    let mark = if open { icon("chevron-down", 14., TEXT_4) } else { glyph(provider) };
    line().child(
        div()
            .id(SharedString::from(format!("fold-{id}")))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(7.))
            .text_color(TEXT_3)
            .cursor_pointer()
            .hover(|s| s.text_color(TEXT_2))
            .on_click(on_toggle)
            .child(slot(mark))
            .child(div().min_w_0().truncate().child(label))
            .when(!open, |d| d.child(icon("chevron-right", 13., TEXT_5))),
    )
}

#[cfg(test)]
mod tests {
    use super::{Live, live_label, settled_label, who};

    #[test]
    fn a_live_fold_says_who_works_and_for_how_long() {
        assert_eq!(live_label(who("claude"), "Opus 5.5", &Live::Working { secs: 7, background: 0 }), "Opus 5.5 working for 7s");
        assert_eq!(live_label(who("codex"), "", &Live::Working { secs: 64, background: 0 }), "Codex working for 1m 4s");
        assert_eq!(live_label("Claude", "Opus 5.5", &Live::Working { secs: 3, background: 1 }), "Opus 5.5 working for 3s · in the background");
        assert_eq!(live_label("Claude", "Opus 5.5", &Live::Working { secs: 3, background: 2 }), "Opus 5.5 working for 3s · 2 in the background");
        assert_eq!(live_label("Claude", "Opus 5.5", &Live::Thinking), "Thinking…");
        assert_eq!(live_label("Claude", "Opus 5.5", &Live::Stopping), "Stopping…");
        assert_eq!(live_label("Claude", "Opus 5.5", &Live::Approval), "Needs your approval");
        assert_eq!(live_label("Claude", "Opus 5.5", &Live::Answer), "Needs your answer");
    }

    #[test]
    fn a_settled_fold_says_how_long_the_agent_worked() {
        assert_eq!(settled_label("Claude", 64_900), "Claude worked for 1m 4s");
    }
}
