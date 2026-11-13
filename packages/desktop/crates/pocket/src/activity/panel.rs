use super::card::col_glyph;
use super::logic::{column, label};
use crate::desktop::Desktop;
use crate::inbox::first_line;
use crate::git_ui::pull_requests::{detail, glyph as pr_glyph, tone};
use crate::util::{ago, now_ms};
use gpui_kit::*;
use theme::*;
use ui::Variant;

impl Desktop {
    fn activity_ask(&self, id: &str, cx: &mut Context<Self>) -> Option<Div> {
        let p = self.agents.pending.iter().find(|p| p.agent_id == id)?;
        let answer = self.activity_answers(id, cx).map(IntoElement::into_any_element).unwrap_or_else(|| div().text_size(px(12.)).text_color(TEXT_3).child("Answer it in the session.").into_any_element());
        Some(div().p(px(10.)).rounded(px(9.)).bg(WAITING_BG).flex().flex_col().gap(px(8.)).child(div().text_size(px(13.)).line_height(px(19.)).child(p.ask())).child(answer))
    }

    pub(super) fn activity_panel(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let id = self.activity.previewed.clone()?;
        let item = self.activity.item(&id)?;
        let col = column(item.status, item.pr_open);
        let chip = div().h(px(22.)).px(px(8.)).flex().flex_none().items_center().gap(px(5.)).rounded(px(11.)).bg(FILL_2).text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child(col_glyph(col)).child(label(item.status, item.pr_open));
        let go = id.clone();
        let open = ui::button("activity-open-session", Variant::Primary, None, "Open session").child(ui::kbd("↵")).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_previewed(&go, window, cx)));
        let close = ui::icon_button_sized("activity-panel-close", "x", 24., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.activity.previewed = None;
            cx.notify();
        }));
        let head = div().flex().items_center().gap(px(8.)).child(chip).child(div().flex_1()).child(open).child(close);
        let title = div().text_size(px(15.)).line_height(px(20.)).font_weight(FontWeight::SEMIBOLD).child(item.title.clone());
        let meta = div().flex().flex_wrap().items_center().gap(px(8.)).text_size(px(12.)).text_color(TEXT_3).child(item.project_name.clone()).child(self.agent_name(&item.provider)).child(ago(item.at, now_ms())).children((!item.branch.is_empty()).then(|| div().font_family(MONO).child(item.branch.clone())));
        let ask = self.activity_ask(&id, cx);
        let said = if item.note.is_empty() { first_line(self.agents.last_text(&id).unwrap_or_default()) } else { item.note.clone() };
        let note = (ask.is_none() && !said.is_empty()).then(|| div().text_size(px(13.)).line_height(px(19.)).text_color(TEXT_2).child(said));
        let pr = self.prs.get(&item.tree).map(|pr| {
            let (name, color) = pr_glyph(pr);
            let line = div().flex().items_center().gap(px(6.)).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(icon(name, 13., color)).child(div().min_w_0().truncate().child(format!("#{} {}", pr.number, pr.title)));
            div().p(px(10.)).rounded(px(9.)).border_1().border_color(SEPARATOR).flex().flex_col().gap(px(4.)).child(line).child(div().text_size(px(12.)).text_color(tone(pr)).child(detail(pr)))
        });
        let panel = ui::pop(div().id("activity-panel"))
            .absolute()
            .top(px(52.))
            .right(px(12.))
            .bottom(px(12.))
            .w(px(400.))
            .p(px(14.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.activity.previewed = None;
                cx.notify();
            }))
            .child(head)
            .child(title)
            .child(meta)
            .children(ask)
            .children(note)
            .children(pr);
        Some(panel)
    }
}
