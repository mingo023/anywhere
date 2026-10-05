use super::{card, row};
use crate::desktop::{Desktop, follow_reduce_motion};
use gpui_kit::*;
use store::Mode;
use ui::Segment;

/// The appearance to force on the app; `None` follows macOS.
pub(crate) fn forced(mode: Mode) -> Option<WindowAppearance> {
    match mode {
        Mode::System => None,
        Mode::Light => Some(WindowAppearance::Light),
        Mode::Dark => Some(WindowAppearance::Dark),
    }
}

/// Whether motion is reduced: the user's choice, else macOS's.
pub(crate) fn reduce_motion(pref: Option<bool>, system: impl FnOnce() -> bool) -> bool {
    pref.unwrap_or_else(system)
}

fn segment<T>(value: T, label: &str) -> Segment<T> {
    Segment { icon: None, value, label: label.to_string().into(), badge: None }
}

impl Desktop {
    pub(crate) fn appearance_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let a = self.store.appearance;
        let modes = [(Mode::System, "System"), (Mode::Light, "Light"), (Mode::Dark, "Dark")].map(|(m, l)| segment(m, l)).into();
        let mode = ui::segmented(modes, a.mode, true, false, |this: &mut Self, m, cx| this.set_mode(m, cx), cx);
        let motions = [(None, "System"), (Some(true), "On"), (Some(false), "Off")].map(|(m, l)| segment(m, l)).into();
        let motion = ui::segmented(motions, a.reduce_motion, true, false, |this: &mut Self, m, cx| this.set_reduce_motion(m, cx), cx);
        card(vec![row("Theme", Some("System follows macOS light and dark."), mode), row("Reduce motion", Some("Stops spinners, cursor blink and tab slides."), motion)])
    }

    fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.store.appearance.mode = mode;
        // The window appearance observer repaints the theme once AppKit applies this.
        cx.set_window_appearance(forced(mode));
        self.save_soon(cx);
        cx.notify();
    }

    fn set_reduce_motion(&mut self, pref: Option<bool>, cx: &mut Context<Self>) {
        self.store.appearance.reduce_motion = pref;
        follow_reduce_motion(pref, cx);
        self.save_soon(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{forced, reduce_motion};
    use gpui_kit::WindowAppearance;
    use store::Mode;

    #[test]
    fn system_mode_forces_nothing() {
        assert_eq!(forced(Mode::System), None);
    }

    #[test]
    fn light_and_dark_force_the_window() {
        assert_eq!([Mode::Light, Mode::Dark].map(forced), [Some(WindowAppearance::Light), Some(WindowAppearance::Dark)]);
    }

    #[test]
    fn a_reduce_motion_override_ignores_macos() {
        assert_eq!([reduce_motion(Some(false), || true), reduce_motion(Some(true), || false), reduce_motion(None, || true)], [false, true, true]);
    }
}
