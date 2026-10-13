use crate::desktop::Desktop;
use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use workspace::Place;

impl Desktop {
    /// Draws the prompt, taking the keys if it wasn't drawn last frame: its pane's last tab closed, or the pane, worktree or screen changed.
    pub(crate) fn empty_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if self.empty_pane.drawn.draw() {
            self.empty_pane.field.hide();
            self.focus_empty_prompt(window, cx);
        }
        let pick = self.launch_pick();
        let tree = self.place().unwrap_or_default();
        let launch = &self.empty_pane.launch;
        let starting = launch.starting_in(&tree).then(|| div().mt(px(12.)).child(ui::busy("empty-starting", "Starting…")));
        let error = launch.error_in(&tree).cloned().map(|(message, detail)| ui::failure(message, detail).w_full().max_w(px(600.)).mt(px(10.)));
        let content = div()
            .flex()
            .flex_col()
            .items_center()
            .child(div().text_size(px(21.)).line_height(px(25.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(format!("What should {} work on?", self.agent_name(&pick.provider))))
            .children(self.empty_branch_line())
            .child(self.empty_input(cx))
            .children(starting)
            .children(error)
            .child(div().w_full().max_w(px(600.)).mt(px(14.)).flex().flex_wrap().justify_center().gap(px(6.)).children(starters(&self.store.agents.starters, cx)))
            .child(div().mt(px(22.)).flex().items_center().gap(px(2.)).text_size(px(12.5)).text_color(TEXT_3).child(div().mr(px(4.)).child("or open")).child(terminal_link(cx)));
        self.with_field(|d| &mut d.empty_pane.field, "empty-pane-body", content, cx)
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

    fn empty_input(&self, cx: &mut Context<Self>) -> Div {
        let c = &self.empty_pane.composer;
        let ready = self.empty_pane.launch.ready(&c.draft.prompt(&c.prompt.read(cx).value()));
        let send = ui::primary(div().id("empty-start").size(px(30.)).flex().flex_none().items_center().justify_center().rounded(px(15.)))
            .child(icon("arrow-up", 15., ON_PRIMARY))
            .when(ready, |d| d.cursor_pointer().on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start_from_empty_pane(window, cx))))
            .when(!ready, |d| d.opacity(0.25));
        let textarea = Textarea::new(&c.prompt).line_height(px(23.));
        let trailing = vec![ui::kbd("⌘↵").mr(px(8.)).into_any_element(), send.into_any_element()];
        div().w_full().max_w(px(600.)).mt(px(20.)).child(self.composer(c, self.project.clone(), textarea, vec![], trailing, cx))
    }
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
