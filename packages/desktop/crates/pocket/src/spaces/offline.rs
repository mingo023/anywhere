//! The page a host's space shows while it can't be reached: the mascot, what the link is doing, Reconnect and Edit host.

use super::{Reach, Space, offline_note, offline_title, shows_session, space_of};
use crate::desktop::Desktop;
use crate::mascot::{BEAT, DOWN, Look, STILL, mascot, waiting};
use agents::locals::is_local;
use gpui_kit::*;
use std::time::Instant;
use theme::*;
use ui::Variant;

const MASCOT: f32 = 112.;

impl Desktop {
    /// The space on screen while it's locked: its machine can't be reached, past any sign-in's grace.
    pub(crate) fn locked_space(&self) -> Option<Space> {
        let space = self.space();
        self.spaces.locked(&space, self.space_reach(&space), Instant::now()).then_some(space)
    }

    /// Notes the reach of the space on screen, and draws again when a sign-in's grace runs out.
    pub(crate) fn track_reach(&mut self, cx: &mut Context<Self>) {
        let space = self.space();
        if let Some(grace) = self.spaces.track(&space, self.space_reach(&space), Instant::now()) {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(grace).await;
                this.update(cx, |_, cx| cx.notify()).ok();
            })
            .detach();
        }
    }

    /// Whether the session in `tree` may show: only the space on screen's machine's, and not while it's locked.
    pub(crate) fn shows_session(&self, tree: &str) -> bool {
        let machine = if is_local(tree) { self.agents.local(tree).map(|l| space_of(&l.project)) } else { Some(space_of(tree)) };
        shows_session(machine.as_ref(), &self.space(), self.locked_space().is_some())
    }

    pub(crate) fn offline_page(&self, space: &Space, cx: &mut Context<Self>) -> Div {
        let Some(id) = space.host().map(str::to_string) else { return div() };
        let (reach, name) = (self.space_reach(space), self.space_name(space));
        let connecting = reach == Reach::Connecting;
        let added = self.added_name(&id);
        let note = offline_note(self.fleet.hosts.links.get(&id), &added);
        let command = format!("ssh {added}");
        let code = HighlightStyle { color: Some(TEXT_2.into()), background_color: Some(FILL_2.into()), ..Default::default() };
        let code: Vec<_> = note.find(&command).map(|at| (at..at + command.len(), code)).into_iter().collect();
        let face = match connecting {
            true if !cx.reduce_motion() => div()
                .with_animation("offline-mascot", Animation::new(BEAT).repeat_synced().with_max_fps(10.), |d, t| d.child(mascot(waiting(t), MASCOT)))
                .into_any_element(),
            true => mascot(STILL, MASCOT).into_any_element(),
            false => mascot(Look { eyes: DOWN, lit: false, ..STILL }, MASCOT).into_any_element(),
        };
        let reconnect = if connecting {
            ui::button(
                "offline-reconnect",
                Variant::Primary,
                None,
                div().flex().items_center().gap(px(7.)).child(spinner("offline-spin", 12., ON_PRIMARY)).child("Connecting\u{2026}"),
            )
            .opacity(0.6)
            .cursor_default()
        } else {
            ui::button("offline-reconnect", Variant::Primary, None, "Reconnect").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.retry_host(&id);
                cx.notify();
            }))
        };
        let edit = ui::button("offline-edit-host", Variant::Ghost, None, "Edit host\u{2026}")
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.show_hosts(window, cx)));
        let body = div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .px(px(24.))
            .pb(px(60.))
            .child(face)
            .child(
                div().mt(px(6.)).text_size(px(17.)).line_height(px(22.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(offline_title(reach, &name)),
            )
            .child(
                div()
                    .max_w(px(360.))
                    .text_center()
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .text_color(TEXT_3)
                    .child(StyledText::new(note).with_highlights(code)),
            )
            .child(div().mt(px(14.)).flex().gap(px(6.)).child(reconnect).child(edit));
        div().flex_1().flex().flex_col().child(self.page_bar(vec![name], Vec::new(), div(), cx)).child(body)
    }
}
