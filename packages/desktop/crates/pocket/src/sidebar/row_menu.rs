use crate::accounts;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, RowMenu, id};
use crate::modals::add_project::icon::EMOJI;
use crate::sidebar::sections::emoji_mark;
use crate::terminals::link;
use agents::Summary;
use agents::locals::is_local;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::{Value, json};
use store::LaunchPick;
use store::prefs::agents::{Agents, Args};
use store::sections::Pin;
use theme::*;
use ui::{self, icon_button_sized};
use workspace::Place;

#[derive(Debug, PartialEq)]
enum SessionItem {
    Pin(bool),
    CopyPath,
    CopyResume(String),
    Fork,
    /// `true` while its handoff is underway, to bring the sheet back.
    Handoff(bool),
    Close,
}

fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The shell line that reopens `session_id` with its provider's command and resume args, or `None` for a provider that can't.
/// The args are shell words as typed, the way pocketd splits them.
fn resume_command(prefs: &Agents, provider: &str, session_id: &str, cwd: &str) -> Option<String> {
    let args = prefs.args(provider, Args::Resume);
    (LaunchPick::PROVIDERS.contains(&provider) && !args.is_empty())
        .then(|| format!("cd {} && {} {args} {}", quoted(cwd), quoted(&prefs.command(provider)), quoted(session_id)))
}

/// Whether `s` can be forked: pocketd takes a fork (`offered`), and its agent has a session and fork args to copy it with.
fn offers_fork(s: &Summary, observe: bool, prefs: &Agents, offered: bool) -> bool {
    offered
        && !observe
        && s.provider_session_id.is_some()
        && LaunchPick::PROVIDERS.contains(&s.provider.as_str())
        && !prefs.args(&s.provider, Args::Fork).is_empty()
}

/// `fork` is whether pocketd takes a fork; `handoff`, whether it can be handed off and whether that is underway.
fn session_items(s: &Summary, observe: bool, prefs: &Agents, fork: bool, handoff: Option<bool>) -> Vec<SessionItem> {
    let resume = s.provider_session_id.as_deref().and_then(|id| resume_command(prefs, &s.provider, id, &s.cwd));
    let fork = offers_fork(s, observe, prefs, fork);
    let pin = (!observe).then_some(SessionItem::Pin(!s.pinned));
    let close = (!observe).then_some(SessionItem::Close);
    [pin, Some(SessionItem::CopyPath), resume.map(SessionItem::CopyResume), fork.then_some(SessionItem::Fork), handoff.map(SessionItem::Handoff), close]
        .into_iter()
        .flatten()
        .collect()
}

