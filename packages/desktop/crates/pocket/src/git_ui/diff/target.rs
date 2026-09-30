use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::dot;

impl Desktop {
    fn session_chip(&self, id: &str) -> (u32, String, String) {
        let a = self.agents.get(id);
        let provider = a.map(|a| a.provider.clone()).unwrap_or_default();
        let branch = a.and_then(|a| self.terminals.sessions.get(&a.terminal_id)).and_then(|s| self.repos.get(&s.info.cwd)).map(|r| r.branch.clone()).unwrap_or_default();
        (provider_color(&provider), provider, branch)
    }

    pub(super) fn target_picker(&self, target: Option<String>, cx: &mut Context<Self>) -> Div {
        let pill = div()
            .id("comment-target")
            .h(px(32.))
            .px(px(12.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(7.))
            .rounded(px(16.))
            .bg(rgba(FILL_3))
            .cursor_pointer()
            .hover(|s| s.bg(rgba(FILL_4)))
            .text_size(px(13.))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.diff.target_menu = !this.diff.target_menu;
                cx.notify();
            }));
        let pill = match &target {
            Some(id) => {
                let (color, name, branch) = self.session_chip(id);
                pill.child(dot(7., color))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
                    .when(!branch.is_empty(), |d| {
                        d.child(div().text_color(rgba(TEXT_6)).child("·")).child(div().font_family(MONO).text_size(px(11.5)).text_color(rgba(TEXT_2)).child(branch))
                    })
            }
            None => pill.text_color(rgba(TEXT_2)).child("No session"),
        };
        let menu = self.diff.target_menu.then(|| {
            let cards = self.project.as_deref().map(|p| self.cards(p)).unwrap_or_default();
            let items = cards.into_iter().enumerate().map(|(i, c)| {
                let (color, name, branch) = self.session_chip(&c.id);
                let picked = target.as_ref() == Some(&c.id);
                let id = c.id.clone();
                div()
                    .id(("target", i))
                    .h(px(34.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(10.))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(FILL_3)))
                    .text_size(px(13.))
                    .child(dot(7., color))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
                    .child(div().flex_1().min_w_0().truncate().text_color(rgba(TEXT_2)).child(c.title))
                    .child(div().font_family(MONO).text_size(px(11.5)).text_color(rgba(TEXT_3)).child(branch))
                    .child(div().size(px(14.)).when(picked, |d| d.child(icon("check", 14., TEXT))))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.diff.target = Some(id.clone());
                        this.diff.target_menu = false;
                        cx.notify();
                    }))
            });
            deferred(
                anchored().anchor(Anchor::BottomLeft).offset(point(px(0.), px(-6.))).snap_to_window_with_margin(px(8.)).child(
                    ui::pop(div().id("target-menu"))
                        .w(px(360.))
                        .p(px(6.))
                        .flex()
                        .flex_col()
                        .children(items)
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.diff.target_menu = false;
                            cx.notify();
                        })),
                ),
            )
            .with_priority(1)
        });
        div().relative().child(pill.child(icon("chevron-down", 12., TEXT_3))).children(menu)
    }
}
