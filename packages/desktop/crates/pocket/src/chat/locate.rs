use crate::desktop::Desktop;
use chat::{Block, ToolKind, Transcript};
use gpui_kit::*;
use std::collections::HashSet;
use std::path::Path;
use theme::*;
use workspace::Doc;

/// Files a chip may mean, offered under it: the agent's id, where it was clicked, the paths.
pub(crate) type Choice = (String, Point<Pixels>, Vec<String>);

/// Files the agent read, edited or wrote, its sub-agents' too, latest first, each once.
pub(super) fn touched(t: &Transcript) -> Vec<String> {
    fn collect(t: &Transcript, paths: &mut Vec<String>) {
        for block in t.turns.iter().flat_map(|turn| &turn.blocks) {
            let Block::Tool(call) = block else { continue };
            match &call.kind {
                ToolKind::Read { path } | ToolKind::Edit { path, .. } | ToolKind::Write { path, .. } => paths.push(path.clone()),
                ToolKind::Agent(sub) => collect(&sub.transcript, paths),
                _ => {}
            }
        }
    }
    let mut paths = Vec::new();
    collect(t, &mut paths);
    paths.reverse();
    let mut seen = HashSet::new();
    paths.retain(|p| seen.insert(p.clone()));
    paths
}

/// Of `paths`, those `written` names: itself, or one ending in `/written`.
fn naming(written: &str, paths: Vec<String>) -> Vec<String> {
    let tail = format!("/{written}");
    paths.into_iter().filter(|p| p == written || p.ends_with(&tail)).collect()
}

/// The files a chip naming `written` may mean: the latest the agent touched, else each in the repo under `cwd`.
fn candidates(written: &str, cwd: &str, touched: Vec<String>, repo: impl FnOnce() -> Vec<String>) -> Vec<String> {
    if let Some(latest) = naming(written, touched).into_iter().next() {
        return vec![latest];
    }
    naming(written, repo()).into_iter().map(|p| Path::new(cwd).join(p).to_string_lossy().into_owned()).collect()
}

impl Desktop {
    /// Opens `path`, a chip's link joined to `cwd`; agents often write a bare name, so a missing one is looked for among the files `agent` touched, then in the repo, and several finds are offered at `at`.
    pub(super) fn chat_open_file(&mut self, agent: &str, path: String, cwd: &str, at: Point<Pixels>, cx: &mut Context<Self>) {
        let written = path.strip_prefix(cwd).map(|w| w.trim_start_matches('/').to_string());
        let (Some(written), (None, _)) = (written, hosts::split(cwd)) else { return self.open_doc(Doc::File(path), false, cx) };
        let touched = self.chats.views.get(agent).map(|v| touched(&v.session.transcript)).unwrap_or_default();
        let (cwd, runner) = (cwd.to_string(), hosts::runner_for(cwd));
        let task = cx.background_executor().spawn(async move {
            if Path::new(&path).exists() {
                return (path, Vec::new());
            }
            let found = candidates(&written, &cwd, touched, || git::ls_files(&runner, &cwd));
            (path, found)
        });
        let agent = agent.to_string();
        cx.spawn(async move |this, cx| {
            let (path, found) = task.await;
            this.update(cx, |d, cx| match &found[..] {
                [] => d.open_doc(Doc::File(path), false, cx),
                [one] => d.open_doc(Doc::File(one.clone()), false, cx),
                _ => {
                    d.close_menus();
                    d.chats.files = Some((agent, at, found));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The files a chip in `agent`'s chat may mean, while they're offered.
    pub(super) fn chat_files_menu(&self, agent: &str, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let (_, at, paths) = self.chats.files.as_ref().filter(|(a, ..)| a == agent)?;
        let cwd = self.agents.get(agent).map(|a| format!("{}/", a.cwd)).unwrap_or_default();
        let mut menu = ui::pop(div().id("chat-files"))
            .w(px(360.))
            .p(px(6.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.chats.files = None;
                cx.notify();
            }));
        for (i, path) in paths.iter().enumerate() {
            let label = path.strip_prefix(&cwd).unwrap_or(path);
            let path = path.clone();
            menu = menu.child(ui::menu_pick(("chat-file", i), file_icon(&path, false, false, 14.), label, false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.chats.files = None;
                this.open_doc(Doc::File(path.clone()), false, cx);
            })));
        }
        Some(deferred(anchored().position(*at).snap_to_window_with_margin(px(8.)).child(ui::menu_in("chat-files-in", menu))).with_priority(1))
    }
}

#[cfg(test)]
mod tests {
    use super::{candidates, touched};
    use chat::{Block, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript, Turn};

    fn call(kind: ToolKind) -> Block {
        Block::Tool(ToolCall { id: String::new(), name: String::new(), kind, status: ToolStatus::Ok, output: None, duration_ms: None, at: 0 })
    }

    fn transcript(blocks: Vec<Block>) -> Transcript {
        Transcript { turns: vec![Turn { blocks, ..Default::default() }] }
    }

    fn read(path: &str) -> Block {
        call(ToolKind::Read { path: path.into() })
    }

    #[test]
    fn touched_files_come_latest_first_once_each_with_a_sub_agents() {
        let sub = transcript(vec![call(ToolKind::Edit { path: "/r/b.rs".into(), diff: None })]);
        let t = transcript(vec![read("/r/a.rs"), call(ToolKind::Agent(SubAgent { transcript: sub, ..Default::default() })), call(ToolKind::Shell { command: "ls".into(), description: String::new() }), read("/r/a.rs"), call(ToolKind::Write { path: "/r/c.rs".into(), diff: None })]);
        assert_eq!(touched(&t), ["/r/c.rs", "/r/a.rs", "/r/b.rs"]);
    }

    #[test]
    fn a_bare_name_means_the_latest_touched_file_so_named_before_any_in_the_repo() {
        let touched = vec!["/r/src/x_add_to_chat.rs".to_string(), "/r/src/b/add_to_chat.rs".to_string(), "/r/src/a/add_to_chat.rs".to_string()];
        assert_eq!(candidates("add_to_chat.rs", "/r", touched, || panic!("the repo isn't listed")), ["/r/src/b/add_to_chat.rs"]);
    }

    #[test]
    fn untouched_names_are_matched_whole_against_the_repo() {
        let repo = || ["src/a/add_to_chat.rs", "src/my_add_to_chat.rs", "b/add_to_chat.rs", "add_to_chat.rs.bak"].map(String::from).to_vec();
        assert_eq!(candidates("add_to_chat.rs", "/r", Vec::new(), repo), ["/r/src/a/add_to_chat.rs", "/r/b/add_to_chat.rs"]);
        assert_eq!(candidates("a/add_to_chat.rs", "/r", Vec::new(), repo), ["/r/src/a/add_to_chat.rs"]);
        assert!(candidates("gone.rs", "/r", Vec::new(), repo).is_empty());
    }
}
