use crate::desktop::Desktop;
use crate::desktop::chrome::Confirm;
use crate::removal::Removal;
use crate::status::{self, Status};
use crate::terminals::close::Busy;
use crate::util::{basename, tilde};
use git::FileStat;
use gpui_kit::*;
use theme::*;
use ui::{self, Variant};
use workspace::Doc;

fn counted(n: usize, one: &str, many: impl FnOnce(usize) -> String) -> Option<String> {
    (n > 0).then(|| if n == 1 { one.to_string() } else { many(n) })
}

fn closes(n: usize) -> Option<String> {
    counted(n, "Closes 1 terminal", |n| format!("Closes {n} terminals"))
}

fn deletes(locals: usize) -> Option<String> {
    counted(locals, "Deletes its added Local", |n| format!("Deletes its {n} added Locals"))
}

#[derive(Debug, PartialEq)]
struct ConfirmText {
    title: String,
    action: &'static str,
    facts: Vec<String>,
    dirty: usize,
    lost: usize,
    danger: bool,
}

impl ConfirmText {
    fn remove_project(name: &str, terminals: usize, locals: usize) -> Self {
        let facts = closes(terminals).into_iter().chain(deletes(locals)).chain(["Its files stay on disk".to_string()]).collect();
        Self { title: format!("Remove {name}?"), action: "Remove", facts, dirty: 0, lost: 0, danger: true }
    }

    /// `lost` counts the commits only the tree's branch, or its detached HEAD, holds; a kept branch keeps them.
    fn delete_worktree(r: &Removal, dirty: usize, lost: usize, terminals: usize) -> Self {
        let facts = closes(terminals).into_iter().chain([format!("Deletes the folder {}", tilde(&r.tree))]).collect();
        let lost = if r.branch.is_some() && !r.delete_branch { 0 } else { lost };
        Self { title: format!("Delete {}?", basename(&r.tree)), action: "Delete", facts, dirty, lost, danger: true }
    }

    fn delete_local(name: &str, terminals: usize) -> Self {
        let facts = closes(terminals).into_iter().chain(["The project's files, branches and other Locals stay as they are".to_string()]).collect();
        Self { title: format!("Delete {name}?"), action: "Delete", facts, dirty: 0, lost: 0, danger: true }
    }

    fn teardown_failed(tree: &str) -> Self {
        let facts = vec!["Its teardown script failed".into(), "Deleting anyway skips it".into()];
        Self { title: format!("Couldn't tear down {}", basename(tree)), action: "Delete anyway", facts, dirty: 0, lost: 0, danger: true }
    }

    fn discard(paths: &[String], files: &[FileStat]) -> Self {
        let untracked = files.iter().filter(|f| f.status == 'A' && !f.staged && paths.contains(&f.path)).count();
        let title = match paths {
            [one] => format!("Discard changes to {}?", basename(one)),
            many => format!("Discard changes to {} files?", many.len()),
        };
        let deletes = (untracked > 0).then(|| format!("Deletes {untracked} untracked {}", if untracked == 1 { "file" } else { "files" }));
        Self { title, action: "Discard", facts: std::iter::once("Unstaged edits can't be restored".to_string()).chain(deletes).collect(), dirty: 0, lost: 0, danger: true }
    }

    fn close_terminals(busy: &Busy, worktree: &str, n: usize) -> Self {
        let fact = match busy {
            Busy::Agent { title } => format!("\"{title}\" is still working in {worktree}"),
            Busy::Shell { command } => format!("\"{command}\" is still working in {worktree}"),
        };
        let (title, action) = if n == 1 { ("Close terminal?".to_string(), "Close terminal") } else { (format!("Close {n} terminals?"), "Close terminals") };
        Self { title, action, facts: vec![fact], dirty: 0, lost: 0, danger: true }
    }

    fn paste(text: &str, title: &str) -> Self {
        let lines = match text.lines().count() {
            0 | 1 => "1 line".to_string(),
            n => format!("{n} lines"),
        };
        Self { title: format!("Paste {lines} into {title}?"), action: "Paste", facts: Vec::new(), dirty: 0, lost: 0, danger: false }
    }

