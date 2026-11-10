use super::Menu;
use super::logic::{Action, Field, LABELS, PRIORITIES, column, menu_items, priority_name, status_actions};
use super::parts::{label_dot, priority_glyph};
use super::values::Values;
use crate::desktop::Desktop;
use agents::providers;
use agents::tickets::Ticket;
use gpui_kit::*;
use std::collections::HashSet;
use std::rc::Rc;
use theme::*;

fn action_row(a: Action) -> Option<(&'static str, &'static str, bool)> {
    Some(match a {
        Action::Start => ("play", "Start", false),
        Action::StartAgain => ("reload", "Start again", false),
        Action::Edit => ("pencil", "Edit", false),
        Action::MoveBack => ("back", "Move back to To do", false),
        Action::Open => ("external", "Open", false),
        Action::CopyBranch => ("copy", "Copy branch name", false),
        Action::Restore => ("redo", "Restore", false),
        Action::Close => ("run-cancelled", "Close ticket", false),
        Action::Stop => ("stop", "Stop agent", false),
        Action::Queue => ("status-queued", "Move to Queued", false),
        Action::Priority => ("priority-medium", "Priority", false),
        Action::Labels => ("filter", "Labels", false),
        Action::Agent => ("bot", "Agent", false),
        Action::Project => ("folder", "Project", false),
        Action::Select => ("check", "Select", false),
        Action::Deselect => ("x", "Deselect", false),
        Action::CreatePr => ("pull-request", "Create PR", false),
        Action::Delete => ("trash", "Delete", true),
        Action::DeleteWorktree => ("trash", "Delete worktree…", true),
        Action::Separator | Action::SetStatus => return None,
    })
}

impl Desktop {
    /// Saves a change to each ticket, a started one taking only its priority and labels.
    fn set_tickets_props(&mut self, ids: &[String], change: impl Fn(&mut Values)) {
        for id in ids {
            let Some(t) = self.agents.board.tickets.iter_mut().find(|t| t.id == *id) else { continue };
            let mut v = Values::of(t);
            change(&mut v);
            let draft = v.draft(&t.title, &t.prompt);
            // The menu can stay open, so the next toggle must build on this one before the daemon echoes it.
            (t.priority, t.labels, t.provider, t.folder, t.base) = (v.priority, v.labels, v.provider, v.folder, v.base);
            self.outbox.local.board_save(draft, false);
        }
        self.board_changed();
    }

    /// The choices for `field`, applied to every one of `ids`; a choice shows as made when all of them have it.
    fn prop_rows(&self, field: Field, ids: Vec<String>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let tickets: Vec<&Ticket> = self.agents.board.tickets.iter().filter(|t| wanted.contains(t.id.as_str())).collect();
        let all = |has: &dyn Fn(&Ticket) -> bool| !tickets.is_empty() && tickets.iter().all(|t| has(t));
        let ids = Rc::new(ids);
        let settle = |this: &mut Self, cx: &mut Context<Self>| {
            this.close_menus();
            cx.notify();
        };
        match field {
            Field::Priority => PRIORITIES
                .into_iter()
                .enumerate()
                .map(|(i, p)| {
                    let ids = ids.clone();
                    ui::menu_pick(("board-priority", i), priority_glyph(p), priority_name(p), all(&|t| t.priority == p))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.set_tickets_props(&ids, |v| v.priority = p);
                            settle(this, cx);
                        }))
                        .into_any_element()
                })
                .collect(),
            Field::Labels => LABELS
                .into_iter()
                .enumerate()
                .map(|(i, label)| {
                    let ids = ids.clone();
                    let on = all(&|t| t.labels.iter().any(|l| l == label));
                    ui::menu_pick(("board-label", i), label_dot(label), label, on)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.set_tickets_props(&ids, |v| v.set_label(label, !on));
                            cx.notify();
                        }))
                        .into_any_element()
                })
                .collect(),
            Field::Agent => providers::IDS
                .iter()
                .enumerate()
                .map(|(i, provider)| {
                    let ids = ids.clone();
                    ui::menu_pick(("board-agent", i), theme::provider_icon(provider, 13., TEXT_2), &self.agent_name(provider), all(&|t| t.provider == *provider))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.set_tickets_props(&ids, |v| v.provider = provider.to_string());
                            settle(this, cx);
                        }))
                        .into_any_element()
                })
                .collect(),
            Field::Project => self
                .board_projects()
                .into_iter()
                .enumerate()
                .map(|(i, folder)| {
                    let ids = ids.clone();
                    let chosen = all(&|t| t.folder == folder);
                    let row = ui::menu_pick(("board-project", i), self.repo_tile_for(&folder, 14.), &self.repo_name(&folder), chosen);
                    row.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set_tickets_props(&ids, |v| v.set_folder(folder.clone()));
                        settle(this, cx);
                    }))
                    .into_any_element()
                })
                .collect(),
            Field::Base => Vec::new(),
        }
    }

    fn action_rows(&self, items: Vec<Action>, id: &str, cx: &mut Context<Self>) -> Vec<AnyElement> {
        items
            .into_iter()
            .enumerate()
            .map(|(i, a)| {
                let Some((name, label, danger)) = action_row(a) else { return ui::menu_divider().into_any_element() };
                let id = id.to_string();
                let row = if danger { ui::danger_row(("board-action", i), name, label) } else { ui::menu_row(("board-action", i), name, label, None) };
                row.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.run_action(a, &id, window, cx))).into_any_element()
            })
            .collect()
    }

    fn status_rows(&self, t: &Ticket, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let items = status_actions(column(t));
        if items.is_empty() {
            return vec![div().p(px(8.)).text_size(px(12.)).text_color(TEXT_3).child("Status follows the agent.").into_any_element()];
        }
        self.action_rows(items, &t.id, cx)
    }

    pub(super) fn card_menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (at, rows) = match self.board.menu.as_ref()? {
            Menu::Card { id, at, sub } => {
                let t = self.ticket(id)?;
                let rows = match sub {
                    Some(field) => self.prop_rows(*field, vec![id.clone()], cx),
                    None => self.action_rows(menu_items(t, self.ticket_reach(t), self.board.selection.picked.contains(id)), id, cx),
                };
                (at, rows)
            }
            Menu::Status { id, at } => (at, self.status_rows(self.ticket(id)?, cx)),
            Menu::Bulk { action: Action::SetStatus, at } => (at, self.bulk_status_rows(cx)),
            Menu::Bulk { action, at } => (at, self.prop_rows(Self::action_field(*action)?, self.bulk_ids(*action), cx)),
            _ => return None,
        };
        (!rows.is_empty()).then(|| deferred(anchored().position(*at).snap_to_window_with_margin(px(8.)).child(ui::menu_in("board-menu-in", self.popup("board-menu", 210., cx).children(rows)))).with_priority(2))
    }
}
