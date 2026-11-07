use super::{Composer, Menu, launch};
use crate::desktop::Desktop;
use crate::modals::new_session::picker::menu_input;
use crate::settings::models::{effort_label, model_label};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;
use store::LaunchPick;
use store::prefs::agents::Provider;
use theme::*;

pub(crate) const ROW: f32 = 34.;
pub(crate) const GAP: f32 = 2.;
pub(crate) const PAD: f32 = 5.;

/// The pill's model and effort: the ones picked, else Settings'; with neither, the agent's name stands for its own default.
fn pill_label(agent: &str, pick: &LaunchPick, app: &Provider, efforts: &[String], names: &[(String, String)]) -> (String, Option<String>) {
    let model = [&pick.model, &app.model].into_iter().find(|m| !m.is_empty()).map_or_else(|| agent.to_string(), |m| model_name(m, names));
    let effort = [&pick.effort, &app.effort].into_iter().find(|e| !e.is_empty()).filter(|e| efforts.contains(e));
    (model, effort.map(|e| effort_label(e)))
}

/// Codex's listed name for its slug, else the model's label.
pub(crate) fn model_name(model: &str, names: &[(String, String)]) -> String {
    names.iter().find(|(slug, _)| slug == model).map_or_else(|| model_label(model), |(_, name)| name.clone())
}

/// The choices whose label holds `query`, any case.
fn matching(choices: Vec<(String, String)>, query: &str) -> Vec<(String, String)> {
    let query = query.trim().to_lowercase();
    choices.into_iter().filter(|(_, label)| label.to_lowercase().contains(&query)).collect()
}

pub(crate) fn row(id: impl Into<ElementId>, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(ROW))
        .px(px(10.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(8.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(13.))
        .when(selected, |d| d.bg(FILL_2))
        .when(!selected, |d| d.hover(|s| s.bg(FILL_2)))
}

pub(crate) fn rows(id: &'static str, rows: Vec<Stateful<Div>>) -> Stateful<Div> {
    div().id(id).flex().flex_col().gap(px(GAP)).children(rows)
}

pub(crate) fn check(on: bool) -> Div {
    div().w(px(16.)).flex().flex_none().justify_end().when(on, |d| d.child(icon("check", 14., TEXT)))
}

impl Desktop {
    /// The agent and model a session starts with; its menu lists each agent's models.
    pub(crate) fn model_pill(&self, c: &Composer, pick: &LaunchPick, project: Option<String>, cx: &mut Context<Self>) -> Div {
        let (slot, open, search) = (c.slot, c.draft.menu == Some(Menu::Model), c.search.clone());
        let (model, _) = pill_label(&self.agent_name(&pick.provider), pick, &self.store.agents.provider(&pick.provider), &self.agents.provider_info(&pick.provider).efforts, self.catalog_models());
        let pill = super::pill().child(provider_icon(&pick.provider, 16., TEXT_2)).child(div().whitespace_nowrap().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(model));
        let chip = super::picker(pill, "composer-model", open).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            this.load_models(cx);
            this.toggle_composer(slot, Menu::Model, cx);
            if slot(this).draft.menu == Some(Menu::Model) {
                search.update(cx, |s, cx| {
                    s.set_value("", window, cx);
                    s.focus(window, cx);
                });
            }
        }));
        let menu = open.then(|| {
            let menu = div()
                .id("composer-model-menu")
                // Menus hang over the page; without this a click on one also lands on what it covers.
                .occlude()
                .child(self.model_side(c, pick, project, cx))
                .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    slot(this).draft.menu = None;
                    cx.notify();
                }));
            ui::dropdown(36., ui::menu_in("composer-model-menu-in", menu))
        });
        div().relative().child(chip).children(menu)
    }

    /// The effort a session starts with, once its agent offers levels.
    pub(crate) fn effort_chip(&self, c: &Composer, pick: &LaunchPick, project: Option<String>, cx: &mut Context<Self>) -> Option<Div> {
        let levels = &self.agents.provider_info(&pick.provider).efforts;
        if levels.is_empty() {
            return None;
        }
        let (slot, open) = (c.slot, c.draft.menu == Some(Menu::Effort));
        let (_, effort) = pill_label(&self.agent_name(&pick.provider), pick, &self.store.agents.provider(&pick.provider), levels, self.catalog_models());
        let chip = launch::chip("composer-effort", open, "bulb", effort.unwrap_or_else(|| "Default".into()), TEXT, move |this, cx| this.toggle_composer(slot, Menu::Effort, cx), cx);
        let menu = open.then(|| {
            let current = pick.clone();
            let choose: launch::Pick = Rc::new(move |this, effort, cx| this.choose_launch(slot, project.clone(), LaunchPick { effort, ..current.clone() }, cx));
            let levels = std::iter::once(String::new()).chain(levels.iter().cloned()).collect();
            super::menu("composer-effort-menu", "Effort", 260., launch::effort_rows("composer-effort-row", levels, &pick.effort, choose, cx), slot, cx)
        });
        Some(div().relative().child(chip).children(menu))
    }

    /// The agents as a rail of icons; pointing at one lists its models, and only picking one of those switches agent.
    fn model_side(&self, c: &Composer, pick: &LaunchPick, project: Option<String>, cx: &mut Context<Self>) -> Div {
        let slot = c.slot;
        let viewed = c.draft.viewing.unwrap_or(&pick.provider);
        let rail = self.store.agents.enabled().into_iter().map(|p| {
            div()
                .id(p)
                .size(px(36.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(9.))
                .cursor_pointer()
                .when(p == viewed, |d| d.bg(FILL_3))
                .when(p != viewed, |d| d.hover(|s| s.bg(FILL_2)))
                .child(provider_icon(p, 16., TEXT))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    let draft = &mut slot(this).draft;
                    if *hovered && draft.viewing != Some(p) {
                        draft.viewing = Some(p);
                        cx.notify();
                    }
                }))
        }).collect::<Vec<_>>();

        let same = viewed == pick.provider;
        let picked = if same { pick.model.as_str() } else { "" };
        let choices = matching(launch::model_choices(viewed, self.catalog_models(), picked, &self.store.agents.provider(viewed).model), &c.search.read(cx).value());
        let chosen = same.then(|| choices.iter().position(|(v, _)| *v == picked)).flatten();
        let empty = choices.is_empty();
        let base = pick.switched(viewed);
        let list_rows = choices
            .into_iter()
            .enumerate()
            .map(|(i, (value, label))| {
                let (project, pick) = (project.clone(), LaunchPick { model: value.clone(), ..base.clone() });
                row(("composer-model-row", i), chosen == Some(i))
                    .child(div().flex_1().min_w_0().truncate().font_weight(FontWeight::MEDIUM).child(label))
                    .child(check(chosen == Some(i)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.choose_launch(slot, project.clone(), pick.clone(), cx)))
            })
            .collect();
        let list = div()
            .id("composer-model-list")
            .max_h(px(300.))
            .overflow_y_scroll()
            .p(px(PAD))
            .child(super::menu_title(self.agent_name(viewed)))
            .child(rows("composer-models", list_rows))
            .when(empty, |d| d.child(div().px(px(10.)).py(px(8.)).text_size(px(13.)).text_color(TEXT_3).child("No models match")));

        ui::pop(div())
            .w(px(380.))
            .flex()
            .overflow_hidden()
            .child(div().p(px(6.)).flex().flex_col().gap(px(4.)).border_r(px(0.5)).border_color(SEPARATOR).children(rail))
            .child(div().flex_1().min_w_0().flex().flex_col().child(menu_input("search", &c.search)).child(list))
    }
}

