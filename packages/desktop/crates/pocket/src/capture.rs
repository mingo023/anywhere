use crate::actions::{ToggleFocus, ToggleRail, ToggleSidebar, TreeMenuArrow};
use crate::add_to_chat::Quote;
use crate::chat::composer::suggest::Prompt;
use crate::chat::tool_row::{self, Tools};
use crate::composer::Menu;
use crate::creating::Create;
use crate::desktop::Desktop;
use crate::explorer::menu::{Arrow, TreeMenu};
use crate::explorer::toast::Notice;
use crate::modals::new_session::picker::Picker;
use crate::desktop::chrome::{Confirm, Layout, Overlay, RowMenu, Screen, Side};
use crate::git_ui::diff::At;
use crate::git_ui::graph::GraphState;
use crate::settings::Section;
use crate::status::Status;
use crate::updates::Update;
use chat::{Block, Command, Row, ToolKind};
use git::Kind;
use git::github::{Check, Checks, Comment, Mergeable, Outcome, Pr, PrState, Review, Thread};
use gpui_kit::component::Root;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use workspace::{Doc, Tab};

type Step = fn(&mut Desktop, &mut Window, &mut Context<Desktop>);

const STEPS: [(&str, Step); 222] = [
    ("session", session),
    ("chat", |d, window, cx| {
        session(d, window, cx);
        if let Some(Tab::Term(t)) = d.pane_tab(d.focused_pane()) {
            d.store.chats.insert(t.clone());
            d.focus_chat_or_terminal(t, window, cx);
        }
    }),
    ("compose", |d, window, cx| d.new_session(Some("Fix the flaky parser test".into()), true, window, cx)),
    ("compose-send", |d, window, cx| d.start_session(window, cx)),
    ("compose-local", |d, window, cx| {
        d.store.agents.start_as_chat = true;
        d.new_session(Some("Fix the flaky parser test".into()), false, window, cx);
    }),
    ("chat-fold-open", |d, _, cx| {
        let Some(agent) = d.session().map(str::to_string) else { return };
        let Some(view) = d.chats.views.get(&agent) else { return };
        let s = &view.session;
        let Some(ix) = s.rows.iter().position(|r| matches!(r, Row::Fold { turn } if !chat::at_work_on(&s.transcript, chat::folded(&s.transcript, *turn)))) else { return };
        let id = s.ids[ix].clone();
        view.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(0.) });
        d.chat_toggle(&agent, &id, false, cx);
    }),
    ("chat-tools-open", |d, _, cx| _ = open_phase(d, is_call, cx)),
    ("chat-thinking-open", |d, _, cx| {
        if let Some((tools, Block::Thinking(m))) = open_phase(d, |b| matches!(b, Block::Thinking(_)), cx) {
            tools.update(cx, |t, cx| t.toggle(&m.id, false, None, cx));
        }
    }),
    ("chat-detail-open", |d, _, cx| {
        if let Some((tools, Block::Tool(call))) = open_phase(d, opens, cx) {
            tools.update(cx, |t, cx| t.toggle(&call.id, false, tool_row::colour(&call), cx));
        }
    }),
    ("chat-sub-open", |d, _, cx| open_agent(d, |c| c.status != chat::ToolStatus::Running, cx)),
    ("chat-details-open", |d, _, cx| {
        let Some(agent) = d.session().map(str::to_string) else { return };
        let Some(v) = d.chats.views.get(&agent) else { return };
        let s = &v.session;
        let shut: Vec<String> = s.rows.iter().zip(&s.ids).filter(|(r, id)| matches!(r, Row::Fold { .. } | Row::Phase { .. }) && !s.open(id, false)).map(|(_, id)| id.clone()).collect();
        let calls: Vec<_> = s.transcript.turns.iter().flat_map(|t| &t.blocks).filter(|b| opens(b)).cloned().collect();
        let tools = v.tools.clone();
        for id in shut {
            d.chat_toggle(&agent, &id, false, cx);
        }
        tools.update(cx, |t, cx| {
            for b in calls {
                if let Block::Tool(call) = b {
                    t.toggle(&call.id, false, tool_row::colour(&call), cx);
                }
            }
        });
    }),
    ("chat-mcp-open", |d, _, cx| _ = open_phase(d, |b| matches!(b, Block::Tool(c) if c.name.starts_with("mcp__")), cx)),
    ("chat-scrolled-down", |d, _, cx| {
        if let Some(v) = d.session().and_then(|a| d.chats.views.get(a)) {
            v.list.scroll_by(px(560.));
            cx.notify();
        }
    }),
    ("chat-sub-open-running", |d, _, cx| open_agent(d, |c| c.status == chat::ToolStatus::Running, cx)),
    // Streams into the sub-agent fixture's running `tu-t2` as pocketd would: its command finishes, it reports, and
    // spawns a sub-agent of its own that searches. A finished tool keeps the time it started, as in pocketd.
    ("chat-sub-live", |d, _, cx| {
        let Some(agent) = d.session().map(str::to_string) else { return };
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
        let items = serde_json::json!([
            {"id": "c4", "seq": 100, "ts": ts - 61_000, "kind": "tool", "parentToolUseId": "tu-t2", "call": {"toolUseId": "tu-c4", "name": "Bash", "detail": {"kind": "shell", "command": "npx depcheck"}, "status": "ok", "output": "Unused dependencies\n* left-pad", "durationMs": 4100}},
            {"id": "c5", "seq": 101, "ts": ts, "kind": "tool", "parentToolUseId": "tu-t2", "call": {"toolUseId": "tu-c5", "name": "Task", "detail": {"kind": "task", "description": "Check who imports left-pad"}}},
            {"id": "c6", "seq": 102, "ts": ts, "kind": "tool", "parentToolUseId": "tu-c5", "call": {"toolUseId": "tu-c6", "name": "Grep", "detail": {"kind": "search", "query": "left-pad"}, "status": "ok", "output": "no matches", "durationMs": 30}},
            {"id": "c7", "seq": 103, "ts": ts, "kind": "assistant", "parentToolUseId": "tu-c5", "text": "Nothing imports left-pad."},
        ]);
        if let Ok(items) = serde_json::from_value(items) {
            d.agents.apply(agents::Event::Items(agent, items));
            cx.notify();
        }
    }),
    ("chat-background-folded", |d, _, cx| {
        d.chats.composer.background_folded = true;
        cx.notify();
    }),
    ("chat-background-show", |d, _, cx| {
        let Some(agent) = d.session().map(str::to_string) else { return };
        let task = d.chats.views.get(&agent).and_then(|v| chat::in_background(&v.session.transcript.turns).last().map(|(turn, block, _)| (turn, block)));
        if let Some((turn, block)) = task {
            d.chat_show_block(&agent, turn, block, cx);
        }
    }),
    ("chat-agent-map", |d, window, cx| {
        if let Some(agent) = d.session().map(str::to_string) {
            d.chat_toggle_map(&agent, window, cx);
        }
    }),
    // After `chat-agent-map`: the details of the first sub-agent still at work, else the first.
    ("chat-agent-map-detail", |d, _, cx| {
        let Some(agent) = d.session().map(str::to_string) else { return };
        let Some(v) = d.chats.views.get(&agent) else { return };
        let map = chat::spawned(&v.session.transcript);
        let Some(ix) = map.iter().position(|s| s.call.status == chat::ToolStatus::Running).or((!map.is_empty()).then_some(0)) else { return };
        let call = map[ix].call.id.clone();
        d.chat_map_show(ix + 1, &call, cx);
    }),
    ("chat-scrolled-up", |d, _, cx| d.scroll_chat_back(30, cx)),
    ("chat-top", |d, _, cx| d.scroll_chat_back(usize::MAX, cx)),
    // Into the first reply, so the first prompt sticks.
    ("chat-sticky", |d, _, cx| scroll_from_prompt(d, 0, 2, cx)),
    // The row over the second prompt at the top, so that prompt shows just under it and pushes the first one's card.
    ("chat-sticky-pushed", |d, _, cx| scroll_from_prompt(d, 1, -1, cx)),
    // After `chat-sticky-pushed` has laid that row out: on until the second prompt's bubble is 30px from the top, partway into the card.
    ("chat-sticky-push-in", |d, _, cx| {
        let Some(v) = d.session().and_then(|a| d.chats.views.get(a)) else { return };
        let Some(ix) = v.session.rows.iter().enumerate().filter(|(_, r)| matches!(r, Row::Prompt { .. })).nth(1).map(|(ix, _)| ix) else { return };
        let Some(above) = ix.checked_sub(1).and_then(|i| v.list.bounds_for_item(i)) else { return };
        let gap = chat::top_gap(v.session.rows.get(ix - 1), &v.session.rows[ix]);
        v.list.scroll_to(ListOffset { item_ix: ix - 1, offset_in_item: px((f32::from(above.size.height) + gap - 30.).max(0.)) });
        cx.notify();
    }),
    ("chat-sticky-open", |d, _, cx| {
        let Some(agent) = d.session().map(str::to_string) else { return };
        if let Some(v) = d.chats.views.get_mut(&agent) {
            v.sticky_open = v.session.rows.iter().position(|r| matches!(r, Row::Prompt { .. })).map(|ix| v.session.ids[ix].clone());
            cx.notify();
        }
    }),
    // The first reply's last two blocks over its wide table (chat/wide-table.json), so the narrow table above shows
    // too. By row, not by pixels: the list knows only measured heights, so a pixel scroll lands wherever the rows it
    // skips were never laid out.
    ("chat-table", |d, _, cx| {
        let Some(v) = d.session().and_then(|a| d.chats.views.get(a)) else { return };
        let Some(ix) = first_reply_end(d) else { return };
        v.list.scroll_to(ListOffset { item_ix: ix.saturating_sub(2), offset_in_item: px(0.) });
        cx.notify();
    }),
    // A sideways swipe over the wide table, so it shows scrolled partway.
    ("chat-table-swipe", |d, window, cx| swipe(d, first_reply_end, -160., window, cx)),
    // A swipe past its far end, so it shows scrolled all the way.
    ("chat-table-end", |d, window, cx| swipe(d, first_reply_end, -4000., window, cx)),
    ("chat-jump", |d, window, cx| {
        if let Some(agent) = d.session().map(str::to_string) {
            d.chat_jump_to_bottom(&agent, window, cx);
        }
    }),
    ("chat-code-swipe", |d, window, cx| swipe(d, last_code, -160., window, cx)),
    ("chat-typed", |d, window, cx| d.set_chat_text("Refactor the parser", window, cx)),
    ("chat-model-open", |d, _, cx| {
        d.chats.composer.draft.toggle(Menu::Model);
        cx.notify();
    }),
    ("chat-access", |d, _, cx| {
        if let Some(a) = d.session().map(str::to_string).and_then(|id| d.agents.list.iter_mut().find(|a| a.id == id)) {
            (a.chat_only, a.access) = (true, Some("edits".into()));
        }
        cx.notify();
    }),
    ("chat-access-open", |d, _, cx| {
        d.chats.composer.draft.toggle(Menu::Access);
        cx.notify();
    }),
    ("chat-model-effort", |d, _, cx| {
        d.chats.composer.draft.side = Some(crate::composer::model_menu::Side::Effort);
        cx.notify();
    }),
    ("chat-model-models", |d, _, cx| {
        d.chats.composer.draft.side = Some(crate::composer::model_menu::Side::Model);
        cx.notify();
    }),
    ("chat-slash", |d, window, cx| d.type_text(Prompt::Chat, "/", window, cx)),
    ("chat-slash-re", |d, window, cx| d.type_text(Prompt::Chat, "/re", window, cx)),
    ("chat-slash-mcp", |d, window, cx| d.type_text(Prompt::Chat, "/mc", window, cx)),
    ("chat-mcp", |d, window, cx| d.type_text(Prompt::Chat, "Look it up with @mcp/", window, cx)),
    ("chat-mcp-li", |d, window, cx| d.type_text(Prompt::Chat, "Look it up with @mcp/li", window, cx)),
    ("chat-mention", |d, window, cx| d.type_text(Prompt::Chat, "Look at @", window, cx)),
    ("chat-mention-rea", |d, window, cx| d.type_text(Prompt::Chat, "Look at @rea", window, cx)),
    ("new-slash", |d, window, cx| d.type_text(Prompt::Launch, "/", window, cx)),
    ("new-mention", |d, window, cx| d.type_text(Prompt::Launch, "Look at @", window, cx)),
    ("chat-marked", |d, window, cx| d.set_chat_text("/review 482 then check @src/fold.rs and @docs/", window, cx)),
    ("chat-marked-sent", |d, _, cx| {
        if let Some(agent) = d.session().map(str::to_string) {
            d.chat_command(&agent, Command::Send("Look at @src/parser.rs and @docs/adr/0003-desktop-code-layout.md, then fold the tool runs. Mail dev@pocket.app".into()), cx);
        }
    }),
    ("chat-command-sent", |d, _, cx| {
        if let Some(agent) = d.session().map(str::to_string) {
            d.chat_command(&agent, Command::Send("/review 482".into()), cx);
        }
    }),
    ("chat-goal", |d, _, cx| {
        let goal = agents::Goal { objective: "Every parser test passes and the CI lint job is green".into(), status: agents::GoalStatus::Active, tokens_used: 16510, token_budget: 50000 };
        if let Some(a) = d.session().map(str::to_string).and_then(|id| d.agents.list.iter_mut().find(|a| a.id == id)) {
            a.goal = Some(goal);
        }
        cx.notify();
    }),
    ("chat-typed-long", |d, window, cx| {
        let text = "Refactor the parser so each tool call folds into its turn.\nKeep the row ids stable and add a test for every kind.";
        d.set_chat_text(text, window, cx);
    }),
    ("chat-sent", |d, _, cx| {
        if let Some(agent) = d.session().map(str::to_string) {
            d.chat_command(&agent, Command::Send("Now run the parser tests".into()), cx);
        }
    }),
    // Queues a send as the composer does while the agent works, without the fixture echoing it.
    ("chat-queued", |d, _, cx| {
        if let Some(v) = d.session().map(str::to_string).and_then(|a| d.chats.views.get_mut(&a)) {
            v.session.outgoing.push("Now run the parser tests".into(), true, crate::util::now_ms());
            cx.notify();
        }
    }),
    ("chat-queued-away", |d, window, cx| {
        if let Some(v) = d.session().map(str::to_string).and_then(|a| d.chats.views.get_mut(&a)) {
            v.session.outgoing.push("Now run the parser tests".into(), true, crate::util::now_ms());
        }
        d.new_session(None, true, window, cx);
    }),
    ("chat-sent-commit", |d, _, cx| {
        if let Some(agent) = d.session().map(str::to_string) {
            d.chat_command(&agent, Command::Send("Then commit the rename".into()), cx);
        }
    }),
    ("chat-stop", |d, _, cx| {
        if let Some(agent) = d.session().map(str::to_string) {
            d.chat_command(&agent, Command::Interrupt, cx);
        }
    }),
    ("chat-send-now", |d, _, cx| {
        if let Some(agent) = d.session().map(str::to_string) {
            d.chat_command(&agent, Command::SendNow, cx);
        }
    }),
    ("chat-queue-remove", |d, window, cx| {
        if let Some((agent, send)) = first_queued(d) {
            d.chat_queue_remove(&agent, send, window, cx);
        }
    }),
    ("chat-queue-edit", |d, window, cx| {
        if let Some((agent, send)) = first_queued(d) {
            d.chat_queue_edit(&agent, send, window, cx);
        }
    }),
    ("chat-queue-steer", |d, window, cx| {
        if let Some((agent, send)) = first_queued(d) {
            d.chat_queue_steer(&agent, send, window, cx);
        }
    }),
    ("chat-resume", |d, _, cx| {
        if let Some(agent) = d.session().map(str::to_string) {
            d.chat_queue_resume(&agent, cx);
        }
    }),
    ("chat-slash", |d, _, cx| {
        if let Some(agent) = d.session().map(str::to_string) {
            d.chat_command(&agent, Command::Send("/compact".into()), cx);
        }
    }),
    // Another agent's chat: one already open, else the first other agent's.
    ("chat-other", |d, window, cx| {
        let here = d.session().map(str::to_string);
        let others: Vec<(String, String)> = d.agents.list.iter().filter(|a| Some(&a.id) != here.as_ref()).map(|a| (a.id.clone(), a.terminal_id.clone())).collect();
        let Some((agent, terminal)) = others.iter().find(|(_, t)| d.store.chats.contains(t)).or(others.first()).cloned() else { return };
        d.store.chats.insert(terminal.clone());
        d.focus_agent(&agent, window, cx);
        d.focus_chat_or_terminal(terminal, window, cx);
    }),
    ("chat-exit", |d, _, _| fixture(d, "exit")),
    ("chat-compacted", |d, _, _| fixture(d, "compaction-after-result")),
    ("chat-clear", |d, _, _| fixture(d, "clear")),
    ("chat-restart", |d, _, _| fixture(d, "restart")),
    ("chat-permission", |d, _, _| fixture(d, "permission")),
    ("chat-question", |d, _, _| fixture(d, "question")),
    ("chat-sub-agent", |d, _, _| fixture(d, "sub-agent")),
    ("chat-perf", |d, _, _| fixture(d, "perf-1200")),
    ("chat-stream-code", |d, _, _| fixture(d, "stream-code")),
    ("worktree", |d, _, _| d.worktree = d.cwd().and_then(|cwd| d.worktree_of(&cwd)).map(|w| w.path.clone())),
    ("rail", |d, window, cx| d.toggle_rail(&ToggleRail, window, cx)),
    ("focus", |d, window, cx| d.toggle_focus(&ToggleFocus, window, cx)),
    ("sidebar", |d, window, cx| d.toggle_sidebar(&ToggleSidebar, window, cx)),
    ("projects", |d, window, cx| d.toggle_project_picker(window, cx)),
    ("add-menu", |d, _, _| d.sidebar.add_menu = true),
    ("organize-menu", |d, _, _| d.sidebar.organize.menu = true),
    ("sections", |d, _, _| {
        let projects = d.projects();
        let sections = &mut d.store.sections;
        let work = sections.add_group("Work");
        sections.add_group("Side");
        for p in projects.iter().skip(1).step_by(2) {
            sections.set_group(p, Some(work));
        }
        if let Some(p) = projects.last() {
            sections.pin(store::sections::Pin::Project(p.clone()));
        }
        let first = projects.first().cloned().unwrap_or_default();
        if let Some(w) = d.listed_trees(&first).and_then(|ts| ts.into_iter().find(|w| !w.main)) {
            d.store.sections.pin(store::sections::Pin::Tree { project: first, tree: w.path });
        }
    }),
    ("filter", |d, window, cx| {
        d.open_filter(window, cx);
        d.sidebar.filter.input.update(cx, |s, cx| s.set_value("fix", window, cx));
    }),
    ("explore", |d, _, cx| {
        (d.side, d.sidebar.panel_open) = (Side::Explorer, true);
        d.refresh_git(cx);
    }),
    ("tree-menu", |d, window, cx| {
        let Some(root) = d.explore_root().map(PathBuf::from) else { return };
        let target = d.explorer.tree.get(&root).and_then(|l| l.first()).map(|(dir, p)| (p.clone(), *dir));
        d.explorer.menu = Some(TreeMenu::new(target, point(px(1150.), px(112.)), false, window.viewport_size().width));
        cx.notify();
    }),
    ("tree-menu-down", |d, window, cx| d.tree_menu_arrow(&TreeMenuArrow(Arrow::Down), window, cx)),
    ("tree-mentioned", |d, _, cx| {
        if let Some(c) = d.chat_choices().into_iter().next() {
            d.show_notice(Notice::Mentioned { provider: c.provider, title: c.title }, cx);
        }
    }),
    ("changes", |d, _, cx| {
        d.open_changes(None, false, cx);
        d.refresh_graph(cx);
    }),
    ("graph-expand", |d, _, cx| d.toggle_commit(0, cx)),
    ("graph-file", |d, _, cx| {
        if let Some(doc) = d.graph.file_doc(0, 0) {
            d.open_doc(doc, false, cx);
        }
    }),
    ("graph-commit", |d, _, cx| {
        if let Some(sha) = d.graph.commits.first().map(|c| c.sha.clone()) {
            d.open_doc(Doc::Commit(sha), false, cx);
        }
    }),
    ("tab-menu", |d, _, _| d.panels.menu = Some(d.focused_pane())),
    ("term-menu", |d, _, _| d.terminal.menu = d.terminal.focused.clone().map(|p| (p, point(px(520.), px(300.))))),
    ("browser", |d, window, cx| d.open_browser(None, window, cx)),
    ("file", |d, _, cx| {
        if let Some(path) = d.cwd().zip(d.repo().and_then(|r| r.files.first())).map(|(root, f)| format!("{root}/{}", f.path)) {
            d.open_file(path, false, cx);
        }
    }),
    ("pill", |d, _, _| d.chat.pill = Quote::prose("README.md", "", "Add to chat").map(|q| (point(px(420.), px(260.)), q))),
    ("chat-menu", |d, window, cx| d.open_chat_menu(window, cx)),
    ("ask-file", |d, _, _| {
        let pane = d.focused_pane();
        let Some((path, text)) = d.preview.pane(pane).and_then(|f| Some((f.file.clone()?, f.text()?.to_string()))) else { return };
        let end = text.match_indices('\n').nth(2).map_or(text.len(), |(i, _)| i);
        d.chat.file = Quote::code(&crate::util::basename(&path), &text, 0..end).map(|q| (point(px(420.), px(260.)), q));
    }),
    ("add-to-chat", |d, window, cx| {
        let pane = d.focused_pane();
        if let Some(i) = d.diff.view(pane).and_then(|v| v.lines.iter().position(|l| l.kind == Kind::Add)) {
            d.open_composer(pane, i, window, cx);
        }
    }),
    ("inbox", |d, window, cx| d.open_inbox(window, cx)),
    ("chat-only-done", |d, _, _| {
        for a in d.agents.list.iter_mut() {
            (a.chat_only, a.status) = if a.id == "parser" { (true, "done".into()) } else { (a.chat_only, "idle".into()) };
        }
    }),
    ("automations", |d, window, cx| d.open_automations(&crate::actions::OpenAutomations, window, cx)),
    ("automations-runs", |d, _, _| d.automations.tab = crate::automations::logic::Tab::Runs),
    ("automations-done-run", |d, _, _| d.automations.selected_run = Some("r5".into())),
    ("automations-editor", |d, window, cx| {
        let first = d.agents.automations.items.first().map(|a| a.id.clone());
        d.edit_automation(first.as_deref(), window, cx);
    }),
    ("automations-new", |d, window, cx| d.edit_automation(None, window, cx)),
    ("settings", |d, window, cx| d.open_settings(&crate::actions::OpenSettings, window, cx)),
    ("settings-appearance", |d, _, _| d.settings.section = Section::Appearance),
    ("settings-notifications", |d, _, _| d.settings.section = Section::Notifications),
    ("settings-keyboard", |d, _, _| d.settings.section = Section::Keyboard),
    ("settings-projects", |d, _, _| d.settings.section = Section::Projects),
    ("settings-agents", |d, _, _| d.settings.section = Section::Agents),
    ("settings-automations", |d, _, _| d.settings.section = Section::Automations),
    ("settings-phone", |d, _, _| d.settings.section = Section::Phone),
    ("settings-hosts", |d, _, _| d.settings.section = Section::Hosts),
    ("hosts-sample", |d, _, _| {
        let devbox = hosts::Host::new(hosts::Target::Alias { name: "devbox".into() }, &[]);
        let gpu = hosts::Host::new(hosts::Target::Address { addr: "10.0.4.12".into(), port: 2222, user: "mingo".into(), key_file: None }, &[]);
        d.fleet.links.0.insert(devbox.id.clone(), hosts::link::Link::Online { version: "1.4.2".into(), uptime: 3 * 86_400 });
        d.fleet.links.0.insert(gpu.id.clone(), hosts::link::Link::Retrying(hosts::ssh::Failure::NotRunning));
        d.store.hosts = vec![devbox, gpu];
    }),
    // The real host the remote hosts e2e runs against; see `.ui-review/remote-hosts/`.
    ("vm", |d, window, cx| {
        let target = hosts::Target::Address { addr: "127.0.0.1".into(), port: 2222, user: "mingo".into(), key_file: Some("/tmp/anywhere-e2e/id_ed25519".into()) };
        if !d.store.hosts.iter().any(|h| h.target == target) {
            d.store.hosts.push(hosts::Host::new(target, &[]));
            d.connect_hosts(window, cx);
        }
    }),
    ("wait", |_, _, _| {}),
    ("vm-install", |d, _, cx| {
        if let Some(id) = d.store.hosts.first().map(|h| h.id.clone()) {
            d.install_host(&id, false, cx);
        }
    }),
    ("vm-update", |d, _, _| {
        if let Some(h) = d.store.hosts.first() {
            (d.confirm, d.overlay) = (Some(Confirm::UpdateHost { id: h.id.clone(), name: h.name.clone(), windows: false }), Some(Overlay::Confirm));
        }
    }),
    ("vm-remove", |d, _, _| {
        if let Some(h) = d.store.hosts.first() {
            (d.confirm, d.overlay) = (Some(Confirm::RemoveHost { id: h.id.clone(), name: h.name.clone() }), Some(Overlay::Confirm));
        }
    }),
    ("vm-uninstall", |d, _, cx| {
        if let Some(id) = d.store.hosts.first().map(|h| h.id.clone()) {
            d.remove_host(&id, true, cx);
        }
    }),
    ("vm-add", |d, window, cx| {
        d.open(Overlay::AddRepo, window, cx);
        d.set_repo_source(crate::modals::add_project::Source::Host);
        if let Some(h) = d.store.hosts.first() {
            d.browse_host(h.id.clone(), window, cx);
        }
    }),
    ("vm-browse", |d, window, cx| d.browse_to("/home/mingo/vm-app".into(), window, cx)),
    ("vm-open", |d, window, cx| d.open_browsed(window, cx)),
    ("vm-save", |d, window, cx| d.save_repo(window, cx)),
    ("vm-shell", |d, _, cx| d.new_shell(workspace::Place::Pane(None), cx)),
    ("vm-uname", |d, _, _| {
        if let Some(id) = d.terminals.sessions.items.iter().rev().map(|s| s.info.id.clone()).find(|id| hosts::session::untag(id).0.is_some()) {
            d.daemon.input(&id, b"uname -a\r");
        }
    }),
    ("vm-new", |d, window, cx| {
        let Some(project) = d.store.projects.iter().find(|p| hosts::split(p).0.is_some()).cloned() else { return };
        d.select_tree(project, None, cx);
        d.new_session(Some("hello from the mac".into()), true, window, cx);
    }),
    ("vm-start", |d, window, cx| d.start_session(window, cx)),
    ("vm-agent", |d, window, cx| {
        if let Some(id) = d.agents.list.iter().map(|a| a.id.clone()).find(|id| hosts::session::untag(id).0.is_some()) {
            d.focus_agent(&id, window, cx);
        }
    }),
    ("vm-image", |d, _, cx| {
        if let Some(project) = d.store.projects.iter().find(|p| hosts::split(p).0.is_some()) {
            d.open_file(format!("{project}/logo.png"), false, cx);
        }
    }),
    ("hosts-add", |d, _, _| {
        (d.fleet.form.open, d.fleet.form.suggestions) = (true, vec!["devbox".into(), "gpu-box".into(), "nas".into()]);
        d.fleet.form.error = Some("Enter the host's address or name".into());
    }),
    ("hosts-password", |d, window, cx| {
        d.fleet.links.0.insert("10-0-4-12".into(), hosts::link::Link::Failed(hosts::ssh::Failure::Auth));
        d.open_password("10-0-4-12".into(), window, cx);
        d.fleet.password.error = Some(crate::fleet::sign_in_error(hosts::ssh::Failure::Auth, "10.0.4.12", false));
    }),
    ("hosts-pick", |d, window, cx| d.suggest_host("devbox".into(), window, cx)),
    ("hosts-remove", |d, _, _| {
        let name = "devbox".to_string();
        (d.confirm, d.overlay) = (Some(Confirm::RemoveHost { id: name.clone(), name }), Some(Overlay::Confirm));
    }),
    ("settings-sidebar", |d, _, _| d.settings.section = Section::Sidebar),
    ("settings-terminal", |d, _, _| d.settings.section = Section::Terminal),
    ("settings-files", |d, _, _| d.settings.section = Section::Files),
    ("settings-browser", |d, _, _| d.settings.section = Section::Browser),
    ("settings-git", |d, _, _| d.settings.section = Section::Git),
    ("settings-diff", |d, _, _| d.settings.section = Section::Diff),
    ("settings-claude", |d, _, _| d.settings.section = Section::Provider(0)),
    ("settings-codex", |d, _, _| d.settings.section = Section::Provider(1)),
    ("settings-accounts", |d, _, _| d.settings.section = Section::Accounts),
    ("accounts-sample", |d, _, _| _ = d.accounts.lists.insert(None, accounts_sample())),
    ("account-signin", |d, window, cx| {
        let id = "fx-signin".to_string();
        let mut term = term::Term::new(100, 30);
        term.write(b"Opening browser to sign in\xe2\x80\xa6\r\nIf the browser didn't open, visit: https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&response_type=code&scope=user%3Ainference&state=Nwd_oVnDKfPnpygUrKjXPJ00\r\nPaste code here if prompted > ");
        d.terminals.sessions.items.push(crate::terminals::sessions::Session { info: daemon::Info { id: id.clone(), ..Default::default() }, term: Some(term), exit: None });
        let mut signin = d.new_sign_in(crate::accounts::Key { host: None, provider: "claude".into(), id: String::new() }, window, cx);
        (signin.terminal, signin.output_open) = (Some(id), true);
        d.accounts.signins.push(signin);
    }),
    ("account-menu", |d, _, _| {
        d.accounts.menu = Some(crate::accounts::Key { host: None, provider: "claude".into(), id: "work".into() });
    }),
    ("accounts-auto", |d, _, _| _ = d.accounts.auto_switch.insert(None, true)),
    ("account-usage", |d, _, cx| {
        d.chats.composer.draft.menu = Some(Menu::Usage { back: false });
        cx.notify();
    }),
    ("account-switch", |d, _, cx| {
        d.chats.composer.draft.menu = Some(Menu::Switch);
        cx.notify();
    }),
    ("account-switching", |d, _, cx| {
        if let Some(a) = d.session().map(str::to_string).and_then(|id| d.agents.list.iter_mut().find(|a| a.id == id)) {
            a.switching = "work".into();
        }
        cx.notify();
    }),
    ("project-accounts", |d, _, _| {
        for p in d.projects() {
            d.store.repos.entry(p).or_default().accounts.insert("claude".into(), "work".into());
        }
    }),
    ("model-menu", |d, _, _| d.settings.menu = Some(match (d.overlay, d.settings.section) {
        (_, Section::Git) => "ai-model",
        (_, Section::Provider(1)) => "codex-model",
        _ => "claude-model",
    })),
    ("service-down", |d, _, _| d.terminals.link.down(Instant::now())),
    ("service-starting", |d, _, _| d.settings.starting = Some(Instant::now())),
    ("settings-search", |d, window, cx| d.settings.search.update(cx, |s, cx| s.set_value("sound", window, cx))),
    ("settings-search-chat", |d, window, cx| d.settings.search.update(cx, |s, cx| s.set_value("chat", window, cx))),
    ("settings-search-shell", |d, window, cx| d.settings.search.update(cx, |s, cx| s.set_value("shell", window, cx))),
    ("settings-advanced", |d, _, _| {
        let id = d.settings.section.id();
        d.store.advanced.insert(id.into());
    }),
    ("settings-reset", |d, _, _| {
        (d.store.sounds.all, d.store.notifications.banners) = (false, false);
        (d.confirm, d.overlay) = (Some(crate::desktop::chrome::Confirm::ResetSection(d.settings.section)), Some(Overlay::Confirm));
    }),
    ("palette", |d, window, cx| d.open(Overlay::Palette, window, cx)),
    ("new-session", |d, window, cx| d.new_session(None, false, window, cx)),
    ("form-model", |d, _, cx| d.toggle_composer(crate::modals::new_session::slot, Menu::Model, cx)),
    ("form-model-list", |d, _, _| crate::modals::new_session::slot(d).draft.side = Some(crate::composer::model_menu::Side::Model)),
    ("form-effort-list", |d, _, _| crate::modals::new_session::slot(d).draft.side = Some(crate::composer::model_menu::Side::Effort)),
    ("form-access", |d, _, cx| d.toggle_composer(crate::modals::new_session::slot, Menu::Access, cx)),
    ("new-worktree", |d, window, cx| d.new_worktree(&crate::actions::NewWorktree, window, cx)),
    ("prompt", |d, window, cx| d.reset_new_form(Some("The RestoreView snapshot fails on CI about 1 in 5 runs. Find out why and fix it, then run the tests.".into()), false, window, cx)),
    ("add-repo", |d, window, cx| d.edit_project(d.projects().into_iter().next(), window, cx)),
    ("clone", |d, window, cx| {
        d.clone_project(window, cx);
        d.clone_form.url.update(cx, |s, cx| s.set_value("https://github.com/zed-industries/zed", window, cx));
        d.clone_url_changed(window, cx);
    }),
    ("clone-group-menu", |d, _, _| d.clone_form.group_menu = true),
    ("project-settings", |d, window, cx| d.project_settings(&crate::actions::ProjectSettings, window, cx)),
    ("project-icons", |d, _, _| {
        let app_icon = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/AppIcon.png"));
        let icons = [store::ProjectIcon::Emoji("🚀".into()), store::ProjectIcon::Image(app_icon)];
        for (p, icon) in d.projects().into_iter().zip(icons) {
            d.store.repos.entry(p).or_default().icon = Some(icon);
        }
    }),
    ("emoji-menu", |d, _, _| d.settings.menu = Some(crate::modals::add_project::icon::EMOJI_MENU)),
    ("phone-access", |d, window, cx| d.open(Overlay::PhoneAccess, window, cx)),
    ("pair-phone", |d, window, cx| d.open(Overlay::PairPhone, window, cx)),
    ("owner", |d, _, _| {
        let strings = |v: &[&str]| v.iter().map(|s| s.to_string()).collect();
        d.agents.apply(agents::Event::Connected { scopes: strings(&["observe", "drive", "approve", "spawn", "owner"]), caps: strings(&["pair.v1", "scopes.v1"]), version: String::new() });
    }),
    ("phones", |d, _, _| {
        let hour = 3_600_000;
        let phone = |id: &str, name: &str, platform: &str, ago: i64| daemon::Device { id: id.into(), name: name.into(), platform: platform.into(), last_seen_at: crate::util::now_ms() - ago, legacy: false };
        d.settings.host.devices = Some(vec![phone("p1", "Minh's iPhone 16 Pro", "iOS 26.1", 0), phone("p2", "iPad Air", "iPadOS 26.0", 72 * hour)]);
    }),
    ("no-phones", |d, _, _| d.settings.host.devices = Some(Vec::new())),
    ("custom-shell", |d, window, cx| {
        (d.store.terminal.shell, d.store.terminal.shell_path) = (store::prefs::terminal::Shell::Custom, "/opt/homebrew/bin/fish".into());
        if let Some(field) = d.settings.fields.get("shell-args").cloned() {
            field.update(cx, |f, cx| f.set_value("--login --private", window, cx));
        }
        d.edit_terminal_color(9, window, cx);
    }),
    ("custom-search", |d, window, cx| {
        d.store.browser.engine = store::prefs::browser::Engine::Custom;
        if let Some(field) = d.settings.fields.get("custom-search").cloned() {
            field.update(cx, |f, cx| f.set_value("search.brave.com/search?q=", window, cx));
        }
    }),
    ("rename-phone", |d, window, cx| d.start_phone_rename("p1".into(), "Minh's iPhone 16 Pro", window, cx)),
    ("tailnet", |d, _, _| {
        d.agents.host = Some(agents::Host { tailnet: true, ..Default::default() });
        d.settings.host.status = Some(daemon::Status { listen: vec!["127.0.0.1:4517".into(), "100.88.12.4:4517".into()], ..Default::default() });
    }),
    ("advanced", |d, _, _| {
        d.store.advanced.insert(d.settings.section.id().to_string());
    }),
    ("custom-keys", |d, _, _| {
        d.store.keys.insert("desktop::NextNeedsYou".into(), "cmd-shift-u".into());
    }),
    ("key-clash", |d, _, _| d.settings.keyboard.recording = Some(("desktop::NewWorktree".into(), Some("cmd-k".into())))),
    ("dark", |d, window, cx| d.set_appearance(WindowAppearance::Dark, window, cx)),
    ("syntax-github", |d, _, cx| syntax(d, store::SyntaxTheme::Github, cx)),
    ("syntax-one", |d, _, cx| syntax(d, store::SyntaxTheme::One, cx)),
    ("syntax-solarized", |d, _, cx| syntax(d, store::SyntaxTheme::Solarized, cx)),
    ("custom-diff", |d, _, _| {
        (d.store.appearance.added, d.store.appearance.removed) = (Some(theme::DIFF_SWATCHES[3]), Some(theme::DIFF_SWATCHES[5]));
        theme::set_diff_colors(d.store.appearance.added, d.store.appearance.removed);
    }),
    ("accent-blue", |d, _, cx| {
        d.store.appearance.accent = Some(theme::ACCENT_SWATCHES[0]);
        theme::set_accent(d.store.appearance.accent, cx);
    }),
    ("ui-font-geist", |d, _, cx| {
        d.store.appearance.ui_font = Some("Geist".into());
        theme::set_fonts(Some("Geist"), d.store.appearance.code_font.as_deref(), cx);
    }),
    ("code-font-menlo", |d, _, cx| {
        d.store.appearance.code_font = Some("Menlo".into());
        theme::set_fonts(d.store.appearance.ui_font.as_deref(), Some("Menlo"), cx);
        d.diff.relayout(true);
    }),
    ("code-size-14", |d, _, _| {
        d.store.appearance.code_size = Some(14);
        d.diff.relayout(true);
    }),
    ("code-font-menu", |d, _, _| d.settings.menu = Some("code-font")),
    ("creating", |d, _, _| {
        let Some(p) = d.project.clone() else { return };
        let now = Instant::now();
        let spec = serde_json::json!({"project": p, "checkout": {"new": {"name": "fix-flaky-snapshot", "base": "main", "copy": true, "setup": true}}, "prompt": "Fix the flaky snapshot"});
        let mut c = Create::new("capture".into(), format!("{p}.worktrees/fix-flaky-snapshot"), spec, now - Duration::from_secs(9));
        for (step, note, ago) in [("verify", "", 8.6), ("fetch", "", 8.3), ("fetch", "Couldn't fetch, using local main", 0.), ("worktree", "", 6.1), ("copy", "", 5.6), ("setup", "", 5.5)] {
            c.reach(step, note, now - Duration::from_secs_f32(ago));
        }
        d.worktree = Some(c.path.clone());
        d.creates.list.push(c);
    }),
    ("fail", |d, _, _| {
        if let Some(c) = d.creates.list.first_mut() {
            c.fail("Setup exited 1".into(), "npm ERR! missing script: setup".into(), Instant::now());
        }
    }),
    ("deleting", |d, _, _| {
        let Some(tree) = d.project.as_ref().and_then(|p| d.listed_trees(p)).and_then(|w| w.into_iter().find(|w| !w.main)).map(|w| w.path) else { return };
        d.removals.start(&tree);
    }),
    ("delete-worktree", |d, _, cx| {
        let Some((project, w)) = d.project.clone().and_then(|p| d.listed_trees(&p)?.into_iter().find(|w| !w.main).map(|w| (p, w))) else { return };
        let removal = crate::removal::Removal { project, tree: w.path, branch: Some(w.branch), delete_branch: true, teardown: true };
        d.confirm = Some(Confirm::DeleteWorktree { removal, dirty: 2, lost: 3 });
        d.overlay = Some(Overlay::Confirm);
        cx.notify();
    }),
    ("error", |d, _, _| d.error = Some("Couldn't save placeholder.tsx: Permission denied (os error 13)".into())),
    ("update-ready", |d, _, _| d.updates.update = Update::Ready { version: "0.2.0".into() }),
    ("whats-new", |d, _, _| d.updates.update = Update::WhatsNew { version: "0.2.0".into() }),
    ("prs", |d, _, _| {
        let Some(trees) = d.project.as_ref().and_then(|p| d.listed_trees(p)) else { return };
        let trees: Vec<String> = trees.into_iter().filter(|w| !w.main).map(|w| w.path).collect();
        let now = Instant::now();
        let checks = [Checks { passed: 4, ..Checks::default() }, Checks { passed: 2, failed: 1, ..Checks::default() }, Checks { passed: 1, pending: 2, ..Checks::default() }, Checks { passed: 5, ..Checks::default() }];
        for (i, tree) in trees.into_iter().enumerate() {
            let number = 120 + i as u32;
            let state = if i == 3 { PrState::Merged } else { PrState::Open };
            let pr = Pr { number, title: format!("Capture PR {number}"), url: format!("https://github.com/acme/app/pull/{number}"), state, checks: checks[i % 4], ..Pr::default() };
            d.prs.start(&tree, now);
            d.prs.apply(tree, now, Ok(Some(pr)), now);
        }
    }),
    ("pr-loop", |d, _, _| {
        let Some(tree) = d.cwd() else { return };
        let at = crate::util::now_ms() as u64;
        let said = |author: &str, body: &str, mins: u64| Comment { author: author.into(), body: body.into(), at: at - mins * 60_000, hunk: String::new() };
        let thread = |id: &str, line: u32, resolved: bool, comments: Vec<Comment>| Thread { id: id.into(), path: "hooks/use-restore-preview.ts".into(), line: Some(line), resolved, comments, ..Thread::default() };
        let run = |name: &str, outcome: Outcome| Check { name: name.into(), outcome, url: "https://github.com/acme/app/actions/runs/1".into() };
        let pr = Pr {
            number: 128,
            title: "Fix stale terminal reveal".into(),
            url: "https://github.com/acme/app/pull/128".into(),
            checks: Checks { passed: 3, failed: 1, pending: 0 },
            runs: vec![run("lint", Outcome::Passed), run("typecheck", Outcome::Passed), run("test", Outcome::Failed), run("build", Outcome::Passed)],
            review: Review::ChangesRequested,
            reviews: vec![("hoang".into(), Review::ChangesRequested)],
            requested: vec!["linh".into()],
            mergeable: Mergeable::Clean,
            base: "main".into(),
            head: "fix/restore-handoff".into(),
            threads: vec![
                thread("t1", 4, false, vec![said("hoang", "This reads the preview before the session restores, so the first frame is stale.", 40)]),
                thread("t2", 12, false, vec![said("hoang", "Can we drop the timeout here?", 35), said("Minh Ngo", "Dropped it in the next push.", 10)]),
                thread("t3", 18, true, vec![said("linh", "Nit: name it restorePreview.", 90)]),
            ],
            ..Pr::default()
        };
        let now = Instant::now();
        d.prs.start(&tree, now);
        d.prs.apply(tree, now, Ok(Some(pr)), now);
    }),
    ("pr-sent", |d, _, _| {
        let Some((tree, pr)) = d.shown_pr() else { return };
        let pr = pr.clone();
        let failed: Vec<&Check> = pr.runs.iter().filter(|c| c.outcome == Outcome::Failed).collect();
        d.pr.track.send(&tree, &pr, &[], &failed, false);
    }),
    ("pr-comments", |d, _, _| d.pr.track.comments = true),
    ("pr-fold", |d, _, _| d.pr.track.folded = true),
    ("pr-diff", |d, _, cx| {
        let first = d.shown_pr().and_then(|(_, pr)| pr.threads.first().map(|t| t.id.clone()));
        if let Some(id) = first {
            d.view_thread(&id, cx);
        }
    }),
    ("names", |d, _, _| {
        let tree = d.project.as_ref().and_then(|p| d.listed_trees(p)).into_iter().flatten().find(|w| !w.main).map(|w| w.path);
        if let Some(tree) = tree {
            d.agents.names.insert(tree, "Fix the login form".into());
        }
    }),
    ("project-menu", |d, _, _| d.row_menu = d.project.clone().map(RowMenu::Project)),
    ("import", |d, _, cx| {
        if let Some(p) = d.project.clone() {
            d.import_worktrees(&p, cx);
        }
    }),
    ("locals", |d, _, cx| {
        let Some(project) = d.project.clone() else { return };
        for (id, name) in [("local-review", "Review"), ("local-3", "Local 3")] {
            d.agents.add_local(agents::locals::Local { id: id.into(), project: project.clone(), name: name.into() });
        }
        d.select_tree(project, Some("local-review".into()), cx);
    }),
    ("local-menu", |d, _, _| d.row_menu = d.project.clone().map(|project| RowMenu::Local { project, local: "local-review".into() })),
    ("rename-local", |d, window, cx| d.start_rename("local-review".into(), window, cx)),
    ("delete-local", |d, _, cx| d.ask_delete_local("local-review".into(), cx)),
    ("form-checkout", |d, _, cx| d.toggle_picker(Picker::Checkout, cx)),
    ("form-project", |d, _, cx| d.toggle_picker(Picker::Project, cx)),
    ("new-place", |d, window, cx| {
        if let Some(p) = d.project.clone() {
            d.new_place(p, window, cx);
        }
    }),
    ("rename", |d, window, cx| {
        if let Some(tree) = d.agents.names.keys().next().cloned() {
            d.start_rename(tree, window, cx);
        }
    }),
    ("daemon-down", |d, _, _| d.terminals.disconnected(Instant::now() - crate::terminals::link::HINT_AFTER)),
    ("service-off", |d, _, _| d.terminals.link.service = Some(daemon::service::Service::RequiresApproval)),
    ("service-updating", |d, _, _| d.terminals.link.connected("0.2.0", "0.1.0", true, Instant::now())),
    ("service-stale", |d, _, _| d.terminals.link.connected("0.2.0", "0.1.0", true, Instant::now() - crate::terminals::link::STALE_AFTER)),
];

