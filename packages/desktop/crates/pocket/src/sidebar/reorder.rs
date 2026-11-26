use super::Sidebar;
use super::aside::{AsideFrame, Body, GroupBody, PinnedItem, ProjectFrame, ScratchRow, SidebarEvent, Tail};
use super::sections::Section;
use crate::slide::Slide;
use gpui_kit::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;
use theme::*;

/// A list of sidebar rows put in another order by dragging one of them.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum List {
    /// A project's worktree and Local rows.
    Trees(String),
    /// The project blocks in a section.
    Projects(Section),
    /// The Threads section's folders.
    Threads,
}

impl List {
    fn gap(&self) -> f32 {
        match self {
            List::Projects(Section::Pinned | Section::Group(_)) => 2.,
            List::Trees(_) | List::Projects(Section::Other) | List::Threads => 1.,
        }
    }

    /// The keys of the rows the list shows in `f`, in order.
    fn keys(&self, f: &AsideFrame) -> Vec<String> {
        let paths = |blocks: &[ProjectFrame]| blocks.iter().map(|b| b.path.clone()).collect();
        let sections = match &f.body {
            Body::Sections(s) => Some(s),
            Body::Empty(_) => None,
        };
        let pinned = || sections.and_then(|s| s.pinned.as_ref()).into_iter().flat_map(|p| &p.items);
        let groups = || sections.into_iter().flat_map(|s| &s.groups);
        let other = || match sections.map(|s| &s.tail) {
            Some(Tail::Other { blocks, .. }) => blocks.as_slice(),
            _ => &[],
        };
        match self {
            List::Threads => f
                .scratch
                .iter()
                .flat_map(|s| &s.rows)
                .filter_map(|r| match r {
                    ScratchRow::Folder { path, .. } => Some(path.clone()),
                    ScratchRow::Loose { .. } => None,
                })
                .collect(),
            List::Projects(Section::Pinned) => pinned()
                .filter_map(|i| match i {
                    PinnedItem::Project(b) => Some(b.path.clone()),
                    PinnedItem::Tree { .. } => None,
                })
                .collect(),
            List::Projects(Section::Group(id)) => groups()
                .find(|g| g.id == *id)
                .map(|g| match &g.body {
                    GroupBody::Blocks(blocks) => paths(blocks),
                    GroupBody::Hidden | GroupBody::Hint => vec![],
                })
                .unwrap_or_default(),
            List::Projects(Section::Other) => paths(other()),
            List::Trees(project) => pinned()
                .filter_map(|i| match i {
                    PinnedItem::Project(b) => Some(b),
                    PinnedItem::Tree { .. } => None,
                })
                .chain(groups().flat_map(|g| match &g.body {
                    GroupBody::Blocks(blocks) => blocks.as_slice(),
                    GroupBody::Hidden | GroupBody::Hint => &[],
                }))
                .chain(other())
                .find(|b| b.path == *project)
                .map(|b| b.rows.iter().map(|r| r.key.clone()).collect())
                .unwrap_or_default(),
        }
    }
}

/// What a row drag carries. The held row draws itself in its list, so the drag has no preview.
#[derive(Clone)]
pub(crate) struct DragRow {
    pub(crate) list: List,
    pub(crate) key: String,
}

/// A row held in its list and the rows sliding round it; once released, every row sliding home.
pub(crate) struct Held {
    list: List,
    slide: Slide,
    /// The list's keys as the hold began.
    keys: Vec<String>,
    /// The order a release asked for, which the list must show next.
    landing: Option<Vec<String>>,
}

impl Held {
    fn new(list: List, from: usize, grab: f32, pointer: f32, keys: Vec<String>) -> Self {
        let gap = list.gap();
        Self { list, slide: Slide::new(from, grab, pointer, gap), keys, landing: None }
    }

    /// Follows the pointer at `y`, `inside` the sidebar or not, with the list scrolled to show `seen`, top to bottom; true when that changes what's drawn.
    fn point(&mut self, y: f32, inside: bool, seen: (f32, f32)) -> bool {
        // Past a project list's edge lies another section, which takes the drop itself.
        let reach = if matches!(self.list, List::Projects(_)) { 0. } else { ui::SIDEBAR_ROW_H / 2. };
        // The held row draws unclipped, so off the scrolled view it would float over the rest and land among rows out of sight.
        let unseen = y < seen.0 || y > seen.1;
        let away = !inside || unseen || self.slide.span().is_some_and(|(top, bottom)| y < top - reach || y > bottom + reach);
        let changed = self.slide.pointer != y || self.slide.away != away;
        (self.slide.pointer, self.slide.away) = (y, away);
        changed
    }

