use crate::desktop::Desktop;
use agents::Host;
use gpui_kit::*;
use theme::*;

const TAILSCALE: &str = "https://tailscale.com/download/mac";

#[derive(Debug, PartialEq)]
enum HostLine {
    NeedsTailscale,
    KeepingAwake,
}

fn host_lines(host: Option<&Host>) -> Vec<HostLine> {
    let Some(h) = host else { return vec![] };
    [(!h.tailnet).then_some(HostLine::NeedsTailscale), h.keeping_awake.then_some(HostLine::KeepingAwake)].into_iter().flatten().collect()
}

impl Desktop {
    pub(super) fn host_lines(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        host_lines(self.agents.host.as_ref())
            .into_iter()
            .map(|line| {
                let row = div().h(px(24.)).px(px(8.)).flex().flex_none().items_center().text_size(px(12.)).text_color(TEXT_2);
                match line {
                    HostLine::NeedsTailscale => row
                        .id("host-tailscale")
                        .cursor_pointer()
                        .hover(|s| s.text_color(TEXT))
                        .child("Phone access needs Tailscale ↗")
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.open_url(TAILSCALE)))
                        .into_any_element(),
                    HostLine::KeepingAwake => row.child("Keeping Mac awake").into_any_element(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{HostLine, host_lines};
    use agents::Host;

    #[test]
    fn tailscale_line_comes_before_keeping_awake() {
        let host = Host { tailnet: false, keeping_awake: true, ..Default::default() };
        assert_eq!(host_lines(Some(&host)), [HostLine::NeedsTailscale, HostLine::KeepingAwake]);
        assert_eq!(host_lines(Some(&Host { tailnet: true, keeping_awake: true, ..Default::default() })), [HostLine::KeepingAwake]);
    }

    #[test]
    fn no_host_no_lines() {
        assert_eq!(host_lines(None), []);
        assert_eq!(host_lines(Some(&Host { tailnet: true, keeping_awake: false, ..Default::default() })), []);
    }
}