/// Steps that drive real input through the window, each taking its next argument from the environment:
/// `key` from `CAPTURE_KEYS` ("enter;cmd-shift-l"), `type` from `CAPTURE_TYPE` ("text|text"), `click` from
/// `CAPTURE_CLICKS` ("x,y;x,y" in window points), `move` from `CAPTURE_MOVES` (the same), `drop` from `CAPTURE_DROPS` ("x,y,file;…"), `drag` from `CAPTURE_DRAGS` (the same), `paste-image` from `CAPTURE_PASTE` (a PNG).
const INPUT_STEPS: [(&str, Step); 9] = [
    ("key", |_, window, cx| {
        let Some(k) = next_arg("CAPTURE_KEYS", ';').and_then(|k| Keystroke::parse(&k).ok()) else { return };
        window.defer(cx, move |window, cx| _ = window.dispatch_keystroke(k, cx));
    }),
    ("type", |_, window, cx| {
        let Some(text) = next_arg("CAPTURE_TYPE", '|') else { return };
        window.defer(cx, move |window, cx| {
            for c in text.chars() {
                let key = if c == ' ' { "space".to_string() } else { c.to_string() };
                if let Ok(k) = Keystroke::parse(&key) {
                    window.dispatch_keystroke(k, cx);
                }
            }
        });
    }),
    ("click", |_, window, cx| {
        let Some(at) = next_arg("CAPTURE_CLICKS", ';').and_then(|a| point_arg(&a)) else { return };
        window.defer(cx, move |window, cx| {
            let modifiers = Modifiers::default();
            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position: at, pressed_button: None, modifiers }), cx);
            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1, first_mouse: false }), cx);
            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1 }), cx);
        });
    }),
    // A pointer over the window for hover. AppKit drops mouse moves posted to an inactive window, since GPUI's window
    // takes them only through its tracking area, which follows the real cursor.
    ("move", |_, window, cx| {
        let Some(at) = next_arg("CAPTURE_MOVES", ';').and_then(|a| point_arg(&a)) else { return };
        window.defer(cx, move |window, cx| {
            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position: at, pressed_button: None, modifiers: Modifiers::default() }), cx);
        });
    }),
    // A Finder drop, but ending in a plain mouse up: the platform's Submit would activate the app.
    ("drop", |_, window, cx| {
        let Some(arg) = next_arg("CAPTURE_DROPS", ';') else { return };
        let Some((at, file)) = arg.rsplit_once(',').and_then(|(at, file)| Some((point_arg(at)?, PathBuf::from(file)))) else { return };
        window.defer(cx, move |window, cx| {
            window.dispatch_event(PlatformInput::FileDrop(FileDropEvent::Entered { position: at, paths: ExternalPaths(vec![file].into()) }), cx);
            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: at, modifiers: Modifiers::default(), click_count: 1 }), cx);
        });
    }),
    // A Finder drag held over a point, not yet dropped.
    ("drag", |_, window, cx| {
        let Some(arg) = next_arg("CAPTURE_DRAGS", ';') else { return };
        let Some((at, file)) = arg.rsplit_once(',').and_then(|(at, file)| Some((point_arg(at)?, PathBuf::from(file)))) else { return };
        window.defer(cx, move |window, cx| _ = window.dispatch_event(PlatformInput::FileDrop(FileDropEvent::Entered { position: at, paths: ExternalPaths(vec![file].into()) }), cx));
    }),
    // Pastes the PNG with cmd-v, then puts back what the clipboard held.
    ("paste-image", |_, window, cx| {
        let Some(bytes) = next_arg("CAPTURE_PASTE", ';').and_then(|p| std::fs::read(p).ok()) else { return };
        let held = cx.read_from_clipboard();
        cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, bytes)));
        window.defer(cx, move |window, cx| {
            if let Ok(k) = Keystroke::parse("cmd-v") {
                window.dispatch_keystroke(k, cx);
            }
            if let Some(held) = held {
                cx.write_to_clipboard(held);
            }
        });
    }),
    ("pause", |_, _, _| {}),
    // The fixture's clock override: the shown chat's sends age past their two minutes.
    ("chat-age-sends", |d, _, cx| {
        let Some((id, working)) = d.session().and_then(|a| d.agents.get(a)).map(|a| (a.id.clone(), crate::status::Status::of(a) == Some(crate::status::Status::Working))) else { return };
        if let Some(v) = d.chats.views.get_mut(&id) {
            v.session.outgoing.settle(&v.session.transcript, working, crate::util::now_ms() + 121_000);
            cx.notify();
        }
    }),
];

