use super::effort_menu::{Effort, effort_menu};
use super::{Composer, Draft, Menu, launch};
use crate::desktop::Desktop;
use crate::modals::new_session::picker::menu_input;
use crate::settings::models::{by_upstream, couldnt_list, effort_label, listed_agents, model_label, model_options, not_found};
use crate::pick_menu::{Line, Memo, checked, pick_list};
use agents::ListedModel;
use agents::providers;
use gpui_kit::component::input::InputState;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::hash::Hash;
use std::rc::Rc;
use store::LaunchPick;
use store::prefs::agents::Provider;
use theme::*;

pub(crate) const ROW: f32 = 34.;
pub(crate) const GAP: f32 = 2.;
pub(crate) const PAD: f32 = 5.;
const PANEL_WIDTH: f32 = 340.;
/// The list's height before it scrolls; a taller rail stretches it further.
const LIST_HEIGHT: f32 = 264.;
const SEARCH_HEIGHT: f32 = 44.;
const TAB: f32 = 36.;
const TAB_GAP: f32 = 4.;
const RAIL_PAD: f32 = 6.;
/// The rail's height before it scrolls: the search over a full list. Ten agents then make the panel no taller than a
/// long list does, so it still hangs clear of the pill that opened it.
const RAIL_HEIGHT: f32 = SEARCH_HEIGHT + LIST_HEIGHT;
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
    /// Who serves it, where its agent's list says; it tells apart two models of one name.
    pub(crate) upstream: String,
    /// Its agent isn't installed, so it's listed but can't be picked.
    pub(crate) missing: bool,
}

/// What the panel lists, models under who serves them, and the line of the model in use.
pub(crate) struct Listed {
    pub(crate) lines: Vec<Line<Entry>>,
    pub(crate) on: Option<usize>,
}

/// The panel's `Listed`, kept from one frame to the next.
pub(crate) type KeptList = Memo<Listed>;

/// Picks the model of a line that was clicked.
pub(crate) type Choose = Rc<dyn Fn(&mut Desktop, &Entry, &mut Context<Desktop>)>;

/// The panel the model pill opens: `rail` beside a search, the models, a `note` on them and the efforts.
pub(crate) fn panel(rail: Div, search: &Entity<InputState>, list: Div, note: Option<String>, effort: Option<Effort>) -> Div {
    let (row, levels) = effort.map_or((None, None), |e| (Some(div().flex_none().p(px(PAD)).border_t(px(0.5)).border_color(SEPARATOR).child(e.row)), e.list));
    let note = note.map(|n| div().flex_none().px(px(PAD + 10.)).pb(px(10.)).text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(n));
    let column = div().flex_1().min_w_0().flex().flex_col().child(menu_input("search", search)).child(list).children(note).children(row);
    let panel = ui::pop(div()).w(px(PANEL_WIDTH)).flex_none().flex().overflow_hidden().child(rail).child(column);
    // The list sits inside this box, not over its edge: a press outside the box closes the menu, and the box keeps the list on screen.
    div().flex().items_end().gap(px(6.)).child(panel).children(levels)
}

/// What the panel says where its list would be.
pub(crate) fn empty_list(note: String) -> Div {
    div().flex_grow(1.).p(px(PAD)).child(div().px(px(10.)).py(px(8.)).text_size(px(13.)).line_height(px(18.)).text_color(TEXT_3).child(note))
}

