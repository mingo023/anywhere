use crate::desktop::Desktop;
use crate::desktop::chrome::id as element_id;
use crate::status;
use crate::terminal_view::mouse::Reporting;
use crate::terminal_view::scroll::WheelRows;
use crate::terminal_view::surface::{self, Metrics};
use crate::terminal_view::{Clicks, Drag, Slot, cursor, menu, scroll};
use crate::terminals::sessions::{Grid, Session};
use crate::util::basename;
use agents::Summary;
use daemon::Info;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::ops::Range;
use std::time::Duration;
use term::Scroll;
use theme::*;
use ui::icon_button_sized;

/// "/bin/zsh -l" reads as "zsh": the login flag the desktop adds says nothing about the pane.
/// A login shell shows as its name only: the script it may run to start an agent is ours, not the user's.
pub fn command_line(info: &Info) -> String {
    if info.args.first().is_some_and(|a| a == "-l") {
        return basename(&info.cmd);
    }
    std::iter::once(basename(&info.cmd)).chain(info.args.iter().cloned()).collect::<Vec<_>>().join(" ")
}

pub fn pane_label(agent: Option<&Summary>, session: Option<&Session>) -> String {
    match (agent, session) {
        (Some(a), _) => a.provider.clone(),
        (None, Some(s)) => s.busy().map_or_else(|| command_line(&s.info), str::to_string),
        (None, None) => "session".into(),
    }
}

/// The banner each agent had when the user closed it, until the agent's banner changes.
#[derive(Default)]
pub struct DismissedBanners(HashMap<String, &'static str>);

impl DismissedBanners {
    pub fn dismiss(&mut self, agent: &str, text: &'static str) {
        self.0.insert(agent.to_string(), text);
    }

    /// The banner to draw for `agent`. Any other banner, or none, forgets the close, so the closed one can come back.
    pub fn shown(&mut self, agent: &str, banner: Option<&'static str>) -> Option<&'static str> {
        if self.0.get(agent).copied() == banner {
            return None;
        }
        self.0.remove(agent);
        banner
    }
}

/// What `Desktop` hands a pane each time it draws it.
pub(crate) struct Look {
    pub metrics: Metrics,
    pub grid: Option<Grid>,
    /// What the pane says in place of a screen.
    pub placeholder: &'static str,
    pub exit: Option<i32>,
    /// A banner above the screen, and the agent it is for.
    pub banner: Option<(&'static str, String)>,
    /// Set when the pane has the window's terminal focus.
    pub focus: Option<FocusHandle>,
    /// Typing reaches the terminal: it holds keyboard focus and no overlay is open.
    pub keyboard: bool,
    pub flashing: bool,
    pub menu: Option<Point<Pixels>>,
    pub observe_only: bool,
    pub known: bool,
    pub may_send: bool,
    pub may_close: bool,
    pub has_agent: bool,
    pub clicks: Clicks,
}

pub(crate) enum PaneEvent {
    /// The size, in cells, the pane has room for.
    Fit(u16, u16),
    /// A press landed in the pane.
    Focus,
    /// Files dropped on it, as the text to paste.
    Dropped(String),
    /// Input as if typed: the view returns to the prompt.
    Input(Vec<u8>),
    /// A mouse event for the program tracking the mouse.
    Report(Vec<u8>),
    OpenLink(String),
    OpenMenu(Point<Pixels>),
    CloseMenu,
    Pick(menu::Item),
    /// The user closed the banner `agent` showed.
    Banner(String, &'static str),
}

/// One terminal's pane: its screen, caret and mouse; `Desktop` hands it a `Look` each time it draws it.
pub struct TerminalPane {
    pub(super) id: String,
    pub(super) look: Option<Look>,
    /// The IME's uncommitted text, drawn at the cursor until it commits.
    marked: Option<String>,
    /// The cursor's cell on screen, where the IME puts its candidate window.
    caret: Option<Bounds<Pixels>>,
    pub(super) selection: Option<Drag>,
    pub(super) reporting: Reporting,
    pub(super) wheel: WheelRows,
    pub(super) autoscroll: Option<Task<()>>,
    pub(super) link_hover: bool,
    blink: Option<Task<()>>,
    blink_on: bool,
    /// Drawn since the blink timer last ticked; a pane that is no longer drawn stops it.
    drawn: bool,
}

impl EventEmitter<PaneEvent> for TerminalPane {}

impl TerminalPane {
    fn new(id: String) -> Self {
        Self {
            id,
            look: None,
            marked: None,
            caret: None,
            selection: None,
            reporting: Reporting::default(),
            wheel: WheelRows::default(),
            autoscroll: None,
            link_hover: false,
            blink: None,
            blink_on: true,
            drawn: false,
        }
    }

    pub(super) fn set_caret(&mut self, at: Bounds<Pixels>) {
        self.caret = Some(at);
    }

    fn set(&mut self, look: Look) {
        self.look = Some(look);
    }

    pub(crate) fn forget_selection(&mut self) {
        self.selection = None;
    }

    /// The view returns to the prompt once input reaches the terminal.
    pub(crate) fn typed(&mut self) {
        self.blink_on = true;
        self.blink = None;
    }

    pub(super) fn scroll_to_bottom(&mut self, cx: &mut Context<Self>) {
        if let Some(t) = self.look.as_ref().and_then(|l| l.grid.as_ref()) {
            t.get().scroll(Scroll::Bottom);
            cx.notify();
        }
    }

    /// Keeps one blink timer running while the cursor should blink.
    fn sync_blink(&mut self, on: bool, cx: &mut Context<Self>) {
        if on == self.blink.is_some() {
            return;
        }
        self.blink_on = true;
        self.blink = on.then(|| {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_millis(530)).await;
                    let ticked = this.update(cx, |p, cx| {
                        if !std::mem::take(&mut p.drawn) {
                            p.blink = None;
                            p.blink_on = true;
                            return false;
                        }
                        p.blink_on = !p.blink_on;
                        cx.notify();
                        true
                    });
                    if !ticked.unwrap_or(false) {
                        break;
                    }
                }
            })
        });
    }
}