    fn close_session(title: &str, working: bool) -> Self {
        let facts = std::iter::once("Closes its terminal".to_string()).chain(working.then(|| "Stops its current turn".to_string())).collect();
        Self { title: format!("Close {title}?"), action: "Close", facts, dirty: 0, lost: 0, danger: true }
    }

    fn close_file(path: &str) -> Self {
        Self { title: format!("Save changes to {}?", basename(path)), action: "Save", facts: vec!["Your edits are lost if you don't save them".into()], dirty: 0, lost: 0, danger: false }
    }

    fn quit(unsaved: usize) -> Self {
        let files = if unsaved == 1 { "1 file has".to_string() } else { format!("{unsaved} files have") };
        Self { title: "Quit without saving?".into(), action: "Quit", facts: vec![format!("{files} unsaved edits")], dirty: 0, lost: 0, danger: true }
    }

    fn open_external(url: &str) -> Self {
        Self { title: "Open in another app?".into(), action: "Open", facts: vec![format!("The page asks to open {url}")], dirty: 0, lost: 0, danger: false }
    }

    fn delete_automation(name: &str) -> Self {
        Self { title: format!("Delete {name}?"), action: "Delete", facts: vec!["Its run history goes too".into()], dirty: 0, lost: 0, danger: true }
    }

    fn revoke_device(name: &str) -> Self {
        Self { title: format!("Revoke {name}?"), action: "Revoke", facts: vec!["It disconnects now and has to pair again".into()], dirty: 0, lost: 0, danger: true }
    }

    fn detail(&self) -> Option<String> {
        (!self.facts.is_empty()).then(|| format!("{}.", self.facts.join(". ")))
    }

    fn warning(&self) -> Option<String> {
        let count = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let files = (self.dirty > 0).then(|| count(self.dirty, "uncommitted file", "uncommitted files"));
        let commits = (self.lost > 0).then(|| count(self.lost, "commit", "commits") + " on no other branch");
        let lost: Vec<String> = files.into_iter().chain(commits).collect();
        (!lost.is_empty()).then(|| format!("{} will be lost.", lost.join(" and ")))
    }
}

