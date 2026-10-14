use super::{ChatComposer, Choice};
use crate::actions::JumpToTab;
use crate::desktop::Desktop;
use crate::palette::{hint, marked, matches};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

fn words(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

/// The choices whose title or agent holds every word.
fn matching<'a>(choices: &'a [Choice], words: &[String], agent: impl Fn(&str) -> String) -> Vec<&'a Choice> {
    choices.iter().filter(|c| matches(words, &[&c.title, &agent(&c.provider)])).collect()
}

/// The row the highlight starts on: `target` while it's shown, else the first that can take a quote, else "New session…" after the rest.
fn first_row(rows: &[&Choice], target: Option<&str>) -> usize {
    rows.iter().position(|c| Some(c.id.as_str()) == target).or_else(|| rows.iter().position(|c| c.ready)).unwrap_or(rows.len())
}

/// The next row from `ix` that can be picked, wrapping; `ready` ends with "New session…", which always can.
fn next_ready(ready: &[bool], ix: usize, down: bool) -> usize {
    let n = ready.len();
    let mut i = ix.min(n - 1);
    for _ in 0..n {
        i = if down { (i + 1) % n } else { (i + n - 1) % n };
        if ready[i] {
            break;
        }
    }
    i
}

/// The session ⌘`n` picks: the `n`th of the first nine rows, if it can take a quote.
fn jump_target<'a>(rows: &[&'a Choice], n: usize) -> Option<&'a Choice> {
    rows.get(n.checked_sub(1)?).copied().filter(|c| c.ready && n <= 9)
}

fn subtitle(agent: &str, note: &str) -> String {
    if note.is_empty() { agent.to_string() } else { format!("{agent} · {note}") }
}

impl ChatComposer {
    fn highlighted_row(&self, total: usize) -> usize {
        self.highlight.min(total)
    }

    fn highlight_row(&mut self, row: usize) {
        self.highlight = row;
        self.scroll.scroll_to_item(row);
    }
}

