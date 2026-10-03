use crate::desktop::Desktop;
use gpui_kit::*;
use std::path::PathBuf;
use std::time::Duration;
use store::{ColumnWidths, WindowGeometry};

pub const MIN_WINDOW: Size<Pixels> = Size { width: px(900.), height: px(600.) };
pub const DEFAULT_WINDOW: Size<Pixels> = Size { width: px(1440.), height: px(900.) };
const SAVE_DELAY: Duration = Duration::from_millis(500);

/// The saved bounds on their display, or the default size centered on the first display.
pub fn restore(saved: Option<&WindowGeometry>, displays: &[(Option<String>, Bounds<Pixels>)]) -> (usize, Bounds<Pixels>) {
    let fit = |want: Size<Pixels>, area: Size<Pixels>| size(want.width.max(MIN_WINDOW.width).min(area.width), want.height.max(MIN_WINDOW.height).min(area.height));
    let known = saved.and_then(|g| displays.iter().position(|(uuid, _)| uuid.is_some() && *uuid == g.display).map(|i| (g, i)));
    if let Some((g, i)) = known {
        let area = displays[i].1;
        let rect = Bounds::new(area.origin + point(px(g.x), px(g.y)), fit(size(px(g.width), px(g.height)), area.size));
        let slack = px(1.);
        if rect.left() + slack >= area.left() && rect.top() + slack >= area.top() && rect.right() <= area.right() + slack && rect.bottom() <= area.bottom() + slack {
            return (i, rect);
        }
    }
    let i = known.map_or(0, |(_, i)| i);
    let area = displays.get(i).map_or(Bounds::new(point(px(0.), px(0.)), DEFAULT_WINDOW), |(_, b)| *b);
    (i, Bounds::centered_at(area.center(), fit(DEFAULT_WINDOW, area.size)))
}

/// What to persist for the window's bounds; fullscreen and maximized keep their restore bounds.
pub fn snapshot(bounds: WindowBounds, display: Option<String>) -> WindowGeometry {
    let b = bounds.get_bounds();
    WindowGeometry { display, x: b.origin.x.into(), y: b.origin.y.into(), width: b.size.width.into(), height: b.size.height.into() }
}

/// The window's latest bounds and the pending `desktop.json` write.
pub struct Geometry {
    window: Option<WindowGeometry>,
    save: Option<Task<()>>,
}

impl Geometry {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let subs = vec![
            cx.observe_window_bounds(window, |this, window, cx| {
                let display = window.display(cx).and_then(|d| d.uuid().ok()).map(|u| u.to_string());
                this.geometry.window = Some(snapshot(window.window_bounds(), display));
                this.save_soon(cx);
            }),
            cx.on_release(|this, _| {
                if let Some((path, raw)) = this.remember_chrome() {
                    store::write(&path, &raw);
                }
            }),
            cx.on_app_quit(|this, _| {
                if let Some((path, raw)) = this.remember_chrome() {
                    store::write(&path, &raw);
                }
                async {}
            }),
        ];
        (Self { window: None, save: None }, subs)
    }
}

impl Desktop {
    /// Writes `desktop.json` once the window, layout, widths and panels have held still for `SAVE_DELAY`.
    pub(crate) fn save_soon(&mut self, cx: &mut Context<Self>) {
        self.geometry.save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            let Ok(Some((path, raw))) = this.update(cx, |d, _| d.remember_chrome()) else { return };
            cx.background_executor().spawn(async move { store::write(&path, &raw) }).await;
        }));
    }

    /// Copies the window, layout, widths and panels into the store and encodes it; capture mode keeps none of them.
    fn remember_chrome(&mut self) -> Option<(PathBuf, Vec<u8>)> {
        if self.capturing {
            return None;
        }
        if let Some(w) = &self.geometry.window {
            self.store.window = Some(w.clone());
        }
        self.store.layout = self.layout;
        self.store.widths = ColumnWidths { projects: self.widths[0], sessions: self.widths[1] };
        self.remember_layouts();
        self.store.encode()
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_WINDOW, MIN_WINDOW, restore, snapshot};
    use gpui_kit::{Bounds, Pixels, WindowBounds, point, px, size};
    use store::WindowGeometry;

    fn display(uuid: &str, width: f32, height: f32) -> (Option<String>, Bounds<Pixels>) {
        (Some(uuid.into()), Bounds::new(point(px(0.), px(0.)), size(px(width), px(height))))
    }

    fn saved(uuid: &str, x: f32, y: f32, width: f32, height: f32) -> WindowGeometry {
        WindowGeometry { display: Some(uuid.into()), x, y, width, height }
    }

    #[test]
    fn a_window_reopens_where_it_was_on_the_same_display() {
        let displays = [display("A", 1512., 982.), display("B", 2560., 1440.)];
        let (i, b) = restore(Some(&saved("B", 100., 50., 1200., 800.)), &displays);
        assert_eq!((i, b), (1, Bounds::new(point(px(100.), px(50.)), size(px(1200.), px(800.)))));
    }

    #[test]
    fn a_window_on_a_missing_display_opens_centered() {
        let displays = [display("A", 2560., 1440.)];
        let (i, b) = restore(Some(&saved("gone", 100., 50., 1200., 800.)), &displays);
        assert_eq!((i, b), (0, Bounds::new(point(px(560.), px(270.)), DEFAULT_WINDOW)));
        assert_eq!(restore(None, &displays), (0, b));
    }

    #[test]
    fn a_saved_size_below_the_minimum_grows_to_it() {
        let (_, b) = restore(Some(&saved("A", 10., 10., 400., 300.)), &[display("A", 2560., 1440.)]);
        assert_eq!(b, Bounds::new(point(px(10.), px(10.)), MIN_WINDOW));
    }

    #[test]
    fn an_off_screen_window_opens_centered() {
        let displays = [display("A", 1512., 982.)];
        let (_, b) = restore(Some(&saved("A", 1400., 900., 1000., 700.)), &displays);
        assert_eq!(b, Bounds::centered_at(point(px(756.), px(491.)), size(px(1440.), px(900.))));
    }

    #[test]
    fn an_off_screen_window_opens_centered_on_its_own_display() {
        let displays = [display("A", 1512., 982.), display("B", 2560., 1440.)];
        let (i, b) = restore(Some(&saved("B", 2500., 1400., 1000., 700.)), &displays);
        assert_eq!((i, b), (1, Bounds::centered_at(point(px(1280.), px(720.)), DEFAULT_WINDOW)));
    }

    #[test]
    fn a_window_flush_to_the_display_edge_stays_put() {
        let displays = [display("A", 1512., 982.)];
        let (_, b) = restore(Some(&saved("A", 512.0004, 82., 1000., 900.)), &displays);
        assert_eq!(b, Bounds::new(point(px(512.0004), px(82.)), size(px(1000.), px(900.))));
    }

    #[test]
    fn fullscreen_saves_its_restore_bounds() {
        let restore_bounds = Bounds::new(point(px(30.), px(40.)), size(px(1000.), px(700.)));
        let got = snapshot(WindowBounds::Fullscreen(restore_bounds), Some("A".into()));
        assert_eq!(got, saved("A", 30., 40., 1000., 700.));
    }
}
