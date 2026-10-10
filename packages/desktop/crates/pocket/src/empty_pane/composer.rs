use super::Menu;
use crate::desktop::Desktop;
use crate::modals::new_session::launch::{self, Pick};
use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;
use store::LaunchPick;
use theme::*;
use workspace::Place;

impl Desktop {
    /// Draws the prompt, taking the keys if it wasn't drawn last frame: its pane's last tab closed, or the pane, worktree or screen changed.
    pub(crate) fn empty_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if self.empty_pane.drawn.draw() {
            self.focus_empty_prompt(window, cx);
        }
        let pick = self.launch_pick();
        let tree = self.cwd().unwrap_or_default();
        let launch = &self.empty_pane.launch;
        let starting = launch.starting_in(&tree).then(|| div().mt(px(12.)).child(ui::busy("empty-starting", "Starting…")));
        let error = launch.error_in(&tree).cloned().map(|(message, detail)| ui::failure(message, detail).w_full().max_w(px(600.)).mt(px(10.)));
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .pt(px(72.))
            .px(px(24.))
            .pb(px(56.))
            .child(div().text_size(px(21.)).line_height(px(25.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(format!("What should {} work on?", self.agent_name(&pick.provider))))
            .children(self.empty_branch_line())
            .child(self.empty_input(&pick, cx))
            .children(starting)
            .children(error)
            .child(div().w_full().max_w(px(600.)).mt(px(14.)).flex().flex_wrap().justify_center().gap(px(6.)).children(starters(&self.store.agents.starters, cx)))
            .child(div().mt(px(22.)).flex().items_center().gap(px(2.)).text_size(px(12.5)).text_color(TEXT_3).child(div().mr(px(4.)).child("or open")).child(terminal_link(cx)))
    }

    fn empty_branch_line(&self) -> Option<Div> {
        let r = self.repo()?;
        let files = match r.files.len() {
            0 => None,
            1 => Some("1 changed file".to_string()),
            n => Some(format!("{n} changed files")),
        };
        Some(
            div()
                .mt(px(7.))
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(13.))
                .text_color(TEXT_3)
                .whitespace_nowrap()
                .child(icon("branch", 13., TEXT_3))
                .child(r.branch.clone())
                .when_some(files, |d, files| d.child("·").child(files)),
        )
    }