thread_local! {
    static TAKEN: std::cell::RefCell<std::collections::HashMap<&'static str, usize>> = Default::default();
}

/// The next of `var`'s `sep`-separated arguments.
fn next_arg(var: &'static str, sep: char) -> Option<String> {
    let all = std::env::var(var).ok()?;
    let n = TAKEN.with(|t| {
        let mut t = t.borrow_mut();
        let n = t.entry(var).or_default();
        *n += 1;
        *n - 1
    });
    all.split(sep).nth(n).map(str::to_string)
}

fn point_arg(s: &str) -> Option<Point<Pixels>> {
    let (x, y) = s.split_once(',')?;
    Some(point(px(x.trim().parse().ok()?), px(y.trim().parse().ok()?)))
}

fn session(d: &mut Desktop, window: &mut Window, cx: &mut Context<Desktop>) {
    if let Some(card) = d.project.clone().and_then(|p| d.cards(&p).into_iter().min_by_key(|c| c.status != Status::NeedsYou)) {
        d.focus_agent(&card.id, window, cx);
    }
}

fn is_call(b: &Block) -> bool {
    matches!(b, Block::Tool(c) if !matches!(c.kind, ToolKind::Agent(_)))
}

/// A call that opens: a failed one to its error, an edit to its diff.
fn opens(b: &Block) -> bool {
    matches!(b, Block::Tool(c) if is_call(b) && (c.status == chat::ToolStatus::Failed || tool_row::colour(c).is_some()))
}

