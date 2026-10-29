//! A diagram in a window of its own, opened from its card, as markdown-preview's popup: 16:9 whatever the diagram's
//! shape, the diagram fitted inside it. Space fits it again.

use super::{Diagram, Diagrams, Viewport, key, surface};
use gpui_kit::*;
use theme::*;

const INITIAL: (f32, f32) = (1440., 810.);
const PAD: f32 = 24.;
/// The window shows one diagram.
const PLACE: u64 = 0;

/// The diagram window, reused for the next diagram opened.
struct Open(WindowHandle<Popup>);

impl Global for Open {}

pub(super) struct Popup {
    diagrams: Entity<Diagrams>,
    source: SharedString,
    focus: FocusHandle,
}

/// The window's size on a screen with `free` room: [`INITIAL`], scaled to 90% of it.
fn preferred(free: (f32, f32)) -> (f32, f32) {
    let k = (free.0 * 0.9 / INITIAL.0).min(free.1 * 0.9 / INITIAL.1);
    ((INITIAL.0 * k).round(), (INITIAL.1 * k).round())
}

fn title(heading: Option<&str>) -> String {
    heading.map_or_else(|| "Mermaid diagram".to_string(), |h| format!("{h} — Mermaid diagram"))
}

/// Shows `source` drawn on `canvas` in the diagram window, centred on `window`'s screen when it opens.
pub(super) fn open(source: SharedString, canvas: Token, heading: Option<SharedString>, window: &Window, cx: &mut App) {
    let title = title(heading.as_deref());
    if let Some(handle) = cx.try_global::<Open>().map(|o| o.0) {
        let shown = handle.update(cx, |popup, window, cx| {
            popup.show(source.clone(), canvas, window, cx);
            window.set_window_title(&title);
            window.activate_window();
        });
        if shown.is_ok() {
            return;
        }
    }
    let display = window.display(cx);
    let free = display.as_ref().map_or(size(px(1280.), px(800.)), |d| d.visible_bounds().size);
    let (w, h) = preferred((f32::from(free.width), f32::from(free.height)));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(display.map(|d| d.id()), size(px(w), px(h)), cx))),
        titlebar: Some(TitlebarOptions { title: Some(title.into()), ..Default::default() }),
        window_min_size: Some(size(px(640.), px(360.))),
        ..Default::default()
    };
    if let Ok(handle) = cx.open_window(options, |window, cx| cx.new(|cx| Popup::new(source, canvas, window, cx))) {
        cx.set_global(Open(handle));
    }
}

impl Popup {
    fn new(source: SharedString, canvas: Token, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self { diagrams: cx.new(|_| Diagrams::standalone(canvas)), source, focus }
    }

    fn show(&mut self, source: SharedString, canvas: Token, window: &mut Window, cx: &mut Context<Self>) {
        if self.diagrams.read(cx).canvas == canvas {
            self.diagrams.update(cx, |d, cx| d.set_view(PLACE, Viewport::default(), cx));
        } else {
            self.diagrams = cx.new(|_| Diagrams::standalone(canvas));
        }
        self.source = source;
        window.focus(&self.focus, cx);
        cx.notify();
    }
}

impl Render for Popup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = is_dark();
        let key = key(&self.source, dark);
        let message = |text: SharedString, color: Token| div().size_full().flex().items_center().justify_center().p(px(PAD)).text_size(px(13.)).text_color(color).child(text).into_any_element();
        let body = match self.diagrams.update(cx, |d, cx| d.get(key, &self.source, dark, cx)) {
            Diagram::Ready(tree) => surface(&self.diagrams, key, PLACE, tree, PAD, None, cx).into_any_element(),
            Diagram::Pending => message("Rendering diagram…".into(), TEXT_3),
            Diagram::Failed(error) => message(error, FAILED),
        };
        div()
            .size_full()
            .bg(self.diagrams.read(cx).canvas)
            .font_family(SANS)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "space" {
                    this.diagrams.update(cx, |d, cx| d.set_view(PLACE, Viewport::default(), cx));
                }
            }))
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::{preferred, title};

    #[test]
    fn the_window_is_sixteen_by_nine_within_ninety_percent_of_the_screen() {
        assert_eq!(preferred((1600., 1000.)), (1440., 810.));
        assert_eq!(preferred((1280., 1000.)), (1152., 648.));
        assert_eq!(preferred((3000., 800.)), (1280., 720.));
    }

    #[test]
    fn the_title_names_the_heading_above_the_diagram() {
        assert_eq!(title(Some("Architecture")), "Architecture — Mermaid diagram");
        assert_eq!(title(None), "Mermaid diagram");
    }
}
