use super::logic::Reach;
use std::collections::{HashMap, HashSet};
use theme::Token;

/// What `Desktop` reads for one ticket's card or row.
pub(crate) struct Extra {
    pub(crate) reach: Reach,
    /// Its short ID, once it has a number.
    pub(crate) key: Option<String>,
    /// What its agent is at.
    pub(crate) activity: Option<String>,
    /// What its session is asking, in words.
    pub(crate) asking: Option<String>,
    /// The request its Allow and Deny buttons answer, when a card can answer it.
    pub(crate) answer: Option<String>,
    pub(crate) pr: Option<Pr>,
    /// Its uncommitted lines added and removed, when it has files changed.
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

/// What the page reads of `Desktop`, handed down each time it draws.
#[derive(Default)]
pub(crate) struct Side {
    /// The tickets drawn, the open menu's and the open page's, by their place in the snapshot.
    pub(crate) extras: HashMap<usize, Extra>,
    /// The projects a ticket can be made in, in the order pickers list them.
    pub(crate) pickable: Vec<String>,
    /// Those and every folder the page names.
    pub(crate) projects: HashMap<String, Project>,
    /// Each agent's name where Settings label it.
    pub(crate) labels: HashMap<String, String>,
    pub(crate) installed: HashSet<&'static str>,
    pub(crate) offered: Vec<&'static str>,
    /// The saved views' names, which of them the filters and tab are on, and whether Save view has anything to add.
    pub(crate) views: Vec<String>,
    pub(crate) view_on: Option<usize>,
    pub(crate) can_save_view: bool,
    /// Whether the composer's folder is a git repository.
    pub(crate) composer_repo: bool,
}

pub(super) static NO_EXTRA: Extra =
    Extra { reach: Reach { session: false, tree: false }, key: None, activity: None, asking: None, answer: None, pr: None, diff: None };