/// Opens the shown chat's first phase holding a block `pick` takes, and the fold over it, and returns that block.
fn open_phase(d: &mut Desktop, pick: impl Fn(&Block) -> bool, cx: &mut Context<Desktop>) -> Option<(Entity<Tools>, Block)> {
    let agent = d.session()?.to_string();
    let view = d.chats.views.get(&agent)?;
    let s = &view.session;
    let (ix, block) = s.rows.iter().enumerate().find_map(|(ix, row)| match row {
        Row::Phase { turn, blocks } => s.transcript.turns[*turn].blocks[blocks.clone()].iter().find(|b| pick(b)).map(|b| (ix, b.clone())),
        _ => None,
    })?;
    let (phase, open, tools) = (s.ids[ix].clone(), s.open(&s.ids[ix], false), view.tools.clone());
    d.chat_reveal(&agent, ix, cx);
    if !open {
        d.chat_toggle(&agent, &phase, false, cx);
    }
    Some((tools, block))
}

/// Opens the shown chat's first sub-agent whose call `pick` takes.
fn open_agent(d: &mut Desktop, pick: fn(&chat::ToolCall) -> bool, cx: &mut Context<Desktop>) {
    let agent_call = |b: &Block| matches!(b, Block::Tool(c) if matches!(c.kind, ToolKind::Agent(_)) && pick(c));
    let Some((tools, Block::Tool(c))) = open_phase(d, agent_call, cx) else { return };
    let failed = c.status == chat::ToolStatus::Failed;
    tools.update(cx, |t, cx| {
        if !t.details.open(&c.id, failed) {
            t.toggle(&c.id, failed, None, cx);
        }
    });
}

