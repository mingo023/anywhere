use super::effort_menu::{Effort, effort_menu};
use super::{Composer, Draft, Menu, launch};
use crate::desktop::Desktop;
use crate::modals::new_session::picker::menu_input;
use crate::settings::models::{effort_label, listed_models, model_label, model_options};
use agents::ListedModel;
use gpui_kit::component::input::InputState;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;
use store::LaunchPick;
use store::prefs::agents::Provider;
use theme::*;

pub(crate) const ROW: f32 = 34.;
pub(crate) const GAP: f32 = 2.;
pub(crate) const PAD: f32 = 5.;
const PANEL_WIDTH: f32 = 340.;
const ROW_GROUP: &str = "model-row";
/// The rail's first entry; `Draft::viewing` holds it where it would an agent.
pub(crate) const FAVORITES: &str = "favorites";

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

/// A model the panel lists; the favorites mix providers.
pub(crate) struct Entry {
    pub(crate) provider: String,
    pub(crate) value: String,
    pub(crate) label: String,
}

/// The panel the model pill opens: `rail` beside a search, the models and the efforts.
pub(crate) fn panel(rail: Div, viewed: &str, search: &Entity<InputState>, models: Vec<Stateful<Div>>, effort: Option<Effort>) -> Div {
    let empty = models.is_empty().then_some(if viewed == FAVORITES { "No favorites yet" } else { "No models match" });
    let list = div()
        .id("model-list")
        .max_h(px(264.))
        .overflow_y_scroll()
        .p(px(PAD))
        .flex()
        .flex_col()
        .gap(px(GAP))
        .children(models)
        .children(empty.map(|note| div().px(px(10.)).py(px(8.)).text_size(px(13.)).text_color(TEXT_3).child(note)));
    let (row, levels) = effort.map_or((None, None), |e| (Some(div().flex_none().p(px(PAD)).border_t(px(0.5)).border_color(SEPARATOR).child(e.row)), e.list));
    let panel = ui::pop(div()).w(px(PANEL_WIDTH)).flex_none().flex().overflow_hidden().child(rail).child(div().flex_1().min_w_0().flex().flex_col().child(menu_input("search", search)).child(list).children(row));
    // The list sits inside this box, not over its edge: a press outside the box closes the menu, and the box keeps the list on screen.
    div().flex().items_end().gap(px(6.)).child(panel).children(levels)
}

/// A model's row, checked if it's the one in use, its star showing if set, on the row in use, or under the pointer.
/// The favorites mix providers, so `mixed` names the row's.
pub(crate) fn model_row(id: &'static str, i: usize, entry: &Entry, on: bool, mixed: bool, starred: bool, cx: &mut Context<Desktop>) -> Stateful<Div> {
    let (provider, model) = (entry.provider.clone(), entry.value.clone());
    let star = (!entry.value.is_empty()).then(|| {
        div()
            .id(("model-star", i))
            .size(px(22.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .hover(|s| s.bg(FILL_3))
            .when(!on && !starred, |d| d.opacity(0.).group_hover(ROW_GROUP, |s| s.opacity(1.)))
            .child(icon(if starred { "star-fill" } else { "star" }, 14., if starred { WAITING_DOT } else { TEXT_3 }))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.store.agents.toggle_favorite(&provider, &model);
                this.store.save();
                cx.notify();
            }))
    });
    row((id, i), on)
        .group(ROW_GROUP)
        .pr(px(6.))
        .when(mixed, |d| d.child(provider_icon(&entry.provider, 14., TEXT_3)))
        .child(div().flex_1().min_w_0().truncate().font_weight(FontWeight::MEDIUM).child(entry.label.clone()))
        .child(check(on))
        .children(star)
}

/// Empties the panel's search and gives it the keys.
pub(crate) fn focus_search(search: &Entity<InputState>, window: &mut Window, cx: &mut Context<Desktop>) {
    search.update(cx, |s, cx| {
        s.set_value("", window, cx);
        s.focus(window, cx);
    });
}

/// The agent's listed name for the model, else the model's label.
pub(crate) fn model_name(model: &str, names: &[ListedModel]) -> String {
    names.iter().find(|m| m.value == model).map_or_else(|| model_label(model), |m| m.name.clone())
}

/// Whether `label` holds `query`, any case.
fn holds(label: &str, query: &str) -> bool {
    label.to_lowercase().contains(&query.trim().to_lowercase())
}

/// The choices whose label holds `query`.
fn matching(choices: Vec<(String, String)>, query: &str) -> Vec<(String, String)> {
    choices.into_iter().filter(|(_, label)| holds(label, query)).collect()
}

