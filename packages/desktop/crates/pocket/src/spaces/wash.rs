//! A space's theme washed over the window: its colours as gradients under everything, grain on them, the sidebar clear
//! over them and the page lifted onto them as a card.

use super::Space;
use super::theme_card::{CardEvent, ThemeCard};
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use std::sync::{Arc, OnceLock};
use store::prefs::sidebar::{LookColour, LookMode, SpaceLook};
use theme::wash::{self, Dot};
use theme::*;

/// The grain tile's side in points; it is baked at twice that for Retina.
const GRAIN_TILE: f32 = 256.;
/// How opaque the grain is at full turn of the dial.
const GRAIN_MAX: f32 = 0.4;
pub(super) const PAD: f32 = 340.;
const DOT_GAP: f32 = 4.5;
const DOT_RADIUS: f32 = 0.5;
/// How far the page card sits in from the window's edges, and how round it is.
const INSET: f32 = 8.;
const CARD_RADIUS: f32 = 12.;

/// The key a space's look is saved under: its host's id, or "" for This Mac.
pub(crate) fn look_key(space: &Space) -> &str {
    space.host().unwrap_or("")
}

pub(crate) fn dot(c: LookColour) -> Dot {
    Dot { x: c.x, y: c.y, s: c.s }
}

pub(crate) fn colour(d: Dot) -> LookColour {
    LookColour { x: d.x, y: d.y, s: d.s }
}

/// The appearance a look paints in; `None` leaves the app's.
pub(crate) fn look_dark(look: &SpaceLook) -> Option<bool> {
    match look.mode {
        LookMode::Auto => None,
        LookMode::Light => Some(false),
        LookMode::Dark => Some(true),
    }
}

/// A BGRA image GPUI can draw, `side` px square, each pixel's grey and alpha from `at`. GPUI takes straight alpha: a white
/// speck is 255 grey at its alpha, or it draws at about alpha squared.
fn baked(side: u32, at: impl Fn(u32, u32) -> (u8, u8)) -> Arc<RenderImage> {
    let mut px = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            let (grey, alpha) = at(x, y);
            px.extend([grey, grey, grey, alpha]);
        }
    }
    let buffer = image::ImageBuffer::from_raw(side, side, px).expect("a buffer of side² pixels");
    Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}

/// Film grain: fractal Perlin noise at about a pixel's scale, kept to its brighter half as specks, black for a light window and white for a dark one.
fn grain_tile(white: bool) -> Arc<RenderImage> {
    let noise = Fbm::<Perlin>::new(7).set_octaves(2).set_frequency(0.85);
    baked((GRAIN_TILE * 2.) as u32, |x, y| {
        let n = (noise.get([x as f64, y as f64]) as f32 + 1.) / 2.;
        let alpha = ((1.7 * n - 0.68).clamp(0., 1.) * 255.) as u8;
        (if white { 255 } else { 0 }, alpha)
    })
}

pub(super) fn grain_image(white: bool) -> Arc<RenderImage> {
    static LIGHT: OnceLock<Arc<RenderImage>> = OnceLock::new();
    static DARK: OnceLock<Arc<RenderImage>> = OnceLock::new();
    (if white { &DARK } else { &LIGHT }).get_or_init(|| grain_tile(white)).clone()
}

pub(super) fn dots_image() -> Arc<RenderImage> {
    static DOTS: OnceLock<Arc<RenderImage>> = OnceLock::new();
    DOTS.get_or_init(|| {
        let (scale, gap) = (2., DOT_GAP * 2.);
        baked((PAD * scale) as u32, |x, y| {
            // Centred on a pixel corner, a dot lights 2×2 pixels as Arc's do, not one pixel that blurs away.
            let near = |v: u32| ((v as f32 + 0.5) % gap - (gap / 2. - 0.5)).abs();
            let cover = (DOT_RADIUS * scale + 0.5 - near(x).hypot(near(y))).clamp(0., 1.);
            let alpha = (cover * 0.5 * 255.) as u8;
            (255, alpha)
        })
    })
    .clone()
}

/// The wash under a window `size`: the first two colours corner to corner, a third rising from the bottom left, grain over them.
fn layer(look: &SpaceLook, dark: bool, size: Size<Pixels>) -> Div {
    let colours: Vec<Hsla> = look.colours.iter().map(|c| wash::wash(dot(*c), dark, look.intensity)).collect();
    let (first, second) = (colours[0], *colours.get(1).unwrap_or(&colours[0]));
    let fill = || div().absolute().inset_0();
    fill()
        .child(fill().bg(linear_gradient(135., linear_color_stop(first, 0.), linear_color_stop(second, 1.))))
        .children(colours.get(2).map(|c| fill().bg(linear_gradient(45., linear_color_stop(*c, 0.), linear_color_stop(c.opacity(0.), 0.6)))))
        .when(look.grain > 0., |d| d.child(grain(look.grain, dark, size)))
}

/// Grain tiled over `size`: GPUI draws an image scaled, never repeated.
fn grain(amount: f32, dark: bool, size: Size<Pixels>) -> Div {
    let tile = grain_image(dark);
    let (cols, rows) = ((f32::from(size.width) / GRAIN_TILE).ceil() as usize, (f32::from(size.height) / GRAIN_TILE).ceil() as usize);
    let tiles = (0..rows)
        .flat_map(|r| (0..cols).map(move |c| (c, r)))
        .map(move |(c, r)| img(tile.clone()).absolute().left(px(c as f32 * GRAIN_TILE)).top(px(r as f32 * GRAIN_TILE)).size(px(GRAIN_TILE)));
    div().absolute().inset_0().opacity(amount.clamp(0., 1.) * GRAIN_MAX).children(tiles)
}