/// The shown chat's last code block row.
fn last_code(d: &Desktop) -> Option<usize> {
    let s = &d.chats.views.get(d.session()?)?.session;
    s.rows.iter().rposition(|row| {
        let Row::Text { turn, block, range } = row else { return false };
        let text = match s.transcript.turns.get(*turn).and_then(|t| t.blocks.get(*block)) {
            Some(Block::Text(m)) => m.text.get(range.clone()),
            _ => None,
        };
        text.and_then(crate::chat::markdown::fence).is_some()
    })
}

/// The shown chat's agent and its first send held in the queue.
fn first_queued(d: &Desktop) -> Option<(String, u64)> {
    let agent = d.session()?.to_string();
    let send = d.chats.views.get(&agent)?.session.outgoing.list().iter().find(|p| p.delivery == chat::Delivery::Queued)?.id;
    Some((agent, send))
}

/// Asks the fixture's pocketd to play `scenario` into the shown agent's timeline: a `.ui-review/fixture/chat/<scenario>.json`, or an ask.
fn fixture(d: &mut Desktop, scenario: &str) {
    if let Some(agent) = d.session().map(str::to_string) {
        d.outbox.prompt(&agent, &format!("/fixture {scenario}"));
    }
}

/// `pocket-desktop --capture <dir> <name>=<step>,<step> …` renders each screen in an off-screen,
/// unfocused window, writes `<dir>/impl-<name>.png` and quits.
pub struct Capture {
    dir: PathBuf,
    screens: Vec<(String, Vec<Step>)>,
}