impl Render for TerminalPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.drawn = true;
        let Some(look) = self.look.take() else { return div() };
        let id = self.id.clone();
        let m = &look.metrics;
        let focused = look.focus.is_some();
        let banner = look.banner.clone().map(|(text, agent)| {
            let close = icon_button_sized(element_id(format!("banner-close:{id}")), "x", 20., TEXT_3).rounded(px(5.)).on_click(cx.listener(
                move |_, _: &ClickEvent, _, cx| {
                    cx.emit(PaneEvent::Banner(agent.clone(), text));
                    cx.notify();
                },
            ));
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(8.))
                .pl(px(16.))
                .pr(px(8.))
                .py(px(4.))
                .border_b(px(0.5))
                .border_color(SEPARATOR)
                .bg(FILL_2)
                .text_size(px(12.))
                .text_color(TEXT_2)
                .child(div().flex_1().min_w_0().child(text))
                .child(close)
        });
        let typing = focused && look.keyboard;
        let mut frame_blink = false;
        let (body, grid, scrolled, caret) = match look.grid.as_ref() {
            Some(t) => {
                let mut t = t.get();
                let (f, cells) = t.frame();
                if typing {
                    frame_blink = f.cursor_visible == 1 && f.cursor_blink == 1;
                }
                let c = cursor::cursor(&f, typing, self.blink_on);
                let caret = (focused || c.is_some()).then_some(((f.cursor_x, f.cursor_y), f.rows, c));
                (surface::screen(&f, cells, m), Some((f.cols, f.rows)), f.at_bottom == 0, caret)
            }
            None => (div().text_color(TEXT_3).child(look.placeholder), None, false, None),
        };
        self.sync_blink(cursor::blinks(frame_blink, cx.reduce_motion(), window.is_window_active()), cx);
        let caret = caret.map(|(at, rows, c)| {
            let preedit = self.marked.clone().filter(|_| typing);
            cursor::overlay(focused.then(|| cx.entity()), at, rows, c, preedit, m)
        });
        let screen = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .child(surface::surface(cx.entity(), m, grid, look.focus.clone()))
            .child(body)
            .children(caret)
            .children(scrolled.then(|| scroll::jump_pill(&id, cx)))
            .when(look.flashing, |d| d.child(div().absolute().inset_0().bg(FILL_4)));
        let menu = look.menu.map(|at| Self::menu(at, &look, cx));
        let exit = look.exit;
        let (pad, font, size, line) = (m.pad, m.font.family.clone(), m.size, m.line);
        self.look = Some(look);
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(SURFACE_SUNKEN)
            .overflow_hidden()
            .on_any_mouse_down(cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(PaneEvent::Focus)))
            .drag_over::<ExternalPaths>(|s, _, _, _| s.shadow(vec![BoxShadow { inset: true, ..ui::ring(ACCENT, 1.5) }]))
            .on_drop(cx.listener(|_, paths: &ExternalPaths, _, cx| cx.emit(PaneEvent::Dropped(term::dropped_paths(paths.paths())))))
            .children(banner)
            .children(menu)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .py(px(pad / 2.))
                    .px(px(pad))
                    .font_family(font)
                    .text_size(px(size))
                    .line_height(px(line))
                    .text_color(TEXT)
                    .child(screen),
            )
            .children(exit.map(|c| {
                div().px(px(24.)).pb(px(12.)).text_size(px(12.)).text_color(if c == 0 { TEXT_3 } else { FAILED }).child(format!("Process exited with code {c}"))
            }))
    }
}