#[cfg(test)]
mod tests {
    use super::{matching, pill_label};
    use store::LaunchPick;
    use store::prefs::agents::Provider;

    fn pick(provider: &str, model: &str, effort: &str) -> LaunchPick {
        LaunchPick { provider: provider.into(), model: model.into(), effort: effort.into(), ..LaunchPick::default() }
    }

    #[test]
    fn the_pill_names_the_pick_else_settings_else_the_agent() {
        let none = Provider::default();
        let settings = Provider { model: "sonnet".into(), effort: "xhigh".into(), ..Provider::default() };
        let efforts = &agents::providers::builtin("claude").efforts;
        assert_eq!(pill_label("Claude Code", &pick("claude", "", ""), &none, efforts, &[]), ("Claude Code".into(), None));
        assert_eq!(pill_label("Claude Code", &pick("claude", "", ""), &settings, efforts, &[]), ("Sonnet".into(), Some("Extra high".into())));
        assert_eq!(pill_label("Claude Code", &pick("claude", "opus", "high"), &settings, efforts, &[]), ("Opus".into(), Some("High".into())));
    }

    #[test]
    fn a_codex_pill_shows_the_listed_name_and_no_effort() {
        let settings = Provider { effort: "high".into(), ..Provider::default() };
        let names = [("gpt-6-astra".to_string(), "GPT-6-Astra".to_string())];
        assert_eq!(pill_label("Codex", &pick("codex", "gpt-6-astra", ""), &settings, &[], &names), ("GPT-6-Astra".into(), None));
        assert_eq!(pill_label("Codex", &pick("codex", "gpt-5", ""), &settings, &[], &names).0, "gpt-5");
    }

    #[test]
    fn a_search_keeps_the_models_whose_name_holds_it() {
        let choices = |names: &[&str]| names.iter().map(|n| (n.to_lowercase(), n.to_string())).collect::<Vec<_>>();
        let found = matching(choices(&["Default", "GPT-6-Sol", "GPT-6-Astra", "GPT-5.6-Sol"]), " sol ");
        assert_eq!(found.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), ["GPT-6-Sol", "GPT-5.6-Sol"]);
        assert_eq!(matching(choices(&["Default", "Opus"]), "").len(), 2);
    }
}
