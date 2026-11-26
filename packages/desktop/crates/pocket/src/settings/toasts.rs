use super::catalog::Setting;
use super::{Setter, Settings, SettingsEvent};
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;
use std::time::{Duration, Instant};
use theme::*;

/// How long a toast with Undo stays.
const UNDO_FOR: Duration = Duration::from_secs(6);
/// How long a toast that only tells stays.
const TELL_FOR: Duration = Duration::from_millis(2600);
/// The most toasts shown at once; a new one pushes the oldest out.
const MOST: usize = 3;

/// What a toast's Undo puts back, run on the Desktop that holds the store and files.
pub(crate) type Restore = Rc<dyn Fn(&mut Desktop, &mut Window, &mut Context<Desktop>)>;

#[derive(Clone)]
pub(crate) struct Undo(pub Restore);

impl Undo {
    pub(crate) fn new(restore: impl Fn(&mut Desktop, &mut Window, &mut Context<Desktop>) + 'static) -> Self {
        Self(Rc::new(restore))
    }
}

/// What Retry does after a failed save.
#[derive(Clone)]
pub(crate) enum Retry {
    /// `desktop.json` couldn't be written: write it again, without rerunning any setting's effect.
    Save,
    /// The setting's effect failed outside the app (macOS refused a login item): apply it again.
    Write(&'static Setting, Setter),
}

pub(crate) struct Toast {
    pub text: String,
    pub undo: Option<Undo>,
    pub retry: Option<Retry>,
    /// A failure, drawn in red.
    pub bad: bool,
    id: u64,
    until: Instant,
}

impl Toast {
    pub(crate) fn tell(text: impl Into<String>) -> Self {
        Self { text: text.into(), undo: None, retry: None, bad: false, id: 0, until: Instant::now() }
    }

    pub(crate) fn undo(text: impl Into<String>, undo: Undo) -> Self {
        Self { undo: Some(undo), ..Self::tell(text) }
    }

    pub(crate) fn failed(text: impl Into<String>, retry: Option<Retry>) -> Self {
        Self { retry, bad: true, ..Self::tell(text) }
    }

    fn stays(&self) -> Duration {
        if self.undo.is_some() || self.retry.is_some() || self.bad { UNDO_FOR } else { TELL_FOR }
    }
}

/// The toasts Settings shows over its page, newest last.
#[derive(Default)]
pub(crate) struct Toasts {
    list: Vec<Toast>,
    next: u64,
}

impl Toasts {
    /// Shows `toast` from `now`; returns when it goes, for the timer that drops it.
    pub(crate) fn push(&mut self, mut toast: Toast, now: Instant) -> Instant {
        self.next += 1;
        (toast.id, toast.until) = (self.next, now + toast.stays());
        if self.list.len() == MOST {
            self.list.remove(0);
        }
        let until = toast.until;
        self.list.push(toast);
        until
    }

    /// Drops the toasts whose time is up; true when any went.
    pub(crate) fn prune(&mut self, now: Instant) -> bool {
        let before = self.list.len();
        self.list.retain(|t| t.until > now);
        self.list.len() != before
    }

    pub(crate) fn take(&mut self, id: u64) -> Option<Toast> {
        let i = self.list.iter().position(|t| t.id == id)?;
        Some(self.list.remove(i))
    }

    /// Drops every toast offering to retry `setting`, once its row gets a new value.
    pub(crate) fn forget(&mut self, setting: &str) {
        self.list.retain(|t| !matches!(&t.retry, Some(Retry::Write(s, _)) if s.id == setting));
    }

    pub(crate) fn shown(&self) -> &[Toast] {
        &self.list
    }
}

/// A save that failed, shown on its row with Retry until a save or a new value clears it.
#[derive(Clone)]
pub(crate) struct RowError {
    pub setting: &'static Setting,
    pub message: String,
    pub retry: Retry,
}

/// Which rows were written since `desktop.json` was last saved, and which failed.
#[derive(Default)]
pub(crate) struct Saves {
    pending: Vec<&'static Setting>,
    errors: Vec<RowError>,
}

impl Saves {
    /// A new value supersedes whatever failed on its row; the next save says how this one went.
    pub(crate) fn wrote(&mut self, setting: &'static Setting) {
        self.errors.retain(|e| e.setting.id != setting.id);
        if !self.pending.iter().any(|s| s.id == setting.id) {
            self.pending.push(setting);
        }
    }

    /// `desktop.json` was written: what was pending is saved, and earlier failed saves with it.
    pub(crate) fn saved(&mut self) {
        self.pending.clear();
        self.errors.retain(|e| !matches!(e.retry, Retry::Save));
    }

