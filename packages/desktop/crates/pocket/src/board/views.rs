use super::logic::{COLS, Choice, FilterCat, Filters, Tab, priority_name};
use crate::desktop::Desktop;
use gpui_kit::*;
use store::BoardView;

/// `filters` kept as a view named `name`; the search is not part of it.
fn view_of(filters: &Filters, name: String) -> BoardView {
    BoardView {
        name,
        status: filters.status.values.iter().map(|c| c.name().to_string()).collect(),
        agent: filters.agent.values.clone(),
        project: filters.project.values.clone(),
        priority: filters.priority.values.clone(),
        label: filters.label.values.clone(),
        not: FilterCat::ALL.into_iter().filter(|c| filters.is_not(*c)).map(|c| c.name().to_string()).collect(),
    }
}

/// The filters a view sets, leaving `query` as typed.
fn filters_of(view: &BoardView, query: String) -> Filters {
    let not = |cat: FilterCat| view.not.iter().any(|n| n == cat.name());
    Filters {
        status: Choice { values: view.status.iter().filter_map(|s| COLS.into_iter().find(|c| c.name() == s)).collect(), not: not(FilterCat::Status) },
        agent: Choice { values: view.agent.clone(), not: not(FilterCat::Agent) },
        project: Choice { values: view.project.clone(), not: not(FilterCat::Project) },
        priority: Choice { values: view.priority.clone(), not: not(FilterCat::Priority) },
        label: Choice { values: view.label.clone(), not: not(FilterCat::Label) },
        query,
    }
}

/// What a view calls this field: the value when there is one, else the field.
fn name_part<T>(choice: &Choice<T>, cat: FilterCat, name: impl Fn(&T) -> String) -> Option<String> {
    let named = match choice.values.as_slice() {
        [] => return None,
        [v] => name(v),
        _ => cat.name().to_string(),
    };
    Some(if choice.not { format!("Not {named}") } else { named })
}

fn same_filters(view: &BoardView, filters: &Filters) -> bool {
    *filters == filters_of(view, filters.query.clone())
}

/// Whether the page is showing `view` right now.
pub(super) fn view_is_on(view: &BoardView, filters: &Filters, tab: Tab) -> bool {
    tab == Tab::All && same_filters(view, filters)
}

/// Whether Save view has anything to add.
pub(super) fn can_save_view(views: &[BoardView], filters: &Filters) -> bool {
    filters.any() && !views.iter().any(|v| same_filters(v, filters))
}

impl Desktop {
    pub(super) fn save_board_view(&mut self, cx: &mut Context<Self>) {
        let f = &self.board.filters;
        let parts = [
            name_part(&f.status, FilterCat::Status, |c| c.name().to_string()),
            name_part(&f.agent, FilterCat::Agent, |a| self.agent_name(a)),
            name_part(&f.project, FilterCat::Project, |p| self.repo_name(p)),
            name_part(&f.priority, FilterCat::Priority, |p| priority_name(*p).to_string()),
            name_part(&f.label, FilterCat::Label, Clone::clone),
        ];
        let view = view_of(f, parts.into_iter().flatten().collect::<Vec<_>>().join(", "));
        self.store.board_views.push(view);
        self.store.save();
        cx.notify();
    }

    pub(super) fn apply_board_view(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(view) = self.store.board_views.get(index) else { return };
        self.board.filters = filters_of(view, std::mem::take(&mut self.board.filters.query));
        self.board.tab = Tab::All;
        self.board_changed();
        self.aim_board_composer(cx);
        cx.notify();
    }

    pub(super) fn remove_board_view(&mut self, index: usize, cx: &mut Context<Self>) {
        self.store.board_views.remove(index);
        self.store.save();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{can_save_view, filters_of, name_part, view_is_on, view_of};
    use crate::board::logic::{Choice, Col, FilterCat, Filters, Tab};

    fn working() -> Filters {
        let one = |v: &str| Choice { values: vec![v.to_string()], not: false };
        Filters { status: Choice { values: vec![Col::Working, Col::Done], not: true }, agent: one("claude"), project: one("/app"), priority: Choice { values: vec![1, 2], not: false }, label: one("Bug"), query: String::new() }
    }

    #[test]
    fn a_saved_view_gives_back_the_filters_it_kept() {
        assert_eq!(filters_of(&view_of(&working(), "Mine".into()), String::new()), working());
    }

    #[test]
    fn a_view_is_named_for_a_single_value_or_else_its_field() {
        let name = |c: &Choice<u8>| name_part(c, FilterCat::Priority, u8::to_string);
        assert_eq!(name(&Choice::default()), None);
        assert_eq!(name(&Choice { values: vec![1], not: false }), Some("1".to_string()));
        assert_eq!(name(&Choice { values: vec![1, 2], not: false }), Some("Priority".to_string()));
        assert_eq!(name(&Choice { values: vec![1], not: true }), Some("Not 1".to_string()));
    }

    #[test]
    fn a_view_leaves_the_search_as_typed() {
        assert_eq!(filters_of(&view_of(&working(), "Mine".into()), "login".into()).query, "login");
    }

    #[test]
    fn a_view_is_on_only_on_all_tickets_with_its_filters() {
        let view = view_of(&working(), "Mine".into());
        assert!(view_is_on(&view, &working(), Tab::All));
        assert!(view_is_on(&view, &Filters { query: "login".into(), ..working() }, Tab::All));
        assert!(!view_is_on(&view, &working(), Tab::Active));
        assert!(!view_is_on(&view, &Filters::default(), Tab::All));
    }

    #[test]
    fn a_view_stays_on_when_its_values_are_picked_in_another_order() {
        let view = view_of(&working(), "Mine".into());
        let mut reordered = working();
        reordered.priority.values.reverse();
        assert!(view_is_on(&view, &reordered, Tab::All));
        assert!(!can_save_view(std::slice::from_ref(&view), &reordered));
    }

    #[test]
    fn only_filters_not_yet_saved_can_be_saved() {
        assert!(!can_save_view(&[], &Filters::default()));
        assert!(can_save_view(&[], &working()));
        assert!(!can_save_view(&[view_of(&working(), "Mine".into())], &working()));
    }
}