/// The panel's list. An agent can list hundreds of models, so only the lines in view are built: `mixed` is whether each
/// row names its agent, and `choose` what a click on a model does.
pub(crate) fn model_list(id: &'static str, listed: Rc<Listed>, mixed: bool, scroll: &UniformListScrollHandle, choose: Choose, cx: &mut Context<Desktop>) -> Div {
    let pitch = ROW + GAP;
    // The last row's gap stands in for part of the padding under it.
    let count = listed.lines.len();
    let height = (count as f32 * pitch + 2. * PAD - GAP).min(LIST_HEIGHT);
    let line = move |this: &mut Desktop, i: usize, cx: &mut Context<Desktop>| {
        let line = div().w_full().h(px(pitch));
        match &listed.lines[i] {
            Line::Heading(name) => line.child(header(name)),
            Line::Pick(entry) => {
                let row = model_row(id, i, entry, listed.on == Some(i), mixed, this.store.agents.is_favorite(&entry.provider, &entry.value), cx);
                let (choose, listed) = (choose.clone(), listed.clone());
                line.child(row.when(!entry.missing, |row| {
                    row.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if let Line::Pick(entry) = &listed.lines[i] {
                            this.store.agents.used(&entry.provider, &entry.value);
                            this.store.save();
                            choose(this, entry, cx);
                        }
                    }))
                }))
            }
        }
    };
    let fit = |list: UniformList| list.size_full().px(px(PAD)).pt(px(PAD)).pb(px(PAD - GAP));
    pick_list("model-list", count, scroll, div().h(px(height)).flex_grow(1.).min_h_0(), fit, line, cx)
}

/// What the models under it share, small and in capitals.
fn header(name: &str) -> Div {
    div().h(px(ROW)).px(px(10.)).pb(px(5.)).flex().items_end().text_size(px(10.)).font_weight(FontWeight::MEDIUM).text_color(TEXT_4).child(name.to_string())
}

/// A model's row, checked if it's the one in use, its star showing if set, on the row in use, or under the pointer.
/// The favorites mix providers, so `mixed` names the row's. One whose agent isn't installed is dimmed and says why.
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
    let label = div().flex_1().min_w_0().truncate().font_weight(FontWeight::MEDIUM).child(entry.label.clone());
    let lead = mixed.then(|| provider_icon(&entry.provider, 14., TEXT_3));
    if entry.missing {
        let why = not_found(&entry.provider);
        // Only the name dims: the star still takes the favorite off.
        let dimmed = div().flex_1().min_w_0().flex().items_center().gap(px(8.)).opacity(0.3).children(lead).child(label);
        return row((id, i), on).group(ROW_GROUP).pr(px(6.)).cursor_default().child(dimmed).child(check(on)).children(star).tooltip(move |window, cx| Tooltip::new(why.clone()).build(window, cx));
    }
    row((id, i), on).group(ROW_GROUP).pr(px(6.)).children(lead).child(label).child(check(on)).children(star)
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

/// The entries whose model name or who serves it holds `query`, any case. Where the list is `mixed`, their agent's name
/// counts too; in an agent's own list it would match every model.
fn matching(entries: Vec<Entry>, query: &str, mixed: bool) -> Vec<Entry> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return entries;
    }
    let agent = |e: &Entry| if mixed { provider_name(&e.provider) } else { "" };
    entries.into_iter().filter(|e| format!("{} {} {}", e.label, e.upstream, agent(e)).to_lowercase().contains(&query)).collect()
}

/// `provider`'s `models` as the panel lists them, after a `current` one the list lacks.
fn entries(provider: &str, models: &[ListedModel], current: &str) -> Vec<Entry> {
    model_options(models, current).map(|(value, label, upstream)| Entry { provider: provider.into(), value: value.into(), label: label.into(), upstream: upstream.into(), missing: false }).collect()
}

/// The `saved` models of the agents `shown`, under the names their agents' `models` give them.
fn saved_entries<'m>(saved: &[(String, String)], shown: &[&str], models: impl Fn(&str) -> &'m [ListedModel]) -> Vec<Entry> {
    saved
        .iter()
        .filter(|(provider, _)| shown.contains(&provider.as_str()))
        .map(|(provider, model)| {
            let listed = models(provider).iter().find(|m| m.value == *model);
            let (label, upstream) = listed.map_or_else(|| (model_label(model), String::new()), |m| (m.name.clone(), m.upstream.clone()));
            Entry { provider: provider.clone(), value: model.clone(), label, upstream, missing: false }
        })
        .collect()
}

/// `entries` as the lines `viewed` lists: under who serves each, where `by_upstream` groups them.
fn lines(viewed: &str, entries: Vec<Entry>) -> Vec<Line<Entry>> {
    by_upstream(viewed, entries, |e| &e.upstream).into_iter().flat_map(|(who, group)| who.map(Line::Heading).into_iter().chain(group.into_iter().map(Line::Pick))).collect()
}