impl Desktop {
    pub(super) fn confirm_view(&mut self, cx: &mut Context<Self>) -> Div {
        let text = match &self.confirm {
            Some(Confirm::ResetSection(section)) => return self.reset_sheet(*section, cx),
            Some(Confirm::RemoveProject(p)) => ConfirmText::remove_project(&self.repo_name(p), self.project_terminals(p).len(), self.agents.locals_removed_with(p).count()),
            Some(Confirm::DeleteWorktree { removal, dirty, lost }) => ConfirmText::delete_worktree(removal, *dirty, *lost, self.tree_terminals(&removal.tree).len()),
            Some(Confirm::DeleteLocal(id)) => ConfirmText::delete_local(&self.local_name(id), self.tree_terminals(id).len()),
            Some(Confirm::TeardownFailed { removal, .. }) => ConfirmText::teardown_failed(&removal.tree),
            Some(Confirm::Discard(paths)) => ConfirmText::discard(paths, self.repo().map_or(&[], |r| r.files.as_slice())),
            Some(Confirm::CloseSession(id)) => {
                let Some(a) = self.agents.get(id) else { return div() };
                let card = status::card(a, &a.cwd);
                ConfirmText::close_session(&card.title, card.status == Status::Working)
            }
            Some(Confirm::Paste { pane, text }) => ConfirmText::paste(text, &self.pane_label(pane)),
            Some(Confirm::CloseTerminals { ids, busy, worktree }) => ConfirmText::close_terminals(busy, worktree, ids.len()),
            Some(Confirm::CloseFile(path)) => ConfirmText::close_file(path),
            Some(Confirm::Quit(n)) => ConfirmText::quit(*n),
            Some(Confirm::OpenExternal(url)) => ConfirmText::open_external(url),
            Some(Confirm::DeleteAutomation(id)) => ConfirmText::delete_automation(self.agents.automations.items.iter().find(|a| &a.id == id).map_or("this automation", |a| a.name.as_str())),
            Some(Confirm::RevokeDevice { name, .. }) => ConfirmText::revoke_device(name),
            None => return div(),
        };
        let mut body = Vec::new();
        if let Some(detail) = text.detail() {
            body.push(div().text_size(px(13.)).text_color(TEXT_2).child(detail).into_any_element());
        }
        if let Some(Confirm::DeleteWorktree { removal: Removal { branch: Some(branch), delete_branch, .. }, .. }) = &self.confirm {
            body.push(
                div()
                    .id("confirm-delete-branch")
                    .mt(px(4.))
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .text_color(TEXT)
                    .child(ui::checkbox(*delete_branch).mt(px(2.)))
                    .child(div().min_w_0().child(format!("Also delete branch {branch}")))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        if let Some(Confirm::DeleteWorktree { removal, .. }) = &mut this.confirm {
                            removal.delete_branch = !removal.delete_branch;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        let worktree = matches!(self.confirm, Some(Confirm::DeleteWorktree { .. }));
        if let Some(warning) = text.warning().filter(|_| worktree) {
            body.push(div().text_size(px(12.5)).text_color(FAILED_TEXT).child(warning).into_any_element());
        } else if let Some(warning) = text.warning() {
            body.push(div().w_full().mt(px(4.)).px(px(12.)).py(px(8.)).rounded(px(8.)).bg(FAILED_BG).text_size(px(12.5)).text_color(FAILED).child(warning).into_any_element());
        }
        if let Some(Confirm::TeardownFailed { tail, .. }) = &self.confirm {
            body.push(div().w_full().px(px(10.)).py(px(8.)).rounded(px(8.)).bg(FILL_2).text_left().font_family(MONO).text_size(px(11.5)).text_color(TEXT_2).child(tail.clone()).into_any_element());
        }
        let variant = if text.danger { Variant::Danger } else { Variant::Primary };
        let action = ui::button("confirm-go", variant, None, text.action).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.confirmed(window, cx)));
        let dont_save = matches!(self.confirm, Some(Confirm::CloseFile(_)))
            .then(|| ui::button("confirm-dont-save", Variant::Secondary, None, "Don't save").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_unsaved(window, cx))));
        let cancel = ui::button("confirm-cancel", Variant::Secondary, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        if worktree {
            return ui::dialog(&text.title, body, [cancel, action]);
        }
        ui::alert(&text.title, body, std::iter::once(action).chain(dont_save).chain([cancel]))
    }

