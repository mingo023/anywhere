use crate::desktop::Desktop;
use crate::desktop::chrome::Confirm;
use crate::status::{self, Status};
use crate::terminals::close::Busy;
use crate::util::{basename, tilde};
use git::FileStat;
use gpui_kit::*;
use theme::*;
use ui::{self, Variant};
use workspace::Doc;

fn closes(n: usize) -> Option<String> {
    match n {
        0 => None,
        1 => Some("Closes 1 terminal".into()),
        n => Some(format!("Closes {n} terminals")),
    }
}

#[derive(Debug, PartialEq)]
struct ConfirmText {
    title: String,
    action: &'static str,
    facts: Vec<String>,
    dirty: usize,
    danger: bool,
}

impl ConfirmText {
    fn remove_project(name: &str, terminals: usize) -> Self {
        let facts = closes(terminals).into_iter().chain(["Its files stay on disk".to_string()]).collect();
        Self { title: format!("Remove {name}?"), action: "Remove", facts, dirty: 0, danger: true }
    }

    fn delete_worktree(tree: &str, branch: &str, dirty: usize, terminals: usize) -> Self {
        let facts = closes(terminals).into_iter().chain([format!("Deletes the folder {}", tilde(tree)), format!("Keeps the branch {branch}")]).collect();
        Self { title: format!("Delete {}?", basename(tree)), action: "Delete", facts, dirty, danger: true }
    }

    fn discard(paths: &[String], files: &[FileStat]) -> Self {
        let untracked = files.iter().filter(|f| f.status == 'A' && !f.staged && paths.contains(&f.path)).count();
        let title = match paths {
            [one] => format!("Discard changes to {}?", basename(one)),
            many => format!("Discard changes to {} files?", many.len()),
        };
        let deletes = (untracked > 0).then(|| format!("Deletes {untracked} untracked {}", if untracked == 1 { "file" } else { "files" }));
        Self { title, action: "Discard", facts: std::iter::once("Unstaged edits can't be restored".to_string()).chain(deletes).collect(), dirty: 0, danger: true }
    }

    fn close_terminals(busy: &Busy, worktree: &str, n: usize) -> Self {
        let fact = match busy {
            Busy::Agent { title } => format!("\"{title}\" is still working in {worktree}"),
            Busy::Shell { command } => format!("\"{command}\" is still working in {worktree}"),
        };
        let (title, action) = if n == 1 { ("Close terminal?".to_string(), "Close terminal") } else { (format!("Close {n} terminals?"), "Close terminals") };
        Self { title, action, facts: vec![fact], dirty: 0, danger: true }
    }

    fn paste(text: &str, title: &str) -> Self {
        let lines = match text.lines().count() {
            0 | 1 => "1 line".to_string(),
            n => format!("{n} lines"),
        };
        Self { title: format!("Paste {lines} into {title}?"), action: "Paste", facts: Vec::new(), dirty: 0, danger: false }
    }

    fn close_session(title: &str, working: bool) -> Self {
        let facts = std::iter::once("Closes its terminal".to_string()).chain(working.then(|| "Stops its current turn".to_string())).collect();
        Self { title: format!("Close {title}?"), action: "Close", facts, dirty: 0, danger: true }
    }

    fn close_file(path: &str) -> Self {
        Self { title: format!("Save changes to {}?", basename(path)), action: "Save", facts: vec!["Your edits are lost if you don't save them".into()], dirty: 0, danger: false }
    }

    fn quit(unsaved: usize) -> Self {
        let files = if unsaved == 1 { "1 file has".to_string() } else { format!("{unsaved} files have") };
        Self { title: "Quit without saving?".into(), action: "Quit", facts: vec![format!("{files} unsaved edits")], dirty: 0, danger: true }
    }

    fn open_external(url: &str) -> Self {
        Self { title: "Open in another app?".into(), action: "Open", facts: vec![format!("The page asks to open {url}")], dirty: 0, danger: false }
    }

    fn detail(&self) -> Option<String> {
        (!self.facts.is_empty()).then(|| format!("{}.", self.facts.join(". ")))
    }

    fn warning(&self) -> Option<String> {
        let files = if self.dirty == 1 { "file" } else { "files" };
        (self.dirty > 0).then(|| format!("{} uncommitted {files} will be lost.", self.dirty))
    }
}

