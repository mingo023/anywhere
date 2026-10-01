use agents::{Level, level, percent};
use gpui_kit::*;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::time::Duration;
use theme::*;

pub(crate) fn glyph(level: Level) -> Token {
    match level {
        Level::Low => TEXT_3,
        Level::Warn => WAITING,
        Level::Danger => FAILED,
    }
}

pub(crate) fn text(level: Level) -> Token {
    match level {
        Level::Low => TEXT_2,
        Level::Warn => WAITING_TEXT,
        Level::Danger => FAILED_TEXT,
    }
}

fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

struct ContextCard {
    used: u64,
    window: u64,
}

impl Render for ContextCard {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        ui::pop(div())
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .text_size(px(12.))
            .text_color(TEXT_2)
            .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("Context window"))
            .child(format!("{} / {} tokens", thousands(self.used), thousands(self.window)))
            .child(format!("{} tokens remaining", thousands(self.window.saturating_sub(self.used))))
    }
}

pub(crate) fn ring(used: u64, window: u64) -> Stateful<Div> {
    let share = percent(used, window) as f32 / 100.;
    let color = Hsla::from(glyph(level(used, window)));
    let arc = canvas(
        |_, _, _| {},
        move |bounds, _, win, _| {
            let r = px(6.);
            let c = bounds.center();
            let at = |t: f32| {
                let a = TAU * t - FRAC_PI_2;
                point(c.x + r * a.cos(), c.y + r * a.sin())
            };
            let mut track = PathBuilder::stroke(px(2.));
            track.move_to(at(0.));
            track.arc_to(point(r, r), px(0.), false, true, at(0.5));
            track.arc_to(point(r, r), px(0.), false, true, at(0.));
            if let Ok(p) = track.build() {
                win.paint_path(p, Hsla::from(SEPARATOR_STRONG));
            }
            if share > 0. {
                // An arc can't end where it starts, so a full ring stops just short.
                let mut fill = PathBuilder::stroke(px(2.));
                fill.move_to(at(0.));
                fill.arc_to(point(r, r), px(0.), share > 0.5, true, at(share.min(0.999)));
                if let Ok(p) = fill.build() {
                    win.paint_path(p, color);
                }
            }
        },
    )
    .size(px(16.));
    div()
        .id("context-ring")
        .flex_none()
        .size(px(16.))
        .child(arc)
        .tooltip(move |_, cx| cx.new(|_| ContextCard { used, window }).into())
        .tooltip_show_delay(Duration::from_millis(350))
}

#[cfg(test)]
mod tests {
    use super::{glyph, text, thousands};
    use agents::Level;
    use theme::{FAILED, FAILED_TEXT, TEXT_2, TEXT_3, WAITING, WAITING_TEXT};

    #[test]
    fn thousands_groups_digits() {
        assert_eq!([thousands(0), thousands(999), thousands(1_000), thousands(184_000), thousands(1_000_000)], ["0", "999", "1,000", "184,000", "1,000,000"]);
    }

    #[test]
    fn context_colours_follow_the_level() {
        assert_eq!([Level::Low, Level::Warn, Level::Danger].map(glyph), [TEXT_3, WAITING, FAILED]);
        assert_eq!([Level::Low, Level::Warn, Level::Danger].map(text), [TEXT_2, WAITING_TEXT, FAILED_TEXT]);
    }
}