/// What Favorites lists: the starred models, then under Recent the ones picked lately that have no star.
fn favorite_lines(favorites: Vec<Entry>, recent: Vec<Entry>) -> Vec<Line<Entry>> {
    let recent: Vec<_> = recent.into_iter().filter(|r| !favorites.iter().any(|f| (&f.provider, &f.value) == (&r.provider, &r.value))).collect();
    let heading = (!recent.is_empty()).then(|| Line::Heading("RECENT".into()));
    favorites.into_iter().map(Line::Pick).chain(heading).chain(recent.into_iter().map(Line::Pick)).collect()
}

/// What `viewed`, named `name`, says with nothing to list.
fn empty_note(viewed: &str, name: &str, searching: bool, installed: bool, loading: bool) -> String {
    match viewed {
        FAVORITES if !searching => "No favorite or recent models".into(),
        agent if agent != FAVORITES && !installed => not_found(agent),
        _ if searching => "No matching models".into(),
        _ if loading => format!("Loading {name} models\u{2026}"),
        _ => couldnt_list(name, ""),
    }
}

/// The line of the model a launch of `pick` runs: the one picked, else `settings`' for its agent.
fn launching_line(lines: &[Line<Entry>], pick: &LaunchPick, settings: &str) -> Option<usize> {
    let runs = if pick.model.is_empty() { settings } else { &pick.model };
    checked(lines, |e| e.provider == pick.provider && e.value == runs)
}

