//! A lead's workers nest and fold under it, in the Projects and Sessions lists alike.

use super::fold::{self, Fold};
use super::sections::TreeRow;
use super::sessions::nest_by;
use super::{SidebarState, tree_key};
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use crate::status::{Card, Status};
use crate::team::plural;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use theme::*;

/// The leads whose workers are folded.
#[derive(Default)]
pub(crate) struct TeamFolds(HashSet<String>);

impl TeamFolds {
    pub(crate) fn folded(&self, lead: &str) -> bool {
        self.0.contains(lead)
    }

    /// Unfolds every one of `leads` when all are folded, else folds them all. Returns whether they're folded now.
    fn toggle_all(&mut self, leads: &[String]) -> bool {
        let fold = !leads.iter().all(|l| self.folded(l));
        for lead in leads {
            if fold {
                self.0.insert(lead.clone());
            } else {
                self.0.remove(lead);
            }
        }
        fold
    }

    /// Unfolds every session `id` sits under, so it has a row.
    pub(crate) fn reveal(&mut self, id: &str, parent_of: impl Fn(&str) -> Option<String>) {
        let mut at = parent_of(id);
        while let Some(parent) = at.filter(|p| !p.is_empty()) {
            self.0.remove(&parent);
            at = parent_of(&parent);
        }
    }

    /// `rows`, less what sits under a folded row; `held` keeps every team open, as a search does.
    pub(crate) fn shown(&self, rows: Vec<(Card, usize)>, held: bool) -> Vec<(Card, usize)> {
        let mut under = None;
        rows.into_iter()
            .filter(|(c, depth)| {
                if under.is_some_and(|d| *depth > d) {
                    return false;
                }
                under = (!held && self.folded(&c.id)).then_some(*depth);
                true
            })
            .collect()
    }

    /// `rows` split into each top row and what sits under it; `session` is the one on screen.
    pub(crate) fn branches<'a>(&self, rows: &'a [(Card, usize)], held: bool, session: Option<&str>) -> Vec<Branch<'a>> {
        let mut out = Vec::new();
        let mut rest = rows;
        while let Some(((card, depth), after)) = rest.split_first() {
            let (kids, next) = after.split_at(after.iter().take_while(|(_, d)| d > depth).count());
            let folded = !held && self.folded(&card.id);
            out.push(Branch { card, kids, folded, hides_session: folded && kids.iter().any(|(k, _)| Some(k.id.as_str()) == session) });
            rest = next;
        }
        out
    }
}

/// A session's card and the cards under it.
pub(crate) struct Branch<'a> {
    pub(crate) card: &'a Card,
    pub(crate) kids: &'a [(Card, usize)],
    pub(crate) folded: bool,
    /// Folded over the session on screen, so the card stands in for it.
    pub(crate) hides_session: bool,
}

impl Branch<'_> {
    /// How tall the cards under it stand open.
    pub(crate) fn kids_height(&self) -> f32 {
        2. + fold::stacked(self.kids.iter().map(|(k, _)| ui::session_row_h(k.pinned, k.notice.is_some())), 2.)
    }
}

impl SidebarState {
    pub(crate) fn toggle_team(&mut self, leads: &[String]) {
        if self.team_folds.toggle_all(leads) {
            for lead in leads {
                self.closing.start(Fold::Team(lead.clone()));
            }
        }
    }
}

/// What a tree row with workers' trees under it folds.
pub(crate) struct RowTeam {
    /// The leads whose workers those trees hold; the row folds them together.
    pub(crate) leads: Vec<String>,
    pub(crate) kids: Vec<TreeRow>,
    pub(crate) folded: bool,
    /// A filter keeps it open.
    pub(crate) held: bool,
}

impl RowTeam {
    /// Whether `tree` is one of the rows it's folded over.
    pub(crate) fn hides(&self, tree: Option<&str>) -> bool {
        self.folded && self.kids.iter().any(|k| Some(k.key.as_str()) == tree)
    }
}

/// The tree a worker's tree sits under, and the lead that puts it there.
#[derive(Debug, PartialEq)]
pub(crate) struct TreeLead {
    tree: String,
    lead: String,
}

/// By each worker's tree, what it sits under; `tree_of` places folders.
pub(crate) fn tree_leads(cards: &[Card], tree_of: impl Fn(&str) -> Option<String>) -> HashMap<String, TreeLead> {
    let by_id: HashMap<&str, &Card> = cards.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut leads = HashMap::new();
    for card in cards {
        let Some(lead) = by_id.get(card.parent.as_str()) else { continue };
        if let (Some(tree), Some(parent)) = (tree_key(card, &tree_of), tree_key(lead, &tree_of))
            && tree != parent
        {
            leads.entry(tree).or_insert(TreeLead { tree: parent, lead: lead.id.clone() });
        }
    }
    leads
}