/// A create that starts a copy of `s` in its folder, with its model, effort and account.
/// `efforts` are what the session's model takes; `in_terminal` runs the copy in its agent's TUI even where Settings would start a chat.
fn fork_spec(s: &Summary, project: &str, prefs: &Agents, live: &agents::Agents, efforts: &[String], in_terminal: bool) -> Option<Value> {
    let account = if s.account.is_empty() { accounts::BUILT_IN.into() } else { s.account.clone() };
    let pick = LaunchPick {
        provider: s.provider.clone(),
        model: s.model.clone().unwrap_or_default(),
        effort: s.effort.clone().unwrap_or_default(),
        account,
        ..LaunchPick::default()
    };
    let mut spec = pick.spec(project, json!({"worktree": s.cwd}), "");
    spec["fork"] = s.provider_session_id.clone()?.into();
    prefs.launch(&mut spec, live, efforts);
    if in_terminal {
        spec["chat"] = false.into();
    }
    Some(spec)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MenuAction {
    ProjectSettings(String),
    Keep(String),
    Pin(Pin),
    Import(String),
    SetGroup { project: String, group: Option<u32> },
    NewGroup(Option<String>),
    RemoveProject(String),
    Rename(String),
    RenameGroup(u32),
    DeleteWorktree { project: String, tree: String },
    DeleteLocal(String),
    DeleteScratch(String),
    GroupIcon { group: u32, emoji: &'static str },
    DeleteGroup(u32),
    SessionPin { id: String, pin: bool },
    CopyPath(String),
    CopyResume(String),
    Fork(String),
    Handoff(String),
    Close(String),
}

#[derive(Clone, PartialEq)]
pub(crate) enum MenuItem {
    Row { id: &'static str, icon: &'static str, label: String, action: MenuAction, danger: bool },
    Divider,
    Label(&'static str),
    Pick { id: String, lead: Option<String>, label: String, chosen: bool, action: MenuAction },
    Icons { group: u32, chosen: Option<String> },
}

#[derive(Clone, PartialEq)]
pub(crate) struct MenuFrame {
    pub(crate) menu: RowMenu,
    pub(crate) at: Option<Point<Pixels>>,
    pub(crate) items: Vec<MenuItem>,
}

#[derive(Clone)]
pub(crate) enum MenuEvent {
    Button(RowMenu),
    Context(RowMenu, Point<Pixels>),
    Shut,
    Pick(MenuAction),
}

pub(crate) type Send<V> = fn(&mut V, MenuEvent, &mut Window, &mut Context<V>);

pub(crate) fn open_on_right_click<V: 'static>(menu: RowMenu, send: Send<V>, cx: &mut Context<V>) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |this, e: &MouseDownEvent, window, cx| send(this, MenuEvent::Context(menu.clone(), e.position), window, cx))
}

pub(crate) fn menu_button<V: 'static>(key: &str, menu: RowMenu, shown: Option<&MenuFrame>, send: Send<V>, cx: &mut Context<V>) -> AnyElement {
    let frame = shown.filter(|f| f.menu == menu);
    let opens = menu;
    let button = ui::popover_trigger(
        icon_button_sized(id(format!("aside-more:{key}")), "more", 22., TEXT_3).rounded(px(6.)),
        frame.is_some(),
        cx.listener(move |this, _: &MouseDownEvent, window, cx| send(this, MenuEvent::Button(opens.clone()), window, cx)),
    );
    div().relative().child(button).when_some(frame, |d, f| d.child(menu_pop(f, send, cx))).into_any_element()
}

fn menu_pop<V: 'static>(frame: &MenuFrame, send: Send<V>, cx: &mut Context<V>) -> AnyElement {
    let items: Vec<AnyElement> = frame.items.iter().map(|item| menu_item(item, send, cx)).collect();
    let pop = ui::menu_in(
        "aside-menu-in",
        ui::pop(div().id("aside-menu"))
            .w(px(210.))
            .p(px(6.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, window, cx| send(this, MenuEvent::Shut, window, cx)))
            .children(items),
    );
    match frame.at {
        Some(at) => deferred(anchored().position(at).snap_to_window_with_margin(px(8.)).child(pop)).with_priority(1).into_any_element(),
        None => ui::dropdown(26., pop).into_any_element(),
    }
}

fn picks<V: 'static>(action: &MenuAction, send: Send<V>, cx: &mut Context<V>) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let action = action.clone();
    cx.listener(move |this, _: &ClickEvent, window, cx| send(this, MenuEvent::Pick(action.clone()), window, cx))
}

fn menu_item<V: 'static>(item: &MenuItem, send: Send<V>, cx: &mut Context<V>) -> AnyElement {
    match item {
        MenuItem::Row { id, icon, label, action, danger } => {
            let row = if *danger { ui::danger_row(*id, icon, label) } else { ui::menu_row(*id, icon, label, None) };
            row.on_click(picks(action, send, cx)).into_any_element()
        }
        MenuItem::Divider => ui::menu_divider().into_any_element(),
        MenuItem::Label(text) => ui::menu_label(text).into_any_element(),
        MenuItem::Pick { id, lead, label, chosen, action } => {
            ui::menu_pick(SharedString::from(id.clone()), div().children(lead.clone().map(emoji_mark)), label, *chosen)
                .on_click(picks(action, send, cx))
                .into_any_element()
        }
        MenuItem::Icons { group, chosen } => {
            let icons = EMOJI.iter().enumerate().map(|(i, &e)| {
                let action = MenuAction::GroupIcon { group: *group, emoji: e };
                div()
                    .id(("aside-menu-icon", i))
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .text_size(px(16.))
                    .cursor_pointer()
                    .hover(|d| d.bg(FILL_2))
                    .when(chosen.as_deref() == Some(e), |d| d.bg(FILL_3))
                    .child(e)
                    .on_click(picks(&action, send, cx))
            });
            div().py(px(4.)).flex().flex_wrap().children(icons).into_any_element()
        }
    }
}

fn row(id: &'static str, icon: &'static str, label: &str, action: MenuAction) -> MenuItem {
    MenuItem::Row { id, icon, label: label.to_string(), action, danger: false }
}

