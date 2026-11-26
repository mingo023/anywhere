//! The find bar over the chat's top right: what the user looks for, where it stands among the matches, and the steps
//! between them.

use crate::desktop::Desktop;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

const WIDTH: f32 = 372.;

pub(crate) struct FindBar {
    input: Entity<InputState>,
    /// The chat it's open over.
    agent: Option<String>,
}

impl FindBar {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Find in conversation"));
        let subs = vec![cx.subscribe_in(&input, window, |this, input, ev: &InputEvent, _, cx| match ev {
            InputEvent::Change => {
                let query = input.read(cx).value().to_string();
                if let Some(v) = this.chat_find_view() {
                    v.update(cx, |v, cx| v.find(&query, cx));
                }
            }
            InputEvent::PressEnter { shift, .. } => this.chat_find_step(!shift, cx),
            _ => {}
        })];
        (Self { input, agent: None }, subs)
    }
}

impl Desktop {
    /// Opens the find bar over the shown chat, or selects what it holds when it's open there already.
    pub(crate) fn chat_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(agent) = self.session().map(str::to_string) else { return cx.propagate() };
        if !self.chats.views.contains_key(&agent) {
            return cx.propagate();
        }
        if self.chats.find.agent.as_ref().is_some_and(|a| *a != agent) {
            self.chat_close_find(window, cx);
        }
        let input = self.chats.find.input.clone();
        input.update(cx, |i, cx| {
            i.focus(window, cx);
            i.select_all(window, cx);
        });
        self.chats.find.agent = Some(agent);
        let query = input.read(cx).value().to_string();
        if let Some(view) = self.chat_find_view() {
            view.update(cx, |v, cx| v.find(&query, cx));
        }
        cx.notify();
    }

    pub(crate) fn chat_find_step(&mut self, down: bool, cx: &mut Context<Self>) {
        if let Some(view) = self.chat_find_view() {
            view.update(cx, |v, cx| v.find_step(down, cx));
        }
    }

    /// Shuts the find bar, clearing its matches; false when it wasn't showing, so Esc goes on to what is.
    pub(crate) fn chat_close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(agent) = self.chats.find.agent.take() else { return false };
        let view = self.chats.views.get(&agent).cloned();
        let shown = view.is_some() && self.session() == Some(agent.as_str());
        if let Some(view) = view {
            view.update(cx, |v, cx| v.end_find(cx));
        }
        self.chats.find.input.update(cx, |i, cx| i.set_value("", window, cx));
        cx.notify();
        shown
    }

    fn chat_find_view(&self) -> Option<Entity<super::transcript::View>> {
        self.chats.views.get(self.chats.find.agent.as_ref()?).cloned()
    }

    /// The find bar over `agent`'s chat, while it's open there.
    pub(crate) fn chat_find_bar(&mut self, agent: &str, cx: &mut Context<Self>) -> Option<AnimationElement<Div>> {
        if self.chats.find.agent.as_deref() != Some(agent) {
            return None;
        }
        let (count, found) = self.chat_find_view()?.read(cx).find_count().unwrap_or((None, false));
        let step = cx.listener(|this, down: &bool, _, cx| this.chat_find_step(*down, cx));
        let close = cx.listener(|this, _: &(), window, cx| _ = this.chat_close_find(window, cx));
        Some(bar(&self.chats.find.input, count, found, move |down, window, cx| step(&down, window, cx), move |window, cx| close(&(), window, cx)))
    }
}

pub(crate) fn bar(
    input: &Entity<InputState>,
    count: Option<String>,
    found: bool,
    step: impl Fn(bool, &mut Window, &mut App) + 'static,
    close: impl Fn(&mut Window, &mut App) + 'static,
) -> AnimationElement<Div> {
    let step = std::rc::Rc::new(step);
    let arrow = |id: &'static str, name: &str, tip: &'static str, down: bool| {
        let step = step.clone();
        ui::icon_button_sized(id, name, 26., TEXT_2)
            .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
            .on_click(move |_, window, cx| step(down, window, cx))
    };
    let none = count.as_deref() == Some("No results");
    let shown = ui::pop(div())
        .absolute()
        .top(px(12.))
        .right(px(16.))
        .w(px(WIDTH))
        .h(px(40.))
        .flex()
        .items_center()
        .gap(px(2.))
        .pl(px(12.))
        .pr(px(4.))
        .rounded(px(12.))
        .occlude()
        .child(icon("search", 14., TEXT_3))
        .child(div().flex_1().min_w_0().px(px(8.)).child(Input::new(input).appearance(false).p_0().text_size(px(14.))))
        .children(
            count.map(|c| div().flex_none().pr(px(6.)).whitespace_nowrap().text_size(px(12.)).text_color(if none { FAILED_TEXT } else { TEXT_3 }).child(c)),
        )
        .when(found, |d| {
            d.child(arrow("chat-find-prev", "chevron-up", "Previous (⇧↵)", false))
                .child(arrow("chat-find-next", "chevron-down", "Next (↵)", true))
                .child(div().flex_none().w(px(0.5)).h(px(16.)).mx(px(4.)).bg(SEPARATOR))
        })
        .child(
            ui::icon_button_sized("chat-find-close", "x", 26., TEXT_2)
                .tooltip(|window, cx| Tooltip::new("Close (esc)").build(window, cx))
                .on_click(move |_, window, cx| close(window, cx)),
        );
    ui::menu_in("chat-find-bar", shown)
}
