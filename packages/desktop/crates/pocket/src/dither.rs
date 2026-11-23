use gpui_kit::*;
use std::cell::RefCell;
use std::sync::Arc;

const DOT_PX: f32 = 3.;
const NOISE_PX: f32 = 260.;
const MIN_COVERAGE: f32 = 0.5;
const MAX_COVERAGE: f32 = 0.9;
const DOT_ALPHA: f32 = 0.85;

const BAYER: [u8; 64] = [
    0, 32, 8, 40, 2, 34, 10, 42, 48, 16, 56, 24, 50, 18, 58, 26, 12, 44, 4, 36, 14, 46, 6, 38, 60, 28, 52, 20, 62, 30, 54, 22, 3, 35, 11, 43, 1, 33, 9, 41, 51, 19, 59, 27, 49, 17, 57, 25, 15, 47, 7, 39, 13, 45, 5, 37, 63, 31, 55, 23, 61, 29, 53, 21,
];

#[derive(Clone, Copy, PartialEq, Debug)]
struct Key {
    width: u32,
    height: u32,
    cell: u32,
    bgra: [u8; 4],
}

impl Key {
    fn new(area: Size<Pixels>, scale: f32, color: Hsla) -> Self {
        let Rgba { r, g, b, .. } = color.into();
        let byte = |c: f32| (c.clamp(0., 1.) * 255.).round() as u8;
        Key {
            width: (f32::from(area.width) * scale).ceil() as u32,
            height: (f32::from(area.height) * scale).ceil() as u32,
            cell: (DOT_PX * scale).round().max(1.) as u32,
            bgra: [byte(b), byte(g), byte(r), byte(DOT_ALPHA)],
        }
    }
}

struct Shown {
    key: Key,
    image: Arc<RenderImage>,
}

#[derive(Default)]
struct Dots {
    shown: Option<Shown>,
    building: Option<Key>,
}

// One bitmap the size of the display, anchored at the window's corner, so every backdrop clips the same dots and a resize builds nothing.
thread_local! {
    static DOTS: RefCell<Dots> = RefCell::default();
}

pub(crate) fn layer(color: Hsla) -> impl IntoElement {
    canvas(|_, _, _| {}, move |bounds, _, window, cx| paint(bounds, color, window, cx)).absolute().inset_0()
}

fn paint(bounds: Bounds<Pixels>, color: Hsla, window: &mut Window, cx: &mut App) {
    let viewport = window.viewport_size();
    // Display bounds are in platform points, while the viewport and scale factor are zoomed.
    let display = window.display(cx).map_or(viewport, |d| d.bounds().size.map(|v| v / window.zoom()));
    let area = size(display.width.max(viewport.width), display.height.max(viewport.height));
    let scale = window.scale_factor();
    let key = Key::new(area, scale, color);
    DOTS.with_borrow_mut(|dots| {
        if let Some(shown) = &dots.shown {
            let drawn = size(px(shown.key.width as f32 / scale), px(shown.key.height as f32 / scale));
            window.paint_image(bounds, Bounds::new(Point::default(), drawn), Corners::default(), shown.image.clone(), 0, false).ok();
        }
        if dots.shown.as_ref().is_some_and(|s| s.key == key) || dots.building == Some(key) {
            return;
        }
        dots.building = Some(key);
        let built = cx.background_spawn(async move { image(key) });
        window
            .spawn(cx, async move |cx| {
                let image = built.await;
                DOTS.with_borrow_mut(|dots| dots.building.take_if(|b| *b == key));
                cx.update(|window, _| {
                    let old = DOTS.with_borrow_mut(|dots| dots.shown.replace(Shown { key, image }));
                    if let Some(old) = old {
                        window.drop_image(old.image).ok();
                    }
                    window.refresh();
                })
                .ok();
            })
            .detach();
    });
}

fn image(key: Key) -> Arc<RenderImage> {
    let buffer = image::ImageBuffer::from_raw(key.width, key.height, bitmap(key)).expect("bitmap is width × height BGRA pixels");
    Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}

fn bitmap(key: Key) -> Vec<u8> {
    let Key { width, height, cell, bgra } = key;
    let stride = width as usize * 4;
    let mut out = vec![0; stride * height as usize];
    for row in 0..height.div_ceil(cell) {
        let top = (row * cell) as usize;
        let bottom = ((row + 1) * cell).min(height) as usize;
        let line = &mut out[top * stride..(top + 1) * stride];
        for col in (0..width.div_ceil(cell)).filter(|&c| lit(c, row)) {
            let (left, right) = ((col * cell) as usize, ((col + 1) * cell).min(width) as usize);
            for px in line[left * 4..right * 4].as_chunks_mut::<4>().0 {
                *px = bgra;
            }
        }
        for y in top + 1..bottom {
            out.copy_within(top * stride..(top + 1) * stride, y * stride);
        }
    }
    out
}

fn lit(col: u32, row: u32) -> bool {
    let density = MIN_COVERAGE + (MAX_COVERAGE - MIN_COVERAGE) * crate::field::fbm(col as f32 * DOT_PX / NOISE_PX, row as f32 * DOT_PX / NOISE_PX);
    let threshold = (BAYER[(row % 8 * 8 + col % 8) as usize] as f32 + 0.5) / 64.;
    density > threshold
}

#[cfg(test)]
mod tests {
    use super::{BAYER, Key, MAX_COVERAGE, MIN_COVERAGE, bitmap, lit};

    fn key(width: u32, height: u32, cell: u32) -> Key {
        Key { width, height, cell, bgra: [1, 2, 3, 200] }
    }

    #[test]
    fn the_bayer_matrix_holds_each_threshold_once() {
        let mut seen = BAYER.to_vec();
        seen.sort();
        assert_eq!(seen, (0..64).collect::<Vec<u8>>());
    }

    #[test]
    fn dots_cover_between_the_least_and_most_coverage() {
        let cells = 400 * 400;
        let covered = (0..400).flat_map(|r| (0..400).map(move |c| (c, r))).filter(|&(c, r)| lit(c, r)).count() as f32 / cells as f32;
        assert!(covered > MIN_COVERAGE && covered < MAX_COVERAGE, "{covered}");
    }

    #[test]
    fn each_dot_is_a_solid_square_of_its_colour() {
        let k = key(30, 21, 6);
        let px = bitmap(k);
        assert_eq!(px.len(), 30 * 21 * 4);
        for y in 0..21u32 {
            for x in 0..30u32 {
                let i = ((y * 30 + x) * 4) as usize;
                let want = if lit(x / 6, y / 6) { k.bgra } else { [0; 4] };
                assert_eq!(px[i..i + 4], want, "({x}, {y})");
            }
        }
    }

    #[test]
    fn the_pattern_stays_put_when_the_bitmap_grows() {
        let (small, big) = (bitmap(key(12, 12, 3)), bitmap(key(24, 24, 3)));
        for y in 0..12 {
            assert_eq!(small[y * 48..(y + 1) * 48], big[y * 96..y * 96 + 48]);
        }
    }
}
