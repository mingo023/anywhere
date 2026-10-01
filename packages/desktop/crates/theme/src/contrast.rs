/// `top` (0xRRGGBBAA) painted over an opaque `bottom`, as an opaque colour.
pub fn over(top: u32, bottom: u32) -> u32 {
    let a = (top & 0xff) as f32 / 255.;
    let mix = |s: u32| (((top >> s) & 0xff) as f32 * a + ((bottom >> s) & 0xff) as f32 * (1. - a)).round() as u32;
    (mix(24) << 24) | (mix(16) << 16) | (mix(8) << 8) | 0xff
}

/// WCAG 2.x contrast ratio of `fg` over an opaque `bg`, with `fg`'s alpha composited first.
pub fn ratio(fg: u32, bg: u32) -> f32 {
    let (a, b) = (luminance(over(fg, bg)), luminance(bg));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn luminance(c: u32) -> f32 {
    let lin = |s: u32| {
        let v = ((c >> s) & 0xff) as f32 / 255.;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(24) + 0.7152 * lin(16) + 0.0722 * lin(8)
}

#[cfg(test)]
mod tests {
    use super::{over, ratio};

    #[test]
    fn black_on_white_is_twenty_one_to_one() {
        assert!((ratio(0x000000ff, 0xffffffff) - 21.).abs() < 0.01);
    }

    #[test]
    fn ratio_composites_alpha_over_the_background() {
        assert_eq!(over(0x00000080, 0xffffffff), 0x7f7f7fff);
        assert!((ratio(0x00000080, 0xffffffff) - ratio(0x7f7f7fff, 0xffffffff)).abs() < 0.001);
        assert!((ratio(0x00000000, 0xffffffff) - 1.).abs() < 0.001);
    }
}