/// Two Claude Code accounts on this Mac, one at its limit, and a Codex login nobody signed in to.
fn accounts_sample() -> Vec<daemon::Account> {
    use daemon::{Account, Usage, Window};
    let now = crate::util::now_ms();
    let usage = |session: f64, week: f64| Some(Usage { session: Some(Window { percent: session, resets_at: now + 2 * 3_600_000 }), week: Some(Window { percent: week, resets_at: now + 4 * 86_400_000 }), at: now, stale: false });
    let claude = |id: &str| Account { id: id.into(), provider: "claude".into(), signed_in: true, ..Default::default() };
    vec![
        Account { email: "me@example.com".into(), plan: "Max".into(), default: true, usage: usage(42., 81.), ..claude("builtin") },
        Account { label: "Work".into(), email: "work@example.com".into(), plan: "Team".into(), running: 2, usage: usage(100., 64.), ..claude("work") },
        Account { id: "builtin".into(), provider: "codex".into(), default: true, ..Default::default() },
    ]
}

fn fail(msg: &str) -> ! {
    eprintln!("pocket-desktop: {msg}");
    std::process::exit(2)
}

impl Capture {
    pub fn from_args() -> Option<Self> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let (flag, rest) = args.split_first()?;
        if flag != "--capture" {
            return None;
        }
        let Some((dir, specs)) = rest.split_first() else { fail("usage: --capture <dir> <name>=<step>,<step> …") };
        let screens = specs
            .iter()
            .map(|spec| {
                let (name, steps) = spec.split_once('=').unwrap_or((spec, ""));
                let steps = steps.split(',').filter(|s| !s.is_empty()).map(|s| {
                    STEPS.iter().chain(&INPUT_STEPS).find(|(n, _)| *n == s).map(|(_, f)| *f).unwrap_or_else(|| {
                        let known: Vec<&str> = STEPS.iter().chain(&INPUT_STEPS).map(|(n, _)| *n).collect();
                        fail(&format!("unknown step '{s}' in {spec}; known: {}", known.join(", ")))
                    })
                });
                (name.to_string(), steps.collect())
            })
            .collect();
        Some(Self { dir: dir.into(), screens })
    }

    /// Never ordered on screen; parked far away too, so the real cursor can't trigger hover styles.
    /// With `CAPTURE_HOLD` set it is shown instead, still unfocused, so it can be measured and driven from outside.
    pub fn hide(opts: &mut WindowOptions) {
        if holding() {
            opts.focus = false;
            return;
        }
        if let Some(WindowBounds::Windowed(bounds)) = &mut opts.window_bounds {
            bounds.origin = point(px(-10000.), px(-10000.));
        }
        opts.show = false;
        opts.focus = false;
    }

    pub fn run(self, window: WindowHandle<Root>, cx: &mut App) {
        let desktop = window.read(cx).ok().and_then(|r| r.view().clone().downcast::<Desktop>().ok()).expect("desktop root");
        let window = AnyWindowHandle::from(window);
        desktop.update(cx, |d, _| d.capturing = true);
        cx.spawn(async move |cx| {
            settle(cx, window, &desktop, 1500).await;
            for (name, steps) in &self.screens {
                for step in std::iter::once(reset as Step).chain(steps.iter().copied()) {
                    cx.update_window(window, |_, window, cx| desktop.update(cx, |d, cx| step(d, window, cx))).ok();
                    settle(cx, window, &desktop, 300).await;
                }
                settle(cx, window, &desktop, 300).await;
                let path = self.dir.join(format!("impl-{name}.png"));
                match cx.update_window(window, |_, window, cx| shoot(window, cx, &path)) {
                    Ok(Ok(())) => println!("{}", path.display()),
                    Ok(Err(e)) => eprintln!("{name}: {e}"),
                    Err(e) => eprintln!("{name}: {e}"),
                }
            }
            if !holding() {
                cx.update(|cx| cx.quit());
            }
        })
        .detach();
    }
}