    fn close_unsaved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Confirm::CloseFile(path)) = self.confirm.take() {
            self.preview.discard(&path);
            self.close_doc(&Doc::File(path), cx);
        }
        self.close_overlay(window, cx);
    }

    pub(crate) fn confirmed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.confirm.take() {
            Some(Confirm::RemoveProject(p)) => self.remove_project(&p, cx),
            Some(Confirm::DeleteWorktree { removal, .. }) => self.delete_worktree(removal, cx),
            Some(Confirm::DeleteLocal(id)) => self.delete_local(&id, cx),
            Some(Confirm::TeardownFailed { removal, .. }) => self.delete_worktree(Removal { teardown: false, ..removal }, cx),
            Some(Confirm::Discard(paths)) => self.discard(paths, cx),
            Some(Confirm::CloseSession(id)) => {
                if let Some(term) = self.agents.get(&id).map(|a| a.terminal_id.clone()) {
                    self.close_pane(&term, cx);
                }
            }
            Some(Confirm::Paste { pane, text }) => self.paste_into(&pane, &text, cx),
            Some(Confirm::CloseTerminals { ids, .. }) => ids.iter().for_each(|id| self.close_pane(id, cx)),
            Some(Confirm::CloseFile(path)) => self.save_file(path, true, cx),
            Some(Confirm::Quit(_)) => cx.quit(),
            Some(Confirm::OpenExternal(url)) => cx.open_url(&url),
            Some(Confirm::DeleteAutomation(id)) => self.delete_automation(&id),
            Some(Confirm::ResetSection(section)) => self.reset_section(section, window, cx),
            Some(Confirm::RevokeDevice { id, .. }) => self.revoke_device(id, cx),
            None => {}
        }
        self.close_overlay(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{Busy, ConfirmText, FileStat};
    use crate::removal::Removal;

    fn text(title: &str, action: &'static str, facts: &[&str], dirty: usize) -> ConfirmText {
        ConfirmText { title: title.into(), action, facts: facts.iter().map(|f| f.to_string()).collect(), dirty, lost: 0, danger: true }
    }

    #[test]
    fn close_confirm_mentions_the_turn_only_while_working() {
        assert_eq!(ConfirmText::close_session("Fix login", false), text("Close Fix login?", "Close", &["Closes its terminal"], 0));
        assert_eq!(ConfirmText::close_session("Fix login", true), text("Close Fix login?", "Close", &["Closes its terminal", "Stops its current turn"], 0));
    }

    #[test]
    fn deleting_an_automation_warns_that_its_history_goes() {
        assert_eq!(ConfirmText::delete_automation("Nightly"), text("Delete Nightly?", "Delete", &["Its run history goes too"], 0));
    }

    #[test]
    fn facts_read_as_one_sentence_each() {
        assert_eq!(ConfirmText::remove_project("app", 1, 0).detail(), Some("Closes 1 terminal. Its files stay on disk.".into()));
        assert_eq!(ConfirmText::paste("ls", "zsh").detail(), None);
    }

    #[test]
    fn removing_a_project_keeps_its_files() {
        let keeps = "Its files stay on disk";
        assert_eq!(ConfirmText::remove_project("app", 0, 0), text("Remove app?", "Remove", &[keeps], 0));
        assert_eq!(ConfirmText::remove_project("app", 1, 0), text("Remove app?", "Remove", &["Closes 1 terminal", keeps], 0));
        assert_eq!(ConfirmText::remove_project("app", 3, 0), text("Remove app?", "Remove", &["Closes 3 terminals", keeps], 0));
        assert_eq!(ConfirmText::remove_project("app", 0, 1), text("Remove app?", "Remove", &["Deletes its added Local", keeps], 0));
        assert_eq!(ConfirmText::remove_project("app", 0, 2), text("Remove app?", "Remove", &["Deletes its 2 added Locals", keeps], 0));
    }

    #[test]
    fn deleting_a_local_closes_its_terminals_and_keeps_the_files() {
        let keeps = "The project's files, branches and other Locals stay as they are";
        assert_eq!(ConfirmText::delete_local("Local 2", 0), text("Delete Local 2?", "Delete", &[keeps], 0));
        assert_eq!(ConfirmText::delete_local("Local 2", 2), text("Delete Local 2?", "Delete", &["Closes 2 terminals", keeps], 0));
    }

    fn removal(branch: Option<&str>, delete_branch: bool) -> Removal {
        Removal { project: "/src/app".into(), tree: "/src/feat".into(), branch: branch.map(String::from), delete_branch, teardown: true }
    }

    #[test]
    fn deleting_a_worktree_names_its_folder_and_warns_about_uncommitted_files() {
        let got = ConfirmText::delete_worktree(&removal(Some("feat-x"), true), 2, 0, 1);
        assert_eq!(got, text("Delete feat?", "Delete", &["Closes 1 terminal", "Deletes the folder /src/feat"], 2));
        let warnings = [0, 1, 2].map(|dirty| ConfirmText::delete_worktree(&removal(Some("feat-x"), true), dirty, 0, 0).warning());
        assert_eq!(warnings, [None, Some("1 uncommitted file will be lost.".into()), Some("2 uncommitted files will be lost.".into())]);
    }

    #[test]
    fn commits_only_the_branch_holds_are_lost_only_while_it_goes_too() {
        let warning = |delete_branch| ConfirmText::delete_worktree(&removal(Some("feat-x"), delete_branch), 1, 3, 0).warning();
        assert_eq!(warning(true), Some("1 uncommitted file and 3 commits on no other branch will be lost.".into()));
        assert_eq!(warning(false), Some("1 uncommitted file will be lost.".into()));
    }

    #[test]
    fn a_detached_worktree_loses_its_commits() {
        let got = ConfirmText::delete_worktree(&removal(None, false), 0, 2, 0);
        assert_eq!((got.warning(), got.facts), (Some("2 commits on no other branch will be lost.".into()), vec!["Deletes the folder /src/feat".to_string()]));
    }

    #[test]
    fn a_failed_teardown_offers_to_delete_anyway() {
        let got = ConfirmText::teardown_failed("/src/feat");
        assert_eq!(got, text("Couldn't tear down feat", "Delete anyway", &["Its teardown script failed", "Deleting anyway skips it"], 0));
    }

    fn file(path: &str, status: char, staged: bool) -> FileStat {
        FileStat { path: path.into(), added: 1, removed: 0, staged, unstaged: !staged, status }
    }

    #[test]
    fn discarding_counts_the_unstaged_new_files_it_deletes() {
        let files = [file("src/new.rs", 'A', false), file("src/old.rs", 'M', false), file("src/staged.rs", 'A', true), file("other.rs", 'A', false)];
        let paths = ["src/new.rs", "src/old.rs", "src/staged.rs"].map(String::from);
        let lost = "Unstaged edits can't be restored";
        assert_eq!(ConfirmText::discard(&paths, &files), text("Discard changes to 3 files?", "Discard", &[lost, "Deletes 1 untracked file"], 0));
        assert_eq!(ConfirmText::discard(&paths[1..2], &files), text("Discard changes to old.rs?", "Discard", &[lost], 0));
        let new = [file("a", 'A', false), file("b", 'A', false)];
        assert_eq!(ConfirmText::discard(&["a".into(), "b".into()], &new).facts[1], "Deletes 2 untracked files");
    }

    #[test]
    fn paste_confirm_counts_lines() {
        let paste = |t: &str| ConfirmText::paste(t, "zsh");
        assert_eq!(paste("ls\nrm -rf ~\n").title, "Paste 2 lines into zsh?");
        assert_eq!(paste("a\x1b[201~b").title, "Paste 1 line into zsh?");
        assert_eq!(paste("ls\n"), ConfirmText { title: "Paste 1 line into zsh?".into(), action: "Paste", facts: vec![], dirty: 0, lost: 0, danger: false });
    }

    #[test]
    fn closing_an_edited_file_offers_to_save_it() {
        let got = ConfirmText::close_file("/r/src/app.tsx");
        assert_eq!((got.title.as_str(), got.action, got.danger), ("Save changes to app.tsx?", "Save", false));
    }

    #[test]
    fn quitting_with_unsaved_edits_counts_the_files() {
        assert_eq!(ConfirmText::quit(1).detail(), Some("1 file has unsaved edits.".into()));
        assert_eq!(ConfirmText::quit(3).detail(), Some("3 files have unsaved edits.".into()));
    }

    #[test]
    fn opening_another_app_shows_the_link_and_is_not_destructive() {
        let got = ConfirmText::open_external("zoommtg://zoom.us/join");
        assert_eq!((got.detail(), got.danger), (Some("The page asks to open zoommtg://zoom.us/join.".into()), false));
    }

    #[test]
    fn close_confirm_names_the_worktree() {
        let agent = Busy::Agent { title: "Fix login".into() };
        let got = ConfirmText::close_terminals(&agent, "feat-x", 1);
        assert_eq!(got, text("Close terminal?", "Close terminal", &["\"Fix login\" is still working in feat-x"], 0));
        let shell = Busy::Shell { command: "npm test".into() };
        let got = ConfirmText::close_terminals(&shell, "app", 3);
        assert_eq!(got, text("Close 3 terminals?", "Close terminals", &["\"npm test\" is still working in app"], 0));
    }
}