/// The top rows of `rows`, each holding the workers' trees under it; `held` keeps every team open, as a filter does.
pub(crate) fn nest_rows(rows: Vec<TreeRow>, leads: &HashMap<String, TreeLead>, folds: &TeamFolds, held: bool) -> Vec<TreeRow> {
    let parent = |r: &TreeRow| leads.get(&r.key).map_or(String::new(), |l| l.tree.clone());
    let mut top: Vec<TreeRow> = Vec::new();
    for (mut row, depth) in nest_by(rows, |r| r.key.clone(), parent) {
        row.depth = depth;
        let Some(above) = top.last_mut().filter(|_| depth > 0) else {
            top.push(row);
            continue;
        };
        let team = above.team.get_or_insert_with(|| RowTeam { leads: vec![], kids: vec![], folded: false, held });
        if let Some(l) = leads.get(&row.key).filter(|l| depth == 1 && !team.leads.contains(&l.lead)) {
            team.leads.push(l.lead.clone());
        }
        team.kids.push(row);
    }
    for team in top.iter_mut().filter_map(|r| r.team.as_mut()) {
        team.folded = !held && team.leads.iter().all(|l| folds.folded(l));
    }
    top
}

/// How many are in each state that's worth saying, most urgent first.
pub(crate) fn tally(statuses: &[Status]) -> Vec<(Status, String)> {
    [(Status::NeedsYou, "needs you"), (Status::Failed, "failed"), (Status::Working, "working"), (Status::Done, "done")]
        .into_iter()
        .filter_map(|(status, word)| {
            let n = statuses.iter().filter(|s| **s == status).count();
            (n > 0).then(|| (status, format!("{n} {word}")))
        })
        .collect()
}

/// `id` if `ids` lists it, else the nearest session it sits under that's listed.
pub(crate) fn nearest_listed(ids: &[String], id: &str, parent_of: impl Fn(&str) -> Option<String>) -> Option<String> {
    let mut at = Some(id.to_string());
    while let Some(here) = at.filter(|h| !h.is_empty()) {
        if ids.contains(&here) {
            return Some(here);
        }
        at = parent_of(&here);
    }
    None
}

/// The line down the side of a lead's workers, `left` of them.
pub(super) fn rail(left: f32, bottom: f32) -> Div {
    div().absolute().top(px(2.)).bottom(px(bottom)).left(px(left)).w(px(1.)).bg(SEPARATOR)
}

/// `mark`, then while `row` is folded how many rows it hides.
pub(super) fn with_count(mark: Option<AnyElement>, row: &TreeRow) -> Option<AnyElement> {
    let Some(team) = row.team.as_ref().filter(|t| t.folded) else { return mark };
    Some(div().flex().items_center().gap(px(6.)).children(mark).child(div().px(px(2.)).text_size(px(11.)).text_color(TEXT_3).child(team.kids.len().to_string())).into_any_element())
}

impl Desktop {
    fn toggle_team_on_click(leads: Vec<String>, cx: &mut Context<Self>) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            this.sidebar.toggle_team(&leads);
            cx.notify();
        })
    }

    /// The line under a lead's card that says how its workers are doing and, unless `held`, folds them.
    pub(super) fn team_disclosure(b: &Branch, held: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let statuses: Vec<Status> = b.kids.iter().map(|(k, _)| k.status).collect();
        let mut sum = div().flex_1().min_w_0().flex().gap(px(4.)).overflow_hidden().whitespace_nowrap().child(plural(statuses.len(), "worker", "workers"));
        for (status, text) in tally(&statuses) {
            sum = sum.child("·").child(if status == Status::NeedsYou { div().font_weight(FontWeight::SEMIBOLD).text_color(WAITING).child(text) } else { div().child(text) });
        }
        let leads = vec![b.card.id.clone()];
        div()
            .id(id(format!("team-sum:{}", b.card.id)))
            .h(px(26.))
            .px(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .text_size(px(12.))
            .text_color(TEXT_3)
            .when(!held, |d| {
                d.cursor_pointer().hover(|s| s.bg(FILL_2)).active(|s| s.bg(FILL_3)).child(icon(if b.folded { "chevron-right" } else { "chevron-down" }, 12., TEXT_4)).on_click(Self::toggle_team_on_click(leads, cx))
            })
            .child(sum)
    }

    /// The chevron that folds the workers under `row`, if it has any and no filter holds them open.
    pub(super) fn team_chevron(row: &TreeRow, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let team = row.team.as_ref().filter(|t| !t.held)?;
        Some(ui::chevron(id(format!("aside-team:{}", row.key)), !team.folded).on_click(Self::toggle_team_on_click(team.leads.clone(), cx)))
    }

    /// The workers' rows under `row`, on a rail, springing shut when it folds, and the height they take up.
    pub(super) fn team_rows(&self, p: &str, row: &TreeRow, current: Option<&str>, setups: &HashMap<String, String>, cx: &mut Context<Self>) -> Option<(AnyElement, f32)> {
        let team = row.team.as_ref()?;
        let drawn = team.leads.iter().any(|l| self.sidebar.closing.drawn(&Fold::Team(l.clone()), !team.folded));
        let rows: Vec<AnyElement> = if drawn { team.kids.iter().map(|k| self.place_row(p, k, current, setups, None, cx)).collect() } else { vec![] };
        let open_height = 2. + fold::stacked(vec![ui::SIDEBAR_ROW_H; team.kids.len()], 1.);
        let body = div().relative().flex().flex_col().gap(px(1.)).pt(px(1.)).pb(px(1.)).when(!rows.is_empty(), |d| d.child(rail(26., 6.))).children(rows);
        let rows = fold::springing(id(format!("aside-team-fold:{}", row.key)), !team.folded, open_height, body).into_any_element();
        Some((rows, if team.folded { 0. } else { open_height }))
    }
}