/// Typed text arrives here rather than as key-downs so IMEs (Telex, dead keys, CJK) can compose it.
impl EntityInputHandler for TerminalPane {
    fn text_for_range(&mut self, _: Range<usize>, _: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        None
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|text| 0..text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        if self.look.as_ref().is_some_and(|l| l.keyboard) {
            cx.emit(PaneEvent::Input(text.as_bytes().to_vec()));
        }
    }

    fn replace_and_mark_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: Option<Range<usize>>, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = (!text.is_empty() && self.look.as_ref().is_some_and(|l| l.keyboard)).then(|| text.to_string());
        cx.notify();
    }

    fn bounds_for_range(&mut self, _: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        self.caret
    }

    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}

impl Desktop {
    pub fn pane_label(&self, id: &str) -> String {
        pane_label(self.summary(id), self.terminals.sessions.get(id))
    }

    /// The body of a terminal tab; only the focused pane's takes keys.
    pub(crate) fn term_body(&mut self, id: &str, focused: bool, window: &mut Window, cx: &mut Context<Self>) -> Div {
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .border_t(px(0.5))
            .border_color(SEPARATOR)
            .when(focused, |d| {
                d.key_context(keys::CONTEXT)
                    .track_focus(&self.terminal.focus)
                    .on_key_down(cx.listener(Self::on_term_key))
                    .on_action(cx.listener(Self::copy_selection))
                    .on_action(cx.listener(Self::select_all))
                    .on_action(cx.listener(Self::paste))
            })
            .child(self.terminal_pane(id, window, cx))
    }

