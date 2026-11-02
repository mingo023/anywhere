//! Files an agent shares with the user through Anywhere's `share_file` tool, and how the chat shows them.

use crate::{ToolCall, ToolKind, ToolStatus};

/// What Claude names `share_file`: the tool of the `anywhere` server in pocketd's `anywhere` plugin.
pub const TOOL: &str = "mcp__plugin_anywhere_anywhere__share_file";

/// How a shared file shows in the chat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    /// Inline, opening in the preview.
    Image,
    /// As a card that opens in the preview.
    Preview,
    /// As a card that opens in the Mac's app for it.
    App,
}

const IMAGE: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg"];
/// Files the preview can't show, which the Mac opens in their own app.
const APP: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "key", "pages", "numbers", "rtf", "heic", "mp4", "mov", "m4v", "webm", "mp3",
    "wav", "m4a", "aac", "zip",
];

/// How the file at `path` shows. A file on a host goes to the preview: `img` reads only this Mac's paths, and no app
/// here opens one there.
pub fn shown(path: &str, remote: bool) -> Shown {
    let ext = name(path).rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        _ if remote => Shown::Preview,
        e if IMAGE.contains(&e) => Shown::Image,
        e if APP.contains(&e) => Shown::App,
        _ => Shown::Preview,
    }
}

/// The file the agent named to share.
pub fn path(call: &ToolCall) -> &str {
    match &call.kind {
        ToolKind::Shared { path, .. } => path,
        _ => "",
    }
}

/// Where Anywhere keeps the copy the user sees: what the tool answered, once the share went through.
pub fn kept(call: &ToolCall) -> Option<&str> {
    let out = call.output.as_deref()?.trim();
    (call.status == ToolStatus::Ok && !out.is_empty()).then_some(out)
}

/// What the card reads after the file's name: its caption, that it's on its way, or why it didn't go through.
pub fn detail(call: &ToolCall) -> String {
    let caption = match &call.kind {
        ToolKind::Shared { caption, .. } => caption.as_str(),
        _ => "",
    };
    match call.status {
        ToolStatus::Running => "Sharing…".into(),
        ToolStatus::Failed => call.output.as_deref().map(str::trim).filter(|o| !o.is_empty()).unwrap_or("Couldn't share it").into(),
        ToolStatus::Ok => caption.split_whitespace().collect::<Vec<_>>().join(" "),
    }
}

/// The file's own name, as the card shows it.
pub fn name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolKind;

    fn share(status: ToolStatus, caption: &str, output: Option<&str>) -> ToolCall {
        let kind = ToolKind::Shared { path: "out/chart.png".into(), caption: caption.into() };
        ToolCall { id: "t".into(), name: TOOL.into(), kind, status, output: output.map(Into::into), duration_ms: None, at: 0 }
    }

    #[test]
    fn images_show_inline_documents_open_in_their_app_and_the_rest_in_the_preview() {
        assert_eq!(shown("/r/out/Chart.PNG", false), Shown::Image);
        assert_eq!(shown("/r/logo.svg", false), Shown::Image);
        assert_eq!(shown("/r/report.pdf", false), Shown::App);
        assert_eq!(shown("/r/deck.KEY", false), Shown::App);
        assert_eq!(shown("/r/notes.md", false), Shown::Preview);
        assert_eq!(shown("/r/build.log", false), Shown::Preview);
        assert_eq!(shown("/r.d/Makefile", false), Shown::Preview, "a dot in a folder's name isn't an extension");
    }

    #[test]
    fn a_file_on_a_host_always_opens_in_the_preview() {
        assert_eq!(shown("/srv/chart.png", true), Shown::Preview);
        assert_eq!(shown("/srv/report.pdf", true), Shown::Preview);
    }

    #[test]
    fn only_a_share_that_went_through_has_a_kept_copy() {
        assert_eq!(kept(&share(ToolStatus::Ok, "", Some("/h/attachments/0a1b-chart.png\n"))), Some("/h/attachments/0a1b-chart.png"));
        assert_eq!(kept(&share(ToolStatus::Running, "", None)), None);
        assert_eq!(kept(&share(ToolStatus::Failed, "", Some("There's no file at out/chart.png"))), None);
        assert_eq!(kept(&share(ToolStatus::Ok, "", Some(" "))), None);
    }

    #[test]
    fn a_card_reads_the_caption_that_it_is_on_its_way_or_why_it_failed() {
        assert_eq!(detail(&share(ToolStatus::Ok, "Q3  revenue\nby month", Some("/k"))), "Q3 revenue by month");
        assert_eq!(detail(&share(ToolStatus::Running, "Q3", None)), "Sharing…");
        assert_eq!(detail(&share(ToolStatus::Failed, "Q3", Some("Files over 20 MB can't be shared\n"))), "Files over 20 MB can't be shared");
        assert_eq!(detail(&share(ToolStatus::Failed, "Q3", None)), "Couldn't share it");
    }

    #[test]
    fn a_card_names_the_file_by_its_own_name() {
        assert_eq!(name("/r/out/chart.png"), "chart.png");
        assert_eq!(name(r"C:\r\chart.png"), "chart.png");
        assert_eq!(name("chart.png"), "chart.png");
    }
}
