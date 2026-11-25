use super::logic::Reach;
use std::collections::{HashMap, HashSet};
use theme::Token;

pub(crate) struct Extra {
    pub(crate) reach: Reach,
    pub(crate) key: Option<String>,
    pub(crate) activity: Option<String>,
    pub(crate) asking: Option<String>,
    pub(crate) answer: Option<String>,
    pub(crate) pr: Option<Pr>,
    pub(crate) diff: Option<(usize, usize)>,
}

pub(crate) struct Pr {
    pub(crate) glyph: (&'static str, Token),
    pub(crate) number: u32,
    pub(crate) green: bool,
}

pub(crate) struct Project {
    pub(crate) name: String,
    pub(crate) mark: ui::Mark,
}

#[derive(Default)]
pub(crate) struct Side {
    pub(crate) extras: HashMap<String, Extra>,
    pub(crate) pickable: Vec<String>,
    pub(crate) projects: HashMap<String, Project>,
    pub(crate) labels: HashMap<String, String>,
    pub(crate) installed: HashSet<&'static str>,
    pub(crate) offered: Vec<&'static str>,
    pub(crate) views: Vec<String>,
    pub(crate) view_on: Option<usize>,
    pub(crate) can_save_view: bool,
    pub(crate) composer_repo: bool,
}

pub(super) static NO_EXTRA: Extra =
    Extra { reach: Reach { session: false, tree: false }, key: None, activity: None, asking: None, answer: None, pr: None, diff: None };
