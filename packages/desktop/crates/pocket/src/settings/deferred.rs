use super::Settings;
use gpui_kit::*;

/// Runs `run` once the current event is handled, in the window the press came from, which a control's handler isn't given.
pub(super) fn with_window(cx: &mut Context<Settings>, run: impl FnOnce(&mut Settings, &mut Window, &mut Context<Settings>) + 'static) {
    let (this, handle) = (cx.entity(), cx.active_window());
    cx.defer(move |cx| {
        if let Some(handle) = handle {
            handle.update(cx, |_, window, cx| this.update(cx, |s, cx| run(s, window, cx))).ok();
        }
    });
}
