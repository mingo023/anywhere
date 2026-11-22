use super::{Primary, Run, Stage, Tag, primary, tag};
use crate::desktop::Desktop;
use crate::spaces::Space;
use crate::spaces::header::tile;
use crate::util::plural;
use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use handoff::Step;
use handoff::checks::Problem;
use theme::*;
use ui::{self, Variant};

const WIDTH: f32 = 520.;
const TOP: f32 = 108.;

fn sheet(children: impl IntoIterator<Item = AnyElement>) -> Div {
    div().absolute().top(px(TOP)).left_0().right_0().flex().justify_center().child(
        ui::pop(div().id("handoff-sheet").w(px(WIDTH)).pt(px(22.)).px(px(24.)).pb(px(20.)).flex().flex_col().gap(px(16.)))
            .rounded(px(R_DIALOG))
            .occlude()
            .children(children),
    )
}

fn x_button(id: &'static str, name: &str) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(28.))
        .mt(px(-1.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(14.))
        .cursor_pointer()
        .hover(|s| s.bg(FILL_3))
        .child(icon(name, 15., TEXT_3))
}

fn header(title: String, close: Stateful<Div>) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap(px(12.))
        .child(div().flex_1().text_size(px(20.)).line_height(px(26.)).font_weight(FontWeight::BOLD).child(title))
        .child(close.mr(px(-4.)))
        .into_any_element()
}

fn sub(text: String) -> AnyElement {
    div().mt(px(-8.)).text_size(px(13.5)).line_height(px(20.)).text_color(TEXT_2).child(text).into_any_element()
}

fn label(text: String) -> AnyElement {
    div().mx(px(2.)).mb(px(-8.)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child(text).into_any_element()
}

/// Rows in a rounded surface, split by hairlines.
fn boxed(rows: Vec<AnyElement>) -> AnyElement {
    let n = rows.len();
    div()
        .flex()
        .flex_col()
        .rounded(px(12.))
        .bg(SURFACE)
        .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5), ui::shadow(rgba(0x0000000a), 1., 2.)])
        .overflow_hidden()
        .children(rows.into_iter().enumerate().map(move |(i, r)| div().when(i > 0 && i < n, |d| d.border_t(px(0.5)).border_color(SEPARATOR)).child(r)))
        .into_any_element()
}

fn foot(buttons: Vec<Stateful<Div>>) -> AnyElement {
    div().mt(px(4.)).flex().items_center().justify_end().gap(px(8.)).children(buttons).into_any_element()
}

fn button(id: &'static str, v: Variant, text: impl Into<SharedString>) -> Stateful<Div> {
    ui::large(ui::button(id, v, None, text.into()))
}

fn tag_view(t: Tag) -> AnyElement {
    let row = div().flex().flex_none().items_center().gap(px(5.)).text_size(px(12.)).whitespace_nowrap();
    match t {
        Tag::Checking => row.text_color(TEXT_3).child(spinner("handoff-checking", 10., TEXT_3)).child("Checking…"),
        Tag::Ready => row.text_color(SUCCESS_TEXT).child(icon("check", 12., SUCCESS_TEXT)).child("Ready"),
        Tag::NotReady => row.text_color(WAITING_TEXT).child("Not ready"),
        Tag::SummaryOnly => row.text_color(WAITING_TEXT).child("Summary only"),
    }
    .into_any_element()
}

fn moves_row(glyph: &str, text: AnyElement, meta: AnyElement) -> AnyElement {
    div()
        .px(px(12.))
        .py(px(9.))
        .flex()
        .items_center()
        .gap(px(10.))
        .text_size(px(13.))
        .child(div().w(px(14.)).flex().flex_none().justify_center().child(icon(glyph, 13., TEXT_3)))
        .child(div().flex_1().min_w_0().truncate().child(text))
        .child(meta)
        .into_any_element()
}

