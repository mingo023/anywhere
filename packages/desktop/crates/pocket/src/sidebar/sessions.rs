use crate::desktop::Desktop;
use crate::desktop::chrome::{RowMenu, empty, state};
use crate::sidebar::row_menu::{MenuEvent, MenuFrame, menu_button, open_on_right_click};
use crate::status::{Card, Kind};
use crate::util::{ago, now_ms};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, State};

/// The sessions whose title holds `query`, in the order given.
pub(crate) fn matching(cards: Vec<Card>, query: &str) -> Vec<Card> {
    let query = query.to_lowercase();
    cards.into_iter().filter(|c| c.title.to_lowercase().contains(&query)).collect()
}

/// A session's card, as the list draws it.
#[derive(PartialEq)]
pub(crate) struct SessionRow {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) provider: String,
    pub(crate) pinned: bool,
    pub(crate) selected: bool,
    pub(crate) pill: State,
    pub(crate) branch: Option<String>,
    pub(crate) notice: Option<ui::Notice>,
    /// The model and account under a loose card's agent; pinned ones show none.
    pub(crate) label: String,
    /// How long ago it last changed, as the card says; empty when the card shows no time.
    pub(crate) ago: String,
}

/// What `Desktop` hands the list each frame.
#[derive(PartialEq)]
pub(crate) struct SessionsFrame {
    pub(crate) has_project: bool,
    pub(crate) rows: Vec<SessionRow>,
    pub(crate) chips: bool,
    pub(crate) diff: bool,
    pub(crate) menu: Option<MenuFrame>,
}

pub(crate) enum SessionsEvent {
    Focus(String),
    Menu(MenuEvent),
}

impl EventEmitter<SessionsEvent> for SessionList {}

/// The panel's Sessions side: the worktree's sessions under a search field.
pub(crate) struct SessionList {
    search: Entity<InputState>,
    frame: Option<SessionsFrame>,
}

impl SessionList {
    pub(crate) fn new(search: Entity<InputState>) -> Self {
        Self { search, frame: None }
    }

    pub(crate) fn set(&mut self, frame: SessionsFrame, cx: &mut Context<Self>) {
        if self.frame.as_ref() != Some(&frame) {
            self.frame = Some(frame);
            cx.notify();
        }
    }

    fn send(&mut self, ev: MenuEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(SessionsEvent::Menu(ev));
    }

    fn card(&self, i: usize, r: &SessionRow, f: &SessionsFrame, cx: &mut Context<Self>) -> Stateful<Div> {
        let menu = RowMenu::Session(r.id.clone());
        let open = f.menu.as_ref().is_some_and(|m| m.menu == menu);
        let when = if open {
            String::new().into_any_element()
        } else if f.chips && i < 9 {
            ui::jump_chip(i + 1).into_any_element()
        } else {
            r.ago.clone().into_any_element()
        };
        let more = div().absolute().top(px(7.)).right(px(6.)).when(!open, |d| d.invisible().group_hover(ui::SESSION_ROW, |s| s.visible())).child(menu_button(
            &r.id,
            menu.clone(),
            f.menu.as_ref(),
            Self::send,
            cx,
        ));
        let row = if r.pinned {
            ui::pinned_row(("card", i), r.selected, when, r.title.clone(), r.notice.clone(), r.branch.clone(), Some(r.pill), f.diff)
        } else {
            ui::session_row(
                ("card", i),
                r.selected,
                ui::agent_label(&r.provider, r.label.clone()),
                when,
                r.title.clone(),
                r.notice.clone(),
                r.branch.clone(),
                Some(r.pill),
                f.diff,
            )
        };
        let id = r.id.clone();
        row.relative()
            .child(more)
            .on_mouse_down(MouseButton::Right, open_on_right_click(menu, Self::send, cx))
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SessionsEvent::Focus(id.clone()))))
    }
}

