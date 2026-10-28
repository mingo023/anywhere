//! Where a diagram sits in its card as it's panned and zoomed. Sizes are `(width, height)` in pixels.

pub(super) const MAX_ZOOM: f32 = 4.;
/// Past this many pixels a zoomed raster stops growing and blurs instead: 32 MB, half the image budget.
const MAX_PIXELS: f32 = 8_000_000.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Viewport {
    /// Over the diagram's natural size.
    pub zoom: f32,
    /// How far the diagram's top-left sits above and left of the card's.
    pub offset: (f32, f32),
}

/// The zoom that shows the diagram's whole width, never past natural size.
fn fit_zoom(diagram: (f32, f32), card: (f32, f32)) -> f32 {
    (card.0 / diagram.0).min(1.)
}

impl Viewport {
    pub fn fit(diagram: (f32, f32), card: (f32, f32)) -> Self {
        Self { zoom: fit_zoom(diagram, card), offset: (0., 0.) }
    }

    /// Zoomed by `factor`, keeping the diagram's point under `at`, card-relative, where it was.
    pub fn zoomed(self, factor: f32, at: (f32, f32), diagram: (f32, f32), card: (f32, f32)) -> Self {
        let zoom = (self.zoom * factor).clamp(fit_zoom(diagram, card), MAX_ZOOM);
        let k = zoom / self.zoom;
        Self { zoom, offset: ((self.offset.0 + at.0) * k - at.0, (self.offset.1 + at.1) * k - at.1) }.clamped(diagram, card)
    }

    /// Moved along with a drag or swipe of `by`.
    pub fn panned(self, by: (f32, f32), diagram: (f32, f32), card: (f32, f32)) -> Self {
        Self { offset: (self.offset.0 - by.0, self.offset.1 - by.1), ..self }.clamped(diagram, card)
    }

    /// How far the diagram runs past the card on each axis.
    pub fn overflow(self, diagram: (f32, f32), card: (f32, f32)) -> (f32, f32) {
        ((diagram.0 * self.zoom - card.0).max(0.), (diagram.1 * self.zoom - card.1).max(0.))
    }

    fn clamped(self, diagram: (f32, f32), card: (f32, f32)) -> Self {
        let (x, y) = self.overflow(diagram, card);
        Self { offset: (self.offset.0.clamp(0., x), self.offset.1.clamp(0., y)), ..self }
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
    use super::{MAX_ZOOM, Viewport, raster_scale, raster_zoom};

    const DIAGRAM: (f32, f32) = (1000., 800.);
    const CARD: (f32, f32) = (500., 400.);

    #[test]
    fn a_wide_diagram_fits_the_cards_width_and_a_small_one_keeps_its_size() {
        assert_eq!(Viewport::fit(DIAGRAM, CARD).zoom, 0.5);
        assert_eq!(Viewport::fit((200., 100.), (200., 100.)).zoom, 1.);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer_in_place() {
        let v = Viewport::fit(DIAGRAM, CARD).zoomed(2., (100., 100.), DIAGRAM, CARD);
        assert_eq!(v, Viewport { zoom: 1., offset: (100., 100.) });
        let under = |v: Viewport| ((v.offset.0 + 100.) / v.zoom, (v.offset.1 + 100.) / v.zoom);
        assert_eq!(under(v), under(Viewport::fit(DIAGRAM, CARD)));
    }

    #[test]
    fn zoom_stays_between_fitting_and_four_times() {
        let fit = Viewport::fit(DIAGRAM, CARD);
        assert_eq!(fit.zoomed(0.1, (0., 0.), DIAGRAM, CARD), fit);
        assert_eq!(fit.zoomed(100., (0., 0.), DIAGRAM, CARD).zoom, MAX_ZOOM);
    }

    #[test]
    fn panning_stops_at_the_diagrams_edges() {
        let v = Viewport::fit(DIAGRAM, CARD).zoomed(2., (0., 0.), DIAGRAM, CARD);
        assert_eq!(v.panned((-50., -30.), DIAGRAM, CARD).offset, (50., 30.));
        assert_eq!(v.panned((-5000., -5000.), DIAGRAM, CARD).offset, (500., 400.));
        assert_eq!(v.panned((5000., 5000.), DIAGRAM, CARD).offset, (0., 0.));
    }

    #[test]
    fn a_fitted_diagram_taller_than_its_card_pans_only_down() {
        let tall = (500., 1000.);
        let v = Viewport::fit(tall, CARD);
        assert_eq!(v.overflow(tall, CARD), (0., 600.));
        assert_eq!(v.panned((-80., -80.), tall, CARD).offset, (0., 80.));
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
