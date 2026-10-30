use agents::Level;
use theme::*;

pub(crate) fn glyph(level: Level) -> Token {
    match level {
        Level::Low => TEXT_3,
        Level::Warn => WAITING,
        Level::Danger => FAILED,
    }
}

pub(crate) fn thousands(n: u64) -> String {
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

#[cfg(test)]
mod tests {
    use super::{glyph, thousands};
    use agents::Level;
    use theme::{FAILED, TEXT_3, WAITING};

    #[test]
    fn thousands_groups_digits() {
        assert_eq!([thousands(0), thousands(999), thousands(1_000), thousands(184_000), thousands(1_000_000)], ["0", "999", "1,000", "184,000", "1,000,000"]);
    }

    #[test]
    fn context_colours_follow_the_level() {
        assert_eq!([Level::Low, Level::Warn, Level::Danger].map(glyph), [TEXT_3, WAITING, FAILED]);
    }
}