fn meta(text: String) -> AnyElement {
    div().flex_none().text_size(px(12.)).text_color(TEXT_3).whitespace_nowrap().child(text).into_any_element()
}

impl Desktop {
    pub(crate) fn handoff_view(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(h) = self.handoff.open.as_ref() else { return div() };
        match (&h.stage, h.run.as_ref().map(|r| r.failed.is_some())) {
            (Stage::Pick, _) => self.pick_sheet(cx),
            (Stage::Summary, _) => self.summary_sheet(cx),
            (Stage::Moving, Some(true)) => self.failed_sheet(cx),
            (Stage::Moving, _) => self.progress_sheet(cx),
            (Stage::Done, _) => self.done_sheet(cx),
        }
    }

    fn close_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        x_button("handoff-close", "x").on_click(cx.listener(|d, _: &ClickEvent, window, cx| d.close_overlay(window, cx)))
    }

    fn pick_sheet(&mut self, cx: &mut Context<Self>) -> Div {
        let online = self.online_hosts();
        let h = self.handoff.open.as_ref().expect("open");
        let s = &h.source;
        let title = header("Hand off to a host".into(), self.close_button(cx));
        if online.is_empty() {
            let empty = div()
                .px(px(20.))
                .py(px(28.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.))
                .text_center()
                .child(icon("server", 22., TEXT_4))
                .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child("No host is connected"))
                .child(div().max_w(px(320.)).text_size(px(12.5)).text_color(TEXT_3).child("Hosts you add in Settings show here while they’re connected."))
                .child(ui::button("handoff-add-host", Variant::Secondary, Some("plus"), "Add a host…").mt(px(6.)).on_click(cx.listener(
                    |d, _: &ClickEvent, window, cx| {
                        d.close_overlay(window, cx);
                        d.show_hosts(window, cx);
                    },
                )));
            let close = button("handoff-cancel", Variant::Ghost, "Close").on_click(cx.listener(|d, _: &ClickEvent, window, cx| d.close_overlay(window, cx)));
            return sheet([title, boxed(vec![empty.into_any_element()]), foot(vec![close])]);
        }
        let conversation = s.conversation();
        let drafted = !self.handoff.summary.read(cx).value().trim().is_empty();
        let picked = h.host.clone().unwrap_or_default();
        let order = self.space_order();
        let rows = online
            .iter()
            .map(|id| {
                let host = self.fleet.hosts.list.iter().find(|x| &x.id == id);
                let space = Space::on(Some(id));
                let dot = div().absolute().right(px(-2.)).bottom(px(-2.)).size(px(9.)).rounded(px(5.)).bg(SUCCESS).shadow(vec![ui::ring(SURFACE, 1.5)]);
                let pick = id.clone();
                div()
                    .id(SharedString::from(format!("handoff-host-{id}")))
                    .px(px(12.))
                    .py(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(11.))
                    .cursor_pointer()
                    .when(*id == picked, |d| d.bg(FILL_2))
                    .when(*id != picked, |d| d.hover(|s| s.bg(FILL_1)))
                    .child(ui::radio(*id == picked))
                    .child(tile(self.space_glyph(&space), self.space_colour(&space, &order), 28.).relative().child(dot))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(self.host_name(id)))
                            .child(div().truncate().text_size(px(12.)).text_color(TEXT_3).child(host.map(|h| h.address()).unwrap_or_default())),
                    )
                    .child(tag_view(tag(h.probes.get(id), conversation)))
                    .on_click(cx.listener(move |d, _: &ClickEvent, _, cx| {
                        if let Some(h) = d.handoff.open.as_mut() {
                            h.host = Some(pick.clone());
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        let host_name = self.host_name(&picked);
        let t = tag(h.probe(), conversation);
        let mut body = vec![title, sub(format!("“{}” · {}", s.title, s.agent_name)), label("Host".into()), boxed(rows)];
        if let Some(p) = h.probe().filter(|p| !p.problems.is_empty()) {
            body.push(label(format!("Fix on {host_name}, then check again")));
            body.push(boxed(p.problems.iter().enumerate().map(|(i, problem)| self.fix_row(i, problem, &picked, &host_name, &s.agent_name, cx)).collect()));
        } else if let Some(why) = &s.why_not {
            body.push(
                div()
                    .flex()
                    .gap(px(10.))
                    .px(px(12.))
                    .py(px(11.))
                    .rounded(px(10.))
                    .bg(WAITING_BG)
                    .text_size(px(12.5))
                    .line_height(px(18.))
                    .text_color(TEXT_BODY)
                    .child(div().mt(px(2.)).child(icon("warning", 14., WAITING)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(format!("The conversation can’t move to {host_name}")))
                            .child(format!("{why} A new {} session starts from a summary instead.", s.agent_name)),
                    )
                    .into_any_element(),
            );
        }
        let branch = div().font_family(MONO).text_size(px(12.5)).child(s.branch.clone()).into_any_element();
        let files = div().child(plural(s.files, "changed file", "changed files")).into_any_element();
        let conv = if conversation {
            meta(if s.messages > 0 { plural(s.messages, "message", "messages") } else { String::new() })
        } else {
            div()
                .h(px(22.))
                .pl(px(7.))
                .pr(px(9.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(5.))
                .rounded(px(11.))
                .bg(FILL_2)
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(TEXT_2)
                .child(icon("sparkle", 12., TEXT_3))
                .child("Sent as a summary")
                .into_any_element()
        };
        body.push(label("What moves".into()));
        body.push(boxed(vec![
            moves_row("branch", branch, meta(plural(s.commits, "commit", "commits"))),
            moves_row("file", files, ui::diffstat(s.added, s.removed).flex_none().into_any_element()),
            moves_row("comment", div().when(!conversation, |d| d.text_color(TEXT_3)).child("Conversation").into_any_element(), conv),
        ]));
        let cancel = button("handoff-cancel", Variant::Ghost, "Cancel").on_click(cx.listener(|d, _: &ClickEvent, window, cx| d.close_overlay(window, cx)));
        let go = match primary(t, drafted) {
            Primary::CheckAgain => button("handoff-go", Variant::Primary, "Check again").on_click(cx.listener(|d, _: &ClickEvent, _, cx| d.check_hosts(cx))),
            Primary::HandOff => button("handoff-go", Variant::Primary, "Hand off").on_click(cx.listener(|d, _: &ClickEvent, _, cx| d.start_handoff(cx))),
            Primary::ReviewSummary | Primary::WriteSummary => {
                let text = if primary(t, drafted) == Primary::ReviewSummary { "Review summary…" } else { "Write summary…" };
                button("handoff-go", Variant::Primary, text).on_click(cx.listener(|d, _: &ClickEvent, window, cx| {
                    if let Some(h) = d.handoff.open.as_mut() {
                        h.stage = Stage::Summary;
                    }
                    d.handoff.summary.update(cx, |t, cx| t.focus(window, cx));
                    cx.notify();
                }))
            }
            Primary::Waiting => button("handoff-go", Variant::Primary, if conversation { "Hand off" } else { "Review summary…" }).opacity(0.4).cursor_default(),
        };
        body.push(foot(vec![cancel, go]));
        sheet(body)
    }

    fn fix_row(&self, i: usize, problem: &Problem, host: &str, host_name: &str, agent: &str, cx: &mut Context<Self>) -> AnyElement {
        let command = problem.fix().map(|fix| {
            let copy = fix.clone();
            div()
                .mt(px(8.))
                .h(px(30.))
                .pl(px(10.))
                .pr(px(4.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(8.))
                .bg(FILL_1)
                .shadow(vec![ui::ring(SEPARATOR, 0.5)])
                .child(div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(12.)).text_color(TEXT).child(fix))
                .child(
                    div()
                        .id(SharedString::from(format!("handoff-copy-{i}")))
                        .h(px(22.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .text_size(px(12.))
                        .text_color(TEXT_2)
                        .hover(|s| s.bg(FILL_3))
                        .child(icon("copy", 12., TEXT_2))
                        .child("Copy")
                        .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))),
                )
        });
        let host = host.to_string();
        let terminal = ui::button(SharedString::from(format!("handoff-terminal-{i}")), Variant::Secondary, Some("terminal"), "Open terminal")
            .h(px(26.))
            .text_size(px(12.))
            .on_click(cx.listener(move |d, _: &ClickEvent, window, cx| d.open_host_shell(&host, window, cx)));
        div()
            .flex()
            .gap(px(10.))
            .px(px(12.))
            .py(px(10.))
            .text_size(px(13.))
            .line_height(px(18.))
            .child(div().w(px(16.)).h(px(18.)).flex().flex_none().items_center().justify_center().child(icon("warning", 14., WAITING)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(problem.title(host_name, agent)))
                    .children(problem.why().map(|w| div().mt(px(1.)).text_size(px(12.5)).text_color(TEXT_3).child(w)))
                    .children(command)
                    .when(*problem != Problem::Offline, |d| d.child(div().mt(px(8.)).flex().child(terminal))),
            )
            .into_any_element()
    }

    fn summary_sheet(&mut self, cx: &mut Context<Self>) -> Div {
        let h = self.handoff.open.as_ref().expect("open");
        let agent = h.source.agent_name.clone();
        let from_conversation = h.source.session.is_some();
        let head = div()
            .flex()
            .items_start()
            .gap(px(12.))
            .child(x_button("handoff-back-arrow", "back").ml(px(-6.)).on_click(cx.listener(back_to_pick)))
            .child(div().flex_1().text_size(px(20.)).line_height(px(26.)).font_weight(FontWeight::BOLD).child(format!("Summary for the new {agent} session")))
            .child(self.close_button(cx).mr(px(-4.)));
        let field = ui::field_box()
            .h_auto()
            .min_h(px(150.))
            .py(px(11.))
            .items_start()
            .text_size(px(13.))
            .line_height(px(20.))
            .child(Textarea::new(&self.handoff.summary).appearance(false));
        let empty = self.handoff.summary.read(cx).value().trim().is_empty();
        let go = button("handoff-go", Variant::Primary, "Move code & start");
        let go = if empty { go.opacity(0.4).cursor_default() } else { go.on_click(cx.listener(|d, _: &ClickEvent, _, cx| d.start_handoff(cx))) };
        sheet([
            head.into_any_element(),
            sub(if from_conversation { "It becomes the new session’s first prompt.".into() } else { "What should the new session know?".into() }),
            field.into_any_element(),
            foot(vec![button("handoff-back", Variant::Ghost, "Back").on_click(cx.listener(back_to_pick)), go]),
        ])
    }

    fn steps_box(&self) -> AnyElement {
        let h = self.handoff.open.as_ref().expect("open");
        let Some(run) = h.run.as_ref() else { return div().into_any_element() };
        let host = self.host_name(h.host.as_deref().unwrap_or_default());
        let s = &h.source;
        let done = h.stage == Stage::Done;
        let online = self.fleet.hosts.links.online(h.host.as_deref().unwrap_or_default());
        let rows = run
            .steps
            .iter()
            .enumerate()
            .map(|(i, step)| {
                let failed = run.failed.as_ref().filter(|_| i == run.at);
                let (now, todo) = (i == run.at && !done && failed.is_none(), i > run.at && !done);
                let sending = run.sent.filter(|_| now && *step == Step::Conversation);
                let detail = match (failed, step) {
                    (Some(_), _) if !online => Some(format!("{host} went offline")),
                    (Some(e), _) => Some(e.clone()),
                    (_, Step::Pack) => Some(format!("{} · {}", plural(s.files, "file", "files"), plural(s.commits, "commit", "commits"))),
                    (_, Step::Send) => run.pack.as_ref().map(|p| megabytes(p.bundle.len())),
                    (_, Step::Conversation) if sending.is_some() => sending.map(|(done, of)| sent(done, of)),
                    (_, Step::Conversation) if s.messages > 0 => Some(plural(s.messages, "message", "messages")),
                    _ => None,
                };
                let bar = sending.map(|(done, of)| {
                    let fill = div().h_full().w(relative(done as f32 / of.max(1) as f32)).rounded(px(2.)).bg(TEXT_2);
                    div().mt(px(4.)).w_full().h(px(4.)).rounded(px(2.)).bg(FILL_4).overflow_hidden().child(fill)
                });
                let mark = if failed.is_some() {
                    icon("x", 13., FAILED).into_any_element()
                } else if now {
                    spinner(SharedString::from(format!("handoff-step-{i}")), 12., TEXT_2).into_any_element()
                } else if todo {
                    div().size(px(12.)).rounded(px(6.)).border(px(1.5)).border_color(TEXT_5).into_any_element()
                } else {
                    icon("check", 13., SUCCESS).into_any_element()
                };
                div()
                    .px(px(12.))
                    .py(px(7.))
                    .flex()
                    .items_start()
                    .gap(px(11.))
                    .text_size(px(13.))
                    .text_color(if todo { TEXT_4 } else { TEXT })
                    .child(div().w(px(16.)).h(px(18.)).flex().flex_none().items_center().justify_center().child(mark))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .when(now, |d| d.font_weight(FontWeight::MEDIUM))
                                    .when(failed.is_some(), |d| d.text_color(FAILED_TEXT))
                                    .child(step.label(&host, &s.agent_name)),
                            )
                            .children(detail.map(|t| div().text_size(px(12.)).text_color(if todo { TEXT_5 } else { TEXT_3 }).child(t)))
                            .children(bar),
                    )
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        boxed(vec![div().py(px(4.)).flex().flex_col().children(rows).into_any_element()])
    }

    fn progress_sheet(&mut self, cx: &mut Context<Self>) -> Div {
        let h = self.handoff.open.as_ref().expect("open");
        let host = self.host_name(h.host.as_deref().unwrap_or_default());
        let cancellable = h.run.as_ref().is_some_and(|r| r.cancellable());
        let hide = |d: &mut Desktop, _: &ClickEvent, window: &mut Window, cx: &mut Context<Desktop>| d.close_overlay(window, cx);
        let safe = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .child(icon("laptop", 13., TEXT_3))
            .child("Keep this Mac awake until it’s done.");
        let mut buttons = vec![];
        if cancellable {
            buttons.push(
                button("handoff-cancel", Variant::Ghost, "Cancel handoff").on_click(cx.listener(|d, _: &ClickEvent, window, cx| d.cancel_handoff(window, cx))),
            );
        }
        buttons.push(button("handoff-hide", Variant::Secondary, "Hide").on_click(cx.listener(hide)));
        sheet([
            header(format!("Handing off to {host}"), x_button("handoff-close", "x").on_click(cx.listener(hide))),
            self.steps_box(),
            safe.into_any_element(),
            foot(buttons),
        ])
    }

    fn failed_sheet(&mut self, cx: &mut Context<Self>) -> Div {
        let h = self.handoff.open.as_ref().expect("open");
        let id = h.host.clone().unwrap_or_default();
        let host = self.host_name(&id);
        let agent = h.source.agent_name.clone();
        let step = h.run.as_ref().and_then(|r| r.step());
        let mac = if h.run.as_ref().is_some_and(Run::stopped_turn) { "This session is still here; its turn stopped." } else { "Nothing changed on this Mac." };
        let online = self.fleet.hosts.links.online(&id);
        let (title, body) = if !online {
            (format!("{host} went offline"), format!("{mac} It retries when {host} is back."))
        } else if step == Some(Step::Check) {
            (format!("The conversation didn’t open on {host}"), format!("{agent} opened empty there. {mac}"))
        } else {
            (format!("The handoff to {host} stopped"), mac.to_string())
        };
        sheet([
            header(title, self.close_button(cx)),
            self.steps_box(),
            div().text_size(px(13.5)).line_height(px(20.)).text_color(TEXT_2).child(body).into_any_element(),
            foot(vec![
                button("handoff-cancel", Variant::Ghost, "Close").on_click(cx.listener(|d, _: &ClickEvent, window, cx| d.close_overlay(window, cx))),
                button("handoff-go", Variant::Primary, if online { "Retry" } else { "Retry now" })
                    .on_click(cx.listener(|d, _: &ClickEvent, _, cx| d.retry_handoff(cx))),
            ]),
        ])
    }

    fn done_sheet(&mut self, cx: &mut Context<Self>) -> Div {
        let h = self.handoff.open.as_ref().expect("open");
        let host = self.host_name(h.host.as_deref().unwrap_or_default());
        let agent = h.source.agent_name.clone();
        let (title, lede) = if h.source.conversation() {
            (format!("{agent} made it to {host}!"), "It picks up where you left off. You can close this Mac.".to_string())
        } else {
            (format!("A fresh {agent} is up on {host}"), "It starts from your summary and all the code. This session stays open here.".to_string())
        };
        let cheers = h.cheers;
        let still = cx.reduce_motion() || self.capturing;
        let cheer = div()
            .relative()
            .pt(px(4.))
            .pb(px(2.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.))
            .text_center()
            .child(super::cheer::mascot(cheers, still).on_click(cx.listener(|d, _: &ClickEvent, _, cx| {
                if let Some(h) = d.handoff.open.as_mut() {
                    h.cheers += 1;
                }
                cx.notify();
            })))
            .child(div().mt(px(4.)).text_size(px(20.)).line_height(px(26.)).font_weight(FontWeight::BOLD).child(title))
            .child(div().max_w(px(380.)).text_size(px(13.5)).line_height(px(20.)).text_color(TEXT_2).child(lede));
        sheet([
            div().mb(px(-14.)).flex().justify_end().child(self.close_button(cx).mr(px(-4.))).into_any_element(),
            cheer.into_any_element(),
            foot(vec![
                button("handoff-cancel", Variant::Ghost, "Done").on_click(cx.listener(|d, _: &ClickEvent, window, cx| d.close_overlay(window, cx))),
                button("handoff-go", Variant::Primary, format!("Open on {host}"))
                    .on_click(cx.listener(|d, _: &ClickEvent, window, cx| d.open_handed_off(window, cx))),
            ]),
        ])
    }
}

fn back_to_pick(d: &mut Desktop, _: &ClickEvent, _: &mut Window, cx: &mut Context<Desktop>) {
    if let Some(h) = d.handoff.open.as_mut() {
        h.stage = Stage::Pick;
    }
    cx.notify();
}

fn sent(done: usize, of: usize) -> String {
    format!("{} of {}", megabytes(done), megabytes(of))
}

fn megabytes(bytes: usize) -> String {
    let mb = bytes as f64 / 1_000_000.;
    if mb < 0.1 { format!("{} KB", (bytes / 1000).max(1)) } else { format!("{mb:.1} MB") }
}

#[cfg(test)]
mod tests {
    use super::{megabytes, sent};

    #[test]
    fn a_bundle_size_reads_in_megabytes_down_to_a_tenth_then_kilobytes() {
        assert_eq!(megabytes(1_420_000), "1.4 MB");
        assert_eq!(megabytes(42_000), "42 KB");
        assert_eq!(megabytes(10), "1 KB");
    }

    #[test]
    fn the_conversation_reads_how_much_of_it_is_on_the_host() {
        assert_eq!(sent(4_194_304, 118_300_000), "4.2 MB of 118.3 MB");
    }
}