#[cfg(test)]
mod tests {
    use super::super::sections::{Kind, TreeRow};
    use super::{TeamFolds, TreeLead, nearest_listed, nest_rows, tally, tree_leads};
    use crate::status::{self, Card, Status};
    use agents::Summary;
    use std::collections::HashMap;

    fn card(id: &str, parent: &str, cwd: &str) -> Card {
        status::card(&Summary { id: id.into(), status: "idle".into(), attached: true, parent: parent.into(), ..Default::default() }, cwd)
    }

    fn row(key: &str) -> TreeRow {
        TreeRow { key: key.into(), label: key.into(), tip: String::new(), kind: Kind::Worktree, depth: 0, team: None }
    }

    fn under(tree: &str, lead: &str) -> TreeLead {
        TreeLead { tree: tree.into(), lead: lead.into() }
    }

    fn leads() -> HashMap<String, TreeLead> {
        [("/w1".to_string(), under("/lead", "lead")), ("/w2".to_string(), under("/lead", "lead"))].into()
    }

    fn folded(leads: &[&str]) -> TeamFolds {
        let mut folds = TeamFolds::default();
        folds.toggle_all(&leads.iter().map(|l| l.to_string()).collect::<Vec<_>>());
        folds
    }

    fn deck() -> Vec<(Card, usize)> {
        vec![(card("lead", "", "/"), 0), (card("w1", "lead", "/"), 1), (card("w2", "lead", "/"), 1), (card("x", "", "/"), 0)]
    }

    #[test]
    fn a_workers_tree_sits_under_its_leads_tree_but_a_child_in_the_same_tree_doesnt_move_it() {
        let cards = vec![card("lead", "", "/lead"), card("w1", "lead", "/w1"), card("here", "lead", "/lead"), card("x", "", "/x")];
        let got = tree_leads(&cards, |cwd| Some(cwd.to_string()));
        assert_eq!(got, HashMap::from([("/w1".to_string(), under("/lead", "lead"))]));
    }

    #[test]
    fn a_session_opened_in_a_local_places_its_tree_by_the_local() {
        let lead = Card { local: "l1".into(), ..card("lead", "", "/p") };
        let got = tree_leads(&[lead, card("w1", "lead", "/w1")], |cwd| Some(cwd.to_string()));
        assert_eq!(got, HashMap::from([("/w1".to_string(), under("l1", "lead"))]));
    }

    #[test]
    fn workers_trees_follow_their_leads_a_level_in() {
        let got = nest_rows(vec![row("/w2"), row("/x"), row("/lead"), row("/w1")], &leads(), &TeamFolds::default(), false);
        assert_eq!(got.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["/x", "/lead"]);
        let team = got[1].team.as_ref().unwrap();
        let kids: Vec<(&str, usize)> = team.kids.iter().map(|r| (r.key.as_str(), r.depth)).collect();
        assert_eq!((team.leads.as_slice(), kids), (["lead".to_string()].as_slice(), vec![("/w2", 1), ("/w1", 1)]));
    }

    #[test]
    fn a_workers_tree_whose_leads_tree_isnt_listed_stays_at_the_top() {
        let got = nest_rows(vec![row("/w1")], &leads(), &TeamFolds::default(), false);
        assert_eq!((got[0].depth, got[0].team.is_some()), (0, false));
    }