/// Scrolls the shown chat to `rows` rows after its `n`th prompt (before, if negative). By row, not by pixels: the list
/// knows only measured heights, so a pixel scroll lands wherever the rows it skips were never laid out.
fn scroll_from_prompt(d: &mut Desktop, n: usize, rows: isize, cx: &mut Context<Desktop>) {
    let Some(v) = d.session().and_then(|a| d.chats.views.get(a)) else { return };
    let Some(ix) = v.session.rows.iter().enumerate().filter(|(_, r)| matches!(r, Row::Prompt { .. })).nth(n).map(|(ix, _)| ix) else { return };
    v.list.scroll_to(ListOffset { item_ix: ix.saturating_add_signed(rows), offset_in_item: px(0.) });
    cx.notify();
}

/// Swipes the shown chat's `row` sideways by `dx` px.
fn swipe(d: &mut Desktop, row: fn(&Desktop) -> Option<usize>, dx: f32, window: &mut Window, cx: &mut Context<Desktop>) {
    let Some(v) = d.session().and_then(|a| d.chats.views.get(a)) else { return };
    let Some(at) = row(d).and_then(|ix| v.list.bounds_for_item(ix)).map(|b| b.center()) else { return };
    window.defer(cx, move |window, cx| {
        window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position: at, pressed_button: None, modifiers: Modifiers::default() }), cx);
        let delta = ScrollDelta::Pixels(point(px(dx), px(0.)));
        window.dispatch_event(PlatformInput::ScrollWheel(ScrollWheelEvent { position: at, delta, ..Default::default() }), cx);
    });
}

