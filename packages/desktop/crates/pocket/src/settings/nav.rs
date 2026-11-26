use super::{Section, Settings, SettingsEvent, catalog, focus_ring, nav_groups};
use crate::desktop::chrome::{HEADER, drag_area};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// The nav item ↑ or ↓ moves focus to from `at`, wrapping round; `None` for another key.
fn next_item(key: &str, at: usize, count: usize) -> Option<usize> {
    match key {
        "down" => Some((at + 1) % count),
        "up" => Some((at + count - 1) % count),
        _ => None,
    }
}

impl Settings {
    pub(crate) fn nav(&self, width: f32, cx: &mut Context<Self>) -> Div {
        let words = self.query(cx);
        let hits = catalog::search(&words, &self.store, &self.live);
        let searching = !words.is_empty();
        let groups = nav_groups().into_iter().map(|(title, sections)| {
            let items = sections.into_iter().map(|s| {
                let at = Section::ALL.iter().position(|a| *a == s).unwrap_or(0);
                let count = hits.iter().find(|(h, _)| *h == s).map_or(0, |(_, rows)| rows.len());
                let selected = !searching && self.section.nav() == s;
                let changed = !searching && self.changed_in(s) > 0;
                item(("settings-section", at), s.icon(), s.label(), selected)
                    .track_focus(&self.nav_focus[at])
                    .when(searching && count == 0, |d| d.opacity(0.45))
                    .when(count > 0, |d| {
                        d.child(
                            div()
                                .ml_auto()
                                .min_w(px(18.))
                                .h(px(16.))
                                .px(px(5.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(8.))
                                .bg(FILL_3)
                                .text_size(px(11.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(TEXT_2)
                                .child(count.to_string()),
                        )
                    })
                    .when(changed, |d| d.child(div().ml_auto().child(ui::dot(6., TEXT_2))))
                    .on_key_down(cx.listener(move |this, ev: &KeyDownEvent, window, cx| {
                        if let Some(next) = next_item(&ev.keystroke.key, at, Section::ALL.len()) {
                            window.focus(&this.nav_focus[next], cx);
                            cx.stop_propagation();
                        }
                    }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.show_section(s, cx);
                        this.clear_search(window, cx);
                        cx.notify();
                    }))
            });
            div()
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(div().px(px(8.)).pb(px(4.)).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_4).child(title))
                .children(items)
        });
        let empty = self.search.read(cx).value().is_empty();
        let search = div()
            .h(px(30.))
            .px(px(9.))
            .flex()
            .items_center()
            .gap(px(7.))
            .rounded(px(8.))
            .bg(FILL_3)
            .child(icon("search", 14., TEXT_3))
            .child(div().flex_1().min_w_0().text_size(px(13.)).child(Input::new(&self.search).appearance(false).p_0().text_size(px(13.))))
            .when(empty, |d| d.child(div().flex_none().text_size(px(11.5)).text_color(TEXT_4).child(ui::os_keys("⌘F"))));
        let back = item("settings-back", "back", "Back", false).tab_index(0).on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Close)));
        // The list scrolls under Back and the search, fading at each edge so a cut-off item reads as more to scroll to.
        let fade = |top: bool| {
            let (from, to) = (Hsla::from(SIDE), Hsla::from(SIDE).opacity(0.));
            div().absolute().left_0().right_0().h(px(if top { 10. } else { 18. })).when(top, |d| d.top_0()).when(!top, |d| d.bottom_0()).bg(linear_gradient(
                if top { 180. } else { 0. },
                linear_color_stop(from, 0.),
                linear_color_stop(to, 1.),
            ))
        };
        let list = div()
            .id("settings-nav-list")
            .size_full()
            .overflow_y_scroll()
            .child(div().pt(px(4.)).px(px(10.)).pb(px(16.)).flex().flex_col().gap(px(14.)).children(groups));
        ui::side(div())
            .w(px(width))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .child(drag_area(div()).h(px(HEADER)).flex_none())
            .child(div().flex_none().px(px(10.)).pb(px(8.)).child(back))
            .child(div().flex_none().px(px(12.)).pb(px(6.)).child(search))
            .child(div().relative().flex_1().min_h_0().child(list).child(fade(true)).child(fade(false)))
    }
}

fn item(id: impl Into<ElementId>, icon_name: &str, label: &'static str, selected: bool) -> Stateful<Div> {
    let item = div()
        .id(id)
        .h(px(30.))
        .px(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(9.))
        .rounded(px(7.))
        .cursor_pointer()
        .text_size(px(13.))
        .when(selected, |d| d.bg(FILL_4).text_color(TEXT).font_weight(FontWeight::SEMIBOLD))
        .when(!selected, |d| d.text_color(TEXT_BODY).font_weight(FontWeight(450.)).hover(|h| h.bg(FILL_2)))
        .child(icon(icon_name, 17., if selected { TEXT } else { TEXT_3 }))
        .child(label);
    focus_ring(item)
}

#[cfg(test)]
mod tests {
    use super::next_item;

    #[test]
    fn arrows_move_through_the_nav_and_wrap_at_its_ends() {
        assert_eq!([next_item("down", 0, 16), next_item("up", 0, 16), next_item("down", 15, 16), next_item("left", 3, 16)], [Some(1), Some(15), Some(0), None]);
    }
}
