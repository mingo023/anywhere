//! The plan sheet: the lead's tasks as you'll start them.

use super::plan::{Kept, branch, can_start};
use super::{PROPOSED, plural};
use crate::composer::model_menu::model_name;
use crate::settings::models::model_options;
use crate::composer::{menu_title, picker, pill};
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use gpui_kit::component::input::Input;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

const WIDTH: f32 = 580.;

impl Desktop {
    /// Each task with its agent and model, what it runs without asking, and ×; then Not now and Start.
    pub(crate) fn team_sheet(&self, cx: &mut Context<Self>) -> Div {
        let Some(lead) = self.team.island.sheet.clone() else { return div() };
        let Some(edits) = self.team.island.edits.get(&lead) else { return div() };
        let proposed = self.agents.team(&lead).is_some_and(|t| t.state == PROPOSED);
        let n = edits.tasks().len();
        let head = div()
            .pt(px(20.))
            .px(px(22.))
            .pb(px(14.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(div().text_size(px(17.)).line_height(px(22.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(format!("Start a team of {}?", plural(n, "agent", "agents"))))
            .child(div().text_size(px(13.)).line_height(px(19.)).text_color(TEXT_2).child(format!(
                "Each one works in its own worktree from {} and hands its work back to this chat. Anything they ask comes to you, unless it's on their allowlist.",
                self.lead_branch(&lead)
            )));
        let rows: Vec<AnyElement> = if n == 0 {
            vec![div().px(px(22.)).py(px(14.)).text_size(px(12.5)).text_color(TEXT_3).child("No tasks left. Tell the lead what to plan instead.").into_any_element()]
        } else {
            edits.tasks().iter().enumerate().map(|(i, t)| self.sheet_row(&lead, i, t, edits.after_label(t), edits.removable(&t.id), cx).into_any_element()).collect()
        };
        let startable = can_start(proposed, n);
        let start = ui::button("team-start", Variant::Primary, None, format!("Start {}", plural(n, "agent", "agents")))
            .child(ui::button_kbd("⌘↵"))
            .when(startable, |b| b.on_click(cx.listener(|this, _: &ClickEvent, window, cx| _ = this.team_start(window, cx))))
            .when(!startable, |b| b.opacity(0.4));
        let later = ui::button("team-later", Variant::Ghost, None, "Not now").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let note = if proposed { "Up to 4 run at once." } else { "This plan has already started." };
        let foot = div().pt(px(14.)).pb(px(16.)).pl(px(22.)).pr(px(16.)).flex().items_center().gap(px(8.)).child(div().flex_1().text_size(px(12.5)).text_color(TEXT_3).child(note)).child(later).child(start);
        div().absolute().inset_0().flex().items_center().justify_center().child(
            ui::pop(div().id("team-sheet"))
                .w(px(WIDTH))
                .rounded(px(R_DIALOG))
                .occlude()
                .flex()
                .flex_col()
                .child(head)
                .child(div().border_t(px(0.5)).border_b(px(0.5)).border_color(HAIRLINE).flex().flex_col().children(rows))
                .child(foot),
        )
    }

    fn sheet_row(&self, lead: &str, i: usize, t: &Kept, after: Option<String>, removable: bool, cx: &mut Context<Self>) -> Div {
        let num = div()
            .size(px(20.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .bg(FILL_3)
            .text_size(px(11.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(TEXT_2)
            .child((i + 1).to_string());
        let sub = div()
            .mt(px(3.))
            .flex()
            .items_center()
            .gap(px(8.))
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(12.))
            .text_color(TEXT_3)
            .child(div().flex().items_center().gap(px(4.)).font_family(MONO).text_size(px(11.5)).child(icon("branch", 11., TEXT_3)).child(branch(lead, &t.id)))
            .child(format!("{} in scope", plural(t.files.len(), "file", "files")))
            .children(after);
        let main = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(div().truncate().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(t.title.clone()))
            .child(sub);
        let task = t.id.clone();
        let remove = ui::icon_button(id(format!("team-remove:{}", t.id)), "x")
            .when(removable, |b| b.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.team_remove(&task, cx))))
            .when(!removable, |b| b.opacity(0.3).tooltip(|window, cx| Tooltip::new("Another task waits for this one").build(window, cx)));
        div()
            .pl(px(22.))
            .pr(px(12.))
            .py(px(10.))
            .when(i > 0, |d| d.border_t(px(0.5)).border_color(HAIRLINE))
            .child(div().flex().items_start().gap(px(12.)).child(num).child(main).child(self.agent_chip(t, cx)).child(remove))
            // Under the title, across the chip's width too, so "+ Allow" follows the pills instead of wrapping early.
            .child(div().pl(px(32.)).child(self.allow_row(t, cx)))
    }

    /// The task's agent and model; its menu picks both.
    fn agent_chip(&self, t: &Kept, cx: &mut Context<Self>) -> Div {
        let open = self.team.island.menu.as_deref() == Some(t.id.as_str());
        let label = div().whitespace_nowrap().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(model_name(&t.model, self.models_of(&t.agent)));
        let task = t.id.clone();
        let chip = picker(pill().child(provider_icon(&t.agent, 14., TEXT_2)).child(label), id(format!("team-agent:{}", t.id)), open).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            this.team_toggle_menu(&task, cx);
        }));
        let menu = open.then(|| ui::dropdown_right(34., ui::menu_in("team-model-menu-in", self.model_menu(t, cx))));
        div().relative().flex_none().child(chip).children(menu)
    }

    /// Each agent's models, as the composer lists them; with none picked, the one Settings starts that agent with is checked.
    fn model_menu(&self, t: &Kept, cx: &mut Context<Self>) -> Stateful<Div> {
        let enabled = self.store.agents.enabled();
        let mut rows: Vec<AnyElement> = Vec::new();
        for agent in agents::providers::IDS.into_iter().filter(|a| enabled.contains(a)) {
            let settings = self.store.agents.provider(agent).model;
            let picked = if t.agent == agent { t.model.as_str() } else { "" };
            rows.push(menu_title(self.agent_name(agent)).into_any_element());
            let runs = if picked.is_empty() { settings.as_str() } else { picked };
            for (i, (model, label)) in model_options(agent, self.models_of(agent), picked).into_iter().enumerate() {
                let chosen = t.agent == agent && model == runs;
                let task = t.id.clone();
                let row = ui::menu_pick(id(format!("team-model:{}:{agent}:{i}", t.id)), provider_icon(agent, 14., TEXT_2), &label, chosen)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.team_pick(&task, agent, &model, cx)));
                rows.push(row.into_any_element());
            }
        }
        let task = t.id.clone();
        ui::pop(div().id("team-model-menu"))
            .w(px(260.))
            .max_h(px(360.))
            .overflow_y_scroll()
            .p(px(5.))
            .flex()
            .flex_col()
            .gap(px(2.))
            // Menus hang over the sheet; without this a click on one also lands on what it covers.
            .occlude()
            .children(rows)
            .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                if this.team.island.menu.as_deref() == Some(task.as_str()) {
                    this.team.island.menu = None;
                    cx.notify();
                }
            }))
    }

    /// What the task runs without asking: each prefix with its ×, then Allow, or the input while adding.
    fn allow_row(&self, t: &Kept, cx: &mut Context<Self>) -> Div {
        let pills: Vec<AnyElement> = t
            .allow
            .iter()
            .enumerate()
            .map(|(i, prefix)| {
                let (task, p) = (t.id.clone(), prefix.clone());
                let x = div()
                    .id(id(format!("team-disallow:{}:{i}", t.id)))
                    .size(px(16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .hover(|s| s.bg(FILL_4))
                    .child(icon("x", 9., TEXT_3))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.team_disallow(&task, &p, cx)));
                div().h(px(20.)).pl(px(6.)).pr(px(2.)).flex().flex_none().items_center().gap(px(2.)).rounded(px(5.)).bg(FILL_2).font_family(MONO).text_size(px(11.5)).text_color(TEXT_2).child(prefix.clone()).child(x).into_any_element()
            })
            .collect();
        let add = if self.team.island.adding.as_deref() == Some(t.id.as_str()) {
            div().w(px(160.)).h(px(20.)).px(px(6.)).flex().items_center().rounded(px(5.)).bg(SURFACE).shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)]).font_family(MONO).child(Input::new(&self.team.allow).appearance(false).p_0().text_size(px(11.5))).into_any_element()
        } else {
            let task = t.id.clone();
            div()
                .id(id(format!("team-allow:{}", t.id)))
                .h(px(20.))
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(3.))
                .rounded(px(5.))
                .cursor_pointer()
                .text_size(px(11.5))
                .text_color(TEXT_3)
                .hover(|s| s.bg(FILL_2).text_color(TEXT))
                .child(icon("plus", 10., TEXT_3))
                .child("Allow")
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.team_begin_allow(&task, window, cx)))
                .into_any_element()
        };
        div().mt(px(6.)).flex().flex_wrap().items_center().gap(px(4.)).text_size(px(12.)).text_color(TEXT_3).child(div().mr(px(2.)).child("Runs without asking")).children(pills).child(add)
    }
}