/// The `favorites` of `enabled` agents that hold `query`, under the names their agents give them.
fn favorite_entries(favorites: &[(String, String)], enabled: &[&str], options: impl Fn(&str) -> Vec<(String, String)>, query: &str) -> Vec<Entry> {
    favorites
        .iter()
        .filter(|(provider, _)| enabled.contains(&provider.as_str()))
        .map(|(provider, model)| {
            let label = options(provider).into_iter().find(|(v, _)| v == model).map_or_else(|| model_label(model), |(_, l)| l);
            Entry { provider: provider.clone(), value: model.clone(), label }
        })
        .filter(|e| holds(&e.label, query))
        .collect()
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
    /// The agent, model and effort a session starts with, and the panel that picks them.
    pub(crate) fn model_pill(&self, c: &Composer, pick: &LaunchPick, project: Option<String>, cx: &mut Context<Self>) -> Div {
        let slot = c.slot;
        let names = listed_models(&pick.provider, self.models_of(&pick.provider));
        let levels = self.launch_efforts(&pick.provider, &pick.model);
        let (model, effort) = pill_label(&self.agent_name(&pick.provider), pick, &self.store.agents.provider(&pick.provider), &levels, &names);
        let open = c.draft.menu == Some(Menu::Model);
        let chip = super::picker(model_effort_pill(&pick.provider, model, effort), "composer-model", open).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            this.load_models(cx);
            this.toggle_composer(slot, Menu::Model, cx);
            let c = slot(this);
            if c.draft.menu == Some(Menu::Model) {
                let search = c.search.clone();
                focus_search(&search, window, cx);
            }
        }));
        let menu = open.then(|| {
            let menu = div()
                .id("composer-model-menu")
                // Menus hang over the page; without this a click on one also lands on what it covers.
                .occlude()
                .child(self.launch_panel(c, pick, project, &levels, &names, cx))
                .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    slot(this).draft.menu = None;
                    cx.notify();
                }));
            ui::dropdown(36., ui::menu_in("composer-model-menu-in", menu))
        });
        div().relative().child(chip).children(menu)
    }

    /// Favorites over the agents; clicking one lists its models, and only picking a model acts on it.
    pub(crate) fn model_rail(&self, viewed: &str, draft: impl Fn(&mut Desktop) -> &mut Draft + Copy + 'static, cx: &mut Context<Self>) -> Div {
        let tab = |id: &'static str, glyph: Svg, cx: &mut Context<Self>| {
            div()
                .id(id)
                .size(px(36.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(9.))
                .cursor_pointer()
                .when(id == viewed, |d| d.bg(FILL_3))
                .when(id != viewed, |d| d.hover(|s| s.bg(FILL_2)))
                .child(glyph)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    draft(this).view(id);
                    cx.notify();
                }))
        };
        let favorites = tab(FAVORITES, icon("star", 16., TEXT_2), cx);
        let agents: Vec<_> = self.store.agents.enabled().into_iter().map(|p| tab(p, provider_icon(p, 16., TEXT), cx)).collect();
        div().flex_none().p(px(6.)).flex().flex_col().gap(px(4.)).border_r(px(0.5)).border_color(SEPARATOR).child(favorites).children(agents)
    }

    /// What `viewed` lists that holds `query`: an agent's `options`, or the favorites under the names their agents give them.
    pub(crate) fn model_entries(&self, viewed: &str, options: impl Fn(&Self, &str) -> Vec<(String, String)>, query: &str) -> Vec<Entry> {
        if viewed != FAVORITES {
            return matching(options(self, viewed), query).into_iter().map(|(value, label)| Entry { provider: viewed.to_string(), value, label }).collect();
        }
        favorite_entries(&self.store.agents.favorites, &self.store.agents.enabled(), |p| options(self, p), query)
    }

    /// The rail, the viewed entry's models and the picked model's efforts; only picking a model switches agent.
    fn launch_panel(&self, c: &Composer, pick: &LaunchPick, project: Option<String>, levels: &[String], names: &[ListedModel], cx: &mut Context<Self>) -> Div {
        let slot = c.slot;
        let viewed = c.draft.viewing.unwrap_or(&pick.provider);
        let rail = self.model_rail(viewed, move |d| &mut slot(d).draft, cx);
        let picked = |p: &str| if p == pick.provider { pick.model.clone() } else { String::new() };
        let entries = self.model_entries(viewed, |d, p| model_options(p, d.models_of(p), &picked(p)), &c.search.read(cx).value());
        // With no pick, the session starts on Settings' model.
        let runs = if pick.model.is_empty() { self.store.agents.provider(&pick.provider).model } else { pick.model.clone() };
        let chosen = entries.iter().position(|e| e.provider == pick.provider && e.value == runs);
        let models = entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let base = pick.switched(&entry.provider);
                // An effort the new model lacks would still be sent, though its chip no longer shows it.
                let effort = if self.launch_efforts(&entry.provider, &entry.value).contains(&base.effort) { base.effort.clone() } else { String::new() };
                let (project, pick) = (project.clone(), LaunchPick { model: entry.value.clone(), effort, ..base.clone() });
                model_row("composer-model-row", i, entry, chosen == Some(i), viewed == FAVORITES, self.store.agents.is_favorite(&entry.provider, &entry.value), cx)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.choose_launch(slot, project.clone(), pick.clone(), cx)))
            })
            .collect();

        // Another entry lists other models, but the efforts below would still be the picked model's.
        let effort = (viewed == pick.provider && !levels.is_empty()).then(|| {
            let current = pick.clone();
            let choose: launch::Pick = Rc::new(move |this, effort, cx| this.adjust_launch(slot, project.clone(), LaunchPick { effort, ..current.clone() }, cx));
            let on = effort_shown(pick, &self.store.agents.provider(&pick.provider), levels, names).cloned();
            effort_menu("composer-effort", agents::providers::known(&pick.provider).effort_word, (levels, on.as_ref()), &c.draft.effort, move |d: &mut Desktop| &mut slot(d).draft, choose, cx)
        });
        panel(rail, viewed, &c.search, models, effort)
    }
}

#[cfg(test)]
mod tests {
    use super::{favorite_entries, matching, pill_label};
    use store::LaunchPick;
    use store::prefs::agents::Provider;

    #[test]
    fn favorites_list_only_enabled_agents_under_their_own_names() {
        let favorites = [("claude".to_string(), "opus".to_string()), ("codex".to_string(), "sol".to_string()), ("claude".to_string(), "claude-gone-9".to_string())];
        let options = |_: &str| vec![("opus".to_string(), "Opus".to_string())];
        let labels = |query: &str| favorite_entries(&favorites, &["claude"], options, query).into_iter().map(|e| e.label).collect::<Vec<_>>();
        assert_eq!(labels(""), [("Opus".to_string()), super::model_label("claude-gone-9")]);
        assert_eq!(labels("opu"), ["Opus"]);
    }

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