impl Desktop {
    fn chat_rows<'a>(&self, choices: &'a [Choice], cx: &App) -> Vec<&'a Choice> {
        matching(choices, &words(&self.chat.search.read(cx).value()), |p| self.agent_name(p))
    }

    pub(super) fn reset_chat_highlight(&mut self, cx: &App) {
        let choices = self.chat_choices();
        let row = first_row(&self.chat_rows(&choices, cx), self.chat_target(&choices).as_deref());
        self.chat.highlight_row(row);
    }

    pub(crate) fn open_chat_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.chat.menu = true;
        self.chat.search.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        self.chat.scroll.set_offset(Point::default());
        self.reset_chat_highlight(cx);
        // A scroll is dropped until the list has been laid out once: the next frame lays it out, the one after can scroll it.
        cx.on_next_frame(window, |_, window, cx| {
            cx.on_next_frame(window, |this, _, cx| {
                this.chat.highlight_row(this.chat.highlight);
                cx.notify();
            })
        });
        cx.notify();
    }

    pub(super) fn close_chat_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.chat.menu = false;
        self.chat.input.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    fn pick_chat_target(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.chat.target = Some(id);
        self.close_chat_menu(window, cx);
    }

    fn on_chat_menu_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let choices = self.chat_choices();
        let rows = self.chat_rows(&choices, cx);
        let ix = self.chat.highlighted_row(rows.len());
        match ev.keystroke.key.as_str() {
            "enter" => match rows.get(ix) {
                Some(c) if c.ready => self.pick_chat_target(c.id.clone(), window, cx),
                Some(_) => return,
                None => self.chat_in_new_session(window, cx),
            },
            key @ ("up" | "down") => {
                let ready: Vec<bool> = rows.iter().map(|c| c.ready).chain([true]).collect();
                self.chat.highlight_row(next_ready(&ready, ix, key == "down"));
                cx.notify();
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    fn highlight_on_hover(row: usize, cx: &Context<Self>) -> impl Fn(&MouseMoveEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
            if this.chat.highlight != row {
                this.chat.highlight = row;
                cx.notify();
            }
        })
    }

    pub(super) fn chat_target_picker(&self, chosen: Option<&Choice>, choices: &[Choice], cx: &mut Context<Self>) -> Div {
        let open = self.chat.menu;
        let pill = div()
            .id("chat-target")
            .h(px(32.))
            .px(px(12.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(7.))
            .rounded(px(16.))
            .bg(FILL_3)
            .cursor_pointer()
            .hover(|s| s.bg(FILL_4))
            .when(open, |d| d.bg(FILL_4))
            .text_size(px(13.))
            // On press, not click: by the click, the menu's own press-outside has already closed it.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if open { this.close_chat_menu(window, cx) } else { this.open_chat_menu(window, cx) }
                }),
            );
        let pill = match chosen {
            Some(c) => pill
                .child(provider_icon(&c.provider, 13., TEXT))
                .child(div().font_weight(FontWeight::SEMIBOLD).child(self.agent_name(&c.provider)))
                .child(div().text_color(TEXT_6).child("·"))
                .child(div().max_w(px(160.)).truncate().text_color(TEXT_2).child(c.title.clone())),
            None => pill.text_color(TEXT_2).child("No agent"),
        };
        let menu = open.then(|| {
            let query = self.chat.search.read(cx).value();
            let words = words(&query);
            let rows = self.chat_rows(choices, cx);
            let highlight = self.chat.highlighted_row(rows.len());
            let picked = chosen.map(|c| c.id.clone());
            let items = rows.iter().enumerate().map(|(i, c)| {
                let id = c.id.clone();
                let shortcut = if picked.as_ref() == Some(&c.id) {
                    icon("check", 14., TEXT).into_any_element()
                } else if jump_target(&rows, i + 1).is_some() {
                    ui::kbd(&format!("⌘{}", i + 1)).into_any_element()
                } else {
                    div().into_any_element()
                };
                div()
                    .id(("chat-choice", i))
                    .flex_none()
                    .px(px(10.))
                    .py(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(8.))
                    .text_size(px(13.))
                    .when(i == highlight, |d| d.bg(FILL_3))
                    .child(provider_icon(&c.provider, 15., TEXT))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(marked(c.title.clone(), &words)))
                            .child(div().truncate().text_size(px(11.5)).text_color(TEXT_3).child(marked(subtitle(&self.agent_name(&c.provider), &c.note), &words))),
                    )
                    .child(shortcut)
                    .when(!c.ready, |d| d.opacity(0.5))
                    .when(c.ready, |d| {
                        d.cursor_pointer()
                            .on_mouse_move(Self::highlight_on_hover(i, cx))
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.pick_chat_target(id.clone(), window, cx)))
                    })
            });
            let empty = (rows.is_empty() && !words.is_empty()).then(|| div().px(px(10.)).pt(px(10.)).pb(px(6.)).text_size(px(12.5)).text_color(TEXT_3).child(format!("No session matches “{}”.", query.trim())));
            let new = div()
                .id("chat-new-session")
                .h(px(34.))
                .px(px(10.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(10.))
                .rounded(px(8.))
                .cursor_pointer()
                .when(highlight == rows.len(), |d| d.bg(FILL_3))
                .text_size(px(13.))
                .child(icon("plus", 15., TEXT_2))
                .child("New session…")
                .on_mouse_move(Self::highlight_on_hover(rows.len(), cx))
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.chat_in_new_session(window, cx)));
            let search = div()
                .h(px(42.))
                .px(px(14.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(8.))
                .border_b(px(0.5))
                .border_color(SEPARATOR)
                .child(icon("search", 14., TEXT_3))
                .child(div().flex_1().text_size(px(13.)).child(Input::new(&self.chat.search).appearance(false).p_0().text_size(px(13.))));
            let keys = div()
                .h(px(32.))
                .px(px(14.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(14.))
                .border_t(px(0.5))
                .border_color(SEPARATOR)
                .text_size(px(11.))
                .text_color(TEXT_3)
                .child(hint("↑↓", "Move"))
                .child(hint("↵", "Choose"))
                .child(hint("⌘1–9", "Jump"))
                .child(hint("esc", "Close"));
            deferred(
                anchored().anchor(Anchor::BottomLeft).offset(point(px(0.), px(-6.))).snap_to_window_with_margin(px(8.)).child(ui::menu_in(
                    "chat-menu-in",
                    ui::pop(div().id("chat-menu"))
                        .w(px(360.))
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .occlude()
                        .capture_key_down(cx.listener(Self::on_chat_menu_key))
                        // ⌘1–9 jump to a tab elsewhere; here they pick the nth row.
                        .on_action(cx.listener(|this, a: &JumpToTab, window, cx| {
                            let choices = this.chat_choices();
                            let rows = this.chat_rows(&choices, cx);
                            if let Some(c) = jump_target(&rows, a.0) {
                                this.pick_chat_target(c.id.clone(), window, cx);
                            }
                        }))
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                            this.chat.menu = false;
                            cx.notify();
                            // The press then focuses what it lands on; take focus back unless that's something else, like a terminal.
                            cx.defer_in(window, |this, window, cx| {
                                let stayed = this.root.is_focused(window) || this.chat.search.focus_handle(cx).is_focused(window);
                                if stayed && this.chat_quote().is_some() {
                                    this.chat.input.update(cx, |s, cx| s.focus(window, cx));
                                }
                            });
                        }))
                        .child(search)
                        .child(div().id("chat-menu-list").max_h(px(260.)).overflow_y_scroll().track_scroll(&self.chat.scroll).p(px(5.)).flex().flex_col().children(items).children(empty).child(new))
                        .child(keys),
                )),
            )
            // Above the file's popover (2), which holds this picker: under it, the menu is hidden yet still takes clicks.
            .with_priority(3)
        });
        div().relative().child(pill.child(icon("chevron-down", 12., TEXT_3))).children(menu)
    }
}