    /// Writing `desktop.json` failed: every row written since the last save shows `message`. Returns those rows.
    pub(crate) fn save_failed(&mut self, message: &str) -> Vec<&'static Setting> {
        let rows = self.pending.clone();
        for &setting in &rows {
            self.fail(RowError { setting, message: message.to_string(), retry: Retry::Save });
        }
        rows
    }

    /// `error` replaces whatever its row showed.
    pub(crate) fn fail(&mut self, error: RowError) {
        self.errors.retain(|e| e.setting.id != error.setting.id);
        self.errors.push(error);
    }

    pub(crate) fn error(&self, setting: &str) -> Option<&RowError> {
        self.errors.iter().find(|e| e.setting.id == setting)
    }
}

impl Settings {
    /// Shows `toast` over the page, and drops it when its time is up.
    pub(crate) fn toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        let until = self.toasts.push(toast, Instant::now());
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(until.saturating_duration_since(Instant::now())).await;
            this.update(cx, |s, cx| {
                if s.toasts.prune(Instant::now()) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// The row's effect failed outside the app: its row says why, with Retry applying `retry` again.
    pub(crate) fn row_failed(&mut self, setting: &'static Setting, message: String, retry: Setter, cx: &mut Context<Self>) {
        let retry = Retry::Write(setting, retry);
        self.saves.fail(RowError { setting, message, retry: retry.clone() });
        self.toast(Toast::failed(format!("Couldn\u{2019}t save {}", setting.label), Some(retry)), cx);
    }

    /// Writing `desktop.json` failed; false when no row was waiting on it.
    pub(crate) fn save_failed(&mut self, message: String, cx: &mut Context<Self>) -> bool {
        let rows = self.saves.save_failed(&format!("Couldn\u{2019}t save: {message}"));
        let text = match rows.as_slice() {
            [] => return false,
            [one] => format!("Couldn\u{2019}t save {}", one.label),
            many => format!("Couldn\u{2019}t save {} settings", many.len()),
        };
        self.toast(Toast::failed(text, Some(Retry::Save)), cx);
        true
    }

    pub(crate) fn saved(&mut self, cx: &mut Context<Self>) {
        self.saves.saved();
        self.toasts.list.retain(|t| !matches!(t.retry, Some(Retry::Save)));
        cx.notify();
    }

    pub(crate) fn retry(&mut self, retry: &Retry, cx: &mut Context<Self>) {
        match retry {
            Retry::Save => cx.emit(SettingsEvent::SaveAgain),
            Retry::Write(setting, apply) => self.write(Some(setting), cx, {
                let apply = apply.clone();
                move |s, live| apply(s, live)
            }),
        }
        cx.notify();
    }

    fn press_toast(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(toast) = self.toasts.take(id) else { return };
        if let Some(undo) = toast.undo {
            cx.emit(SettingsEvent::Undo(undo));
        } else if let Some(retry) = &toast.retry {
            self.retry(retry, cx);
        }
        cx.notify();
    }

    /// The toasts, centred above the change bar.
    pub(crate) fn toasts_view(&self, cx: &mut Context<Self>) -> Option<Div> {
        let shown = self.toasts.shown();
        if shown.is_empty() {
            return None;
        }
        let toasts = shown.iter().map(|t| {
            let id = t.id;
            let button = t.undo.as_ref().map(|_| "Undo").or(t.retry.as_ref().map(|_| "Retry")).map(|label| {
                div()
                    .id(("toast-act", id as usize))
                    .px(px(8.))
                    .py(px(3.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .tab_index(0)
                    .hover(|d| d.bg(rgba(0xffffff1f)))
                    .focus_visible(|d| d.shadow(vec![ui::ring(rgba(0xffffffcc), 2.).inset()]))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.press_toast(id, cx)))
            });
            div()
                .id(("toast", id as usize))
                .occlude()
                .pl(px(14.))
                .pr(px(if button.is_some() { 8. } else { 14. }))
                .py(px(8.))
                .flex()
                .items_center()
                .gap(px(12.))
                .rounded(px(11.))
                .bg(if t.bad { rgb(0x3b1517) } else { rgb(0x1d1d1f) })
                .shadow(vec![ui::shadow(rgba(0x00000040), 10., 30.)])
                .text_size(px(12.5))
                .text_color(rgb(0xf5f5f7))
                .max_w(px(420.))
                .child(div().min_w_0().when(!t.bad, |d| d.whitespace_nowrap()).child(t.text.clone()))
                .children(button)
                .with_animation(("toast-in", id as usize), Animation::new(MENU_IN).with_easing(ease_out_quint()), |d, p| d.opacity(p).mt(px(8. * (1. - p))))
        });
        Some(div().absolute().left_0().right_0().bottom(px(66.)).flex().flex_col().items_center().gap(px(8.)).children(toasts))
    }
}

#[cfg(test)]
mod tests {
    use super::{Retry, RowError, Saves, Setter, TELL_FOR, Toast, Toasts, UNDO_FOR, Undo};
    use crate::settings::catalog::Setting;
    use std::rc::Rc;
    use std::time::Instant;

    static LOGIN: Setting = Setting::switch("open-at-login", "Startup", "Open at login", |s| s.general.open_at_login, |s, on| s.general.open_at_login = on);
    static RESUME: Setting = Setting::switch("resume", "Startup", "Resume agents", |s| s.general.resume_agents, |s, on| s.general.resume_agents = on);

    #[test]
    fn a_toast_with_undo_stays_six_seconds_and_one_that_only_tells_goes_sooner() {
        let (mut toasts, now) = (Toasts::default(), Instant::now());
        let undo = toasts.push(Toast::undo("Moved Ink to the Trash", Undo::new(|_, _, _| {})), now);
        let tell = toasts.push(Toast::tell("Reset 3 settings"), now);
        assert_eq!((undo - now, tell - now), (UNDO_FOR, TELL_FOR));
        assert!(toasts.prune(now + TELL_FOR));
        assert_eq!(toasts.shown().iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["Moved Ink to the Trash"]);
        assert!(toasts.prune(now + UNDO_FOR));
        assert!(toasts.shown().is_empty());
    }

    #[test]
    fn a_failure_stays_as_long_as_a_toast_with_undo() {
        let (mut toasts, now) = (Toasts::default(), Instant::now());
        assert_eq!(toasts.push(Toast::failed("Couldn\u{2019}t save settings: disk full", None), now) - now, UNDO_FOR);
    }

    #[test]
    fn a_new_toast_pushes_the_oldest_out_past_three() {
        let (mut toasts, now) = (Toasts::default(), Instant::now());
        for text in ["a", "b", "c", "d"] {
            toasts.push(Toast::tell(text), now);
        }
        assert_eq!(toasts.shown().iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["b", "c", "d"]);
    }

    #[test]
    fn pressing_a_toast_takes_it_off_the_stack() {
        let (mut toasts, now) = (Toasts::default(), Instant::now());
        toasts.push(Toast::tell("a"), now);
        let id = toasts.shown()[0].id;
        assert_eq!(toasts.take(id).map(|t| t.text), Some("a".into()));
        assert!(toasts.take(id).is_none());
    }

    #[test]
    fn a_failed_save_marks_every_row_written_since_the_last_save_until_one_succeeds() {
        let mut saves = Saves::default();
        saves.wrote(&LOGIN);
        saves.wrote(&RESUME);
        saves.wrote(&LOGIN);
        let failed: Vec<_> = saves.save_failed("Couldn\u{2019}t save: disk full").iter().map(|s| s.id).collect();
        assert_eq!(failed, ["open-at-login", "resume"]);
        assert_eq!(saves.error("resume").map(|e| e.message.as_str()), Some("Couldn\u{2019}t save: disk full"));
        saves.saved();
        assert!(saves.error("resume").is_none() && saves.error("open-at-login").is_none());
        assert!(saves.save_failed("again").is_empty(), "nothing was written since");
    }

    #[test]
    fn a_failed_effect_stays_on_its_row_through_a_save() {
        let mut saves = Saves::default();
        let apply: Setter = Rc::new(|s, _| s.general.open_at_login = true);
        saves.fail(RowError { setting: &LOGIN, message: "macOS refused".into(), retry: Retry::Write(&LOGIN, apply) });
        saves.saved();
        assert!(matches!(saves.error("open-at-login").map(|e| &e.retry), Some(Retry::Write(..))));
    }

    #[test]
    fn changing_a_row_by_hand_clears_its_failure() {
        let mut saves = Saves::default();
        let apply: Setter = Rc::new(|s, _| s.general.open_at_login = true);
        saves.fail(RowError { setting: &LOGIN, message: "macOS refused".into(), retry: Retry::Write(&LOGIN, apply) });
        saves.fail(RowError { setting: &RESUME, message: "disk full".into(), retry: Retry::Save });
        saves.wrote(&LOGIN);
        saves.wrote(&RESUME);
        assert!(saves.error("open-at-login").is_none() && saves.error("resume").is_none());
    }
}