    fn empty_input(&self, pick: &LaunchPick, cx: &mut Context<Self>) -> Div {
        let ready = self.empty_pane.launch.ready(&self.empty_pane.prompt.read(cx).value());
        let send = ui::primary(div().id("empty-start").size(px(30.)).flex().flex_none().items_center().justify_center().rounded(px(15.)))
            .child(icon("arrow-up", 15., ON_PRIMARY))
            .when(ready, |d| d.cursor_pointer().on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start_from_empty_pane(window, cx))))
            .when(!ready, |d| d.opacity(0.25));
        div()
            .w_full()
            .max_w(px(600.))
            .mt(px(20.))
            .flex()
            .flex_col()
            .rounded(px(14.))
            .bg(SURFACE)
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5), ui::shadow(rgba(0x1111130a), 1., 2.), ui::shadow(rgba(0x1111130d), 10., 30.)])
            // The textarea pads itself 8px × 10px and wraps 10px short of its edge; the frame restores the design's 14/16/4 and its line breaks.
            .child(div().pt(px(6.)).pl(px(6.)).mr(px(-6.)).child(Textarea::new(&self.empty_pane.prompt).appearance(false).text_size(px(15.)).line_height(px(23.))))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .pt(px(4.))
                    .px(px(10.))
                    .pb(px(10.))
                    .text_size(px(13.))
                    .child(self.empty_agent(&pick.provider, cx))
                    .child(self.empty_model(pick, cx))
                    .child(self.empty_access(&pick.access, cx))
                    .child(div().flex_1())
                    .child(ui::kbd("⌘↵").mr(px(8.)))
                    .child(send),
            )
    }

    fn empty_agent(&self, provider: &str, cx: &mut Context<Self>) -> Div {
        let open = self.empty_pane.menu == Some(Menu::Agent);
        let chip = ui::menu_chip("empty-agent", open)
            .child(provider_icon(provider, 13., TEXT))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(self.agent_name(provider)))
            .child(icon("chevron-down", 12., TEXT_4))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.toggle_empty_menu(Menu::Agent, cx);
            }));
        let menu = open.then(|| {
            let rows = self.store.agents.enabled().into_iter().map(|p| {
                ui::pick_row(p, p == provider, Some(provider_icon(p, 13., TEXT)), div().child(self.agent_name(p)), None)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_empty(|pick| pick.switched(p), cx)))
                    .into_any_element()
            });
            empty_menu("empty-agent-menu", 220., rows.collect(), cx)
        });
        div().relative().child(chip).children(menu)
    }

    fn empty_model(&self, pick: &LaunchPick, cx: &mut Context<Self>) -> Div {
        let app = self.store.agents.provider(&pick.provider).model;
        let open = self.empty_pane.menu == Some(Menu::Model);
        let chip = launch::chip("empty-model", open, "sparkle", launch::model_shown(&pick.model, &app), TEXT, |this, cx| {
            this.load_models(cx);
            this.toggle_empty_menu(Menu::Model, cx);
        }, cx);
        let menu = open.then(|| {
            let choose: Pick = Rc::new(|this, model, cx| this.pick_empty(|pick| LaunchPick { model, ..pick }, cx));
            let rows = launch::model_rows("empty-model-row", launch::model_choices(&pick.provider, self.codex_models(), &pick.model, &app), &pick.model, choose, cx);
            empty_menu("empty-model-menu", 220., rows, cx)
        });
        div().relative().child(chip).children(menu)
    }

    fn empty_access(&self, access: &str, cx: &mut Context<Self>) -> Div {
        let app = self.store.agents.access();
        let open = self.empty_pane.menu == Some(Menu::Access);
        let chip = launch::access_chip("empty-access", open, access, app, |this, cx| this.toggle_empty_menu(Menu::Access, cx), cx);
        let menu = open.then(|| {
            let choose: Pick = Rc::new(|this, access, cx| this.pick_empty(|pick| LaunchPick { access, ..pick }, cx));
            empty_menu("empty-access-menu", 300., launch::access_rows("empty-access-row", launch::access_choices(app), access, choose, cx), cx)
        });
        div().relative().child(chip).children(menu)
    }
}

/// A chip's menu, closed by a press outside it.
fn empty_menu(id: &'static str, width: f32, rows: Vec<AnyElement>, cx: &mut Context<Desktop>) -> Div {
    let menu = ui::pop(div().id(id)).w(px(width)).max_h(px(360.)).overflow_y_scroll().p(px(5.)).flex().flex_col().children(rows).on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| this.close_empty_menu(cx)));
    ui::dropdown(36., ui::menu_in(SharedString::from(format!("{id}-in")), menu))
}

fn starters(prompts: &[String], cx: &mut Context<Desktop>) -> Vec<Stateful<Div>> {
    prompts.iter().enumerate().map(|(i, text)| {
        let text: SharedString = text.clone().into();
        div()
            .id(("empty-starter", i))
            .h(px(30.))
            .px(px(12.))
            .flex()
            .items_center()
            .rounded(px(15.))
            .bg(SURFACE_SUNKEN)
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .whitespace_nowrap()
            .cursor_pointer()
            .hover(|s| s.bg(SURFACE).text_color(TEXT))
            .child(text.clone())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.fill_empty_prompt(text.clone(), window, cx)))
    }).collect()
}

fn terminal_link(cx: &mut Context<Desktop>) -> Stateful<Div> {
    div()
        .id("empty-terminal")
        .h(px(26.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(7.))
        .text_color(TEXT_2)
        .cursor_pointer()
        .hover(|s| s.bg(FILL_2).text_color(TEXT))
        .child(icon("prompt", 13., TEXT_2))
        .child("Terminal")
        .child(ui::kbd("⌘T"))
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Place::Pane(Some(this.focused_pane())), cx)))
}