fn danger(id: &'static str, icon: &'static str, label: &str, action: MenuAction) -> MenuItem {
    MenuItem::Row { id, icon, label: label.to_string(), action, danger: true }
}

impl Desktop {
    pub(crate) fn can_fork(&self, s: &Summary) -> bool {
        offers_fork(s, self.agents.observe_only(), &self.store.agents, self.agents.fork_offered())
    }

    /// Starts a copy of agent `id`'s session; with a `terminal` place, in its agent's TUI with its tab there (see `fork_spec`).
    pub(crate) fn fork_session(&mut self, id: &str, terminal: Option<Place>, cx: &mut Context<Self>) {
        if self.terminals.read(cx).link.stale().is_some() {
            self.fail(link::UPDATING, cx);
            return cx.notify();
        }
        let projects = self.projects(cx);
        let Some(s) = self.agents.get(id) else { return };
        let Some(spec) = self.agent_project(s, &s.cwd, &projects).and_then(|p| {
            fork_spec(s, p, &self.store.agents, &self.agents, &self.efforts_on(&s.provider, s.model.as_deref().unwrap_or_default()), terminal.is_some())
        }) else {
            return;
        };
        if let (Some(request), Some(place)) = (self.send_create(spec, cx), terminal) {
            self.terminals.update(cx, |t, _| _ = t.tab_terms.insert(request, place));
        }
        cx.notify();
    }

    fn ask_close_session(&mut self, id: String, cx: &mut Context<Self>) {
        self.ask(Confirm::CloseSession(id), cx);
        cx.notify();
    }

    pub(crate) fn menu_frame(&self, cx: &App) -> Option<MenuFrame> {
        let menu = self.row_menu.clone()?;
        let at = self.sidebar.read(cx).menu_at;
        let items = self.row_menu_items(&menu, cx);
        Some(MenuFrame { menu, at, items })
    }