    /// Lets the row go, once; the keys of the row moved and the row it takes the place of, when that moves it.
    fn release(&mut self) -> Option<(String, String)> {
        let (from, to) = self.slide.release()?;
        let mut order = self.keys.clone();
        let key = order.remove(from);
        order.insert(to, key);
        self.landing = Some(order);
        (from != to).then(|| (self.keys[from].clone(), self.keys[to].clone()))
    }

    /// Steps the slide a frame, with the list's rows laid at `slots` and showing `shown`; whether any row is still on its way, or `None` once the hold is over.
    fn frame(&mut self, slots: Vec<(f32, f32)>, shown: &[String], dragging: bool, now: Instant, reduce_motion: bool) -> Option<bool> {
        // gpui ends a drag on any mouse up, even one the sidebar never sees; a hold outliving it would keep its row lifted.
        if !self.slide.lay(slots) || !(self.slide.released || dragging) {
            return None;
        }
        // The offsets are for the order the release asked for; any other means the move didn't happen as asked.
        if self.landing.as_ref().unwrap_or(&self.keys) != shown {
            return None;
        }
        let moving = self.slide.ease(now, reduce_motion);
        (moving || !self.slide.released).then_some(moving)
    }

    /// Item `ix` of `list`'s offset from its slot and whether it's the held one.
    fn offset(&self, list: &List, ix: usize) -> Option<(f32, bool)> {
        (self.list == *list).then(|| (self.slide.offsets.get(ix).copied().unwrap_or(0.), self.slide.held() == Some(ix)))
    }
}

/// The sidebar's lists' layouts, and a row held in one.
#[derive(Default)]
pub(crate) struct Reorder {
    lists: RefCell<HashMap<List, ScrollHandle>>,
    held: Option<Held>,
    /// The scrolled areas the lists show in: the projects' and the Threads section's.
    pub(super) projects: ScrollHandle,
    pub(super) threads: ScrollHandle,
}

impl Sidebar {
    /// Records where `list`'s rows, `el`'s children, lie, so a row held in it knows which it passes.
    pub(super) fn tracks(&self, list: List, el: Stateful<Div>) -> Stateful<Div> {
        el.track_scroll(self.reorder.lists.borrow_mut().entry(list).or_default())
    }

    /// Makes `el` drag row `key`, item `ix` of `list`; the list slides round it only when it `slides`, otherwise the drag can only drop elsewhere.
    pub(super) fn holds(&self, el: Stateful<Div>, list: List, ix: usize, key: String, slides: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let this = cx.weak_entity();
        el.on_drag(DragRow { list: list.clone(), key }, move |_, grab, window, cx| {
            if slides {
                let pointer = f32::from(window.mouse_position().y);
                this.update(cx, |this, cx| {
                    let keys = this.frame.as_ref().map(|f| list.keys(f)).unwrap_or_default();
                    this.reorder.held = Some(Held::new(list.clone(), ix, grab.y.into(), pointer, keys));
                    cx.notify();
                })
                .ok();
            }
            cx.new(|_| EmptyView)
        })
    }

    /// `el`, item `ix` of `list`, drawn where a held row's slide has it; the held row draws over the rest.
    pub(super) fn placed(&self, el: AnyElement, list: &List, ix: usize) -> AnyElement {
        let Some((offset, lifted)) = self.reorder.held.as_ref().and_then(|h| h.offset(list, ix)) else { return el };
        // The slot keeps the row's place, so the list can measure where rows sit while they slide.
        let visual = div().relative().top(px(offset)).child(el);
        let visual =
            if lifted { deferred(visual.rounded(px(8.)).bg(nav_fill()).shadow(ui::row_shadow())).into_any_element() } else { visual.into_any_element() };
        div().child(visual).into_any_element()
    }

    pub(super) fn move_row(&mut self, e: &DragMoveEvent<DragRow>, cx: &mut Context<Self>) {
        let list = &e.drag(cx).list;
        let Some(held) = self.reorder.held.as_mut().filter(|h| h.list == *list) else { return };
        let (x, y) = (e.event.position.x, f32::from(e.event.position.y));
        let view = if held.list == List::Threads { &self.reorder.threads } else { &self.reorder.projects }.bounds();
        let seen = (f32::from(view.top()), f32::from(view.bottom()));
        // A drag move comes every frame, moved or not; notifying on each would redraw forever.
        if held.point(y, e.bounds.left() <= x && x <= e.bounds.right(), seen) {
            cx.notify();
        }
    }

