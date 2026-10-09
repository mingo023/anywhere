use super::catalog::{Look, Setting};
use super::host::{port_text, tailnet_line};
use super::{action, field_box, row};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use crate::modals::may_open;
use crate::util::{ago_long, now_ms};
use daemon::Device;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

const PHONE_ACCESSES: [&str; 3] = ["ask", "edits", "auto"];

pub(super) const ROWS: &[Setting] = &[
    Setting::custom("pair-phone", "Devices", "Pair a new phone", |d, s, cx| d.pair_row(s, cx)).hint("Shows a QR code that expires after 5 minutes"),
    Setting::custom("phones", "Devices", "Paired phones", |d, _, cx| d.devices_rows(cx)),
    Setting::choice("phone-max-access", "Access", "Most the phone can approve", &["Ask", "Edits", "Auto"], Look::Segmented, |s| {
        PHONE_ACCESSES.iter().position(|a| *a == s.phone.max_access).unwrap_or(0)
    }, |s, i| s.phone.max_access = PHONE_ACCESSES[i].into())
    .hint("The phone never allows more than this, whatever the session's own mode")
    .effect(|d, _| d.outbox.set_phone_access(&d.store.phone.max_access)),
    Setting::choice("reachable-from", "Network", "Reachable from", &["This Mac", "Tailnet"], Look::Segmented, |s| s.phone.tailnet as usize, |s, i| s.phone.tailnet = i == 1)
        .hint("Tailnet lets the phone connect from anywhere over Tailscale")
        .effect(|d, cx| d.set_reach("listen", if d.store.phone.tailnet { "auto" } else { "loopback" }.into(), cx)),
    Setting::custom("tailscale", "Network", "Tailscale", |d, s, cx| d.tailscale_row(s, cx)).under(|s| s.phone.tailnet),
    Setting::custom("phone-port", "Network", "Port", |d, s, _| d.port_row(s))
        .note(|_| Some(format!("Change it only if another app already uses {}", channel::phone_port())))
        .advanced()
        .resets(|s| if s.phone.port == 0 { channel::phone_port() } else { s.phone.port }.to_string(), |s, defaults| s.phone.port = defaults.phone.port)
        .effect(|d, cx| d.set_reach("port", port_text(d.store.phone.port), cx)),
];

/// "Last seen 3d ago · iOS 26.1".
fn device_line(d: &Device, now: i64) -> String {
    let seen = match ago_long(d.last_seen_at, now) {
        a if a.is_empty() => "Not connected yet".to_string(),
        a if a == "now" => "Seen just now".to_string(),
        a => format!("Last seen {a}"),
    };
    if d.platform.is_empty() { seen } else { format!("{seen} · {}", d.platform) }
}

impl Desktop {
    fn pair_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let pair = may_open(Overlay::PairPhone, &self.agents).then(|| {
            ui::button("settings-pair-phone", ui::Variant::Primary, Some("plus"), "Pair a phone")
                .h(px(28.))
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::PairPhone, window, cx)))
        });
        row(setting.label, setting.hint, div().children(pair))
    }

    /// pocketd listens on the tailnet when Tailscale runs; without it a phone can't reach this Mac.
    fn tailscale_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let value = match self.agents.host.as_ref().map(|h| h.tailnet) {
            Some(true) => {
                let listen = self.settings.host.status.as_ref().map(|s| s.listen.as_slice()).unwrap_or_default();
                div().flex().items_center().gap(px(6.)).text_size(px(13.)).text_color(TEXT_2).child(ui::dot(7., SUCCESS)).child(tailnet_line(listen)).into_any_element()
            }
            Some(false) => action("settings-get-tailscale", "Set up Tailscale")
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.open_url(crate::sidebar::host::TAILSCALE)))
                .into_any_element(),
            None => div().text_size(px(13.)).text_color(TEXT_3).child("Checking\u{2026}").into_any_element(),
        };
        row(setting.label, setting.hint, value)
    }

    fn port_row(&mut self, setting: &'static Setting) -> Div {
        row(setting.label, setting.hint, field_box().w(px(72.)).child(Input::new(&self.settings.host.port).appearance(false).p_0().font_family(MONO).text_size(px(12.))))
    }

    fn devices_rows(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(devices) = self.settings.host.devices.clone() else {
            return div().px(px(14.)).py(px(18.)).text_size(px(12.5)).text_color(TEXT_3).child("Paired phones show here once the background service answers.");
        };
        if devices.is_empty() {
            return div()
                .px(px(14.))
                .py(px(18.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.))
                .child(div().size(px(36.)).mb(px(4.)).flex().items_center().justify_center().rounded(px(9.)).bg(FILL_2).child(icon("phone", 16., TEXT_2)))
                .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child("No phones paired yet"))
                .child(div().max_w(px(380.)).text_center().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child("Pair the Anywhere iPhone app to approve requests, read output and start sessions when you're away from the Mac."));
        }
        let now = now_ms();
        let renaming = self.settings.host.renaming.as_ref().map(|r| (r.id.clone(), r.field.clone()));
        let rows = devices.into_iter().enumerate().map(|(i, d)| {
            let editing = renaming.as_ref().filter(|(id, _)| *id == d.id).map(|(_, field)| field.clone());
            let tile = div().size(px(28.)).flex().flex_none().items_center().justify_center().rounded(px(7.)).bg(FILL_2).child(icon("phone", 14., TEXT_2));
            let text = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(match editing.clone() {
                    Some(field) => field_box().w(px(240.)).child(div().flex_1().min_w_0().child(Input::new(&field).appearance(false).p_0().text_size(px(13.)))).into_any_element(),
                    None => div().truncate().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(d.name.clone()).into_any_element(),
                })
                .child(div().truncate().text_size(px(12.)).text_color(TEXT_3).child(device_line(&d, now)));
            let (id, name) = (d.id.clone(), d.name.clone());
            let rename = (!d.legacy && editing.is_none()).then(|| {
                ui::icon_button_sized(("settings-rename", i), "pencil", 26., TEXT_3)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_phone_rename(id.clone(), &name, window, cx)))
            });
            let revoke = (!d.legacy).then(|| {
                ui::button(("settings-revoke", i), ui::Variant::Danger, None, "Revoke").h(px(26.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    (this.confirm, this.overlay) = (Some(Confirm::RevokeDevice { id: d.id.clone(), name: d.name.clone() }), Some(Overlay::Confirm));
                    cx.notify();
                }))
            });
            div().px(px(14.)).py(px(9.)).flex().items_center().gap(px(12.)).when(i > 0, |r| r.border_t(px(0.5)).border_color(SEPARATOR)).child(tile).child(text).children(rename).children(revoke)
        });
        div().flex().flex_col().children(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::device_line;
    use daemon::Device;

    #[test]
    fn a_phone_says_when_it_was_last_seen_and_what_it_runs() {
        let hour = 3_600_000;
        let seen = Device { last_seen_at: 1, platform: "iOS 26.1".into(), ..Device::default() };
        assert_eq!(device_line(&seen, 1 + 3 * hour), "Last seen 3h ago · iOS 26.1");
        assert_eq!(device_line(&Device::default(), hour), "Not connected yet");
    }
}
