use gpui_kit::*;
use std::time::{Duration, Instant};
use theme::{Placement, Scene};

/// Pixel art moves in steps, so a dozen frames a second reads as smooth while costing a fifth of 60.
const FRAME: Duration = Duration::from_millis(83);

/// The theme's scene as a view of its own: a frame notifies only this view, so the cached parts of the window keep
/// their last paint.
pub(crate) struct SceneView {
    started: Instant,
    _ticker: Task<()>,
}

impl SceneView {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(FRAME).await;
                let ticked = this.update(cx, |_, cx| {
                    if !cx.reduce_motion() {
                        cx.notify();
                    }
                });
                if ticked.is_err() {
                    break;
                }
            }
        });
        Self { started: Instant::now(), _ticker: ticker }
    }
}

impl Render for SceneView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = if cx.reduce_motion() { 0. } else { self.started.elapsed().as_secs_f32() };
        let scene = theme::scene();
        canvas(|_, _, _| (), move |bounds, _, window, _| {
            window.with_content_mask(Some(ContentMask { bounds }), |window| scene.into_iter().for_each(|scene| paint(&scene, t, bounds, window)))
        })
        .size_full()
    }
}

fn paint(scene: &Scene, t: f32, bounds: Bounds<Pixels>, window: &mut Window) {
    let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    for sprite in &scene.sprites {
        let (p, columns) = (sprite.pixel, sprite.cells.iter().map(Vec::len).max().unwrap_or(0));
        for i in 0..sprite.count {
            let Placement { x, y, faces_left } = sprite.place(i, t, width, height);
            for (row, cells) in sprite.cells.iter().enumerate() {
                for (column, cell) in cells.iter().enumerate() {
                    let Some(color) = cell else { continue };
                    let column = if faces_left { columns - 1 - column } else { column };
                    let at = bounds.origin + point(px(x + column as f32 * p), px(y + row as f32 * p));
                    window.paint_quad(fill(Bounds::new(at, size(px(p), px(p))), rgba(*color)));
                }
            }
        }
    }
}