#[cfg(test)]
mod tests {
    use super::{Choice, first_row, jump_target, matching, next_ready, subtitle, words};

    fn choice(id: &str, provider: &str, title: &str, ready: bool) -> Choice {
        Choice { id: id.into(), provider: provider.into(), title: title.into(), note: String::new(), ready }
    }

    fn agent(p: &str) -> String {
        if p == "claude" { "Claude Code".into() } else { "Codex".into() }
    }

    fn sessions() -> Vec<Choice> {
        vec![choice("a", "claude", "Fix markdown glass", true), choice("b", "codex", "Audit sign-out paths", false), choice("c", "codex", "Virtualize the Explorer", true)]
    }

    fn ids(rows: Vec<&Choice>) -> Vec<&str> {
        rows.into_iter().map(|c| c.id.as_str()).collect()
    }

    #[test]
    fn typing_keeps_the_sessions_whose_title_or_agent_holds_every_word() {
        let all = sessions();
        assert_eq!(ids(matching(&all, &words(""), agent)), ["a", "b", "c"]);
        assert_eq!(ids(matching(&all, &words(" EXPL "), agent)), ["c"]);
        assert_eq!(ids(matching(&all, &words("codex audit"), agent)), ["b"]);
        assert!(matching(&all, &words("zzz"), agent).is_empty());
    }

    #[test]
    fn the_highlight_starts_on_the_target_else_the_first_ready_else_new_session() {
        let all = sessions();
        let rows = matching(&all, &[], agent);
        assert_eq!(first_row(&rows, Some("c")), 2);
        assert_eq!(first_row(&rows, None), 0);
        assert_eq!(first_row(&rows[1..], Some("a")), 1);
        assert_eq!(first_row(&[], Some("a")), 0);
    }

    #[test]
    fn arrows_pass_over_sessions_that_cant_take_a_quote_and_wrap() {
        let ready = [true, false, true, true];
        assert_eq!(next_ready(&ready, 0, true), 2);
        assert_eq!(next_ready(&ready, 2, false), 0);
        assert_eq!(next_ready(&ready, 3, true), 0);
        assert_eq!(next_ready(&ready, 0, false), 3);
        assert_eq!(next_ready(&[true], 0, true), 0);
    }

    #[test]
    fn command_digits_pick_the_nth_row_only_when_it_can_take_a_quote() {
        let all = sessions();
        let rows = matching(&all, &[], agent);
        assert_eq!(jump_target(&rows, 1).map(|c| c.id.as_str()), Some("a"));
        assert!(jump_target(&rows, 2).is_none());
        assert_eq!(jump_target(&rows, 3).map(|c| c.id.as_str()), Some("c"));
        assert!(jump_target(&rows, 0).is_none());
        assert!(jump_target(&rows, 4).is_none());
        let many: Vec<Choice> = (0..12).map(|i| choice(&i.to_string(), "claude", "s", true)).collect();
        let rows = matching(&many, &[], agent);
        assert!(jump_target(&rows, 9).is_some());
        assert!(jump_target(&rows, 10).is_none());
    }

    #[test]
    fn a_session_without_a_note_is_subtitled_by_its_agent_alone() {
        assert_eq!(subtitle("Codex", ""), "Codex");
        assert_eq!(subtitle("Codex", "main"), "Codex · main");
    }
}
