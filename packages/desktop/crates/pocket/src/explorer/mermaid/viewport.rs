//! Where a diagram sits in its card as it's panned and zoomed, as markdown-preview places it. Sizes are `(width, height)` in pixels.

const MIN_SCALE: f32 = 0.25;
const MAX_SCALE: f32 = 8.;
/// A zoom button's step.
pub(super) const STEP: f32 = 1.25;
/// Past this many pixels a zoomed raster stops growing and blurs instead: 32 MB, half the image budget.
const MAX_PIXELS: f32 = 8_000_000.;

/// The zoom that shows the whole diagram `pad` inside the card's edges, never past natural size.
fn fit_zoom(diagram: (f32, f32), card: (f32, f32), pad: f32) -> f32 {
    ((card.0 - 2. * pad) / diagram.0).min((card.1 - 2. * pad) / diagram.1).clamp(f32::EPSILON, 1.)
}

/// The card's view, over the diagram fitted and centred in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Viewport {
    /// Over the fitted diagram: 1 shows it whole, the 100% the zoom buttons show.
    pub scale: f32,
    /// Where the card's top-left lands once scaled about it.
    pub offset: (f32, f32),
}

impl Default for Viewport {
    fn default() -> Self {
        Self { scale: 1., offset: (0., 0.) }
    }
}

/// The diagram's top-left in the card and its zoom over natural size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Placed {
    pub origin: (f32, f32),
    pub zoom: f32,
}

impl Viewport {
    /// Zoomed by `factor`, keeping the point under `at`, card-relative, where it was; centred again once it fits.
    pub fn zoomed(self, factor: f32, at: (f32, f32), card: (f32, f32)) -> Self {
        let scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        if scale == self.scale {
            return self;
        }
        let k = scale / self.scale;
        let offset = match scale <= 1.001 {
            true => (card.0 * (1. - scale) / 2., card.1 * (1. - scale) / 2.),
            false => (at.0 - (at.0 - self.offset.0) * k, at.1 - (at.1 - self.offset.1) * k),
        };
        Self { scale, offset }
    }

    /// Moved along with a drag of `by`.
    pub fn panned(self, by: (f32, f32)) -> Self {
        Self { offset: (self.offset.0 + by.0, self.offset.1 + by.1), ..self }
    }

    /// Back to fitted when zoomed, else zoomed in twice around `at`.
    pub fn double_clicked(self, at: (f32, f32), card: (f32, f32)) -> Self {
        match (self.scale - 1.).abs() > 0.001 {
            true => Self::default(),
            false => self.zoomed(2., at, card),
        }
    }

    /// Kept on a card that resized from `was`: zoomed out, it stays centred.
    pub fn resized(self, was: (f32, f32), card: (f32, f32)) -> Self {
        if self.scale >= 1. {
            return self;
        }
        let k = (1. - self.scale) / 2.;
        Self { offset: (self.offset.0 + (card.0 - was.0) * k, self.offset.1 + (card.1 - was.1) * k), ..self }
    }

    pub fn placed(self, diagram: (f32, f32), card: (f32, f32), pad: f32) -> Placed {
        let fit = fit_zoom(diagram, card, pad);
        let centred = ((card.0 - diagram.0 * fit) / 2., (card.1 - diagram.1 * fit) / 2.);
        Placed { origin: (self.offset.0 + self.scale * centred.0, self.offset.1 + self.scale * centred.1), zoom: fit * self.scale }
    }

    pub fn level(self) -> String {
        format!("{}%", (self.scale * 100.).round())
    }
}

/// The zoom a diagram is rasterized for: whole steps, so a pinch redraws it a few times, not every frame.
pub(super) fn raster_zoom(zoom: f32) -> f32 {
    zoom.max(1.).ceil()
}

/// The raster scale for `zoom` on a `scale` display, held under [`MAX_PIXELS`] but never under the display's own.
pub(super) fn raster_scale(scale: f32, zoom: f32, diagram: (f32, f32)) -> f32 {
    (scale * zoom).min((MAX_PIXELS / (diagram.0 * diagram.1)).sqrt().max(scale))
}

