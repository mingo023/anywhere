use super::recap::Recap;
use agents::Permission;
use git::github::Pr;
use std::collections::HashMap;
use theme::Token;

/// The files the drawer's Changes box lists; the rest are a click away in the session.
pub(super) const FILES_SHOWN: usize = 5;

/// What `Desktop` reads for one session's card.
pub(crate) struct Extra {
    pub(crate) mark: ui::Mark,
    /// Its PR's glyph and number.
    pub(crate) pr: Option<((&'static str, Token), u32)>,
    /// Its uncommitted lines added and removed, when it has files changed.
    pub(crate) diff: Option<(usize, usize)>,
    /// What the session that needs you is asked.
    pub(crate) ask: Option<Permission>,
    /// A failed turn has its prompt to send again.
    pub(crate) retry: bool,
}

/// The previewed session's drawer.
pub(crate) struct Drawer {
    pub(crate) recap: Recap,
    pub(crate) files: Option<Vec<(String, usize, usize)>>,
    pub(crate) pr: Option<Pr>,
}

/// What the page reads of `Desktop`, handed down each time it draws.
#[derive(Default)]
pub(crate) struct Side {
    pub(crate) extras: HashMap<String, Extra>,
    pub(crate) observe_only: bool,
    /// The names the open menu's values go by.
    pub(crate) names: HashMap<String, String>,
    pub(crate) drawer: Option<Drawer>,
}