    #[test]
    fn trees_that_lead_each_other_both_stay_listed() {
        let leads = [("/a".to_string(), under("/b", "b")), ("/b".to_string(), under("/a", "a"))].into();
        assert_eq!(nest_rows(vec![row("/a"), row("/b")], &leads, &TeamFolds::default(), false).len(), 2);
    }

    #[test]
    fn a_filter_holds_a_folded_team_open() {
        let fold = |held| nest_rows(vec![row("/lead"), row("/w1")], &leads(), &folded(&["lead"]), held)[0].team.as_ref().map(|t| (t.folded, t.held));
        assert_eq!((fold(false), fold(true)), (Some((true, false)), Some((false, true))));
    }

    #[test]
    fn a_folded_row_hides_only_the_trees_under_it_and_is_selected_for_them() {
        let got = nest_rows(vec![row("/lead"), row("/w1")], &leads(), &folded(&["lead"]), false);
        assert!(got[0].selected(Some("/w1")) && got[0].selected(Some("/lead")) && !got[0].selected(Some("/x")) && !got[0].selected(None));
        let open = nest_rows(vec![row("/lead"), row("/w1")], &leads(), &TeamFolds::default(), false);
        assert!(!open[0].selected(Some("/w1")));
    }

    #[test]
    fn a_row_with_two_teams_is_folded_only_while_both_are() {
        let leads = [("/w1".to_string(), under("/lead", "a")), ("/w2".to_string(), under("/lead", "b"))].into();
        let fold = |folds: &TeamFolds| nest_rows(vec![row("/lead"), row("/w1"), row("/w2")], &leads, folds, false)[0].team.as_ref().map(|t| (t.folded, t.leads.len()));
        assert_eq!((fold(&folded(&["a"])), fold(&folded(&["a", "b"]))), (Some((false, 2)), Some((true, 2))));
    }

    #[test]
    fn toggling_folds_all_unless_all_are_folded_then_unfolds_them() {
        let both = ["a".to_string(), "b".to_string()];
        let mut folds = folded(&["a"]);
        assert!(folds.toggle_all(&both));
        assert!(folds.folded("a") && folds.folded("b"));
        assert!(!folds.toggle_all(&both));
        assert!(!folds.folded("a") && !folds.folded("b"));
    }

    #[test]
    fn folding_hides_only_what_sits_under_the_lead_unless_held() {
        let ids = |held| folded(&["lead"]).shown(deck(), held).into_iter().map(|(c, _)| c.id).collect::<Vec<_>>();
        assert_eq!(ids(false), ["lead", "x"]);
        assert_eq!(ids(true), ["lead", "w1", "w2", "x"]);
    }

    #[test]
    fn a_folded_lead_keeps_its_workers_and_stands_in_for_the_one_on_screen() {
        let rows = deck();
        let folds = folded(&["lead"]);
        let got = folds.branches(&rows, false, Some("w2"));
        let shape: Vec<(&str, usize, bool, bool)> = got.iter().map(|b| (b.card.id.as_str(), b.kids.len(), b.folded, b.hides_session)).collect();
        assert_eq!(shape, [("lead", 2, true, true), ("x", 0, false, false)]);
        assert!(!folds.branches(&rows, false, Some("x"))[0].hides_session);
        let held = folds.branches(&rows, true, Some("w2"));
        assert!(!held[0].folded && !held[0].hides_session);
    }

    #[test]
    fn going_to_a_folded_worker_unfolds_all_it_sits_under_and_leaves_other_teams() {
        let parent_of = |id: &str| Some(match id { "w" => "mid", "mid" => "lead", _ => "" }.to_string());
        let mut folds = folded(&["lead", "mid", "other"]);
        folds.reveal("w", parent_of);
        assert_eq!((folds.folded("lead"), folds.folded("mid"), folds.folded("other")), (false, false, true));
    }

    #[test]
    fn a_hidden_session_counts_from_the_nearest_row_it_sits_under() {
        let ids = ["a".to_string(), "lead".to_string(), "b".to_string()];
        let parent_of = |id: &str| Some(if id == "w" { "lead" } else { "" }.to_string());
        assert_eq!(nearest_listed(&ids, "w", parent_of).as_deref(), Some("lead"));
        assert_eq!(nearest_listed(&ids, "b", parent_of).as_deref(), Some("b"));
        assert_eq!(nearest_listed(&ids, "gone", parent_of), None);
    }

    #[test]
    fn the_tally_puts_what_needs_you_first_and_leaves_out_the_idle() {
        let got = tally(&[Status::Working, Status::Idle, Status::NeedsYou, Status::Working]);
        assert_eq!(got, [(Status::NeedsYou, "1 needs you".to_string()), (Status::Working, "2 working".to_string())]);
    }
}