#[cfg(test)]
mod tests {
    use super::{Placed, Viewport, raster_scale, raster_zoom};

    const DIAGRAM: (f32, f32) = (1000., 500.);
    const CARD: (f32, f32) = (532., 432.);

    #[test]
    fn a_large_diagram_fits_inside_the_padding_and_sits_centred() {
        assert_eq!(Viewport::default().placed(DIAGRAM, CARD, 16.), Placed { origin: (16., 91.), zoom: 0.5 });
    }

    #[test]
    fn a_small_diagram_keeps_its_natural_size_in_the_middle() {
        assert_eq!(Viewport::default().placed((100., 50.), CARD, 16.), Placed { origin: (216., 191.), zoom: 1. });
    }

    #[test]
    fn zooming_in_keeps_the_point_under_the_pointer_in_place() {
        let at = (100., 100.);
        let under = |v: Viewport| {
            let p = v.placed(DIAGRAM, CARD, 16.);
            ((at.0 - p.origin.0) / p.zoom, (at.1 - p.origin.1) / p.zoom)
        };
        let v = Viewport::default().zoomed(2., at, CARD);
        assert_eq!(v.scale, 2.);
        assert_eq!(under(v), under(Viewport::default()));
    }

    #[test]
    fn zoomed_out_the_diagram_stays_centred() {
        let v = Viewport::default().panned((40., 40.)).zoomed(0.5, (0., 0.), CARD);
        assert_eq!(v, Viewport { scale: 0.5, offset: (133., 108.) });
        let p = v.placed(DIAGRAM, CARD, 16.);
        assert_eq!((p.origin.0 * 2. + 1000. * p.zoom, p.origin.1 * 2. + 500. * p.zoom), CARD);
    }

    #[test]
    fn zoom_stays_between_a_quarter_and_eight_times_the_fit() {
        assert_eq!(Viewport::default().zoomed(0.01, (0., 0.), CARD).scale, 0.25);
        assert_eq!(Viewport::default().zoomed(100., (0., 0.), CARD).scale, 8.);
        let max = Viewport::default().zoomed(100., (0., 0.), CARD);
        assert_eq!(max.zoomed(2., (50., 50.), CARD), max);
    }

    #[test]
    fn panning_moves_freely() {
        assert_eq!(Viewport::default().panned((-5000., 30.)).offset, (-5000., 30.));
    }

    #[test]
    fn double_click_zooms_in_twice_then_fits_again() {
        let zoomed = Viewport::default().double_clicked((10., 10.), CARD);
        assert_eq!(zoomed, Viewport { scale: 2., offset: (-10., -10.) });
        assert_eq!(zoomed.double_clicked((10., 10.), CARD), Viewport::default());
    }

    #[test]
    fn a_zoomed_out_view_stays_centred_on_a_resized_card() {
        let v = Viewport::default().zoomed(0.5, (0., 0.), CARD);
        let wider = (CARD.0 + 100., CARD.1);
        assert_eq!(v.resized(CARD, wider), Viewport::default().zoomed(0.5, (0., 0.), wider));
        let zoomed = Viewport::default().zoomed(2., (0., 0.), CARD);
        assert_eq!(zoomed.resized(CARD, wider), zoomed);
    }

    #[test]
    fn the_level_reads_the_scale_as_a_percentage() {
        assert_eq!(Viewport::default().level(), "100%");
        assert_eq!(Viewport { scale: 1.2345, offset: (0., 0.) }.level(), "123%");
    }

    #[test]
    fn a_zoomed_diagram_rasterizes_in_whole_steps_within_its_pixel_cap() {
        assert_eq!((raster_zoom(0.5), raster_zoom(1.), raster_zoom(1.2), raster_zoom(3.9)), (1., 1., 2., 4.));
        assert_eq!(raster_scale(2., 2., (400., 300.)), 4.);
        let big = raster_scale(2., 4., (1000., 800.));
        assert!(big < 8. && 1000. * 800. * big * big <= 8_000_001.);
        assert_eq!(raster_scale(2., 4., (8000., 8000.)), 2., "never under the display's own scale");
    }
}
