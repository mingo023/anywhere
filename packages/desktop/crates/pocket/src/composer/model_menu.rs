use super::{Composer, Menu, launch};
use crate::desktop::Desktop;
use crate::modals::new_session::picker::menu_input;
use crate::settings::models::{effort_label, listed_models, model_label, model_options};
use agents::ListedModel;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;
use store::LaunchPick;
use store::prefs::agents::Provider;
use theme::*;

pub(crate) const ROW: f32 = 34.;
pub(crate) const GAP: f32 = 2.;
pub(crate) const PAD: f32 = 5.;

/// The pill's model and effort: the ones picked, else Settings'; with neither, the agent's name stands for its own default
/// model, and the model's own default effort for the effort.
fn pill_label(agent: &str, pick: &LaunchPick, app: &Provider, efforts: &[String], names: &[ListedModel]) -> (String, Option<String>) {
    let model = [&pick.model, &app.model].into_iter().find(|m| !m.is_empty());
    (model.map_or_else(|| agent.to_string(), |m| model_name(m, names)), effort_shown(pick, app, efforts, names).map(|e| effort_label(e)))
}

/// The effort a launch runs on, as far as the app knows: picked, else Settings', else the model's own.
fn effort_shown<'a>(pick: &'a LaunchPick, app: &'a Provider, efforts: &[String], names: &'a [ListedModel]) -> Option<&'a String> {
    let model = [&pick.model, &app.model].into_iter().find(|m| !m.is_empty());
    let default = model.and_then(|m| names.iter().find(|n| n.value == *m)).map(|n| &n.default_effort);
    [&pick.effort, &app.effort].into_iter().chain(default).find(|e| efforts.contains(e))
}

/// The provider's icon, the model and, after it, the effort it runs on.
pub(crate) fn model_effort_pill(provider: &str, model: String, effort: Option<String>) -> Div {
    super::pill()
        .max_w(px(248.))
        .min_w_0()
        .flex_shrink(1.)
        .child(provider_icon(provider, 16., TEXT_2))
        .child(div().truncate().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(model))
        .children(effort.map(|e| div().flex_none().whitespace_nowrap().text_color(TEXT_3).child(e)))
}

/// A row of the model pill's menu: what it sets, its value, and the list it opens.
fn pill_menu_row(id: &'static str, label: &'static str, value: impl IntoElement, open: bool) -> Stateful<Div> {
    row(id, open)
        .child(div().flex_none().font_weight(FontWeight::MEDIUM).child(label))
        .child(div().flex_1().min_w_0().flex().justify_end().text_color(TEXT_3).child(value))
        .child(icon("chevron-right", 12., TEXT_4))
}

/// The Model row's value: the agent's icon and the model.
pub(crate) fn model_value(provider: &str, name: String) -> Div {
    div().min_w_0().flex().items_center().gap(px(6.)).child(provider_icon(provider, 14., TEXT_3)).child(div().truncate().child(name))
}

/// Opens a list beside the model pill's menu.
pub(crate) type OpenBeside = Rc<dyn Fn(&mut Desktop, Menu, &mut Window, &mut Context<Desktop>)>;

/// The model pill's menu: the effort row, as (what the agent calls it, its value), if the model has efforts, over the
/// Model row showing `model`; pointing at a row or clicking it opens its list with `open`. `ids` are the two rows'.
pub(crate) fn pill_menu(ids: [&'static str; 2], effort: Option<(&'static str, String)>, model: Div, menu: Option<Menu>, open: OpenBeside, cx: &Context<Desktop>) -> Div {
    let row = |id: &'static str, label: &'static str, value: AnyElement, list: Menu| {
        let (hover, click) = (open.clone(), open.clone());
        pill_menu_row(id, label, value, menu == Some(list))
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if *hovered {
                    hover(this, list, window, cx)
                }
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| click(this, list, window, cx)))
    };
    let effort = effort.map(|(word, value)| row(ids[0], word, value.into_any_element(), Menu::Effort));
    let model = row(ids[1], "Model", model.into_any_element(), Menu::Model);
    ui::pop(div()).w(px(240.)).p(px(PAD)).flex().flex_col().gap(px(GAP)).children(effort).child(model)
}

