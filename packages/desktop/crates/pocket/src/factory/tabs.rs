use super::station::Column;

/// The tabs of the Factory ticket page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tab {
    Spec,
    Workers,
    Checks,
    Pr,
    Activity,
}

impl Tab {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Tab::Spec => "Spec & plan",
            Tab::Workers => "Workers",
            Tab::Checks => "Checks",
            Tab::Pr => "PR",
            Tab::Activity => "Activity",
        }
    }
}

/// The tabs that have a body. Plan 6 adds PR.
pub(crate) const OFFERED: [Tab; 4] = [Tab::Spec, Tab::Workers, Tab::Checks, Tab::Activity];

/// The tab where a station's work shows.
pub(crate) fn default_tab(column: Column) -> Tab {
    match column {
        Column::Todo | Column::Spec | Column::Plan => Tab::Spec,
        Column::Implement => Tab::Workers,
        Column::Checks => Tab::Checks,
        Column::Pr | Column::Merged => Tab::Pr,
    }
}

/// The tab the page shows: your pick, else the station's, each only while it is offered.
pub(crate) fn shown(picked: Option<Tab>, column: Column) -> Tab {
    let offered = |t: &Tab| OFFERED.contains(t);
    picked.filter(offered).or(Some(default_tab(column)).filter(offered)).unwrap_or(OFFERED[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_opens_on_the_tab_of_the_tickets_station() {
        let tabs = [Column::Todo, Column::Spec, Column::Plan, Column::Implement, Column::Checks, Column::Pr, Column::Merged].map(default_tab);
        assert_eq!(tabs, [Tab::Spec, Tab::Spec, Tab::Spec, Tab::Workers, Tab::Checks, Tab::Pr, Tab::Pr]);
    }

    #[test]
    fn a_station_whose_tab_has_no_body_yet_shows_spec_and_plan() {
        assert_eq!(shown(None, Column::Plan), Tab::Spec);
        assert_eq!(shown(None, Column::Pr), Tab::Spec);
    }

    #[test]
    fn a_ticket_at_checks_opens_on_the_checks_tab() {
        assert_eq!(shown(None, Column::Checks), Tab::Checks);
        assert_eq!(shown(Some(Tab::Activity), Column::Checks), Tab::Activity);
    }

    #[test]
    fn a_ticket_at_implement_opens_on_its_workers() {
        assert_eq!(shown(None, Column::Implement), Tab::Workers);
        assert_eq!(shown(Some(Tab::Workers), Column::Plan), Tab::Workers);
    }

    #[test]
    fn your_pick_wins_over_the_stations_tab() {
        assert_eq!(shown(Some(Tab::Activity), Column::Plan), Tab::Activity);
        assert_eq!(shown(Some(Tab::Spec), Column::Checks), Tab::Spec);
    }

    #[test]
    fn each_tab_has_its_title() {
        assert_eq!([Tab::Spec, Tab::Workers, Tab::Checks, Tab::Pr, Tab::Activity].map(Tab::label), ["Spec & plan", "Workers", "Checks", "PR", "Activity"]);
    }
}
