use crate::desktop::Desktop;
use crate::modals::new_session::Perm;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Picker {
    Agent,
    Branch,
}

fn chip(id: &'static str, open: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(30.))
        .pl(px(10.))
        .pr(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(7.))
        .rounded(px(8.))
        .whitespace_nowrap()
        .cursor_pointer()
        .bg(rgba(if open { FILL_3 } else { FILL_2 }))
        .hover(|s| s.bg(rgba(FILL_3)))
}

fn pick_head(label: &str) -> Div {
    div().pt(px(8.)).px(px(8.)).pb(px(4.)).text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT_3)).child(label.to_string())
}

fn pick_row(id: impl Into<ElementId>, selected: bool, lead: Option<impl IntoElement>, label: Div, meta: Option<String>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(34.))
        .px(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(9.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(13.))
        .when(selected, |d| d.bg(rgba(FILL_2)))
        .when(!selected, |d| d.hover(|s| s.bg(rgba(FILL_2))))
        .children(lead)
        .child(label.min_w_0().truncate().font_weight(FontWeight::MEDIUM))
        .child(div().ml_auto().pl(px(10.)).flex_none().text_size(px(12.)).text_color(rgba(TEXT_4)).children(meta))
        .child(div().w(px(16.)).flex().flex_none().justify_end().when(selected, |d| d.child(icon("check", 14., TEXT))))
}

fn picker_menu(id: &'static str, width: f32, rows: Vec<AnyElement>, cx: &mut Context<Desktop>) -> Stateful<Div> {
    ui::pop(div().id(id))
        .w(px(width))
        .max_h(px(360.))
        .overflow_y_scroll()
        .p(px(5.))
        .rounded(px(10.))
        .flex()
        .flex_col()
        .children(rows)
        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.new_form.draft.picker = None;
            cx.notify();
        }))
}

impl Desktop {
    pub fn close_picker(&mut self) -> bool {
        self.new_form.draft.picker.take().is_some()
    }

    fn toggle_picker(&mut self, picker: Picker, cx: &mut Context<Self>) {
        let f = &mut self.new_form.draft;
        f.picker = (f.picker != Some(picker)).then_some(picker);
        cx.notify();
    }

    fn agent_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let f = &self.new_form.draft;
        let mut rows = Vec::new();
        for provider in ["claude", "codex"] {
            let model = self.model_hint(provider).unwrap_or_else(|| "Default model".into());
            rows.push(pick_head(provider_name(provider)).into_any_element());
            rows.push(
                pick_row(provider, f.provider == provider, Some(ui::dot(7., provider_color(provider))), div().child(model), None)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        (this.new_form.draft.provider, this.new_form.draft.picker) = (provider, None);
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        rows.push(pick_head("Permissions").into_any_element());
        for (perm, label) in [(Perm::Ask, "Ask"), (Perm::AutoEdit, "Auto-edit"), (Perm::Plan, "Plan only")] {
            rows.push(
                pick_row(label, f.perm == perm, None::<Div>, div().child(label), None)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        (this.new_form.draft.perm, this.new_form.draft.picker) = (perm, None);
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        picker_menu("agent-menu", 260., rows, cx)
    }

    fn branch_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let f = &self.new_form.draft;
        let now = crate::util::now_ms();
        let branch = |name: &str| div().font_family(MONO).text_size(px(12.5)).child(name.to_string());
        let mut rows = Vec::new();
        for (i, (name, at)) in f.branches.iter().enumerate() {
            match i {
                0 => rows.push(pick_head("Default").into_any_element()),
                1 => rows.push(pick_head("Recent").into_any_element()),
                _ => {}
            }
            let meta = at.map(|s| crate::util::ago_long(s * 1000, now));
            rows.push(
                pick_row(("base", i), f.base == i, Some(icon("branch", 13., TEXT_3)), branch(name), meta)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        (this.new_form.draft.base, this.new_form.draft.picker) = (i, None);
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        picker_menu("branch-menu", 300., rows, cx)
    }

    pub(super) fn agent_select(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form.draft;
        let mut model = self.model_hint(f.provider).unwrap_or_else(|| "Default model".into());
        match f.perm {
            Perm::Ask => {}
            Perm::AutoEdit => model.push_str(" · auto-edit"),
            Perm::Plan => model.push_str(" · plan only"),
        }
        let agent = chip("form-agent", f.picker == Some(Picker::Agent))
            .child(ui::dot(7., provider_color(f.provider)))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(provider_name(f.provider)))
            .child(div().text_color(rgba(TEXT_3)).child(model))
            .child(icon("chevron-down", 12., TEXT_4))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.toggle_picker(Picker::Agent, cx);
            }));
        let agent_menu = (f.picker == Some(Picker::Agent)).then(|| ui::dropdown(36., self.agent_picker(cx)));
        div().relative().child(agent).children(agent_menu)
    }

    pub(super) fn branch_select(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form.draft;
        let branch = chip("form-branch", f.picker == Some(Picker::Branch))
            .child(icon("branch", 14., TEXT_3))
            .child(div().font_family(MONO).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(f.base_branch()))
            .child(icon("chevron-down", 12., TEXT_4))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.toggle_picker(Picker::Branch, cx);
            }));
        let branch_menu = (f.picker == Some(Picker::Branch)).then(|| ui::dropdown(36., self.branch_picker(cx)));
        div().relative().child(branch).children(branch_menu)
    }
}
