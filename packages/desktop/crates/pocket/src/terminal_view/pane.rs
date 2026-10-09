use crate::desktop::Desktop;
use crate::status;
use crate::terminal_view::{cursor, scroll};
use crate::terminal_view::surface::{self, Metrics};
use crate::terminals::sessions::Session;
use crate::util::basename;
use agents::Summary;
use daemon::Info;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

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

impl Desktop {
    pub fn pane_label(&self, id: &str) -> String {
        pane_label(self.summary(id), self.terminals.sessions.get(id))
    }

    /// The body of a terminal tab; only the focused pane's takes keys.
    pub(crate) fn term_body(&mut self, id: &str, focused: bool, cx: &mut Context<Self>) -> Div {
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
            .child(self.pane(id, &Metrics::of(&self.store.terminal), cx))
    }

    pub fn pane(&mut self, id: &str, m: &Metrics, cx: &mut Context<Self>) -> Div {
        let focused = self.terminal.focused.as_deref() == Some(id);
        let exit = self.terminals.sessions.get(id).and_then(|s| s.exit);
        let known = self.terminals.sessions.get(id).is_some();
        let banner = self.summary(id).and_then(status::banner).map(|text| {
            div().flex_none().px(px(16.)).py(px(6.)).border_b(px(0.5)).border_color(SEPARATOR).bg(FILL_2).text_size(px(12.)).text_color(TEXT_2).child(text)
        });
        let typing = focused && self.terminal.keyboard;
        let blink_on = self.terminal.blink_on;
        let (body, grid, scrolled, caret) = match self.terminals.sessions.get_mut(id).and_then(|s| s.term.as_mut()) {
            Some(t) => {
                let (f, cells) = t.frame();
                if typing {
                    self.terminal.cursor_blinks = f.cursor_visible == 1 && f.cursor_blink == 1;
                }
                let c = cursor::cursor(&f, typing, blink_on);
                let caret = (focused || c.is_some()).then_some(((f.cursor_x, f.cursor_y), f.rows, c));
                (surface::screen(&f, cells, m), Some((f.cols, f.rows)), f.at_bottom == 0, caret)
            }
            None => (div().text_color(TEXT_3).child(if known { "Connecting…" } else { "This session is not running." }), None, false, None),
        };
        let caret = caret.map(|(at, rows, c)| {
            let preedit = self.terminal.marked.clone().filter(|_| typing);
            cursor::overlay(focused.then(|| cx.entity()), at, rows, c, preedit, m)
        });
        let focus_id = id.to_string();
        let drop_id = id.to_string();
        let screen = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .child(surface::surface(cx.entity(), id.to_string(), m, grid, focused.then(|| self.terminal.focus.clone())))
            .child(body)
            .children(caret)
            .children(scrolled.then(|| scroll::jump_pill(id, cx)))
            .when(self.terminals.bell.flashing.as_deref() == Some(id), |d| d.child(div().absolute().inset_0().bg(FILL_4)));
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(SURFACE_SUNKEN)
            .overflow_hidden()
            .on_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| this.focus_pane(focus_id.clone(), window, cx)))
            .drag_over::<ExternalPaths>(|s, _, _, _| s.shadow(vec![BoxShadow { inset: true, ..ui::ring(ACCENT, 1.5) }]))
            .on_drop(cx.listener(move |this, paths: &ExternalPaths, window, cx| this.drop_paths(drop_id.clone(), paths, window, cx)))
            .children(banner)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .py(px(m.pad / 2.))
                    .px(px(m.pad))
                    .font_family(m.font.family.clone())
                    .text_size(px(m.size))
                    .line_height(px(m.line))
                    .text_color(TEXT)
                    .child(screen),
            )
            .children(exit.map(|c| {
                div().px(px(24.)).pb(px(12.)).text_size(px(12.)).text_color(if c == 0 { TEXT_3 } else { FAILED }).child(format!("Process exited with code {c}"))
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::{Info, command_line, pane_label};
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
        let info = Info { cmd: "/opt/homebrew/bin/fish".into(), args: ["-l", "-c", "$argv; exec fish -l", "claude"].map(String::from).to_vec(), ..Default::default() };
        assert_eq!(command_line(&info), "fish");
        let info = Info { cmd: "pnpm".into(), args: vec!["dev".into(), "--port".into(), "8081".into()], ..Default::default() };
        assert_eq!(command_line(&info), "pnpm dev --port 8081");
    }
}
