use super::Source;
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Picker {
    Agent,
    Source,
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
        .bg(if open { FILL_3 } else { FILL_2 })
        .hover(|s| s.bg(FILL_3))
}

fn pick_head(label: &str) -> Div {
    div().pt(px(8.)).px(px(8.)).pb(px(4.)).text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child(label.to_string())
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
        .when(selected, |d| d.bg(FILL_2))
        .when(!selected, |d| d.hover(|s| s.bg(FILL_2)))
        .children(lead)
        .child(label.min_w_0().truncate().font_weight(FontWeight::MEDIUM))
        .child(div().ml_auto().pl(px(10.)).flex_none().text_size(px(12.)).text_color(TEXT_4).children(meta))
        .child(div().w(px(16.)).flex().flex_none().justify_end().when(selected, |d| d.child(icon("check", 14., TEXT))))
}

fn picker_menu(id: &'static str, width: f32, rows: Vec<AnyElement>, cx: &mut Context<Desktop>) -> Stateful<Div> {
    ui::pop(div().id(id))
        .w(px(width))
        .max_h(px(360.))
        .overflow_y_scroll()
        .p(px(5.))
        .flex()
        .flex_col()
        .children(rows)
        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.new_form.draft.picker = None;
            cx.notify();
        }))
}

pub(crate) fn access_row(id: impl Into<ElementId>, selected: bool, label: &str, hint: &str, color: Token) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(8.))
        .py(px(7.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(9.))
        .rounded(px(6.))
        .cursor_pointer()
        .when(selected, |d| d.bg(FILL_2))
        .when(!selected, |d| d.hover(|s| s.bg(FILL_2)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(color).child(label.to_string()))
                .child(div().text_size(px(11.5)).text_color(TEXT_3).child(hint.to_string())),
        )
        .child(div().w(px(16.)).flex().flex_none().justify_end().when(selected, |d| d.child(icon("check", 14., TEXT))))
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
            rows.push(
                pick_row(provider, f.provider == provider, Some(provider_icon(provider, 13., TEXT)), div().child(provider_name(provider)), None)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.new_form.draft.pick_provider(provider);
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

    fn source_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let f = &self.new_form.draft;
        let mut rows = vec![pick_head("Start from").into_any_element()];
        for source in Source::ALL {
            rows.push(
                pick_row(source.label(), f.source == source, None::<Div>, div().child(source.label()), None)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.pick_source(source, window, cx)))
                    .into_any_element(),
            );
        }
        picker_menu("source-menu", 220., rows, cx)
    }

    /// Local branches, then origin's, that hold what the name field holds. Picking one fills the field.
    fn open_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let f = &self.new_form.draft;
        let typed = self.new_form.name.read(cx).value().trim().to_lowercase();
        let local = f.branches.iter().map(|(b, _)| (b, false));
        let remote = f.remote_branches.iter().map(|b| (b, true));
        let mut rows = Vec::new();
        let mut head = None;
        for (i, (b, on_origin)) in local.chain(remote).filter(|(b, _)| b.to_lowercase().contains(&typed)).take(50).enumerate() {
            if head != Some(on_origin) {
                rows.push(pick_head(if on_origin { "On origin" } else { "Local" }).into_any_element());
                head = Some(on_origin);
            }
            let label = if on_origin { format!("origin/{b}") } else { b.clone() };
            let name = b.clone();
            rows.push(
                pick_row(("open", i), b.to_lowercase() == typed, Some(icon("branch", 13., TEXT_3)), div().font_family(MONO).text_size(px(12.5)).child(label), None)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.new_form.name.update(cx, |s, cx| s.set_value(name.clone(), window, cx));
                        this.new_form.draft.picker = None;
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        picker_menu("open-menu", 300., rows, cx)
    }

    pub(super) fn source_select(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form.draft;
        let source = chip("form-source", f.picker == Some(Picker::Source))
            .child(div().font_weight(FontWeight::MEDIUM).child(f.source.label()))
            .child(icon("chevron-down", 12., TEXT_4))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.toggle_picker(Picker::Source, cx);
            }));
        let source_menu = (f.picker == Some(Picker::Source)).then(|| ui::dropdown(36., ui::menu_in("source-menu-in", self.source_picker(cx))));
        div().relative().child(source).children(source_menu)
    }

    pub(super) fn agent_select(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form.draft;
        let agent = chip("form-agent", f.picker == Some(Picker::Agent))
            .child(provider_icon(f.provider, 13., TEXT))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(provider_name(f.provider)))
            .child(icon("chevron-down", 12., TEXT_4))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.toggle_picker(Picker::Agent, cx);
            }));
        let agent_menu = (f.picker == Some(Picker::Agent)).then(|| ui::dropdown(36., ui::menu_in("agent-menu-in", self.agent_picker(cx))));
        div().relative().child(agent).children(agent_menu)
    }

    /// The base to branch from, or for an existing branch, the list to pick it from.
    pub(super) fn branch_select(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form.draft;
        let label = if f.source == Source::New { f.base_branch() } else { "Branches".into() };
        let branch = chip("form-branch", f.picker == Some(Picker::Branch))
            .child(icon("branch", 14., TEXT_3))
            .child(div().font_family(MONO).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(label))
            .child(icon("chevron-down", 12., TEXT_4))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.toggle_picker(Picker::Branch, cx);
            }));
        let menu = |cx: &mut Context<Self>| if f.source == Source::New { self.branch_picker(cx) } else { self.open_picker(cx) };
        let branch_menu = (f.picker == Some(Picker::Branch)).then(|| ui::dropdown(36., ui::menu_in("branch-menu-in", menu(cx))));
        div().relative().child(branch).children(branch_menu)
    }
}