/// The page's own colour under the card, and under fields on the wash: dark pages are glass, so the solid window colour stands in.
pub(crate) fn card_fill() -> Hsla {
    if is_dark() { WINDOW_SOLID.into() } else { PAGE.into() }
}

/// The page, and the workspace panel beside it, lifted onto the wash as one card.
pub(crate) fn lifted(page: Option<AnyElement>, workspace: Option<Div>) -> Div {
    let card = div()
        .size_full()
        .flex()
        .rounded(px(CARD_RADIUS))
        .bg(card_fill())
        .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5), ui::shadow(rgba(0x0000000d), 1., 2.), ui::shadow(rgba(0x00000014), 6., 20.)])
        .children(page)
        .children(workspace);
    div().flex_1().min_w_0().h_full().p(px(INSET)).pl_0().flex().child(card)
}

impl Desktop {
    pub(crate) fn space_look(&self, space: &Space) -> Option<&SpaceLook> {
        self.store.sidebar.space_looks.get(look_key(space))
    }

    /// The space on screen's look while it washes the window: once it has a colour.
    pub(crate) fn wash(&self) -> Option<&SpaceLook> {
        self.space_look(&self.space()).filter(|l| !l.colours.is_empty())
    }

    /// Paints the theme again when the space on screen asks for another appearance than the one painted.
    pub(crate) fn follow_space_look(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let want = self.space_look(&self.space()).and_then(look_dark);
        if want != self.spaces.look_dark {
            self.spaces.look_dark = want;
            self.apply_theme(cx);
            window.set_background_appearance(theme::window_background());
        }
    }

    /// Rounds a page fill's outer corners as the card's, so it doesn't square them off: all four, or the left two beside the workspace panel.
    pub(crate) fn page_corners<E: Styled>(&self, e: E, cx: &App) -> E {
        match (self.wash(), self.workspace_shown(cx)) {
            (None, _) => e,
            (Some(_), true) => e.rounded_l(px(CARD_RADIUS)),
            (Some(_), false) => e.rounded(px(CARD_RADIUS)),
        }
    }

    pub(crate) fn wash_layer(&self, window: &Window) -> Option<Div> {
        Some(layer(self.wash()?, is_dark(), window.viewport_size()))
    }

    pub(crate) fn open_theme_card(&mut self, cx: &mut Context<Self>) {
        let space = self.space();
        let (key, look) = (look_key(&space).to_string(), self.space_look(&space).cloned().unwrap_or_default());
        self.sidebar.read(cx).theme_card.clone().update(cx, |c, cx| c.open(key, look, cx));
    }

    pub(crate) fn on_theme_card_event(&mut self, _: &Entity<ThemeCard>, ev: &CardEvent, window: &mut Window, cx: &mut Context<Self>) {
        let CardEvent::Changed { key, look } = ev;
        self.store.sidebar.space_looks.insert(key.clone(), look.clone());
        self.save_soon(cx);
        self.follow_space_look(window, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{GRAIN_TILE, LookMode, PAD, Space, SpaceLook, dots_image, grain_tile, look_dark, look_key};

    #[test]
    fn a_host_is_saved_under_its_id_and_this_mac_under_nothing() {
        assert_eq!(look_key(&Space::Host("devbox".into())), "devbox");
        assert_eq!(look_key(&Space::Local), "");
    }

    #[test]
    fn auto_leaves_the_app_its_appearance_and_light_or_dark_take_it() {
        let look = |mode| SpaceLook { mode, ..SpaceLook::default() };
        assert_eq!(look_dark(&look(LookMode::Auto)), None);
        assert_eq!(look_dark(&look(LookMode::Light)), Some(false));
        assert_eq!(look_dark(&look(LookMode::Dark)), Some(true));
    }

    #[test]
    fn grain_is_specks_on_clear_neither_solid_nor_empty() {
        let side = (GRAIN_TILE * 2.) as usize;
        let image = grain_tile(false);
        let bytes = image.as_bytes(0).expect("one frame");
        let alphas: Vec<u8> = bytes.chunks(4).map(|p| p[3]).collect();
        assert_eq!(alphas.len(), side * side);
        let clear = alphas.iter().filter(|a| **a == 0).count() as f32 / alphas.len() as f32;
        assert!(clear > 0.2 && clear < 0.95, "{clear}");
        assert!(bytes.chunks(4).all(|p| p[0] == 0), "light grain is black");
    }

    #[test]
    fn the_pad_grid_has_a_dot_every_gap_and_clear_between() {
        let image = dots_image();
        let bytes = image.as_bytes(0).expect("one frame");
        let side = (PAD * 2.) as usize;
        let alpha = |x: usize, y: usize| bytes[(y * side + x) * 4 + 3];
        assert!(alpha(4, 4) > 0, "a dot at the first gap's centre");
        assert_eq!(bytes[(4 * side + 4) * 4], 255, "white at full grey, as GPUI takes straight alpha");
        assert!(alpha(13, 13) > 0, "and one a gap on");
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(8, 4), 0);
    }
}
