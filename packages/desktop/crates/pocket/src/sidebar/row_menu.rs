use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, RowMenu, id};
use crate::terminals::link;
use crate::util::LOCAL_ICON;
use agents::Summary;
use agents::locals::is_local;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::{Value, json};
use store::LaunchPick;
use store::prefs::agents::{Agents, Args};
use theme::*;
use ui::{self, icon_button_sized};

#[derive(Debug, PartialEq)]
enum SessionItem {
    Pin(bool),
    CopyPath,
    CopyResume(String),
    Fork,
    Close,
}

fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The shell line that reopens `session_id` with its provider's command and resume args, or `None` for a provider that can't.
/// The args are shell words as typed, the way pocketd splits them.
fn resume_command(prefs: &Agents, provider: &str, session_id: &str, cwd: &str) -> Option<String> {
    let args = prefs.args(provider, Args::Resume);
    (LaunchPick::PROVIDERS.contains(&provider) && !args.is_empty()).then(|| format!("cd {} && {} {args} {}", quoted(cwd), quoted(&prefs.command(provider)), quoted(session_id)))
}

/// `fork` is whether pocketd takes a fork.
fn session_items(s: &Summary, observe: bool, prefs: &Agents, fork: bool) -> Vec<SessionItem> {
    let resume = s.provider_session_id.as_deref().and_then(|id| resume_command(prefs, &s.provider, id, &s.cwd));
    let fork = fork && !observe && s.provider_session_id.is_some() && LaunchPick::PROVIDERS.contains(&s.provider.as_str()) && !prefs.args(&s.provider, Args::Fork).is_empty();
    let pin = (!observe).then_some(SessionItem::Pin(!s.pinned));
    let close = (!observe).then_some(SessionItem::Close);
    [pin, Some(SessionItem::CopyPath), resume.map(SessionItem::CopyResume), fork.then_some(SessionItem::Fork), close].into_iter().flatten().collect()
}

/// A create that starts a copy of `s` in its folder, with its model and effort.
fn fork_spec(s: &Summary, project: &str, prefs: &Agents) -> Option<Value> {
    let pick = LaunchPick { provider: s.provider.clone(), model: s.model.clone().unwrap_or_default(), effort: s.effort.clone().unwrap_or_default(), access: String::new() };
    let mut spec = pick.spec(project, json!({"worktree": s.cwd}), "");
    spec["fork"] = s.provider_session_id.clone()?.into();
    prefs.launch(&mut spec);
    Some(spec)
}