    /// Terminal `id`'s pane, given what it draws from now.
    pub(crate) fn terminal_pane(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> Entity<TerminalPane> {
        let view = self.terminal_entity(id, window, cx);
        let focused = self.terminal.focused.as_deref() == Some(id);
        let session = self.terminals.sessions.get(id);
        let (grid, exit, known) = (session.and_then(|s| s.term.clone()), session.and_then(|s| s.exit), session.is_some());
        let placeholder = if grid.is_some() {
            ""
        } else {
            let offline = hosts::session::untag(id).0.is_some_and(|h| hosts::remote::connected(h).is_none());
            match (known, offline) {
                (true, true) => "Disconnected from the host.",
                (true, false) => "Connecting…",
                (false, _) => "This session is not running.",
            }
        };
        let banner = self.summary(id).map(|a| (a.id.clone(), status::banner(a)));
        let banner = banner.and_then(|(agent, banner)| Some((self.terminal.dismissed.shown(&agent, banner)?, agent)));
        let observe_only = self.agents.observe_only();
        let look = Look {
            metrics: Metrics::of(&self.store.terminal),
            grid,
            placeholder,
            exit,
            banner,
            focus: focused.then(|| self.terminal.focus.clone()),
            keyboard: self.terminal.focus.is_focused(window) && self.overlay.is_none(),
            flashing: self.terminals.bell.flashing.as_deref() == Some(id),
            menu: self.terminal.menu.as_ref().filter(|(p, _)| p == id).map(|(_, at)| *at),
            observe_only,
            known,
            may_send: self.terminals.may_send(id, observe_only),
            may_close: self.terminals.may_close(id, observe_only),
            has_agent: self.summary(id).is_some(),
            clicks: Clicks::from(&self.store.terminal),
        };
        view.update(cx, |p, _| p.set(look));
        view
    }

    fn terminal_entity(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> Entity<TerminalPane> {
        if let Some(slot) = self.terminal.panes.get(id) {
            return slot.view.clone();
        }
        let view = cx.new(|_| TerminalPane::new(id.to_string()));
        let key = id.to_string();
        let sub = cx.subscribe_in(&view, window, move |this, _, ev, window, cx| this.on_terminal_pane(&key, ev, window, cx));
        self.terminal.panes.insert(id.to_string(), Slot { view: view.clone(), _sub: sub });
        view
    }
}

#[cfg(test)]
mod tests {
    use super::{DismissedBanners, Info, command_line, pane_label};
    use crate::terminals::sessions::Session;
    use agents::Summary;

    fn shell(foreground: &str, exit: Option<i32>) -> Session {
        let info = Info { cmd: "/bin/zsh".into(), args: vec!["-l".into()], foreground: foreground.into(), ..Default::default() };
        Session { info, term: None, exit }
    }

    #[test]
    fn an_agent_pane_is_labelled_by_its_provider() {
        let agent = Summary { provider: "claude".into(), ..Default::default() };
        assert_eq!(pane_label(Some(&agent), Some(&shell("claude", None))), "claude");
    }

    #[test]
    fn a_shell_pane_shows_its_foreground_command_until_it_exits() {
        assert_eq!(pane_label(None, Some(&shell("npm run dev", None))), "npm run dev");
        assert_eq!(pane_label(None, Some(&shell("", None))), "zsh");
        assert_eq!(pane_label(None, Some(&shell("npm run dev", Some(1)))), "zsh");
    }

    #[test]
    fn a_pane_without_a_session_reads_as_session() {
        assert_eq!(pane_label(None, None), "session");
    }

    #[test]
    fn login_shells_show_as_their_name() {
        let info = Info { cmd: "/bin/zsh".into(), args: vec!["-l".into()], ..Default::default() };
        assert_eq!(command_line(&info), "zsh");
        let info =
            Info { cmd: "/opt/homebrew/bin/fish".into(), args: ["-l", "-c", "$argv; exec fish -l", "claude"].map(String::from).to_vec(), ..Default::default() };
        assert_eq!(command_line(&info), "fish");
        let info = Info { cmd: "pnpm".into(), args: vec!["dev".into(), "--port".into(), "8081".into()], ..Default::default() };
        assert_eq!(command_line(&info), "pnpm dev --port 8081");
    }

    #[test]
    fn a_closed_banner_stays_hidden_until_its_agent_shows_another() {
        let mut dismissed = DismissedBanners::default();
        dismissed.dismiss("a", "Interrupted by restart");
        assert_eq!(dismissed.shown("a", Some("Interrupted by restart")), None);
        assert_eq!(dismissed.shown("b", Some("Interrupted by restart")), Some("Interrupted by restart"));
        assert_eq!(dismissed.shown("a", Some("Couldn't resume")), Some("Couldn't resume"));
    }

    #[test]
    fn a_closed_banner_comes_back_once_its_agent_showed_something_else() {
        let mut dismissed = DismissedBanners::default();
        dismissed.dismiss("a", "Interrupted by restart");
        assert_eq!(dismissed.shown("a", None), None);
        assert_eq!(dismissed.shown("a", Some("Interrupted by restart")), Some("Interrupted by restart"));
    }
}