impl Render for SessionList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let search = div()
            .h(px(44.))
            .flex_none()
            .pl(px(18.))
            .pr(px(14.))
            .flex()
            .items_center()
            .gap(px(9.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .child(icon("search", 14., TEXT_3))
            .child(div().flex_1().text_size(px(13.)).child(Input::new(&self.search).appearance(false).p_0().text_size(px(13.))));
        let list = div().id("cards").flex_1().min_h_0().overflow_y_scroll();
        let wrap = div().flex_1().min_h_0().flex().flex_col().child(search);
        let Some(f) = self.frame.as_ref() else { return wrap };
        if f.has_project {
            let (pinned, rest): (Vec<_>, Vec<_>) = f.rows.iter().enumerate().partition(|(_, r)| r.pinned);
            let pinned: Vec<_> = pinned.into_iter().map(|(i, r)| self.card(i, r, f, cx)).collect();
            let rest: Vec<_> = rest.into_iter().map(|(i, r)| self.card(i, r, f, cx)).collect();
            let cards = div().p(px(8.)).flex().flex_col().gap(px(2.)).when(!pinned.is_empty(), |d| d.child(ui::pinned_group(pinned))).children(rest);
            wrap.child(list.child(cards))
        } else {
            wrap.child(list.child(empty("Add a project with + to start.")))
        }
    }
}

impl Desktop {
    /// Hands the Sessions list what it draws from `Desktop`'s state.
    pub(crate) fn sessions_view(&self, cx: &mut Context<Self>) -> Entity<SessionList> {
        let details = self.store.sidebar.details;
        let rows = self
            .visible_sessions(cx)
            .into_iter()
            .map(|c| {
                let (added, removed) = self.repos.get(&c.cwd).map(|r| r.totals()).unwrap_or_default();
                let pill = match c.kind {
                    _ if matches!(self.handoff_mark(&c.id, cx), Some(crate::handoff::Mark::Moving(_))) => State::Moving,
                    Kind::NotAttached => State::NotAttached,
                    _ => state(c.status, added, removed),
                };
                let branch = self.repos.get(&c.cwd).map(|r| r.branch.clone()).filter(|_| details.branch);
                let notice = self.handoff_notice(&c.id, cx).or(c.notice.map(ui::Notice::from));
                let label = if c.pinned {
                    String::new()
                } else {
                    let model = details.model.then_some(c.model.clone()).filter(|m| !m.is_empty());
                    model.into_iter().chain(self.accounts.session_label(&c.id, &c.provider, &c.account)).collect::<Vec<_>>().join(" \u{b7} ")
                };
                SessionRow {
                    selected: self.session() == Some(c.id.as_str()),
                    pill,
                    branch,
                    notice,
                    label,
                    ago: if details.time { ago(c.at, now_ms()) } else { String::new() },
                    pinned: c.pinned,
                    provider: c.provider,
                    title: c.title,
                    id: c.id,
                }
            })
            .collect();
        let frame = SessionsFrame {
            has_project: self.project.is_some(),
            rows,
            chips: self.chips.shown && self.overlay.is_none(),
            diff: details.diff,
            menu: self.row_menu.as_ref().filter(|m| matches!(m, RowMenu::Session(_))).and(self.menu_frame(cx)),
        };
        let list = self.sidebar.read(cx).sessions.clone();
        list.update(cx, |l, cx| l.set(frame, cx));
        list
    }

    pub(crate) fn on_sessions_event(&mut self, _: &Entity<SessionList>, ev: &SessionsEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            SessionsEvent::Focus(id) => self.focus_agent(id, window, cx),
            SessionsEvent::Menu(ev) => self.on_menu_event(ev.clone(), window, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::matching;
    use crate::status::{self, Card};
    use agents::Summary;

    fn card(title: &str, status: &str) -> Card {
        status::card(&Summary { id: title.into(), title: title.into(), status: status.into(), attached: true, ..Default::default() }, "/p")
    }

    fn titles(cards: Vec<Card>) -> Vec<String> {
        cards.into_iter().map(|c| c.title).collect()
    }

    #[test]
    fn search_keeps_the_sessions_whose_title_matches_in_any_case() {
        let cards = vec![card("Fix CI", "idle"), card("Docs", "working"), card("fix login", "done")];
        assert_eq!(titles(matching(cards, "FIX")), vec!["Fix CI", "fix login"]);
    }

    #[test]
    fn a_status_change_keeps_the_session_order() {
        let before = vec![card("a", "working"), card("b", "idle"), card("c", "working")];
        let after = vec![card("a", "done"), card("b", "needsYou"), card("c", "idle")];
        assert_eq!(titles(matching(before, "")), vec!["a", "b", "c"]);
        assert_eq!(titles(matching(after, "")), vec!["a", "b", "c"]);
    }
}
