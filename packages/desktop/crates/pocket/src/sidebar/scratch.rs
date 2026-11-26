use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Screen};
use crate::modals::form::shared;
use crate::removal::dispose;
use crate::sidebar::aside::{ScratchFrame, ScratchRow};
use crate::sidebar::fold::Fold;
use crate::sidebar::sections::matches;
use crate::sidebar::{rolled_state, row_was};
use crate::spaces::{Space, space_of};
use crate::status::Card;
use crate::util::{now_ms, tilde};
use gpui_kit::*;
use std::path::Path;

impl Desktop {
    /// The Threads section above Projects; `None` when the sidebar's filter leaves nothing in it.
    pub(super) fn scratch_frame(&self, query: &str, space: &Space, cx: &mut Context<Self>) -> Option<ScratchFrame> {
        let filtering = !query.is_empty();
        let rows: Vec<String> =
            self.store.scratches().into_iter().filter(|p| space_of(p) == *space && (!filtering || matches(&self.place_name(p), query))).collect();
        let loose: Vec<String> = self
            .loose_folders(space, cx)
            .into_iter()
            .filter(|p| !filtering || matches(&thread_title(&self.cards(p, cx)), query) || matches(p, query))
            .collect();
        let rows: Vec<String> = rows.into_iter().chain(loose.iter().cloned()).collect();
        if filtering && rows.is_empty() {
            return None;
        }
        let open = filtering || !self.store.scratch_folded;
        let roll = (!open).then(|| rolled_state(&rows.iter().flat_map(|p| self.cards(p, cx)).collect::<Vec<_>>())).flatten();
        let current = self.place(cx).filter(|_| self.screen == Screen::Sessions && self.project.as_deref().is_some_and(|p| rows.iter().any(|r| r == p)));
        let drawn = if self.sidebar.read(cx).closing.drawn(&Fold::Scratch, open) {
            rows.iter()
                .enumerate()
                .map(|(ix, p)| {
                    let selected = current.as_deref() == Some(p.as_str());
                    if loose.contains(p) {
                        let cards = self.cards(p, cx);
                        let title = thread_title(&cards);
                        ScratchRow::Loose {
                            path: p.clone(),
                            selected,
                            state: rolled_state(&cards),
                            was: row_was(&self.renames, p, &title),
                            folder: short_folder(&tilde(p)),
                            title,
                        }
                    } else {
                        let name = self.place_name(p);
                        ScratchRow::Folder {
                            ix,
                            path: p.clone(),
                            selected,
                            state: rolled_state(&self.cards(p, cx)),
                            was: row_was(&self.renames, p, &name),
                            renames: self.agents.names_offered(),
                            name,
                        }
                    }
                })
                .collect()
        } else {
            vec![]
        };
        Some(ScratchFrame { open, roll, count: rows.len(), rows: drawn })
    }

    /// Makes a Scratch folder on the space on screen and returns its key.
    pub(crate) fn make_scratch(&self, cx: &mut Context<Self>) -> Task<Result<String, String>> {
        let seed = now_ms() as usize;
        let Some(id) = self.space().host().map(str::to_string) else {
            let root = project::scratch::root(&shared());
            return cx.background_executor().spawn(async move { project::scratch::create(&root, seed) });
        };
        cx.background_executor().spawn(async move {
            let host = hosts::remote::connected(&id).ok_or_else(|| git::NotConnected::error().to_string())?;
            let root = project::scratch::root(&host.shared_dir().map_err(|e| e.to_string())?);
            let path = project::scratch::create_on(
                &git::Runner::Remote(host.clone()),
                &root,
                seed,
                |p, parents| host.mkdir(p, parents),
                |p| _ = host.remove(p, true),
            )?;
            Ok(host.key(&path))
        })
    }

