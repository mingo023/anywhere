use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, RowMenu, id};
use crate::removal::branch_deletable;
use agents::Summary;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, icon_button_sized};

#[derive(Debug, PartialEq)]
enum SessionItem {
    Pin(bool),
    CopyPath,
    CopyResume(String),
    Close,
}

fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The shell line that reopens `session_id` in its provider's own CLI, or `None` for a provider without one.
fn resume_command(provider: &str, session_id: &str, cwd: &str) -> Option<String> {
    let (cd, id) = (quoted(cwd), quoted(session_id));
    match provider {
        "claude" => Some(format!("cd {cd} && claude --resume {id}")),
        "codex" => Some(format!("cd {cd} && codex resume {id}")),
        _ => None,
    }
}

fn session_items(s: &Summary, observe: bool) -> Vec<SessionItem> {
    let resume = s.provider_session_id.as_deref().and_then(|id| resume_command(&s.provider, id, &s.cwd));
    let pin = (!observe).then_some(SessionItem::Pin(!s.pinned));
    let close = (!observe).then_some(SessionItem::Close);
    [pin, Some(SessionItem::CopyPath), resume.map(SessionItem::CopyResume), close].into_iter().flatten().collect()
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
                vec![
                    first.into_any_element(),
                    ui::menu_divider().into_any_element(),
                    ui::danger_row("aside-menu-remove", "x", if kept { "Remove from Pocket" } else { "Remove" })
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.row_menu = None;
                            this.ask_remove_project(p.clone(), cx);
                        }))
                        .into_any_element(),
                ]
            }
            RowMenu::Tree { project, tree } => {
                let base = self.store.repos.get(&project).map_or("", |r| r.base.as_str());
                let with_branch = self.worktrees.get(&project).into_iter().flatten().any(|w| w.path == tree && branch_deletable(&w.branch, base));
                let (p, t) = (project.clone(), tree.clone());
                let mut rows = vec![
                    ui::danger_row("aside-menu-delete", "trash", "Delete worktree…")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.row_menu = None;
                            this.ask_delete_worktree(p.clone(), t.clone(), false, cx);
                        }))
                        .into_any_element(),
                ];
                if with_branch {
                    rows.push(
                        ui::danger_row("aside-menu-delete-branch", "trash", "Delete worktree and branch…")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.row_menu = None;
                                this.ask_delete_worktree(project.clone(), tree.clone(), true, cx);
                            }))
                            .into_any_element(),
                    );
                }
                rows
            }
            RowMenu::Session(id) => {
                let Some(s) = self.agents.get(&id) else { return vec![] };
                let mut rows = vec![];
                for item in session_items(s, self.agents.observe_only()) {
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
    use super::{SessionItem, resume_command, session_items};
    use agents::Summary;

    #[test]
    fn resume_command_quotes_the_cwd() {
        assert_eq!(resume_command("claude", "s1", "/w/it's here").as_deref(), Some(r"cd '/w/it'\''s here' && claude --resume 's1'"));
    }

    #[test]
    fn resume_command_quotes_the_session_id() {
        assert_eq!(resume_command("claude", "s1; rm -rf ~", "/w").as_deref(), Some("cd '/w' && claude --resume 's1; rm -rf ~'"));
    }

    #[test]
    fn codex_resumes_with_the_resume_verb() {
        assert_eq!(resume_command("codex", "s1", "/w").as_deref(), Some("cd '/w' && codex resume 's1'"));
        assert_eq!(resume_command("gemini", "s1", "/w"), None);
    }

    #[test]
    fn no_resume_item_without_a_provider_session_id() {
        let fresh = Summary { provider: "claude".into(), cwd: "/w".into(), ..Default::default() };
        assert_eq!(session_items(&fresh, false), [SessionItem::Pin(true), SessionItem::CopyPath, SessionItem::Close]);
        let resumable = Summary { provider_session_id: Some("s1".into()), ..fresh };
        assert_eq!(session_items(&resumable, false), [SessionItem::Pin(true), SessionItem::CopyPath, SessionItem::CopyResume("cd '/w' && claude --resume 's1'".into()), SessionItem::Close]);
    }

    #[test]
    fn a_pinned_session_offers_unpin() {
        let s = Summary { provider: "claude".into(), cwd: "/w".into(), pinned: true, ..Default::default() };
        assert_eq!(session_items(&s, false)[0], SessionItem::Pin(false));
    }

    #[test]
    fn an_observer_is_not_offered_close_session() {
        let s = Summary { provider: "claude".into(), cwd: "/w".into(), ..Default::default() };
        assert_eq!(session_items(&s, true), [SessionItem::CopyPath]);
    }
}
