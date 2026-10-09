use super::Menu;
use super::editor::Form;
use super::form::{EVERY_DAY, Values, WEEKDAYS, WEEKENDS};
use super::logic::{WEEK, next_fire, stamp, when_label};
use super::parts::{card, segmented};
use crate::desktop::Desktop;
use crate::util::now_ms;
use chrono::Local;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, icon_button_sized};

const DAY_LETTERS: [&str; 7] = ["S", "M", "T", "W", "T", "F", "S"];
const PRESETS: [(&str, &[u8]); 3] = [("Weekdays", &WEEKDAYS), ("Every day", &EVERY_DAY), ("Weekends", &WEEKENDS)];
const EVERIES: [(&str, u32); 5] = [("15 min", 15), ("30 min", 30), ("1 hour", 60), ("6 hours", 360), ("1 day", 1440)];

fn chip(id: impl Into<ElementId>, label: &str, on: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(26.))
        .px(px(10.))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(13.))
        .cursor_pointer()
        .text_size(px(12.))
        .font_weight(FontWeight(550.))
        .whitespace_nowrap()
        .when(on, |d| d.bg(TEXT).text_color(ON_TEXT))
        .when(!on, |d| d.bg(FILL_2).text_color(TEXT_2).hover(|s| s.bg(FILL_3)))
        .child(label.to_string())
}

fn label(text: &str) -> Div {
    div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child(text.to_string())
}

impl Desktop {
    /// The editor's trigger card: the way to add one, or the one added.
    pub(super) fn trigger_card(&self, v: &Values, cx: &mut Context<Self>) -> Div {
        if !v.scheduled {
            return self.add_trigger(cx);
        }
        let now = now_ms();
        let next = v.valid_schedule().and_then(|s| next_fire(&s, now, &Local)).map(|at| format!("· next {}", stamp(at, now, &Local)));
        let head = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(icon("clock", 16., TEXT_2))
            .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(v.valid_schedule().as_ref().map_or_else(|| "Schedule".to_string(), when_label)))
            .children(next.map(|t| div().text_size(px(12.5)).text_color(TEXT_3).child(t)))
            .child(div().ml_auto().child(icon_button_sized("trigger-remove", "x", 24., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.automations.form.v.scheduled = false;
                this.close_menus();
                cx.notify();
            }))));
        let mode = segmented("trigger-mode", &["Days and time", "Repeat"], v.repeat as usize, true, |this, i, cx| {
            this.automations.form.v.repeat = i == 1;
            cx.notify();
        }, cx);
        let f = &self.automations.form;
        let body = if v.repeat { self.repeat_fields(v, f, cx) } else { self.day_fields(v, f, cx) };
        card().pt(px(12.)).px(px(16.)).pb(px(16.)).flex().flex_col().gap(px(14.)).child(head).child(div().flex().child(mode)).child(body).children(v.field_error().map(|e| div().text_size(px(12.)).text_color(FAILED).child(e)))
    }

    fn add_trigger(&self, cx: &mut Context<Self>) -> Div {
        let open = self.automations.menu == Some(Menu::Add);
        let button = self.menu_toggle(
            div().id("trigger-add").h(px(48.)).w_full().flex().items_center().justify_center().gap(px(8.)).rounded(px(8.)).cursor_pointer().text_size(px(14.)).font_weight(FontWeight::MEDIUM).text_color(TEXT_2).hover(|s| s.bg(FILL_2)).child(icon("plus", 16., TEXT_2)).child("Add trigger"),
            Menu::Add,
            cx,
        );
        let events = ["Pull request opened", "Issue created", "Check failed"].into_iter().enumerate().map(|(i, name)| {
            div().id(("trigger-event", i)).h(px(30.)).px(px(8.)).flex().items_center().gap(px(8.)).text_size(px(13.)).text_color(TEXT_4).child(icon("bolt", 15., TEXT_4)).child(div().flex_1().child(name)).child(div().text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child("Soon"))
        });
        let menu = ui::menu_in(
            "trigger-menu-in",
            self.popup("trigger-menu", 260., cx)
                .rounded(px(12.))
                .child(ui::menu_row("trigger-schedule", "clock", "Schedule", None).h(px(30.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.automations.form.v.add_schedule(&this.store.automations);
                    this.close_menus();
                    cx.notify();
                })))
                .child(ui::menu_divider())
                .child(div().h(px(26.)).px(px(8.)).flex().items_center().text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child("Events"))
                .children(events),
        );
        card().relative().p(px(4.)).child(button).when(open, |d| d.child(ui::dropdown(54., menu)))
    }

    fn day_fields(&self, v: &Values, f: &Form, cx: &mut Context<Self>) -> Div {
        let days = WEEK.iter().enumerate().map(|(i, &day)| {
            let on = v.days[day as usize];
            div()
                .id(("trigger-day", i))
                .size(px(34.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(17.))
                .cursor_pointer()
                .text_size(px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .when(on, |d| d.bg(ACCENT).text_color(ON_TEXT))
                .when(!on, |d| d.bg(FILL_2).text_color(TEXT_2).hover(|s| s.bg(FILL_3)))
                .child(DAY_LETTERS[day as usize])
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.automations.form.v.toggle_day(day);
                    cx.notify();
                }))
        });
        let presets = PRESETS.iter().enumerate().map(|(i, (name, set))| {
            chip(("trigger-preset", i), name, v.has_days(set)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.automations.form.v.set_days(set);
                cx.notify();
            }))
        });
        let time = div().w(px(112.)).h(px(36.)).px(px(12.)).flex().items_center().rounded(px(10.)).bg(FILL_2).font_family(MONO).text_size(px(14.)).child(Input::new(&f.time).appearance(false).p_0());
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(div().flex().gap(px(6.)).children(days))
            .child(div().flex().gap(px(6.)).children(presets))
            .child(div().flex().flex_col().gap(px(6.)).child(label("Time")).child(time))
    }

    fn repeat_fields(&self, v: &Values, f: &Form, cx: &mut Context<Self>) -> Div {
        let current = v.every.trim().parse::<u32>().ok();
        let presets = EVERIES.iter().enumerate().map(|(i, (name, minutes))| {
            let minutes = *minutes;
            chip(("trigger-every", i), name, current == Some(minutes)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.automations.form.every.update(cx, |s, cx| s.set_value(minutes.to_string(), window, cx));
                cx.notify();
            }))
        });
        let input = div().w(px(96.)).h(px(36.)).px(px(12.)).flex().items_center().rounded(px(10.)).bg(FILL_2).font_family(MONO).text_size(px(14.)).child(Input::new(&f.every).appearance(false).p_0());
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(div().flex().flex_col().gap(px(6.)).child(label("Repeat every")).child(div().flex().items_center().gap(px(8.)).child(input).child(div().text_size(px(13.)).text_color(TEXT_2).child("minutes"))))
            .child(div().flex().gap(px(6.)).children(presets))
    }
}