    pub(crate) fn on_menu_event(&mut self, ev: MenuEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            MenuEvent::Button(menu) => {
                self.sidebar.update(cx, |s, cx| {
                    s.hover.press();
                    s.menu_at = None;
                    cx.notify();
                });
                self.row_menu = Some(menu);
                self.panels.update(cx, |p, cx| p.close_tab_menus(cx));
            }
            MenuEvent::Context(menu, at) => {
                self.close_menus(cx);
                self.row_menu = Some(menu);
                self.sidebar.update(cx, |s, cx| {
                    s.menu_at = Some(at);
                    cx.notify();
                });
            }
            MenuEvent::Shut => {
                self.row_menu = None;
                self.sidebar.update(cx, |s, cx| {
                    s.menu_at = None;
                    cx.notify();
                });
            }
            MenuEvent::Pick(action) => return self.menu_action(action, window, cx),
        }
        cx.notify();
    }

    fn menu_action(&mut self, action: MenuAction, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(action, MenuAction::ProjectSettings(_) | MenuAction::GroupIcon { .. }) {
            self.row_menu = None;
        }
        match action {
            MenuAction::ProjectSettings(p) => {
                self.select_project(p, cx);
                self.project_settings(&crate::actions::ProjectSettings, window, cx);
                return;
            }
            MenuAction::Keep(p) => return self.keep_project(&p, cx),
            MenuAction::Pin(pin) => {
                self.store.sections.toggle_pin(pin);
                self.store.save();
            }
            MenuAction::Import(p) => return self.import_worktrees(&p, cx),
            MenuAction::SetGroup { project, group } => {
                self.store.sections.set_group(&project, group);
                self.store.save();
            }
            MenuAction::NewGroup(project) => return self.new_group(project, window, cx),
            MenuAction::RemoveProject(p) => return self.ask_remove_project(p, cx),
            MenuAction::Rename(tree) => return self.start_rename(tree, window, cx),
            MenuAction::RenameGroup(id) => return self.start_group_rename(id, window, cx),
            MenuAction::DeleteWorktree { project, tree } => return self.ask_delete_worktree(project, tree, cx),
            MenuAction::DeleteLocal(local) => return self.ask_delete_local(local, cx),
            MenuAction::DeleteScratch(path) => return self.ask_delete_scratch(path, cx),
            MenuAction::GroupIcon { group, emoji } => {
                self.store.sections.pick_group_icon(group, emoji);
                self.store.save();
            }
            MenuAction::DeleteGroup(id) => {
                self.store.sections.delete_group(id);
                self.store.save();
            }
            MenuAction::SessionPin { id, pin } => self.outbox.pin(&id, pin),
            MenuAction::CopyPath(text) | MenuAction::CopyResume(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            MenuAction::Fork(id) => return self.fork_session(&id, None, cx),
            MenuAction::Handoff(id) => return self.open_handoff(&id, window, cx),
            MenuAction::Close(id) => return self.ask_close_session(id, cx),
        }
        cx.notify();
    }

    fn pin_item(&self, pin: Pin) -> MenuItem {
        let label = if self.store.sections.is_pinned(&pin) { "Unpin" } else { "Pin to top" };
        row("aside-menu-pin", "pin", label, MenuAction::Pin(pin))
    }

    /// Moves `p` into a group, out of every group, or into a new one.
    fn group_items(&self, p: &str) -> Vec<MenuItem> {
        let sections = &self.store.sections;
        let current = sections.group_of(p);
        let mut rows = vec![MenuItem::Label("Move to group")];
        let picks = sections.groups_on(hosts::split(p).0).map(|g| (Some(g.id), g.icon.clone(), g.name.clone())).chain([(None, None, "No group".to_string())]);
        for (group, lead, label) in picks {
            rows.push(MenuItem::Pick {
                id: format!("aside-menu-group:{group:?}"),
                lead,
                label,
                chosen: current == group,
                action: MenuAction::SetGroup { project: p.to_string(), group },
            });
        }
        rows.push(row("aside-menu-new-group", "folder-plus", "New group…", MenuAction::NewGroup(Some(p.to_string()))));
        rows
    }

    /// A group's rename, icons and delete.
    fn group_menu_items(&self, id: u32) -> Vec<MenuItem> {
        let Some(group) = self.store.sections.group(id) else { return vec![] };
        vec![
            row("aside-menu-rename", "compose", "Rename…", MenuAction::RenameGroup(id)),
            MenuItem::Icons { group: id, chosen: group.icon.clone() },
            MenuItem::Divider,
            danger("aside-menu-delete", "trash", "Delete group", MenuAction::DeleteGroup(id)),
        ]
    }

    fn row_menu_items(&self, menu: &RowMenu, cx: &App) -> Vec<MenuItem> {
        match menu.clone() {
            RowMenu::Project(p) => {
                let kept = self.store.projects.contains(&p);
                let first = if kept {
                    row("aside-menu-settings", "settings", "Project settings…", MenuAction::ProjectSettings(p.clone()))
                } else {
                    row("aside-menu-keep", "check", "Keep in Zoes", MenuAction::Keep(p.clone()))
                };
                let mut rows = vec![first];
                if kept {
                    rows.push(self.pin_item(Pin::Project(p.clone())));
                }
                let untracked = self.untracked_trees(&p).len();
                if kept && untracked > 0 {
                    let label = if untracked == 1 { "Import 1 worktree".to_string() } else { format!("Import {untracked} worktrees") };
                    rows.push(row("aside-menu-import", "branch", &label, MenuAction::Import(p.clone())));
                }
                if kept {
                    rows.push(MenuItem::Divider);
                    rows.extend(self.group_items(&p));
                }
                rows.extend([
                    MenuItem::Divider,
                    danger("aside-menu-remove", "x", if kept { "Remove from Zoes" } else { "Remove" }, MenuAction::RemoveProject(p)),
                ]);
                rows
            }
            RowMenu::Tree { project, tree } => {
                let mut rows = vec![];
                if self.store.projects.contains(&project) {
                    rows.push(self.pin_item(Pin::Tree { project: project.clone(), tree: tree.clone() }));
                }
                if self.agents.names_offered() {
                    rows.push(row("aside-menu-rename", "compose", "Rename…", MenuAction::Rename(tree.clone())));
                }
                if !rows.is_empty() {
                    rows.push(MenuItem::Divider);
                }
                rows.push(danger("aside-menu-delete", "trash", "Delete worktree…", MenuAction::DeleteWorktree { project, tree }));
                rows
            }
            RowMenu::Local { project, local } => {
                let mut rows = vec![];
                if self.store.projects.contains(&project) {
                    rows.push(self.pin_item(Pin::Tree { project, tree: local.clone() }));
                }
                if self.agents.renames_offered(&local) {
                    rows.push(row("aside-menu-rename", "compose", "Rename…", MenuAction::Rename(local.clone())));
                }
                if is_local(&local) && self.agents.locals_offered_on(agents::locals::host(&local)) {
                    if !rows.is_empty() {
                        rows.push(MenuItem::Divider);
                    }
                    rows.push(danger("aside-menu-delete", "trash", "Delete Local…", MenuAction::DeleteLocal(local)));
                }
                rows
            }
            RowMenu::Scratch(path) => {
                let mut rows = vec![];
                if self.agents.names_offered() {
                    rows.push(row("aside-menu-rename", "compose", "Rename…", MenuAction::Rename(path.clone())));
                    rows.push(MenuItem::Divider);
                }
                rows.push(danger("aside-menu-delete", "trash", "Delete session…", MenuAction::DeleteScratch(path)));
                rows
            }
            RowMenu::Session(id) => {
                let Some(s) = self.agents.get(&id) else { return vec![] };
                let handoff = self.can_hand_off(s).then(|| self.handoff.read(cx).moving(&id));
                let mut rows = vec![];
                for item in session_items(s, self.agents.observe_only(), &self.store.agents, self.agents.fork_offered(), handoff) {
                    match item {
                        SessionItem::Pin(pin) => {
                            rows.push(row("aside-menu-pin", "pin", if pin { "Pin" } else { "Unpin" }, MenuAction::SessionPin { id: id.clone(), pin }))
                        }
                        SessionItem::CopyPath => rows.push(row("aside-menu-copy-path", "copy", "Copy path", MenuAction::CopyPath(s.cwd.clone()))),
                        SessionItem::CopyResume(line) => {
                            rows.push(row("aside-menu-copy-resume", "terminal", "Copy resume command", MenuAction::CopyResume(line)))
                        }
                        SessionItem::Fork => rows.push(row("aside-menu-fork", "branch", "Fork session", MenuAction::Fork(id.clone()))),
                        SessionItem::Handoff(moving) => {
                            rows.push(row("aside-menu-handoff", "server", crate::handoff::menu_label(moving), MenuAction::Handoff(id.clone())))
                        }
                        SessionItem::Close => {
                            rows.push(MenuItem::Divider);
                            rows.push(danger("aside-menu-close", "x", "Close session…", MenuAction::Close(id.clone())));
                        }
                    }
                }
                rows
            }
            RowMenu::Group(id) => self.group_menu_items(id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SessionItem, fork_spec, resume_command, session_items};
    use agents::Summary;
    use serde_json::json;
    use store::prefs::agents::{Agents, Args};

    #[test]
    fn resume_command_quotes_the_cwd() {
        assert_eq!(resume_command(&Agents::default(), "claude", "s1", "/w/it's here").as_deref(), Some(r"cd '/w/it'\''s here' && 'claude' --resume 's1'"));
    }

    #[test]
    fn resume_command_quotes_the_session_id() {
        assert_eq!(resume_command(&Agents::default(), "claude", "s1; rm -rf ~", "/w").as_deref(), Some("cd '/w' && 'claude' --resume 's1; rm -rf ~'"));
    }

    #[test]
    fn codex_resumes_with_the_resume_verb() {
        assert_eq!(resume_command(&Agents::default(), "codex", "s1", "/w").as_deref(), Some("cd '/w' && 'codex' resume 's1'"));
        assert_eq!(resume_command(&Agents::default(), "gemini", "s1", "/w"), None);
    }

    #[test]
    fn resume_command_runs_the_command_and_resume_args_from_settings() {
        let mut prefs = Agents::default();
        let own = prefs.provider_mut("claude");
        (own.command, own.resume_args) = ("/opt/my claude".into(), Some("-r --verbose".into()));
        assert_eq!(resume_command(&prefs, "claude", "s1", "/w").as_deref(), Some("cd '/w' && '/opt/my claude' -r --verbose 's1'"));
        *prefs.provider_mut("claude").args_mut(Args::Resume) = Some(String::new());
        assert_eq!(resume_command(&prefs, "claude", "s1", "/w"), None);
    }

    #[test]
    fn hand_off_sits_after_fork_and_before_close_and_brings_back_one_underway() {
        let s = Summary { provider: "claude".into(), cwd: "/w".into(), provider_session_id: Some("s1".into()), ..Default::default() };
        let items = session_items(&s, false, &Agents::default(), true, Some(false));
        assert_eq!(items[items.len() - 3..], [SessionItem::Fork, SessionItem::Handoff(false), SessionItem::Close]);
        assert!(session_items(&s, false, &Agents::default(), true, Some(true)).contains(&SessionItem::Handoff(true)));
    }

    #[test]
    fn no_resume_item_without_a_provider_session_id() {
        let fresh = Summary { provider: "claude".into(), cwd: "/w".into(), ..Default::default() };
        assert_eq!(session_items(&fresh, false, &Agents::default(), true, None), [SessionItem::Pin(true), SessionItem::CopyPath, SessionItem::Close]);
        let resumable = Summary { provider_session_id: Some("s1".into()), ..fresh };
        assert_eq!(
            session_items(&resumable, false, &Agents::default(), true, None),
            [
                SessionItem::Pin(true),
                SessionItem::CopyPath,
                SessionItem::CopyResume("cd '/w' && 'claude' --resume 's1'".into()),
                SessionItem::Fork,
                SessionItem::Close
            ]
        );
    }

    #[test]
    fn fork_is_offered_only_when_pocketd_takes_it_and_the_agent_has_fork_args() {
        let s = Summary { provider: "codex".into(), cwd: "/w".into(), provider_session_id: Some("s1".into()), ..Default::default() };
        let forks = |prefs: &Agents, offered: bool, observe: bool| session_items(&s, observe, prefs, offered, None).contains(&SessionItem::Fork);
        let mut prefs = Agents::default();
        assert!(forks(&prefs, true, false));
        assert!(!forks(&prefs, false, false));
        assert!(!forks(&prefs, true, true));
        *prefs.provider_mut("codex").args_mut(Args::Fork) = Some(String::new());
        assert!(!forks(&prefs, true, false));
    }

    #[test]
    fn a_fork_starts_in_the_session_s_folder_with_its_model_and_effort() {
        let s = Summary {
            provider: "claude".into(),
            cwd: "/w/tree".into(),
            provider_session_id: Some("s1".into()),
            model: Some("opus".into()),
            effort: Some("high".into()),
            ..Default::default()
        };
        let claude = agents::providers::builtin_info("claude");
        let live = agents::Agents::default();
        let spec = fork_spec(&s, "/w", &Agents::default(), &live, &claude.efforts, false).unwrap();
        assert_eq!((&spec["project"], &spec["checkout"], &spec["fork"]), (&json!("/w"), &json!({"worktree": "/w/tree"}), &json!("s1")));
        assert_eq!((&spec["provider"], &spec["model"], &spec["effort"]), (&json!("claude"), &json!("opus"), &json!("high")));
        assert!(spec.get("prompt").is_none());
        assert_eq!(fork_spec(&Summary { provider_session_id: None, ..s }, "/w", &Agents::default(), &live, &claude.efforts, false), None);
    }

    #[test]
    fn a_fork_runs_on_the_account_its_session_runs_on() {
        let s = Summary { provider: "codex".into(), cwd: "/w".into(), provider_session_id: Some("s1".into()), account: "work".into(), ..Default::default() };
        let codex = agents::providers::builtin_info("codex");
        let live = agents::Agents::default();
        assert_eq!(fork_spec(&s, "/w", &Agents::default(), &live, &codex.efforts, false).unwrap()["account"], json!("work"));
        let built_in = Summary { account: String::new(), ..s };
        assert_eq!(fork_spec(&built_in, "/w", &Agents::default(), &live, &codex.efforts, false).unwrap()["account"], json!("builtin"));
    }

    #[test]
    fn a_fork_for_a_terminal_starts_no_chat_whatever_settings_say() {
        let s = Summary { provider: "pi".into(), cwd: "/w".into(), provider_session_id: Some("s1".into()), ..Default::default() };
        let prefs = Agents { start_as_chat: true, ..Agents::default() };
        let live = agents::Agents::default();
        assert_eq!(fork_spec(&s, "/w", &prefs, &live, &[], false).unwrap()["chat"], json!(true));
        assert_eq!(fork_spec(&s, "/w", &prefs, &live, &[], true).unwrap()["chat"], json!(false));
    }

    #[test]
    fn a_pinned_session_offers_unpin() {
        let s = Summary { provider: "claude".into(), cwd: "/w".into(), pinned: true, ..Default::default() };
        assert_eq!(session_items(&s, false, &Agents::default(), true, None)[0], SessionItem::Pin(false));
    }

    #[test]
    fn an_observer_is_not_offered_close_session() {
        let s = Summary { provider: "claude".into(), cwd: "/w".into(), ..Default::default() };
        assert_eq!(session_items(&s, true, &Agents::default(), true, None), [SessionItem::CopyPath]);
    }
}