/// A list opened beside the model pill's menu.
pub(crate) fn beside_list(id: &'static str, width: f32, rows: impl IntoIterator<Item = impl IntoElement>) -> Stateful<Div> {
    ui::pop(div().id(id)).w(px(width)).max_h(px(360.)).overflow_y_scroll().p(px(PAD)).flex().flex_col().gap(px(GAP)).children(rows)
}

/// How far a row sits from the menu's edge, `rows` rows in.
pub(crate) fn rows_offset(rows: usize) -> Pixels {
    px(rows as f32 * (ROW + GAP))
}

/// Each of `levels`, the one `on` checked; picking one hands it to `pick`.
pub(crate) fn effort_rows(id: &'static str, levels: &[String], on: Option<&String>, pick: launch::Pick, cx: &mut Context<Desktop>) -> Vec<Stateful<Div>> {
    levels
        .iter()
        .enumerate()
        .map(|(i, level)| {
            let (pick, level, checked) = (pick.clone(), level.clone(), on == Some(level));
            row((id, i), checked)
                .child(div().flex_1().min_w_0().truncate().child(effort_label(&level)))
                .child(check(checked))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| pick(this, level.clone(), cx)))
        })
        .collect()
}

/// The agent's listed name for the model, else the model's label.
pub(crate) fn model_name(model: &str, names: &[ListedModel]) -> String {
    names.iter().find(|m| m.value == model).map_or_else(|| model_label(model), |m| m.name.clone())
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
    /// The agent, model and effort a session starts with; its menu opens the efforts, or each agent's models, beside it.
    pub(crate) fn model_pill(&self, c: &Composer, pick: &LaunchPick, project: Option<String>, cx: &mut Context<Self>) -> Div {
        let (slot, menu) = (c.slot, c.draft.menu);
        let levels = self.launch_efforts(&pick.provider, &pick.model);
        let names = listed_models(&pick.provider, self.models_of(&pick.provider));
        let (model, effort) = pill_label(&self.agent_name(&pick.provider), pick, &self.store.agents.provider(&pick.provider), &levels, &names);
        let open = menu.is_some_and(Menu::of_model_pill);
        let chip = super::picker(model_effort_pill(&pick.provider, model.clone(), effort.clone()), "composer-model", open).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            this.load_models(cx);
            this.toggle_composer(slot, Menu::ModelAndEffort, cx);
        }));
        let menu = open.then(|| {
            let word = agents::providers::known(&pick.provider).effort_word;
            let effort_row = (!levels.is_empty()).then(|| (word, effort.unwrap_or_else(|| "Default".into())));
            let model_index = effort_row.is_some() as usize;
            let open_list: OpenBeside = Rc::new(move |this, list, window, cx| this.open_beside(slot, list, window, cx));
            let rows = pill_menu(["composer-effort", "composer-model-row"], effort_row, model_value(&pick.provider, model), menu, open_list, cx);
            let beside = match menu {
                Some(Menu::Model) => Some(div().mt(rows_offset(model_index)).child(self.model_side(c, pick, project.clone(), cx))),
                Some(Menu::Effort) if !levels.is_empty() => {
                    let current = pick.clone();
                    let choose: launch::Pick = Rc::new(move |this, effort, cx| this.choose_launch(slot, project.clone(), LaunchPick { effort, ..current.clone() }, cx));
                    let on = effort_shown(pick, &self.store.agents.provider(&pick.provider), &levels, &names).cloned();
                    Some(div().child(beside_list("composer-effort-menu", 200., effort_rows("composer-effort-row", &levels, on.as_ref(), choose, cx))))
                }
                _ => None,
            };
            let menu = div()
                .id("composer-model-menu")
                // Menus hang over the page; without this a click on one also lands on what it covers.
                .occlude()
                .flex()
                .items_start()
                .gap(px(4.))
                .child(rows)
                .children(beside)
                .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    slot(this).draft.menu = None;
                    cx.notify();
                }));
            ui::dropdown(36., ui::menu_in("composer-model-menu-in", menu))
        });
        div().relative().child(chip).children(menu)
    }

    /// Opens `list` beside the model pill's menu; the models' search starts empty and takes the keys.
    fn open_beside(&mut self, slot: super::Slot, list: Menu, window: &mut Window, cx: &mut Context<Self>) {
        let c = slot(self);
        if c.draft.menu == Some(list) {
            return;
        }
        c.draft.menu = Some(list);
        if list == Menu::Model {
            c.search.update(cx, |s, cx| {
                s.set_value("", window, cx);
                s.focus(window, cx);
            });
        }
        cx.notify();
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
        let app = self.store.agents.provider(viewed).model;
        let choices = matching(model_options(viewed, self.models_of(viewed), picked), &c.search.read(cx).value());
        // With no pick, the session starts on Settings' model.
        let runs = if picked.is_empty() { app.as_str() } else { picked };
        let chosen = same.then(|| choices.iter().position(|(v, _)| v == runs)).flatten();
        let empty = choices.is_empty();
        let base = pick.switched(viewed);
        let list_rows = choices
            .into_iter()
            .enumerate()
            .map(|(i, (value, label))| {
                // An effort the new model lacks would still be sent, though its chip no longer shows it.
                let effort = if self.launch_efforts(viewed, &value).contains(&base.effort) { base.effort.clone() } else { String::new() };
                let (project, pick) = (project.clone(), LaunchPick { model: value.clone(), effort, ..base.clone() });
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
        let names = [agents::ListedModel { value: "gpt-6-astra".into(), name: "GPT-6-Astra".into(), resolved: "gpt-6-astra".into(), ..agents::ListedModel::default() }];
        assert_eq!(pill_label("Codex", &pick("codex", "gpt-6-astra", ""), &settings, &[], &names), ("GPT-6-Astra".into(), None));
        assert_eq!(pill_label("Codex", &pick("codex", "gpt-5", ""), &settings, &[], &names).0, "gpt-5");
    }

    #[test]
    fn with_no_effort_picked_the_pill_shows_the_models_own() {
        let efforts = ["low".to_string(), "high".to_string()];
        let names = [agents::ListedModel { value: "gpt-6-sol".into(), name: "GPT-6-Sol".into(), default_effort: "low".into(), ..agents::ListedModel::default() }];
        let none = Provider::default();
        assert_eq!(pill_label("Codex", &pick("codex", "gpt-6-sol", ""), &none, &efforts, &names), ("GPT-6-Sol".into(), Some("Low".into())));
        assert_eq!(pill_label("Codex", &pick("codex", "gpt-6-sol", "high"), &none, &efforts, &names).1.as_deref(), Some("High"));
        assert_eq!(pill_label("Codex", &pick("codex", "", ""), &none, &efforts, &names), ("Codex".into(), None));
        let lacking = Provider { effort: "ultra".into(), ..Provider::default() };
        assert_eq!(pill_label("Codex", &pick("codex", "gpt-6-sol", ""), &lacking, &efforts, &names).1.as_deref(), Some("Low"));
    }

    #[test]
    fn a_search_keeps_the_models_whose_name_holds_it() {
        let choices = |names: &[&str]| names.iter().map(|n| (n.to_lowercase(), n.to_string())).collect::<Vec<_>>();
        let found = matching(choices(&["Default", "GPT-6-Sol", "GPT-6-Astra", "GPT-5.6-Sol"]), " sol ");
        assert_eq!(found.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), ["GPT-6-Sol", "GPT-5.6-Sol"]);
        assert_eq!(matching(choices(&["Default", "Opus"]), "").len(), 2);
    }
}