    pub(crate) fn adopt_scratch(&mut self, path: &str, cx: &mut Context<Self>) {
        self.store.add_scratch(path);
        self.store.scratch_folded = false;
        self.store.save();
        // pocketd starts sessions only in folders it lists.
        self.publish_registry(cx);
        // Its row needs this now; git and pocketd list it two seconds from now.
        self.worktrees
            .insert(path.to_string(), vec![git::Worktree { path: path.to_string(), branch: git::INIT_BRANCH.into(), main: true, made: false, created: None }]);
        let tree = agents::worktrees::Tree { path: path.to_string(), branch: git::INIT_BRANCH.into(), is_main: true, created_at: None };
        self.agents.projects.insert(path.to_string(), vec![tree]);
    }

    /// Deletes a folder with nothing to lose straight away, and asks about any other.
    pub(super) fn ask_delete_scratch(&mut self, path: String, cx: &mut Context<Self>) {
        if let Some(host) = hosts::split(&path).0.filter(|h| hosts::remote::connected(h).is_none()) {
            self.fail(format!("{} isn't connected", self.host_name(host)), cx);
            return cx.notify();
        }
        let job = path.clone();
        let task = cx.background_executor().spawn(async move { project::scratch::work(&hosts::runner_for(&job), &job) });
        cx.spawn(async move |this, cx| {
            let work = task.await;
            this.update(cx, |d, cx| {
                if project::scratch::nothing_to_lose(work, d.tree_terminals(&path, cx).len()) {
                    return d.delete_scratch(&path, cx);
                }
                let (dirty, lost) = work.unwrap_or_default();
                d.ask(Confirm::DeleteScratch { path, dirty, lost }, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn delete_scratch(&mut self, path: &str, cx: &mut Context<Self>) {
        let trash = self.store.worktree.trash;
        self.remove_project(path, cx);
        self.workspaces.update(cx, |w, _| w.remove(path));
        self.store.forget_scratch(path);
        self.store.save();
        let folder = path.to_string();
        cx.background_executor().spawn(async move { dispose_scratch(&folder, trash) }).detach();
        cx.notify();
    }
}

/// Removes the Scratch folder `key` from the machine holding it.
/// Only one directly in that machine's Scratch root goes: desktop.json can be edited by hand, and this removes whatever path it names.
fn dispose_scratch(key: &str, trash: bool) {
    match hosts::split(key) {
        (None, path) => {
            if project::scratch::holds(&project::scratch::root(&shared()), path) {
                dispose(Path::new(path), trash);
            }
        }
        (Some(id), path) => {
            let Some(host) = hosts::remote::connected(id) else { return };
            let Ok(shared) = host.shared_dir() else { return };
            if project::scratch::holds(&project::scratch::root(&shared), path) && !(trash && host.trash(path).is_ok()) {
                _ = host.remove(path, true);
            }
        }
    }
}

/// `~/Downloads` stays whole; a deeper folder keeps only its own name, so the title beside it isn't squeezed out.
fn short_folder(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((parent, name)) if !matches!(parent, "~" | "") => format!("…/{name}"),
        _ => path.to_string(),
    }
}

fn thread_title(cards: &[Card]) -> String {
    cards.iter().max_by_key(|c| c.at).map(|c| c.title.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| "Terminal".into())
}

#[cfg(test)]
mod tests {
    use super::{short_folder, thread_title};
    use crate::status::Card;

    fn card(title: &str, at: i64) -> Card {
        Card { title: title.into(), at, ..crate::status::card(&agents::Summary::default(), "/w") }
    }

    #[test]
    fn a_thread_is_named_by_its_newest_session_else_as_a_terminal() {
        assert_eq!(thread_title(&[card("Old", 1), card("Fix login redirect", 9)]), "Fix login redirect");
        assert_eq!(thread_title(&[card("", 9)]), "Terminal");
        assert_eq!(thread_title(&[]), "Terminal");
    }

    #[test]
    fn a_thread_shows_a_folder_under_home_whole_and_a_deeper_one_by_its_name() {
        assert_eq!(short_folder("~/Downloads"), "~/Downloads");
        assert_eq!(short_folder("~/code/clients/acme/api"), "…/api");
        assert_eq!(short_folder("/tmp/x"), "…/x");
        assert_eq!(short_folder("/opt"), "/opt");
    }
}
