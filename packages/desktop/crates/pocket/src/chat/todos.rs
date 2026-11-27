use super::tool_detail::TEXT_SIZE;
use chat::{Todo, TodoStatus};
use gpui_kit::*;
use std::cell::Cell;
use std::rc::Rc;
use theme::*;

/// About nine rows; a longer list scrolls inside the card.
const LIST_MAX_HEIGHT: f32 = 300.;

/// How many todos are done, and how many there are.
pub(crate) fn count(todos: &[Todo]) -> (usize, usize) {
    (todos.iter().filter(|t| t.status == TodoStatus::Done).count(), todos.len())
}

/// "2 of 5", or "Complete" once all are done.
pub(crate) fn progress(todos: &[Todo]) -> String {
    match count(todos) {
        (done, n) if done == n => "Complete".into(),
        (done, n) => format!("{done} of {n}"),
    }
}

/// The heading over a plan-mode plan, whose markdown follows it.
pub(crate) fn plan_header() -> Div {
    div().h(px(26.)).flex().items_center().gap(px(6.)).text_size(px(TEXT_SIZE)).text_color(TEXT_2).child(icon("list-checks", 14., TEXT_2)).child("Plan")
}

fn item(id: &str, i: usize, t: &Todo) -> Div {
    let mark = div().mt(px(1.)).size(px(16.)).flex_none().rounded_full().flex().items_center().justify_center();
    let mark = match t.status {
        TodoStatus::Done => mark.bg(SUCCESS_BG).child(icon("check", 10., SUCCESS)).into_any_element(),
        TodoStatus::Active => mark.child(spinner(SharedString::from(format!("{id}-todo-{i}")), 14., MENTION)).into_any_element(),
        TodoStatus::Pending => mark.border_1().border_color(TEXT_5).into_any_element(),
    };
    let text = div().flex_1().min_w_0().text_size(px(13.)).line_height(px(18.));
    let text = match t.status {
        TodoStatus::Done => text.text_color(TEXT_4).line_through(),
        TodoStatus::Active => text.text_color(TEXT),
        TodoStatus::Pending => text.text_color(TEXT_2),
    };
    div().flex().items_start().gap(px(10.)).px(px(12.)).py(px(6.)).child(mark).child(text.child(t.text.clone()))
}

/// Whether a wheel moved the list's offset from `before` to `now`, given its `max` scroll: the list's
/// own scroll runs first and leaves `now` unclamped.
fn moved(before: f32, now: f32, max: f32) -> bool {
    now.clamp(-max, 0.) != before
}

/// The turn's todo list as a card: what's done, what's on, what's next.
pub(crate) fn card(id: &str, todos: &[Todo], window: &mut Window, cx: &mut App) -> Div {
    let pill = div().rounded_full().bg(FILL_4).px(px(8.)).py(px(2.)).font_family(code_font()).text_size(px(10.5)).text_color(TEXT_3).child(progress(todos));
    let header = div()
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(12.))
        .py(px(9.))
        .border_b(px(0.5))
        .border_color(SEPARATOR)
        .child(icon("list", 15., TEXT_4))
        .child(div().flex_1().font_family(code_font()).text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child("Tasks"))
        .child(pill);
    let scroll = window.use_keyed_state(SharedString::from(format!("{id}-todos-scroll")), cx, |_, _| ScrollHandle::new()).read(cx).clone();
    let clamped = |s: &ScrollHandle| f32::from(s.offset().y).clamp(-f32::from(s.max_offset().y), 0.);
    let before = Rc::new(Cell::new(clamped(&scroll)));
    div().rounded(px(12.)).bg(FILL_1).border_1().border_color(SEPARATOR).overflow_hidden().child(header).child(
        div()
            .id(SharedString::from(format!("{id}-todos")))
            .max_h(px(LIST_MAX_HEIGHT))
            .overflow_y_scroll()
            .track_scroll(&scroll)
            // Keeps the chat still until the list reaches its end.
            .on_scroll(move |_, _, cx| {
                if moved(before.get(), f32::from(scroll.offset().y), f32::from(scroll.max_offset().y)) {
                    cx.stop_propagation();
                }
                before.set(clamped(&scroll));
            })
            .py(px(4.))
            .flex()
            .flex_col()
            .children(todos.iter().enumerate().map(|(i, t)| item(id, i, t))),
    )
}

#[cfg(test)]
mod tests {
    use super::{count, moved, progress};
    use chat::{Todo, TodoStatus};

    #[test]
    fn todos_count_done_of_total() {
        let t = |status| Todo { text: "x".into(), status };
        assert_eq!(count(&[t(TodoStatus::Done), t(TodoStatus::Active), t(TodoStatus::Pending), t(TodoStatus::Done)]), (2, 4));
        assert_eq!(count(&[]), (0, 0));
    }

    #[test]
    fn the_tasks_card_reads_done_of_total_until_complete() {
        let t = |status| Todo { text: "x".into(), status };
        assert_eq!(progress(&[t(TodoStatus::Done), t(TodoStatus::Active), t(TodoStatus::Pending)]), "1 of 3");
        assert_eq!(progress(&[t(TodoStatus::Done), t(TodoStatus::Done)]), "Complete");
    }

    #[test]
    fn a_wheel_stays_in_the_list_until_it_reaches_an_end() {
        assert!(moved(-40., -20., 100.), "scrolled up with rows above");
        assert!(moved(-10., 10., 100.), "reached the top on the way");
        assert!(!moved(0., 20., 100.), "already at the top");
        assert!(moved(-60., -80., 100.), "scrolled down with rows below");
        assert!(!moved(-100., -120., 100.), "already at the bottom");
        assert!(!moved(0., 20., 0.), "a list that doesn't scroll");
    }
}
