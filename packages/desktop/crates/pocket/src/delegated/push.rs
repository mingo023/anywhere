use super::child_view::parent_of;
use crate::glide::Glide;
use agents::Summary;
use std::time::Instant;

/// How far a session slides in from the side it arrives from.
const SLIDE: f32 = 28.;
const RESPONSE: f32 = 0.42;

/// Which way the user went between a chat and a session it delegated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Dir {
    /// To the delegated session.
    Into,
    /// Back to the chat that delegated it.
    Back,
}

pub(super) fn direction(list: &[Summary], from: &str, to: &str) -> Option<Dir> {
    if parent_of(list, to).is_some_and(|p| p.id == from) {
        Some(Dir::Into)
    } else if parent_of(list, from).is_some_and(|p| p.id == to) {
        Some(Dir::Back)
    } else {
        None
    }
}

/// How far across, and how faded in, the arriving session is when it has come `n` of the way.
fn slide(dir: Dir, n: f32) -> (f32, f32) {
    let side = match dir {
        Dir::Into => 1.,
        Dir::Back => -1.,
    };
    (side * SLIDE * (1. - n), n)
}

/// A session arriving from the one the user came from; only the arriving one moves.
pub(super) struct Push {
    pub(super) from: String,
    pub(super) to: String,
    dir: Dir,
    glide: Glide,
}

impl Push {
    /// The push from `from` to `to`. Sent back before `prev` had finished, it starts as faded in as `to` still was.
    pub(super) fn begin(prev: Option<&Push>, from: String, to: String, dir: Dir) -> Self {
        let start = prev.filter(|p| p.from == to).map_or(0., |p| 1. - p.glide.at().clamp(0., 1.));
        Self { from, to, dir, glide: Glide::from(start).response(RESPONSE) }
    }

    /// Steps it; true while it still moves.
    pub(super) fn tick(&mut self, now: Instant, reduce_motion: bool) -> bool {
        self.glide.tick(1., now, reduce_motion)
    }

    /// Where the arriving session is across, and how faded in.
    pub(super) fn offset(&self) -> (f32, f32) {
        slide(self.dir, self.glide.at().clamp(0., 1.))
    }
}

#[cfg(test)]
mod tests {
    use super::{Dir, Push, direction, slide};
    use agents::Summary;
    use std::time::{Duration, Instant};

    fn session(id: &str, parent: &str, relationship: &str) -> Summary {
        Summary { id: id.into(), parent: parent.into(), relationship: relationship.into(), ..Default::default() }
    }

    #[test]
    fn going_to_a_delegated_session_slides_in_from_the_right_and_going_back_from_the_left() {
        assert_eq!(slide(Dir::Into, 0.), (28., 0.));
        assert_eq!(slide(Dir::Back, 0.), (-28., 0.));
        assert_eq!(slide(Dir::Back, 0.5), (-14., 0.5));
        assert_eq!([slide(Dir::Into, 1.), slide(Dir::Back, 1.)], [(0., 1.); 2]);
    }

    #[test]
    fn only_a_move_between_a_chat_and_a_session_it_delegated_pushes() {
        let list = [session("p", "", ""), session("c", "p", "subagent"), session("f", "p", "fork"), session("o", "", "")];
        assert_eq!(direction(&list, "p", "c"), Some(Dir::Into));
        assert_eq!(direction(&list, "c", "p"), Some(Dir::Back));
        assert_eq!([direction(&list, "p", "f"), direction(&list, "p", "o"), direction(&list, "c", "o")], [None, None, None]);
    }

    #[test]
    fn a_push_runs_from_off_to_in_place_and_honours_reduced_motion() {
        let (mut push, start) = (Push::begin(None, "p".into(), "c".into(), Dir::Into), Instant::now());
        assert!(push.tick(start, false));
        assert_eq!(push.offset(), (28., 0.), "it starts off to the side and unseen");
        (1..=90).for_each(|i| _ = push.tick(start + Duration::from_millis(16 * i), false));
        assert_eq!(push.offset(), (0., 1.));
        let mut snapped = Push::begin(None, "p".into(), "c".into(), Dir::Into);
        assert!(!snapped.tick(start, true));
        assert_eq!(snapped.offset(), (0., 1.));
    }

    #[test]
    fn going_back_midway_starts_as_faded_in_as_the_session_left_behind_still_was() {
        let (mut into, start) = (Push::begin(None, "p".into(), "c".into(), Dir::Into), Instant::now());
        (0..=8).for_each(|i| _ = into.tick(start + Duration::from_millis(16 * i), false));
        let (_, shown) = into.offset();
        assert!(shown > 0.1 && shown < 0.9, "midway: {shown}");
        let back = Push::begin(Some(&into), "c".into(), "p".into(), Dir::Back);
        let (_, back_shown) = back.offset();
        assert!((back_shown - (1. - shown)).abs() < 1e-4, "the chat was {} faded in", 1. - shown);
        let elsewhere = Push::begin(Some(&into), "x".into(), "y".into(), Dir::Back);
        assert_eq!(elsewhere.offset().1, 0.);
    }
}
