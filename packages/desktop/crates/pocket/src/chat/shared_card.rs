use super::tool_detail::TEXT_SIZE;
use chat::shared::{self, Shown};
use chat::{ToolCall, ToolStatus};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::PathBuf;
use theme::*;

const IMAGE_W: f32 = 360.;
const IMAGE_H: f32 = 240.;

/// How a file an agent working in `cwd` shared shows, and, once kept, the key its copy opens by: on the agent's host
/// when it runs on one.
pub(crate) fn opening(call: &ToolCall, cwd: &str) -> (Shown, Option<String>) {
    let host = hosts::split(cwd).0;
    let target = shared::kept(call).map(|kept| host.map_or_else(|| kept.to_string(), |h| hosts::place(h, kept)));
    (shared::shown(shared::path(call), host.is_some()), target)
}

/// A file the agent shared: an image inline over a line naming it, any other file as the line alone. Once the share
/// went through, `target` is its copy and a click opens it, by `on_click`.
pub(crate) fn card(id: &str, call: &ToolCall, how: Shown, target: Option<&str>, width: f32, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Div {
    let path = shared::path(call);
    let image = target.filter(|_| how == Shown::Image).map(|kept| {
        let image = img(PathBuf::from(kept)).max_w(px(IMAGE_W.min(width))).max_h(px(IMAGE_H)).rounded(px(6.)).object_fit(ObjectFit::Contain);
        div().p(px(6.)).pb(px(0.)).child(image)
    });
    let failed = call.status == ToolStatus::Failed;
    let line = div()
        .h(px(34.))
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(8.))
        .child(file_icon(path, false, false, 14.))
        .child(div().flex_none().font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child(shared::name(path).to_string()))
        .child(div().flex_1().min_w_0().truncate().text_color(if failed { TEXT_3 } else { TEXT }).child(shared::detail(call)))
        .children((how == Shown::App && target.is_some()).then(|| icon("external", 12., TEXT_3)));
    let card = div()
        .id(SharedString::from(format!("{id}-shared")))
        .max_w(px(width))
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(10.))
        .border_1()
        .border_color(hairline(0.08))
        .bg(ink(0.045))
        .text_size(px(TEXT_SIZE))
        .children(image)
        .child(line)
        .when(target.is_some(), |d| d.cursor_pointer().hover(|s| s.bg(ink(0.07))).on_click(on_click));
    div().py(px(4.)).flex().child(card)
}

#[cfg(test)]
mod tests {
    use super::opening;
    use chat::shared::{Shown, TOOL};
    use chat::{ToolCall, ToolKind, ToolStatus};

    fn share(path: &str, status: ToolStatus, output: Option<&str>) -> ToolCall {
        let kind = ToolKind::Shared { path: path.into(), caption: String::new() };
        ToolCall { id: "t".into(), name: TOOL.into(), kind, status, output: output.map(Into::into), duration_ms: None, at: 0 }
    }

    #[test]
    fn a_file_shared_on_this_mac_opens_its_copy_by_its_path() {
        let call = share("out/chart.png", ToolStatus::Ok, Some("/h/attachments/0a1b-chart.png"));
        assert_eq!(opening(&call, "/r"), (Shown::Image, Some("/h/attachments/0a1b-chart.png".into())));
    }

    #[test]
    fn a_file_shared_on_a_host_opens_its_copy_there_in_the_preview() {
        let call = share("out/report.pdf", ToolStatus::Ok, Some("/srv/attachments/0a1b-report.pdf"));
        assert_eq!(opening(&call, "//box/srv/app"), (Shown::Preview, Some("//box/srv/attachments/0a1b-report.pdf".into())));
    }

    #[test]
    fn a_share_still_on_its_way_or_failed_opens_nothing() {
        assert_eq!(opening(&share("out/chart.png", ToolStatus::Running, None), "/r").1, None);
        assert_eq!(opening(&share("out/chart.png", ToolStatus::Failed, Some("There's no file at out/chart.png")), "/r").1, None);
    }
}
