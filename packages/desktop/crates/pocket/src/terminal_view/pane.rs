use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use crate::status;
use crate::terminal_view::surface::{self, Metrics};
use crate::terminals::sessions::Session;
use crate::util::basename;
use agents::Summary;
use daemon::Info;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, icon_button_sized};

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

pub fn pane_title(agent: Option<&Summary>, session: Option<&Session>) -> String {
    match agent {
        Some(a) => format!("{} — {}", a.provider, basename(&a.cwd)),
        None => pane_label(None, session),
    }
}

/// Numbers a split tab's panes in reading order, for their headers; a lone pane has no header.
pub fn numbered(rows: Vec<Vec<String>>) -> Vec<Vec<(String, Option<usize>)>> {
    let split = rows.len() > 1 || rows[0].len() > 1;
    let mut n = 0;
    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .map(|id| {
                    n += 1;
                    (id, split.then_some(n))
                })
                .collect()
        })
        .collect()
}

impl Desktop {
    pub fn pane_label(&self, id: &str) -> String {
        pane_label(self.summary(id), self.terminals.sessions.get(id))
    }

    pub(crate) fn panes(&mut self, rows: Vec<Vec<String>>, cx: &mut Context<Self>) -> Div {
        let mut out = Vec::new();
        for (r, row) in numbered(rows).into_iter().enumerate() {
            let m = if r == 0 { &surface::MAIN } else { &surface::SMALL };
            let panes: Vec<Div> = row.into_iter().map(|(id, n)| self.pane(&id, n, m, cx)).collect();
            out.push(div().flex().gap(px(0.5)).min_h_0().when(r == 0, |d| d.flex_1()).when(r > 0, |d| d.h(px(250.)).flex_none()).children(panes));
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(0.5))
            .bg(rgba(SEPARATOR))
            .border_t(px(0.5))
            .border_color(rgba(SEPARATOR))
            .key_context(keys::CONTEXT)
            .track_focus(&self.terminal.focus)
            .on_key_down(cx.listener(Self::on_term_key))
            .children(out)
    }

    pub fn pane(&mut self, id: &str, n: Option<usize>, m: &'static Metrics, cx: &mut Context<Self>) -> Div {
        let focused = self.terminal.focused.as_deref() == Some(id);
        let exit = self.terminals.sessions.get(id).and_then(|s| s.exit);
        let known = self.terminals.sessions.get(id).is_some();
        let title = pane_title(self.summary(id), self.terminals.sessions.get(id));
        let banner = self.summary(id).and_then(status::banner).map(|text| {
            div().flex_none().px(px(16.)).py(px(6.)).border_b(px(0.5)).border_color(rgba(SEPARATOR)).bg(rgba(FILL_2)).text_size(px(12.)).text_color(rgba(TEXT_2)).child(text)
        });
        let body = match self.terminals.sessions.get_mut(id).and_then(|s| s.term.as_mut()) {
            Some(t) => {
                let (f, cells) = t.frame();
                surface::screen(&f, cells, m)
            }
            None => div().text_color(rgba(TEXT_3)).child(if known { "Connecting…" } else { "This session is not running." }),
        };
        let close_id = id.to_string();
        let header = n.map(|n| {
            div()
                .h(px(30.))
                .flex_none()
                .pl(px(16.))
                .pr(px(6.))
                .flex()
                .items_center()
                .border_b(px(0.5))
                .border_color(rgba(SEPARATOR))
                .text_color(rgba(if focused { TEXT } else { TEXT_3 }))
                .font_family(MONO)
                .text_size(px(11.5))
                .child(div().flex_1().truncate().child(format!("{n} · {title}")))
                .child(icon_button_sized(self::id(format!("close-{close_id}")), "x", 22., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.close_pane(&close_id, cx);
                })))
        });
        let focus_id = id.to_string();
        let screen = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .child(surface::surface(cx.entity(), id.to_string(), m, focused.then(|| self.terminal.focus.clone())))
            .child(body);
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgba(SURFACE_SUNKEN))
            .when(focused && n.is_some(), |d| d.shadow(vec![BoxShadow { inset: true, ..ui::ring(SEPARATOR_STRONG, 0.5) }]))
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, window, cx| this.focus_pane(focus_id.clone(), window, cx)))
            .children(header)
            .children(banner)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .pt(px(10.))
                    .px(px(20.))
                    .pb(px(14.))
                    .font_family(MONO)
                    .text_size(px(m.size))
                    .line_height(px(m.line))
                    .text_color(rgba(TEXT))
                    .child(screen),
            )
            .children(exit.map(|c| {
                div().px(px(24.)).pb(px(12.)).text_size(px(12.)).text_color(rgba(if c == 0 { TEXT_3 } else { FAILED })).child(format!("Process exited with code {c}"))
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::{Info, command_line, numbered, pane_label, pane_title};
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
    fn an_agent_pane_title_names_its_folder_and_a_shell_one_its_label() {
        let agent = Summary { provider: "codex".into(), cwd: "/code/pocket".into(), ..Default::default() };
        assert_eq!(pane_title(Some(&agent), None), "codex — pocket");
        assert_eq!(pane_title(None, Some(&shell("", None))), "zsh");
    }

    fn rows(rows: &[&[&str]]) -> Vec<Vec<String>> {
        rows.iter().map(|r| r.iter().map(|id| id.to_string()).collect()).collect()
    }

    #[test]
    fn a_lone_pane_goes_unnumbered() {
        assert_eq!(numbered(rows(&[&["a"]])), vec![vec![("a".to_string(), None)]]);
    }

    #[test]
    fn split_panes_are_numbered_in_reading_order() {
        let n = |id: &str, n| (id.to_string(), Some(n));
        assert_eq!(numbered(rows(&[&["a", "b"], &["c"]])), vec![vec![n("a", 1), n("b", 2)], vec![n("c", 3)]]);
        assert_eq!(numbered(rows(&[&["a"], &["b"]])), vec![vec![n("a", 1)], vec![n("b", 2)]]);
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