impl Desktop {
    pub(super) fn open_row_menu(menu: RowMenu, cx: &mut Context<Self>) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, e: &MouseDownEvent, _, cx| {
            this.close_menus();
            this.row_menu = Some(menu.clone());
            this.sidebar.menu_at = Some(e.position);
            cx.notify();
        })
    }

    fn fork_session(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.terminals.link.stale().is_some() {
            self.error = Some(link::UPDATING.into());
            return cx.notify();
        }
        let projects = self.projects();
        let Some(s) = self.agents.get(id) else { return };
        let Some(spec) = self.project_of(&s.cwd, &projects).and_then(|p| fork_spec(s, p, &self.store.agents)) else { return };
        self.outbox.create(spec);
        cx.notify();
    }

    fn ask_close_session(&mut self, id: String, cx: &mut Context<Self>) {
        self.confirm = Some(Confirm::CloseSession(id));
        self.overlay = Some(Overlay::Confirm);
        cx.notify();
    }

    pub(super) fn row_menu_button(&self, key: &str, menu: RowMenu, cx: &mut Context<Self>) -> AnyElement {
        let open = self.row_menu.as_ref() == Some(&menu);
        let toggle = menu.clone();
        let button = icon_button_sized(id(format!("aside-more:{key}")), "more", 22., TEXT_3).rounded(px(6.)).capture_any_mouse_down(cx.listener(
            move |this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.row_menu = (this.row_menu.as_ref() != Some(&toggle)).then(|| toggle.clone());
                this.sidebar.menu_at = None;
                this.panels.menu = None;
                this.panels.actions = None;
                cx.notify();
            },
        ));
        let at = self.sidebar.menu_at;
        div()
            .relative()
            .child(button)
            .when(open, |d| {
                let menu = ui::menu_in(
                    "aside-menu-in",
                    ui::pop(div().id("aside-menu")).w(px(210.)).p(px(6.)).flex().flex_col()
                        .occlude()
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.row_menu = None;
                            this.sidebar.menu_at = None;
                            cx.notify();
                        }))
                        .children(self.row_menu_items(&menu, cx)),
                );
                match at {
                    Some(at) => d.child(deferred(anchored().position(at).snap_to_window_with_margin(px(8.)).child(menu)).with_priority(1)),
                    None => d.child(ui::dropdown(26., menu)),
                }
            })
            .into_any_element()
    }

    fn row_menu_items(&self, menu: &RowMenu, cx: &mut Context<Self>) -> Vec<AnyElement> {
        match menu.clone() {
            RowMenu::Project(p) => {
                let kept = self.store.projects.contains(&p);
                let target = p.clone();
                let first = if kept {
                    ui::menu_row("aside-menu-settings", "settings", "Project settings…", None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.select_project(target.clone(), cx);
                        this.project_settings(&crate::actions::ProjectSettings, window, cx);
                    }))
                } else {
                    ui::menu_row("aside-menu-keep", "check", "Keep in Pocket", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.row_menu = None;
                        this.keep_project(&target, cx);
                    }))
                };
                let mut rows = vec![first.into_any_element()];
                if kept && self.agents.locals_offered() && self.listed_trees(&p).is_some_and(|t| !t.is_empty()) {
                    let target = p.clone();
                    rows.push(
                        ui::menu_row("aside-menu-new-local", LOCAL_ICON, "New Local", None)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.row_menu = None;
                                this.add_local(target.clone(), cx);
                            }))
                            .into_any_element(),
                    );
                }
                let untracked = self.untracked_trees(&p).len();
                if kept && untracked > 0 {
                    let target = p.clone();
                    let label = if untracked == 1 { "Import 1 worktree".to_string() } else { format!("Import {untracked} worktrees") };
                    rows.push(
                        ui::menu_row("aside-menu-import", "branch", &label, None)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.row_menu = None;
                                this.import_worktrees(&target, cx);
                            }))
                            .into_any_element(),
                    );
                }
                rows.extend([
                    ui::menu_divider().into_any_element(),
                    ui::danger_row("aside-menu-remove", "x", if kept { "Remove from Pocket" } else { "Remove" })
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.row_menu = None;
                            this.ask_remove_project(p.clone(), cx);
                        }))
                        .into_any_element(),
                ]);
                rows
            }
            RowMenu::Tree { project, tree } => {
                let mut rows = vec![];
                if self.agents.names_offered() {
                    let rename = tree.clone();
                    rows.push(
                        ui::menu_row("aside-menu-rename", "compose", "Rename…", None)
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.row_menu = None;
                                this.start_rename(rename.clone(), window, cx);
                            }))
                            .into_any_element(),
                    );
                    rows.push(ui::menu_divider().into_any_element());
                }
                rows.push(
                    ui::danger_row("aside-menu-delete", "trash", "Delete worktree…")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.row_menu = None;
                            this.ask_delete_worktree(project.clone(), tree.clone(), cx);
                        }))
                        .into_any_element(),
                );
                rows
            }
            RowMenu::Local(local) => {
                let mut rows = vec![];
                if self.agents.renames_offered(&local) {
                    let rename = local.clone();
                    rows.push(
                        ui::menu_row("aside-menu-rename", "compose", "Rename…", None)
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.row_menu = None;
                                this.start_rename(rename.clone(), window, cx);
                            }))
                            .into_any_element(),
                    );
                }
                if is_local(&local) && self.agents.locals_offered() {
                    if !rows.is_empty() {
                        rows.push(ui::menu_divider().into_any_element());
                    }
                    rows.push(
                        ui::danger_row("aside-menu-delete", "trash", "Delete Local…")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.row_menu = None;
                                this.ask_delete_local(local.clone(), cx);
                            }))
                            .into_any_element(),
                    );
                }
                rows
            }
            RowMenu::Session(id) => {
                let Some(s) = self.agents.get(&id) else { return vec![] };
                let mut rows = vec![];
                for item in session_items(s, self.agents.observe_only(), &self.store.agents, self.agents.fork_offered()) {
                    let row = match item {
                        SessionItem::Pin(pin) => {
                            let id = id.clone();
                            ui::menu_row("aside-menu-pin", "pin", if pin { "Pin" } else { "Unpin" }, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.row_menu = None;
                                this.outbox.pin(&id, pin);
                                cx.notify();
                            }))
                        }
                        SessionItem::CopyPath => {
                            let cwd = s.cwd.clone();
                            ui::menu_row("aside-menu-copy-path", "copy", "Copy path", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.row_menu = None;
                                cx.write_to_clipboard(ClipboardItem::new_string(cwd.clone()));
                                cx.notify();
                            }))
                        }
                        SessionItem::CopyResume(line) => ui::menu_row("aside-menu-copy-resume", "terminal", "Copy resume command", None).on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                this.row_menu = None;
                                cx.write_to_clipboard(ClipboardItem::new_string(line.clone()));
                                cx.notify();
                            },
                        )),
                        SessionItem::Fork => {
                            let id = id.clone();
                            ui::menu_row("aside-menu-fork", "branch", "Fork session", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.row_menu = None;
                                this.fork_session(&id, cx);
                            }))
                        }
                        SessionItem::Close => {
                            rows.push(ui::menu_divider().into_any_element());
                            let id = id.clone();
                            ui::danger_row("aside-menu-close", "x", "Close session…").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.row_menu = None;
                                this.ask_close_session(id.clone(), cx);
                            }))
                        }
                    };
                    rows.push(row.into_any_element());
                }
                rows
            }
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
    fn no_resume_item_without_a_provider_session_id() {
        let fresh = Summary { provider: "claude".into(), cwd: "/w".into(), ..Default::default() };
        assert_eq!(session_items(&fresh, false, &Agents::default(), true), [SessionItem::Pin(true), SessionItem::CopyPath, SessionItem::Close]);
        let resumable = Summary { provider_session_id: Some("s1".into()), ..fresh };
        assert_eq!(
            session_items(&resumable, false, &Agents::default(), true),
            [SessionItem::Pin(true), SessionItem::CopyPath, SessionItem::CopyResume("cd '/w' && 'claude' --resume 's1'".into()), SessionItem::Fork, SessionItem::Close]
        );
    }

    #[test]
    fn fork_is_offered_only_when_pocketd_takes_it_and_the_agent_has_fork_args() {
        let s = Summary { provider: "codex".into(), cwd: "/w".into(), provider_session_id: Some("s1".into()), ..Default::default() };
        let forks = |prefs: &Agents, offered: bool, observe: bool| session_items(&s, observe, prefs, offered).contains(&SessionItem::Fork);
        let mut prefs = Agents::default();
        assert!(forks(&prefs, true, false));
        assert!(!forks(&prefs, false, false));
        assert!(!forks(&prefs, true, true));
        *prefs.provider_mut("codex").args_mut(Args::Fork) = Some(String::new());
        assert!(!forks(&prefs, true, false));
    }

    #[test]
    fn a_fork_starts_in_the_session_s_folder_with_its_model_and_effort() {
        let s = Summary { provider: "claude".into(), cwd: "/w/tree".into(), provider_session_id: Some("s1".into()), model: Some("opus".into()), effort: Some("high".into()), ..Default::default() };
        let spec = fork_spec(&s, "/w", &Agents::default()).unwrap();
        assert_eq!((&spec["project"], &spec["checkout"], &spec["fork"]), (&json!("/w"), &json!({"worktree": "/w/tree"}), &json!("s1")));
        assert_eq!((&spec["provider"], &spec["model"], &spec["effort"]), (&json!("claude"), &json!("opus"), &json!("high")));
        assert!(spec.get("prompt").is_none());
        assert_eq!(fork_spec(&Summary { provider_session_id: None, ..s }, "/w", &Agents::default()), None);
    }

    #[test]
    fn a_pinned_session_offers_unpin() {
        let s = Summary { provider: "claude".into(), cwd: "/w".into(), pinned: true, ..Default::default() };
        assert_eq!(session_items(&s, false, &Agents::default(), true)[0], SessionItem::Pin(false));
    }

    #[test]
    fn an_observer_is_not_offered_close_session() {
        let s = Summary { provider: "claude".into(), cwd: "/w".into(), ..Default::default() };
        assert_eq!(session_items(&s, true, &Agents::default(), true), [SessionItem::CopyPath]);
    }
}