impl Desktop {
    pub(super) fn confirm_view(&mut self, cx: &mut Context<Self>) -> Div {
        let text = match &self.confirm {
            Some(Confirm::RemoveProject(p)) => ConfirmText::remove_project(&self.repo_name(p), self.project_terminals(p).len()),
            Some(Confirm::DeleteWorktree { tree, branch, dirty, .. }) => ConfirmText::delete_worktree(tree, branch, *dirty, self.tree_terminals(tree).len()),
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
            None => return div(),
        };
        let mut body = Vec::new();
        if let Some(detail) = text.detail() {
            body.push(div().text_size(px(13.)).text_color(TEXT_2).child(detail).into_any_element());
        }
        if let Some(warning) = text.warning() {
            body.push(div().w_full().mt(px(4.)).px(px(12.)).py(px(8.)).rounded(px(8.)).bg(FAILED_BG).text_size(px(12.5)).text_color(FAILED).child(warning).into_any_element());
        }
        let variant = if text.danger { Variant::Danger } else { Variant::Primary };
        let action = ui::button("confirm-go", variant, None, text.action).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.confirmed(window, cx)));
        let dont_save = matches!(self.confirm, Some(Confirm::CloseFile(_)))
            .then(|| ui::button("confirm-dont-save", Variant::Secondary, None, "Don't save").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_unsaved(window, cx))));
        let cancel = ui::button("confirm-cancel", Variant::Secondary, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        ui::alert(&text.title, body, std::iter::once(action).chain(dont_save).chain([cancel]))
    }

    fn close_unsaved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Confirm::CloseFile(path)) = self.confirm.take() {
            self.preview.discard(&path);
            self.close_doc(&Doc::File(path), cx);
        }
        self.close_overlay(window, cx);
    }

    fn confirmed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.confirm.take() {
            Some(Confirm::RemoveProject(p)) => self.remove_project(&p, cx),
            Some(Confirm::DeleteWorktree { project, tree, .. }) => self.delete_worktree(project, tree, cx),
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
            None => {}
        }
        self.close_overlay(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{Busy, ConfirmText, FileStat};

    fn text(title: &str, action: &'static str, facts: &[&str], dirty: usize) -> ConfirmText {
        ConfirmText { title: title.into(), action, facts: facts.iter().map(|f| f.to_string()).collect(), dirty, danger: true }
    }

    #[test]
    fn close_confirm_mentions_the_turn_only_while_working() {
        assert_eq!(ConfirmText::close_session("Fix login", false), text("Close Fix login?", "Close", &["Closes its terminal"], 0));
        assert_eq!(ConfirmText::close_session("Fix login", true), text("Close Fix login?", "Close", &["Closes its terminal", "Stops its current turn"], 0));
    }

    #[test]
    fn facts_read_as_one_sentence_each() {
        assert_eq!(ConfirmText::remove_project("app", 1).detail(), Some("Closes 1 terminal. Its files stay on disk.".into()));
        assert_eq!(ConfirmText::paste("ls", "zsh").detail(), None);
    }

    #[test]
    fn removing_a_project_keeps_its_files() {
        let keeps = "Its files stay on disk";
        assert_eq!(ConfirmText::remove_project("app", 0), text("Remove app?", "Remove", &[keeps], 0));
        assert_eq!(ConfirmText::remove_project("app", 1), text("Remove app?", "Remove", &["Closes 1 terminal", keeps], 0));
        assert_eq!(ConfirmText::remove_project("app", 3), text("Remove app?", "Remove", &["Closes 3 terminals", keeps], 0));
    }

    #[test]
    fn deleting_a_worktree_keeps_its_branch_and_warns_about_uncommitted_files() {
        let got = ConfirmText::delete_worktree("/src/feat", "feat-x", 2, 1);
        assert_eq!(got, text("Delete feat?", "Delete", &["Closes 1 terminal", "Deletes the folder /src/feat", "Keeps the branch feat-x"], 2));
        let warnings = [0, 1, 2].map(|dirty| ConfirmText::delete_worktree("/src/feat", "feat-x", dirty, 0).warning());
        assert_eq!(warnings, [None, Some("1 uncommitted file will be lost.".into()), Some("2 uncommitted files will be lost.".into())]);
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
        assert_eq!(paste("ls\n"), ConfirmText { title: "Paste 1 line into zsh?".into(), action: "Paste", facts: vec![], dirty: 0, danger: false });
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
