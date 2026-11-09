//! The team's tasks, over its capsule: what each is doing; a press opens its worker's chat, Stop stops it.

use super::progress::{self, Mark, Verdict};
use crate::team::plural;
use crate::composer::model_menu::model_name;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use agents::{Team, TeamTask};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{State, Variant};

fn mark_glyph(key: ElementId, m: Mark) -> AnyElement {
    let state = match m {
        Mark::Running => State::Working,
        Mark::NeedsYou => State::NeedsYou,
        Mark::Done => State::Done(0, 0),
        Mark::Failed => State::Failed,
        Mark::Merged => return icon("merge", 13., MERGED).into_any_element(),
        Mark::Stopped => return icon("stop", 12., TEXT_4).into_any_element(),
        Mark::Waiting => return div().size(px(11.)).flex_none().rounded_full().border_1().border_color(TEXT_5).into_any_element(),
    };
    ui::tone(state).map_or_else(|| div().into_any_element(), |t| ui::glyph(key, t))
}

impl Desktop {
    pub(super) fn team_list(&self, team: &Team, lead_working: bool, into: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let lead = team.lead.clone();
        let close = ui::icon_button(id(format!("team-list-close:{lead}")), "x").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.team.island.list = None;
            cx.notify();
        }));
        let head = div()
            .h(px(42.))
            .pl(px(16.))
            .pr(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .border_b(px(0.5))
            .border_color(HAIRLINE)
            .text_size(px(13.))
            .child(icon("agent-map", 14., TEXT_2))
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Team"))
            .child(div().min_w_0().truncate().text_size(px(12.5)).text_color(TEXT_3).child(progress::summary(&progress::counts(&team.tasks), team.tasks.len())))
            .child(div().flex_1())
            .child(close);
        let rows: Vec<AnyElement> = team.tasks.iter().enumerate().map(|(i, t)| self.list_row(team, i, t, cx).into_any_element()).collect();
        ui::pop(div().id(id(format!("team-list:{lead}"))))
            .w(px(540.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .child(head)
            .children(rows)
            .children(progress::verdict(team, lead_working, into).map(|v| self.list_verdict(team, v, cx)))
            .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                if this.team.island.list.as_deref() == Some(lead.as_str()) {
                    this.team.island.list = None;
                    cx.notify();
                }
            }))
    }

    /// The lead's verdict, the files a merge conflicted in, the lines changed, and Merge.
    fn list_verdict(&self, team: &Team, v: Verdict, cx: &mut Context<Self>) -> Div {
        let merge = v.merge.map(|ready| {
            let lead = team.lead.clone();
            let variant = if ready { Variant::Primary } else { Variant::Secondary };
            ui::button(id(format!("team-merge:{lead}")), variant, Some("merge"), format!("Merge {}", plural(team.tasks.len(), "branch", "branches")))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.team_merge(&lead, window, cx)))
        });
        let text = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.5))
            .text_color(if v.warn { WAITING_TEXT } else { TEXT_BODY })
            .when(v.warn, |d| d.child(icon("warning", 13., WAITING)))
            .child(div().min_w_0().child(v.text));
        let files = (!v.files.is_empty()).then(|| div().mt(px(6.)).flex().flex_col().gap(px(2.)).font_family(MONO).text_size(px(12.)).text_color(TEXT_BODY).children(v.files.into_iter().map(|f| div().truncate().child(f))));
        let totals = v.merge.and_then(|_| progress::totals(&team.tasks)).map(|(added, removed)| ui::diffstat(added, removed));
        div()
            .px(px(16.))
            .py(px(12.))
            .border_t(px(0.5))
            .border_color(HAIRLINE)
            .flex()
            .items_center()
            .gap(px(10.))
            .child(div().flex_1().min_w_0().flex().flex_col().child(text).children(files))
            .children(totals)
            .children(merge)
    }

    fn list_row(&self, team: &Team, i: usize, t: &TeamTask, cx: &mut Context<Self>) -> Stateful<Div> {
        let m = progress::mark(&t.state);
        let live = matches!(m, Mark::Running | Mark::NeedsYou);
        let stat = progress::stat_counts(&t.stat).filter(|_| matches!(m, Mark::Done | Mark::Merged));
        let stop = live.then(|| {
            let (lead, task) = (team.lead.clone(), t.id.clone());
            ui::button(id(format!("team-stop:{}", t.id)), Variant::Ghost, None, "Stop").h(px(24.)).px(px(8.)).text_size(px(12.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.team_stop(&lead, &task, cx);
            }))
        });
        let worker = t.worker.clone();
        let who = div().flex_none().flex().items_center().gap(px(5.)).text_size(px(12.)).text_color(TEXT_3).child(provider_icon(&t.agent, 12., TEXT_3)).child(model_name(&t.model, self.models_of(&t.agent)));
        div()
            .id(id(format!("team-row:{}", t.id)))
            .h(px(40.))
            .px(px(16.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .text_size(px(13.))
            .when(i > 0, |d| d.border_t(px(0.5)).border_color(HAIRLINE))
            .when(!worker.is_empty(), |d| d.cursor_pointer().hover(|s| s.bg(FILL_2)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.team_open_worker(&worker, window, cx))))
            .child(div().w(px(14.)).flex().flex_none().justify_center().child(mark_glyph(id(format!("team-mark:{}", t.id)), m)))
            .child(div().flex_none().max_w(px(220.)).truncate().font_weight(FontWeight::MEDIUM).child(t.title.clone()))
            .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).text_color(TEXT_3).child(progress::now_text(t, &team.tasks)))
            .child(who)
            .child(div().w(px(64.)).flex().flex_none().justify_end().children(stat.map(|(_, added, removed)| ui::diffstat(added, removed))).children(stop))
    }
}