    pub(super) fn release_row(&mut self, cx: &mut Context<Self>) {
        let Some(held) = self.reorder.held.as_mut() else { return };
        if let Some((from, to)) = held.release() {
            cx.emit(match &held.list {
                List::Trees(project) => SidebarEvent::DropTree { project: project.clone(), from, to },
                List::Projects(section) => SidebarEvent::DropProject { path: from, onto: to, section: *section },
                List::Threads => SidebarEvent::DropScratch { from, to },
            });
        }
        cx.notify();
    }

    /// Steps a held row's slide a frame, asking for the next while rows move.
    pub(super) fn slide_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(held), Some(f)) = (self.reorder.held.as_mut(), self.frame.as_ref()) else { return };
        let slots = self.reorder.lists.borrow().get(&held.list).map(slots).unwrap_or_default();
        match held.frame(slots, &held.list.keys(f), cx.has_active_drag(), Instant::now(), f.reduce_motion) {
            Some(false) => {}
            Some(true) => window.request_animation_frame(),
            None => {
                self.reorder.held = None;
                window.request_animation_frame();
            }
        }
    }
}

/// Each of a list's rows' top and height in the window; none before the list is first drawn.
fn slots(list: &ScrollHandle) -> Vec<(f32, f32)> {
    (0..).map_while(|i| list.bounds_for_item(i)).map(|b| (f32::from(b.top()), f32::from(b.size.height))).collect()
}

#[cfg(test)]
mod tests {
    use super::{Held, List, Section};
    use std::time::{Duration, Instant};

    const ROWS: [(f32, f32); 3] = [(0., 28.), (29., 28.), (58., 28.)];
    const ALL: (f32, f32) = (-1000., 1000.);

    fn keys() -> Vec<String> {
        ["a", "b", "c"].map(String::from).to_vec()
    }

    /// Row `from` of three held 5px below its top, the pointer at `y`.
    fn holding(from: usize, y: f32) -> Held {
        let mut h = Held::new(List::Threads, from, 5., y, keys());
        h.frame(ROWS.to_vec(), &keys(), true, Instant::now(), true);
        h.point(y, true, ALL);
        h
    }

    #[test]
    fn a_released_row_moves_onto_the_row_whose_place_it_takes() {
        assert_eq!(holding(0, 70.).release(), Some(("a".into(), "c".into())));
        assert_eq!(holding(2, 6.).release(), Some(("c".into(), "a".into())));
        assert_eq!(holding(1, 34.).release(), None);
    }

    #[test]
    fn rows_slide_round_the_held_one_and_home_once_the_list_shows_the_new_order() {
        let mut h = holding(0, 50.);
        assert_eq!(h.slide.offsets, [45., -29., -29.]);
        h.release();
        let (moved, t) = (["b", "c", "a"].map(String::from), Instant::now());
        assert_eq!(h.frame(ROWS.to_vec(), &moved, false, t, false), Some(true));
        assert_eq!(h.slide.offsets, [0., 0., -13.]);
        assert_eq!(h.frame(ROWS.to_vec(), &moved, false, t + Duration::from_secs(1), false), None);
    }

    #[test]
    fn a_hold_ends_when_the_list_shows_another_order_than_the_release_asked_for() {
        let mut h = holding(0, 50.);
        h.release();
        assert_eq!(h.frame(ROWS.to_vec(), &keys(), false, Instant::now(), false), None);
    }

    #[test]
    fn a_hold_ends_with_the_drag_unless_released_here() {
        let mut h = holding(0, 70.);
        assert_eq!(h.frame(ROWS.to_vec(), &keys(), false, Instant::now(), false), None);
    }

    #[test]
    fn a_row_dragged_out_of_its_list_lets_the_others_slide_home_and_drops_nowhere() {
        let mut h = holding(0, 70.);
        assert!(h.point(130., true, ALL));
        h.frame(ROWS.to_vec(), &keys(), true, Instant::now(), true);
        assert_eq!(h.slide.offsets, [0.; 3]);
        assert_eq!(h.release(), None);
        let mut h = holding(0, 70.);
        h.point(70., false, ALL);
        assert_eq!(h.release(), None);
    }

    #[test]
    fn a_project_dragged_just_past_its_section_drops_nowhere_in_it() {
        let mut h = Held::new(List::Projects(Section::Other), 0, 5., 70., keys());
        h.frame(ROWS.to_vec(), &keys(), true, Instant::now(), true);
        h.point(96., true, ALL);
        assert_eq!(h.release(), None);
    }

    #[test]
    fn a_row_held_past_the_scrolled_view_drops_nowhere() {
        let mut h = holding(0, 70.);
        h.point(70., true, (0., 60.));
        assert_eq!(h.release(), None);
    }
}