/// The last markdown block of the first reply.
fn first_reply_end(d: &Desktop) -> Option<usize> {
    let rows = &d.chats.views.get(d.session()?)?.session.rows;
    rows.iter().rposition(|r| matches!(r, Row::Text { turn: 0, .. }))
}

/// `CAPTURE_HOLD=1` keeps the window shown and the app running after the last screen.
fn holding() -> bool {
    std::env::var_os("CAPTURE_HOLD").is_some()
}

fn reset(d: &mut Desktop, window: &mut Window, cx: &mut Context<Desktop>) {
    d.cancel_chat(window, cx);
    d.store.chats.clear();
    d.set_chat_text("", window, cx);
    d.close_overlay(window, cx);
    d.automations.reset();
    (d.screen, d.side) = (Screen::Sessions, Side::Sessions);
    d.settings.reset(window, cx);
    d.settings.menu = None;
    (d.layout, d.widths, d.sidebar.panel_open, d.panels.menu) = (Layout::Sidebars, [None; 2], false, None);
    (d.worktree, d.terminal.focused) = (None, None);
    for p in d.preview.panes.values_mut() {
        p.file = None;
    }
    for v in d.diff.panes.values_mut() {
        (v.file, v.at) = (None, At::Working);
    }
    for c in d.commit.panes.values_mut() {
        c.sha = None;
    }
    d.workspaces.clear();
    d.creates.list.clear();
    d.graph = GraphState::default();
    d.pr.track.comments = false;
    d.pr.track.folded = false;
    d.agents.names.clear();
    d.agents.locals.clear();
    d.sidebar.rename = None;
    d.updates.update = Update::Idle;
    d.terminals.link.up();
    d.terminals.link.service = None;
    d.set_appearance(WindowAppearance::Light, window, cx);
    syntax(d, store::SyntaxTheme::Graphite, cx);
    (d.store.appearance.added, d.store.appearance.removed) = (None, None);
    theme::set_diff_colors(None, None);
    (d.store.appearance.accent, d.store.appearance.ui_font, d.store.appearance.code_font, d.store.appearance.code_size) = (None, None, None, None);
    theme::set_fonts(None, None, cx);
    theme::set_accent(None, cx);
}

/// Waits for the latest git refresh to land, then lays out a frame: nothing else draws a hidden window.
async fn settle(cx: &mut AsyncApp, window: AnyWindowHandle, desktop: &Entity<Desktop>, ms: u64) {
    cx.background_executor().timer(Duration::from_millis(ms)).await;
    for _ in 0..50 {
        if desktop.read_with(cx, |d, _| d.git_done == d.git_run && !d.graph.busy()) {
            break;
        }
        cx.background_executor().timer(Duration::from_millis(100)).await;
    }
    cx.update_window(window, |_, window, cx| draw(window, cx)).ok();
}

fn draw(window: &mut Window, cx: &mut App) {
    window.refresh();
    window.draw(cx).clear(cx);
}

fn shoot(window: &mut Window, cx: &mut App, path: &Path) -> Result<(), String> {
    draw(window, cx);
    save(window, path)
}

#[cfg(feature = "capture")]
fn save(window: &Window, path: &Path) -> Result<(), String> {
    let image = window.render_to_image().map_err(|e| e.to_string())?;
    image.save(path).map_err(|e| e.to_string())
}

#[cfg(not(feature = "capture"))]
fn save(_: &Window, _: &Path) -> Result<(), String> {
    Err("rebuild with --features capture".into())
}

fn syntax(d: &mut Desktop, palette: store::SyntaxTheme, cx: &mut Context<Desktop>) {
    d.store.appearance.syntax = palette;
    theme::set_syntax(palette as usize, cx);
}
