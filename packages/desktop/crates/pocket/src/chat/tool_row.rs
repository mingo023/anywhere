use super::diff::{self, Highlights};
use super::shimmer::shimmer;
use super::tool_detail::{self, Details};
use chat::{Diff, ToolCall, ToolKind, ToolStatus};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// A chat's tool views: which rows the user opened and their diffs' colours.
#[derive(Default)]
pub(crate) struct Tools {
    pub(crate) details: Details,
    pub(crate) highlights: Highlights,
}

impl Tools {
    /// Opens or closes a row; a diff opening for the first time gets coloured in the background.
    pub(crate) fn toggle(&mut self, key: &str, default: bool, diff: Option<(String, String, Diff)>, cx: &mut Context<Self>) {
        if self.details.toggle(key, default)
            && let Some((id, path, diff)) = diff
            && self.highlights.start(&id)
        {
            let task = cx.background_executor().spawn(async move { diff::highlight(&path, &diff::lines(&diff)) });
            cx.spawn(async move |this, cx| {
                let spans = task.await;
                this.update(cx, |t, cx| {
                    t.highlights.finish(id, spans);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }
}

/// An edit's diff, to colour once its row opens: the call's id, the file's path and the diff.
pub(crate) fn colour(call: &ToolCall) -> Option<(String, String, Diff)> {
    edit(call).map(|(path, d)| (call.id.clone(), path.clone(), d.clone()))
}

fn edit(call: &ToolCall) -> Option<(&String, &Diff)> {
    match &call.kind {
        ToolKind::Edit { path, diff: Some(d) } | ToolKind::Write { path, diff: Some(d) } if !d.lines.is_empty() => Some((path, d)),
        _ => None,
    }
}

/// What a tool row reads after its verb.
#[derive(Debug, PartialEq)]
pub(crate) enum Arg {
    /// A file, as a chip with its language's badge.
    File(String),
    /// What was looked for or fetched, as plain text.
    Text(String),
    /// The call itself in the code font: a command's first line, or the tool's name.
    Raw(String),
}

/// A call as one line: "Read [a.rs]", "Find fn group", or the command it ran.
pub(crate) fn line(call: &ToolCall) -> (&'static str, Arg) {
    let one = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    match &call.kind {
        ToolKind::Read { path } => ("Read", Arg::File(path.clone())),
        ToolKind::Edit { path, .. } => ("Edit", Arg::File(path.clone())),
        ToolKind::Write { path, .. } => ("Create", Arg::File(path.clone())),
        ToolKind::Search { query } => ("Find", Arg::Text(one(query))),
        ToolKind::Fetch { url } => ("Fetch", Arg::Text(url.clone())),
        // pocketd names the command, or the task, a call on a background task acts on.
        ToolKind::Shell { command } => (verb(&call.name), Arg::Raw(command.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Shell").to_string())),
        ToolKind::Other { description, .. } if !description.is_empty() => (verb(&call.name), Arg::Text(one(description))),
        ToolKind::Other { input, .. } if call.name == "Skill" => ("Skill", Arg::Text(skill(input).map_or_else(String::new, |s| format!("/{s}")))),
        ToolKind::Other { .. } => ("", Arg::Raw(mcp(&call.name).unwrap_or_else(|| call.name.clone()))),
        ToolKind::Agent(a) => ("", Arg::Raw(one(&a.description))),
        ToolKind::Question(_) => ("", Arg::Raw(call.name.clone())),
    }
}

fn verb(name: &str) -> &'static str {
    match name {
        "TaskStop" | "KillShell" => "Stop",
        "TaskOutput" | "BashOutput" => "Read output of",
        "Monitor" => "Watch",
        _ => "",
    }
}

fn skill(input: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(input).ok()?.get("skill")?.as_str().map(str::to_string)
}

/// `mcp__linear__create_issue` as "linear · create_issue".
fn mcp(name: &str) -> Option<String> {
    let (server, tool) = name.strip_prefix("mcp__")?.split_once("__")?;
    Some(format!("{server} · {tool}"))
}

/// A file's language badge: two letters on its colour, blank for one we don't mark.
pub(crate) fn badge(path: &str) -> (&'static str, u32) {
    match path.rsplit('.').next().unwrap_or_default().to_ascii_lowercase().as_str() {
        "rs" => ("RS", 0xC2410C),
        "ts" | "tsx" => ("TS", 0x3178C6),
        "md" => ("MD", 0x3E8EF7),
        "toml" => ("TM", 0x9C4221),
        "json" => ("JS", 0x8B8B94),
        "swift" => ("SW", 0xF05138),
        _ => ("", 0x8B8B94),
    }
}

fn file_chip(path: &str, failed: bool) -> Div {
    let name = path.rsplit(['/', '\\']).find(|c| !c.is_empty()).unwrap_or(path).to_string();
    let (ext, colour) = badge(path);
    div()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(5.))
        .my(px(-2.))
        .pl(px(4.))
        .pr(px(5.))
        .py(px(2.))
        .rounded(px(5.))
        .bg(FILL_4)
        .font_family(code_font())
        .text_size(px(12.5))
        .line_height(px(18.))
        .text_color(if failed { FAILED } else { TEXT_BODY })
        .child(
            div()
                .size(px(13.))
                .flex_none()
                .rounded(px(3.))
                .bg(rgb(colour))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(6.5))
                .font_weight(FontWeight::BOLD)
                .text_color(WHITE)
                .child(ext),
        )
        .child(div().min_w_0().truncate().child(name))
}

/// One tool call: dim and shut when it went well, red with its error a click away when it failed.
/// An edit's chip opens its diff. `settled` says the work around it is over, so a call still running runs in the background.
pub(crate) fn row(call: &ToolCall, settled: bool, tools: &Entity<Tools>, cx: &App) -> Div {
    let state = tools.read(cx);
    let background = settled && call.status == ToolStatus::Running;
    let (running, failed) = (call.status == ToolStatus::Running && !background, call.status == ToolStatus::Failed);
    let (verb, arg) = line(call);
    let toggle = |default: bool, diff: Option<(String, String, Diff)>| {
        let (tools, key) = (tools.clone(), call.id.clone());
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| tools.update(cx, |t, cx| t.toggle(&key, default, diff.clone(), cx))
    };
    let verb = (!verb.is_empty()).then(|| {
        let base = if failed { FAILED } else { TEXT_3 };
        let v = div().flex_none().text_color(base);
        if running { v.child(shimmer(SharedString::from(format!("verb-{}", call.id)), verb, base)) } else { v.child(verb) }
    });
    let arg = match arg {
        Arg::File(path) => {
            let chip = file_chip(&path, failed);
            match colour(call) {
                Some(diff) => div()
                    .id(SharedString::from(format!("edit-{}", call.id)))
                    .min_w_0()
                    .flex()
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.8))
                    .on_click(toggle(false, Some(diff)))
                    .child(chip)
                    .into_any_element(),
                None => chip.into_any_element(),
            }
        }
        Arg::Text(text) => div().min_w_0().pl(px(2.)).truncate().text_color(if failed { FAILED } else { TEXT_BODY }).child(text).into_any_element(),
        Arg::Raw(label) => {
            let base = if failed { FAILED } else { TEXT_2 };
            let raw = div().min_w_0().overflow_hidden().whitespace_nowrap().font_family(code_font()).text_size(px(13.)).text_color(base);
            if running { raw.child(shimmer(SharedString::from(format!("raw-{}", call.id)), label, base)).into_any_element() } else { raw.truncate().child(label).into_any_element() }
        }
    };
    let error = call.output.as_deref().filter(|o| failed && !o.trim().is_empty());
    let open = state.details.open(&call.id, false);
    let caret = error.map(|_| {
        div()
            .id(SharedString::from(format!("err-{}", call.id)))
            .size(px(22.))
            .m(px(-4.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .opacity(0.7)
            .cursor_pointer()
            .hover(|s| s.opacity(1.).bg(FILL_3))
            .on_click(toggle(false, None))
            .child(icon(if open { "chevron-down" } else { "chevron-right" }, 14., FAILED))
    });
    let head = div()
        .py(px(4.))
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(14.))
        .line_height(px(20.))
        .child(div().min_w_0().flex_1().flex().items_center().gap(px(6.)).children(verb).child(arg))
        .when(background, |d| d.child(div().flex_none().text_size(px(12.)).text_color(TEXT_4).child("background")))
        .when(failed, |d| d.child(icon("x", 14., FAILED)))
        .children(caret);
    let body = match (open, error, edit(call)) {
        (true, Some(text), _) => Some(tool_detail::pre(&tool_detail::output(text), FAILED, 0.88).into_any_element()),
        (true, None, Some((_, d))) => Some(div().py(px(4.)).child(diff::body(&diff::lines(d), state.highlights.get(&call.id).as_deref().map(Vec::as_slice))).into_any_element()),
        _ => None,
    };
    div().w_full().min_w_0().flex().flex_col().child(head).children(body)
}

#[cfg(test)]
mod tests {
    use super::{Arg, badge, line};
    use chat::{ToolCall, ToolKind, ToolStatus};

    fn call(name: &str, kind: ToolKind) -> ToolCall {
        ToolCall { id: "t".into(), name: name.into(), kind, status: ToolStatus::Ok, output: None, duration_ms: None, at: 0 }
    }

    fn other(description: &str) -> ToolKind {
        ToolKind::Other { input: "{}".into(), description: description.into() }
    }

    #[test]
    fn each_tool_reads_as_a_verb_and_what_it_acted_on() {
        let cases = [
            (call("Read", ToolKind::Read { path: "src/a.rs".into() }), "Read", Arg::File("src/a.rs".into())),
            (call("Edit", ToolKind::Edit { path: "b.ts".into(), diff: None }), "Edit", Arg::File("b.ts".into())),
            (call("Write", ToolKind::Write { path: "c.md".into(), diff: None }), "Create", Arg::File("c.md".into())),
            (call("Grep", ToolKind::Search { query: "fn  group\n".into() }), "Find", Arg::Text("fn group".into())),
            (call("Bash", ToolKind::Shell { command: "\ncargo test\nmore".into() }), "", Arg::Raw("cargo test".into())),
            (call("Bash", ToolKind::Shell { command: String::new() }), "", Arg::Raw("Shell".into())),
            (call("mcp__linear__list", other("")), "", Arg::Raw("linear · list".into())),
            (call("Skill", ToolKind::Other { input: r#"{"skill": "apple-design"}"#.into(), description: String::new() }), "Skill", Arg::Text("/apple-design".into())),
            (call("NotebookEdit", other("")), "", Arg::Raw("NotebookEdit".into())),
            (call("Monitor", ToolKind::Shell { command: "tail -f build.log".into() }), "Watch", Arg::Raw("tail -f build.log".into())),
            (call("TaskStop", other("Standards review\nof quit diff")), "Stop", Arg::Text("Standards review of quit diff".into())),
            (call("TaskStop", ToolKind::Shell { command: "bun run dev".into() }), "Stop", Arg::Raw("bun run dev".into())),
            (call("TaskOutput", ToolKind::Shell { command: "bun run dev".into() }), "Read output of", Arg::Raw("bun run dev".into())),
        ];
        for (c, verb, arg) in cases {
            assert_eq!(line(&c), (verb, arg), "{}", c.name);
        }
    }

    #[test]
    fn a_file_badge_names_its_language() {
        assert_eq!(badge("src/chat.rs"), ("RS", 0xC2410C));
        assert_eq!(badge("App.TSX"), ("TS", 0x3178C6));
        assert_eq!(badge("Cargo.toml").0, "TM");
        assert_eq!(badge("Makefile"), ("", 0x8B8B94));
    }
}
