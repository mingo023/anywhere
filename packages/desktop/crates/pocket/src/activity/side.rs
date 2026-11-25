use agents::Permission;
use agents::Recap;
use git::github::Pr;
use std::collections::HashMap;
use theme::Token;

pub(super) const FILES_SHOWN: usize = 5;

pub(crate) struct Extra {
    pub(crate) mark: ui::Mark,
    pub(crate) pr: Option<((&'static str, Token), u32)>,
    pub(crate) diff: Option<(usize, usize)>,
    pub(crate) ask: Option<Permission>,
    pub(crate) retry: bool,
}

pub(crate) struct Drawer {
    pub(crate) recap: Recap,
    pub(crate) files: Option<Vec<(String, usize, usize)>>,
    pub(crate) pr: Option<Pr>,
}

#[derive(Default)]
pub(crate) struct Side {
    pub(crate) extras: HashMap<String, Extra>,
    pub(crate) observe_only: bool,
    pub(crate) names: HashMap<String, String>,
    pub(crate) drawer: Option<Drawer>,
}