/// Where `agent`'s tab sits in the rail that lists the `offered` agents and `current` under Favorites.
fn rail_tab(offered: &[&'static str], current: &str, agent: &str) -> Option<usize> {
    listed_agents(offered, &[current]).iter().position(|p| *p == agent).map(|at| at + 1)
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
        let names = self.offered_models(&pick.provider);
        let levels = self.launch_efforts(&pick.provider, &pick.model);
        let (model, effort) = pill_label(&self.agent_name(&pick.provider), pick, &self.store.agents.provider(&pick.provider), &levels, names);
        let (open, provider, opened_in) = (c.draft.menu == Some(Menu::Model), pick.provider.clone(), project.clone());
        let chip = super::picker(model_effort_pill(&pick.provider, model, effort), "composer-model", open).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            this.toggle_composer(slot, Menu::Model, cx);
            let c = slot(this);
            if c.draft.menu == Some(Menu::Model) {
                let search = c.search.clone();
                focus_search(&search, window, cx);
                this.models_opened(move |d| &mut slot(d).draft, &provider, opened_in.as_deref(), cx);
            }
        }));
        let menu = open.then(|| {
            let menu = div()
                .id("composer-model-menu")
                // Menus hang over the page; without this a click on one also lands on what it covers.
                .occlude()
                .child(self.launch_panel(c, pick, project, &levels, names, cx))
                .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    slot(this).draft.menu = None;
                    cx.notify();
                }));
            ui::dropdown(36., ui::menu_in("composer-model-menu-in", menu))
        });
        div().relative().child(chip).children(menu)
    }

    /// Favorites over the agents `project` shows and `current`, the one in use; pointing at one lists its models, and
    /// only picking a model acts on it. More agents than fit scroll.
    pub(crate) fn model_rail(&self, viewed: &str, current: &str, project: Option<&str>, scroll: &ScrollHandle, draft: impl Fn(&mut Desktop) -> &mut Draft + Copy + 'static, cx: &mut Context<Self>) -> Div {
        let tab = |id: &'static str, glyph: Svg, name: String, cx: &mut Context<Self>| {
            let over = id != viewed;
            div()
                .id(id)
                .size(px(TAB))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(9.))
                .cursor_pointer()
                .when(id == viewed, |d| d.bg(FILL_3))
                .when(id != viewed, |d| d.hover(|s| s.bg(FILL_2)))
                .child(glyph)
                .tooltip(move |window, cx| Tooltip::new(name.clone()).build(window, cx))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered && over {
                        this.view_models(draft, id, cx);
                    }
                }))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.view_models(draft, id, cx)))
        };
        let favorites = tab(FAVORITES, icon("star", 16., TEXT_2), "Favorites and recent".into(), cx);
        let agents: Vec<_> = listed_agents(&self.offered_in(project), &[current]).into_iter().map(|p| tab(p, provider_icon(p, 16., TEXT), self.agent_name(p), cx).when(!self.installed(p), |d| d.opacity(0.4))).collect();
        let tabs = (agents.len() + 1) as f32;
        let height = (tabs * (TAB + TAB_GAP) - TAB_GAP + 2. * RAIL_PAD).min(RAIL_HEIGHT);
        let column = div().id("model-rail").absolute().inset_0().overflow_y_scroll().track_scroll(scroll).p(px(RAIL_PAD)).flex().flex_col().gap(px(TAB_GAP)).child(favorites).children(agents);
        // The tabs fill the panel's height, however tall the list beside them makes it; only `height` of them asks for room.
        div().relative().flex_none().w(px(TAB + 2. * RAIL_PAD)).min_h(px(height)).border_r(px(0.5)).border_color(SEPARATOR).child(column)
    }

    /// Lists `entry`'s models, asking an agent's CLI for them once. The favorites ask nobody: pointing at them would
    /// start every starred agent.
    pub(crate) fn view_models(&mut self, draft: impl Fn(&mut Desktop) -> &mut Draft, entry: &'static str, cx: &mut Context<Self>) {
        draft(self).view(entry);
        if entry != FAVORITES {
            self.load_models_of(entry, cx);
        }
        cx.notify();
    }

    /// `draft`'s model panel just opened on `provider` in `project`: its CLI is asked for its models, again if the last
    /// read failed, and its tab is brought into the rail's view.
    pub(crate) fn models_opened(&mut self, draft: impl Fn(&mut Desktop) -> &mut Draft, provider: &str, project: Option<&str>, cx: &mut Context<Self>) {
        self.settings.models.retry();
        self.load_models_of(provider, cx);
        if let Some(tab) = rail_tab(&self.offered_in(project), provider, provider) {
            draft(self).rail.scroll_to_item(tab);
        }
    }

    /// What `model_lines` reads besides what it's given; lines built from it are stale once it changes.
    pub(crate) fn lines_from(&self, project: Option<&str>) -> impl Hash + '_ {
        let agents = &self.store.agents;
        (self.settings.models.revision, &agents.favorites, &agents.recent, &agents.disabled, self.store.hidden(project), LaunchPick::PROVIDERS.map(|p| self.installed(p)))
    }

    /// What `viewed` lists that holds `query`: an agent's models after `current`, one of its own the list lacks, each
    /// under who serves it where `viewed` names them; or the favorite and recent models of the agents `project` shows,
    /// under the names their agents give them.
    pub(crate) fn model_lines(&self, viewed: &str, current: &str, query: &str, project: Option<&str>) -> Vec<Line<Entry>> {
        let found = |entries: Vec<Entry>| matching(entries.into_iter().map(|e| Entry { missing: !self.installed(&e.provider), ..e }).collect(), query, viewed == FAVORITES);
        if viewed != FAVORITES {
            return lines(viewed, found(entries(viewed, self.offered_models(viewed), current)));
        }
        let hidden = self.store.hidden(project);
        let shown: Vec<_> = self.store.agents.enabled().into_iter().filter(|p| !hidden.contains(*p)).collect();
        let saved = |models: &[(String, String)]| found(saved_entries(models, &shown, |p| self.offered_models(p)));
        favorite_lines(saved(&self.store.agents.favorites), saved(&self.store.agents.recent))
    }

    /// What the panel says while `viewed` lists nothing for `query`.
    pub(crate) fn no_models(&self, viewed: &str, query: &str) -> Div {
        empty_list(empty_note(viewed, &self.agent_name(viewed), !query.trim().is_empty(), self.installed(viewed), self.settings.models.loading(viewed)))
    }

    /// What the panel says under `viewed`'s list: that its CLI listed no models, and why.
    pub(crate) fn models_note(&self, viewed: &str) -> Option<String> {
        self.settings.models.failure(viewed).map(|why| couldnt_list(&self.agent_name(viewed), why))
    }

    /// The rail, the viewed entry's models and the picked model's efforts; only picking a model switches agent.
    fn launch_panel(&self, c: &Composer, pick: &LaunchPick, project: Option<String>, levels: &[String], names: &[ListedModel], cx: &mut Context<Self>) -> Div {
        let slot = c.slot;
        let viewed = c.draft.viewing.unwrap_or(&pick.provider);
        let rail = self.model_rail(viewed, &pick.provider, project.as_deref(), &c.draft.rail, move |d| &mut slot(d).draft, cx);
        let query = c.search.read(cx).value();
        // With no pick, the session starts on Settings' model.
        let settings = self.store.agents.provider(&pick.provider).model;
        let listed = c.draft.listed.get((viewed, &*query, &pick.provider, &pick.model, &settings, self.lines_from(project.as_deref())), || {
            let lines = self.model_lines(viewed, if viewed == pick.provider { &pick.model } else { "" }, &query, project.as_deref());
            Listed { on: launching_line(&lines, pick, &settings), lines }
        });
        let list = if listed.lines.is_empty() {
            self.no_models(viewed, &query)
        } else {
            let (current, project) = (pick.clone(), project.clone());
            let choose: Choose = Rc::new(move |this, entry, cx| {
                let base = current.switched(&entry.provider);
                // An effort the new model lacks would still be sent, though its chip no longer shows it.
                let effort = if this.launch_efforts(&entry.provider, &entry.value).contains(&base.effort) { base.effort.clone() } else { String::new() };
                this.choose_launch(slot, project.clone(), LaunchPick { model: entry.value.clone(), effort, ..base }, cx)
            });
            model_list("composer-model-row", listed, viewed == FAVORITES, &c.draft.scroll, choose, cx)
        };

        // Another entry lists other models, but the efforts below would still be the picked model's.
        let effort = (viewed == pick.provider && !levels.is_empty()).then(|| {
            let current = pick.clone();
            let choose: launch::Pick = Rc::new(move |this, effort, cx| this.adjust_launch(slot, project.clone(), LaunchPick { effort, ..current.clone() }, cx));
            let on = effort_shown(pick, &self.store.agents.provider(&pick.provider), levels, names).cloned();
            effort_menu("composer-effort", providers::known(&pick.provider).effort_word, (levels, on.as_ref()), &c.draft.effort, move |d: &mut Desktop| &mut slot(d).draft, choose, cx)
        });
        panel(rail, &c.search, list, self.models_note(viewed), effort)
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, FAVORITES, Line, empty_note, entries, favorite_lines, launching_line, lines, matching, pill_label, rail_tab, saved_entries};
    use agents::ListedModel;
    use store::LaunchPick;
    use store::prefs::agents::Provider;

    fn entry(provider: &str, value: &str, label: &str) -> Entry {
        let upstream = value.split_once('/').map_or("", |(who, _)| who).into();
        Entry { provider: provider.into(), value: value.into(), label: label.into(), upstream, missing: false }
    }

    fn listed(upstream: &str, value: &str, name: &str) -> ListedModel {
        ListedModel { value: value.into(), name: name.into(), resolved: value.into(), upstream: upstream.into(), ..ListedModel::default() }
    }

    fn labels(entries: Vec<Entry>) -> Vec<String> {
        entries.into_iter().map(|e| e.label).collect()
    }

    #[test]
    fn favorites_list_only_enabled_agents_under_their_own_names() {
        let favorites = [("claude".to_string(), "opus".to_string()), ("codex".to_string(), "sol".to_string()), ("claude".to_string(), "claude-gone-9".to_string())];
        let models = [listed("", "opus", "Opus")];
        let found = |query: &str| labels(matching(saved_entries(&favorites, &["claude"], |_| &models), query, true));
        assert_eq!(found(""), [("Opus".to_string()), super::model_label("claude-gone-9")]);
        assert_eq!(found("opu"), ["Opus"]);
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
        let choices = |names: &[&str]| names.iter().map(|n| entry("codex", &n.to_lowercase(), n)).collect::<Vec<_>>();
        assert_eq!(labels(matching(choices(&["Default", "GPT-6-Sol", "GPT-6-Astra", "GPT-5.6-Sol"]), " sol ", false)), ["GPT-6-Sol", "GPT-5.6-Sol"]);
        assert_eq!(matching(choices(&["Default", "Opus"]), "", false).len(), 2);
    }

    #[test]
    fn a_search_also_finds_a_model_by_its_agent_or_by_who_serves_it() {
        let mixed = || vec![entry("claude", "opus", "Opus"), entry("opencode", "anthropic/claude-sonnet-4-6", "Sonnet 4.6"), entry("opencode", "zai/glm-5", "GLM 5"), entry("hermes", "kimi", "Kimi K2.5")];
        assert_eq!(labels(matching(mixed(), "anthropic", true)), ["Sonnet 4.6"]);
        assert_eq!(labels(matching(mixed(), "OpenCode", true)), ["Sonnet 4.6", "GLM 5"]);
        assert_eq!(labels(matching(mixed(), "hermes agent", true)), ["Kimi K2.5"]);
        assert_eq!(labels(matching(mixed(), "claude", true)), ["Opus"], "the id serving a model is not its name");
    }

    #[test]
    fn in_an_agent_s_own_list_its_name_matches_no_model() {
        let omp = || vec![entry("omp", "anthropic/claude-opus-4-5", "Claude Opus 4.5"), entry("omp", "zai/glm-5", "GLM 5"), entry("omp", "google/gemini-3-pro", "Gemini 3 Pro")];
        assert_eq!(labels(matching(omp(), "omp", false)), Vec::<String>::new());
        assert_eq!(labels(matching(omp(), "opus", false)), ["Claude Opus 4.5"]);
        assert_eq!(labels(matching(omp(), "zai", false)), ["GLM 5"], "who serves it still does");
        assert_eq!(labels(matching(omp(), "omp", true)).len(), 3, "among the favorites, where agents mix");
    }

    #[test]
    fn a_model_is_listed_under_who_its_agent_says_serves_it() {
        let hermes = [listed("ChatGPT or Codex Subscription", "openai-codex:gpt-6-sol", "GPT 6 Sol"), listed("DeepSeek", "deepseek:deepseek-flash", "Deepseek Flash")];
        let found = entries("hermes", &hermes, "moa:gone");
        assert_eq!(found.iter().map(|e| (e.label.as_str(), e.upstream.as_str())).collect::<Vec<_>>(), [("moa:gone", ""), ("GPT 6 Sol", "ChatGPT or Codex Subscription"), ("Deepseek Flash", "DeepSeek")]);
        assert_eq!(labels(matching(found, "chatgpt", false)), ["GPT 6 Sol"]);
    }

    fn shown(viewed: &str, entries: Vec<Entry>) -> Vec<String> {
        named(lines(viewed, entries))
    }

    fn named(lines: Vec<Line<Entry>>) -> Vec<String> {
        lines
            .into_iter()
            .map(|l| match l {
                Line::Heading(name) => format!("# {name}"),
                Line::Pick(e) => e.label,
            })
            .collect()
    }

    #[test]
    fn the_agents_that_run_other_makers_models_sit_them_under_who_serves_each_and_the_others_list_theirs_flat() {
        let listed = || vec![entry("opencode", "opencode/big-pickle", "Big Pickle"), entry("opencode", "github-copilot/gpt-5.4", "GPT 5.4"), entry("opencode", "opencode/glm-5", "GLM 5"), entry("opencode", "local", "Local")];
        assert_eq!(shown("opencode", listed()), ["# OPENCODE", "Big Pickle", "GLM 5", "Local", "# GITHUB COPILOT", "GPT 5.4"]);
        assert_eq!(shown("pi", listed()), ["# OPENCODE", "Big Pickle", "GLM 5", "# GITHUB COPILOT", "GPT 5.4", "# PI", "Local"]);
        assert_eq!(shown("cursor", listed()), ["Big Pickle", "GPT 5.4", "GLM 5", "Local"]);
        assert_eq!(shown(FAVORITES, listed()).len(), 4);
    }

    #[test]
    fn favorites_come_first_then_the_models_picked_lately_that_have_no_star() {
        let favorites = || vec![entry("claude", "opus", "Opus"), entry("codex", "sol", "Sol")];
        let recent = || vec![entry("pi", "sol", "Pi Sol"), entry("claude", "opus", "Opus"), entry("claude", "haiku", "Haiku")];
        assert_eq!(named(favorite_lines(favorites(), recent())), ["Opus", "Sol", "# RECENT", "Pi Sol", "Haiku"]);
        assert_eq!(named(favorite_lines(vec![], recent())), ["# RECENT", "Pi Sol", "Opus", "Haiku"]);
        assert_eq!(named(favorite_lines(favorites(), vec![entry("claude", "opus", "Opus")])), ["Opus", "Sol"], "no heading over nothing");
        assert!(favorite_lines(vec![], vec![]).is_empty());
    }

    #[test]
    fn two_models_of_one_name_sit_under_different_makers() {
        let pi = vec![entry("pi", "amazon-bedrock/claude-opus-4-5", "Claude Opus 4.5"), entry("pi", "anthropic/claude-opus-4-5", "Claude Opus 4.5"), entry("pi", "anthropic/claude-sonnet-4-5", "Claude Sonnet 4.5")];
        assert_eq!(shown("pi", pi), ["# AMAZON BEDROCK", "Claude Opus 4.5", "# ANTHROPIC", "Claude Opus 4.5", "Claude Sonnet 4.5"]);
    }

    #[test]
    fn an_agent_s_own_default_sits_under_no_heading() {
        assert_eq!(shown("hermes", vec![entry("hermes", "", "Agent default")]), ["Agent default"]);
    }

    #[test]
    fn an_empty_list_says_why() {
        let note = |viewed: &str, searching: bool, installed: bool, loading: bool| empty_note(viewed, "Hermes Agent", searching, installed, loading);
        assert_eq!(note(FAVORITES, false, true, false), "No favorite or recent models");
        assert_eq!(note(FAVORITES, true, true, false), "No matching models");
        assert_eq!(note("hermes", false, true, true), "Loading Hermes Agent models\u{2026}");
        assert_eq!(note("hermes", true, true, true), "No matching models");
        assert_eq!(note("hermes", true, true, false), "No matching models");
        assert_eq!(note("hermes", false, true, false), "Couldn't list Hermes Agent's models", "nothing was typed, so nothing failed to match");
        assert_eq!(note("omp", false, false, true), "omp not found (`curl -fsSL https://omp.sh/install | sh`). Install it, or restart Anywhere if it is already installed.");
    }

    #[test]
    fn the_line_checked_is_the_model_a_launch_runs() {
        let listed = lines("claude", vec![entry("claude", "haiku", "Haiku"), entry("claude", "opus", "Opus")]);
        assert_eq!(launching_line(&listed, &pick("claude", "opus", ""), "haiku"), Some(1));
        assert_eq!(launching_line(&listed, &pick("claude", "", ""), "haiku"), Some(0), "with none picked, Settings' model");
        assert_eq!(launching_line(&listed, &pick("codex", "opus", ""), ""), None, "another agent's list");
        let own_default = lines("hermes", vec![entry("hermes", "", "Agent default")]);
        assert_eq!(launching_line(&own_default, &pick("hermes", "", ""), ""), Some(0));
    }

    #[test]
    fn the_rail_holds_favorites_then_each_agent_on_offer_and_the_one_in_use() {
        assert_eq!(rail_tab(&["claude", "codex", "hermes"], "hermes", "hermes"), Some(3));
        assert_eq!(rail_tab(&["claude", "hermes"], "pi", "hermes"), Some(3), "Pi is in use though not on offer, and comes before Hermes");
        assert_eq!(rail_tab(&["claude"], "claude", "pi"), None);
    }
}
