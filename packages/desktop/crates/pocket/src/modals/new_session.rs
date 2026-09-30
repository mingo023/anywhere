mod picker;

use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::modals::form::{default_base, home, typed_or};
use crate::terminals::Intent;
use crate::util::tilde;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use picker::Picker;
use std::collections::HashSet;
use std::path::Path;
use theme::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Perm {
    Ask,
    AutoEdit,
    Plan,
}

pub struct NewForm {
    prompt: Entity<TextareaState>,
    name: Entity<InputState>,
    draft: Draft,
}

/// The form's choices besides its text inputs.
struct Draft {
    /// Branches and worktree folders a new worktree's name must not reuse.
    taken: HashSet<String>,
    seed: usize,
    worktree: bool,
    repo: Option<String>,
    branches: Vec<(String, Option<i64>)>,
    base: usize,
    copy_env: bool,
    run_setup: bool,
    provider: &'static str,
    perm: Perm,
    picker: Option<Picker>,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            taken: HashSet::new(),
            seed: 0,
            worktree: false,
            repo: None,
            branches: Vec::new(),
            base: 0,
            copy_env: false,
            run_setup: false,
            provider: "claude",
            perm: Perm::Ask,
            picker: None,
        }
    }
}

impl Draft {
    fn base_branch(&self) -> String {
        self.branches.get(self.base).map(|(b, _)| b.clone()).unwrap_or_default()
    }

    fn auto_name(&self, prompt: &str) -> String {
        auto_name(prompt, self.seed, &self.taken)
    }

    fn argv(&self, prompt: &str) -> Vec<String> {
        let prompt = prompt.trim().to_string();
        let mut args: Vec<String> = match (self.provider, self.perm) {
            (_, Perm::Ask) => vec![],
            ("claude", Perm::AutoEdit) => vec!["--permission-mode".into(), "acceptEdits".into()],
            ("claude", Perm::Plan) => vec!["--permission-mode".into(), "plan".into()],
            (_, Perm::AutoEdit) => vec!["--full-auto".into()],
            (_, Perm::Plan) => vec!["-s".into(), "read-only".into()],
        };
        args.extend((!prompt.is_empty()).then_some(prompt));
        std::iter::once(self.provider.to_string()).chain(args).collect()
    }

    /// `in_tree`: a worktree is open to start the session in.
    fn ready(&self, name: &str, in_tree: bool) -> bool {
        if self.worktree {
            self.repo.is_some() && !self.branches.is_empty() && name_problem(name, &self.taken).is_none()
        } else {
            in_tree
        }
    }
}

fn slug(prompt: &str) -> String {
    prompt.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).take(4).map(str::to_lowercase).collect::<Vec<_>>().join("-")
}

const ADJECTIVES: [&str; 8] = ["brave", "calm", "eager", "fuzzy", "keen", "lucky", "quiet", "swift"];
const NOUNS: [&str; 8] = ["otter", "heron", "maple", "comet", "falcon", "cedar", "koala", "lynx"];

/// Case-insensitive because APFS is; a branch `a/b` owns the name `a`.
fn is_taken(name: &str, taken: &HashSet<String>) -> bool {
    taken.iter().any(|t| t.split('/').next().unwrap_or(t).eq_ignore_ascii_case(name))
}

fn free_name(seed: usize, taken: &HashSet<String>) -> String {
    let count = ADJECTIVES.len() * NOUNS.len();
    (0..count).map(|i| (seed + i) % count).map(|n| format!("{}-{}", ADJECTIVES[n / NOUNS.len()], NOUNS[n % NOUNS.len()])).find(|n| !is_taken(n, taken)).unwrap_or_else(|| unique("worktree", taken))
}

/// `base`, or `base-2`, `base-3`… when taken.
fn unique(base: &str, taken: &HashSet<String>) -> String {
    std::iter::once(base.to_string()).chain((2..).map(|n| format!("{base}-{n}"))).find(|n| !is_taken(n, taken)).unwrap()
}

/// The name a worktree gets when the user leaves Name empty: the prompt's slug, else a random one.
fn auto_name(prompt: &str, seed: usize, taken: &HashSet<String>) -> String {
    match slug(prompt) {
        s if s.is_empty() => free_name(seed, taken),
        s => unique(&s, taken),
    }
}

/// Why `name` can't name a new worktree and its branch, if it can't.
fn name_problem(name: &str, taken: &HashSet<String>) -> Option<&'static str> {
    let valid = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        && !name.starts_with(['-', '.'])
        && !name.ends_with('.')
        && !name.ends_with(".lock")
        && !name.contains("..")
        && name != "HEAD";
    if is_taken(name, taken) {
        Some("A worktree or branch with this name already exists")
    } else if !valid {
        Some("Use letters, digits, - _ or .")
    } else {
        None
    }
}

fn default_first(branches: &mut [(String, Option<i64>)], preferred: &str, current: &str) {
    let default = default_base(branches.iter().map(|(b, _)| b.as_str()), preferred, current);
    if !branches.is_empty() {
        branches[..=default].rotate_right(1);
    }
}

impl NewForm {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let prompt = cx.new(|cx| TextareaState::new(window, cx).placeholder("Describe what the agent should do…").rows(4));
        let name = cx.new(|cx| InputState::new(window, cx));
        let subs = vec![
            cx.subscribe_in(&prompt, window, |this, prompt, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { secondary: true, .. } => this.start_session(window, cx),
                InputEvent::Change => {
                    let name = this.new_form.draft.auto_name(&prompt.read(cx).value());
                    this.new_form.name.update(cx, |b, cx| b.set_placeholder(name, window, cx));
                    cx.notify();
                }
                _ => {}
            }),
            cx.subscribe_in(&name, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { secondary: true, .. } => this.start_session(window, cx),
                _ => cx.notify(),
            }),
        ];
        (Self { prompt, name, draft: Draft::default() }, subs)
    }
}

impl Desktop {
    fn worktrees_dir(&self, repo: &str) -> String {
        let custom = self.store.repos.get(repo).map(|r| r.worktrees.clone()).filter(|w| !w.is_empty());
        custom.unwrap_or_else(|| format!("{}/.worktrees/{}", home(), self.repo_name(repo)))
    }

    /// Files copied into a new worktree: the repository's list, else its root `.env*` files.
    fn copy_list(&self, repo: &str) -> Vec<String> {
        let listed = self.store.repos.get(repo).map(|r| r.copy.clone()).unwrap_or_default();
        if !listed.is_empty() {
            return listed;
        }
        let entries = std::fs::read_dir(repo).into_iter().flatten().flatten();
        entries.filter_map(|e| e.file_name().into_string().ok()).filter(|n| n.starts_with(".env")).collect()
    }

    pub fn reset_new_form(&mut self, prompt: Option<String>, worktree: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = prompt.unwrap_or_default();
        let f = &mut self.new_form;
        f.draft.seed = crate::util::now_ms() as usize;
        f.draft.taken.clear();
        let placeholder = f.draft.auto_name(&text);
        f.prompt.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.focus(window, cx);
        });
        f.name.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.set_placeholder(placeholder, window, cx);
        });
        f.draft.worktree = worktree;
        f.draft.perm = Perm::Ask;
        f.draft.picker = None;
        match self.project.clone() {
            Some(repo) => self.pick_repo(repo, window, cx),
            None => {
                f.draft.repo = None;
                f.draft.branches.clear();
            }
        }
    }

    fn pick_repo(&mut self, repo: String, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = self.store.repos.get(&repo).cloned().unwrap_or_default();
        let folders = self.worktrees_dir(&repo);
        let f = &mut self.new_form.draft;
        f.repo = Some(repo.clone());
        f.branches.clear();
        f.base = 0;
        f.copy_env = !cfg.copy.is_empty();
        f.run_setup = !cfg.setup.is_empty();
        let dir = repo.clone();
        let task = cx.background_executor().spawn(async move {
            let current = git::read(&dir).map(|r| r.branch).unwrap_or_default();
            let all = git::branches(&dir);
            let branches: Vec<(String, Option<i64>)> = all.iter().take(20).map(|b| {
                let at = git::committed_at(&dir, b);
                (b.clone(), at)
            }).collect();
            let entries = std::fs::read_dir(&folders).into_iter().flatten().flatten();
            let taken: HashSet<String> = all.into_iter().chain(entries.filter_map(|e| e.file_name().into_string().ok())).collect();
            (current, branches, taken)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (current, mut branches, taken) = task.await;
            this.update_in(cx, |d, window, cx| {
                let f = &mut d.new_form;
                if f.draft.repo.as_ref() != Some(&repo) {
                    return;
                }
                default_first(&mut branches, &cfg.base, &current);
                f.draft.base = 0;
                f.draft.branches = branches;
                f.draft.taken = taken;
                let name = f.draft.auto_name(&f.prompt.read(cx).value());
                f.name.update(cx, |s, cx| s.set_placeholder(name, window, cx));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn new_name(&self, cx: &App) -> String {
        let f = &self.new_form;
        typed_or(&f.name, || f.draft.auto_name(&f.prompt.read(cx).value()), cx)
    }

    fn session_ready(&self, cx: &App) -> bool {
        self.new_form.draft.ready(&self.new_name(cx), self.cwd().is_some())
    }

    fn start_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session_ready(cx) {
            return;
        }
        let f = &self.new_form.draft;
        let argv = f.argv(&self.new_form.prompt.read(cx).value());
        let worktree = f.worktree;
        match self.cwd().filter(|_| !worktree) {
            Some(tree) => self.send_spawn(daemon::agent_op(&argv, &tree), Intent::Tab(tree), cx),
            None => {
                let f = &self.new_form.draft;
                let Some(repo) = f.repo.clone() else { return };
                let name = self.new_name(cx);
                let path = format!("{}/{name}", self.worktrees_dir(&repo));
                let base = f.base_branch();
                let copy = if f.copy_env { self.copy_list(&repo) } else { Vec::new() };
                let setup = self.store.repos.get(&repo).map(|r| r.setup.clone()).filter(|_| f.run_setup).unwrap_or_default();
                let project = repo.clone();
                let task = cx.background_executor().spawn(async move {
                    git::add_worktree(&repo, &path, &name, &base)?;
                    for rel in copy {
                        let to = Path::new(&path).join(&rel);
                        if let Some(dir) = to.parent() {
                            std::fs::create_dir_all(dir).ok();
                        }
                        std::fs::copy(Path::new(&repo).join(&rel), to).ok();
                    }
                    Ok::<_, String>(path)
                });
                cx.spawn(async move |this, cx| {
                    let res = task.await;
                    this.update(cx, |d, cx| {
                        if res.is_ok() && d.store.collapsed.remove(&project) {
                            d.store.save();
                        }
                        match res {
                            Ok(path) if setup.is_empty() => d.send_spawn(daemon::agent_op(&argv, &path), Intent::Tab(path), cx),
                            Ok(path) => d.send_spawn(daemon::setup_op(&setup, &argv, &path), Intent::Setup(path), cx),
                            Err(e) => d.error = Some(e),
                        }
                        d.refresh_git(cx);
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
        }
        self.close_overlay(window, cx);
    }

    pub fn new_worktree(&mut self, _: &crate::actions::NewWorktree, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.overlay = Some(Overlay::NewSession);
        self.reset_new_form(None, true, window, cx);
        cx.notify();
    }

    pub fn new_worktree_in(&mut self, p: String, window: &mut Window, cx: &mut Context<Self>) {
        self.select_project(p, cx);
        self.new_worktree(&crate::actions::NewWorktree, window, cx);
    }

    pub fn new_session_view(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form.draft;
        let repo = f.repo.clone().unwrap_or_default();
        let name = self.repo_name(&repo);
        let close = ui::icon_button_sized("form-close", "x", 28., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let header = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(4.))
            .pb(px(2.))
            .child(div().text_size(px(16.)).font_weight(FontWeight::BOLD).child(if f.worktree { "New worktree" } else { "New session" }))
            .child(div().flex().items_center().gap(px(6.)).text_size(px(13.)).text_color(rgba(TEXT_3)).child(ui::repo_tile(&crate::util::initials(&name), 18., false, None)).child(name))
            .child(div().ml_auto().child(close));
        let agent = self.agent_select(cx);
        let branch = f.worktree.then(|| self.branch_select(cx));
        let ready = self.session_ready(cx);
        let send = ui::primary(div().id("form-start").ml_auto().size(px(32.)).flex().flex_none().items_center().justify_center().rounded(px(16.)).cursor_pointer())
            .child(icon("arrow-up", 16., WHITE))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start_session(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let problem = if f.worktree { name_problem(&self.new_name(cx), &f.taken) } else { None };
        let name_field = f.worktree.then(|| {
            ui::field_box()
                .child(icon("worktree", 14., TEXT_3))
                .child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(&self.new_form.name).appearance(false).p_0().text_size(px(13.))))
                .children(problem.map(|p| div().flex_none().text_size(px(12.)).text_color(rgba(FAILED)).child(p)))
        });
        let composer = div()
            .flex()
            .flex_col()
            .rounded(px(14.))
            .bg(rgba(WHITE))
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5), ui::shadow(0x1111130a, 1., 2.)])
            // The textarea pads itself 8px × 10px and wraps 10px short of its edge; the frame restores the design's 14/16/4 and its line breaks.
            .child(div().pt(px(6.)).pl(px(6.)).mr(px(-6.)).child(Textarea::new(&self.new_form.prompt).appearance(false).h(px(105.)).text_size(px(15.)).line_height(px(23.25))))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .pt(px(6.))
                    .px(px(10.))
                    .pb(px(10.))
                    .text_size(px(13.))
                    .child(agent)
                    .children(branch)
                    .child(send),
            );
        let mono = |s: String| div().font_family(MONO).text_color(rgba(TEXT_2)).child(s);
        let summary: Vec<AnyElement> = if f.worktree {
            vec![
                div().child("New branch").into_any_element(),
                mono(self.new_name(cx)).into_any_element(),
                div().child("from").into_any_element(),
                mono(f.base_branch()).into_any_element(),
            ]
        } else {
            let place = self.repo().map(|r| r.branch.clone()).or_else(|| self.cwd().map(|c| tilde(&c))).unwrap_or_default();
            vec![div().child("In").into_any_element(), mono(place).into_any_element()]
        };
        let footer = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(6.))
            .text_size(px(12.))
            .text_color(rgba(TEXT_3))
            .whitespace_nowrap()
            .children(summary)
            .child(div().ml_auto().text_color(rgba(TEXT_4)).child("⌘↵ to start · esc to cancel"));
        div().absolute().top(px(110.)).left_0().right_0().flex().justify_center().child(
            // The design's 0.5px border renders 1px wide and insets the sheet's content.
            ui::pop(div().w(px(640.)).pt(px(17.)).px(px(17.)).pb(px(15.)).flex().flex_col().gap(px(10.))).occlude().child(header).children(name_field).child(composer).child(footer),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Draft, Perm, auto_name, default_first, name_problem, slug};
    use std::collections::HashSet;

    #[test]
    fn a_session_in_the_open_tree_needs_only_an_open_tree() {
        let draft = Draft::default();
        assert_eq!((draft.ready("", true), draft.ready("fix", false)), (true, false));
    }

    #[test]
    fn a_new_worktree_needs_a_repository_its_branches_and_a_usable_name() {
        let taken: HashSet<String> = ["main".to_string()].into();
        let draft = Draft { worktree: true, repo: Some("/src/app".into()), branches: vec![("main".into(), None)], taken, ..Draft::default() };
        assert!(draft.ready("fix-ci", false));
        assert!(!draft.ready("main", true));
        assert!(!draft.ready("fix ci", true));
        assert!(!Draft { repo: None, ..draft }.ready("fix-ci", true));
        let draft = Draft { worktree: true, repo: Some("/src/app".into()), ..Draft::default() };
        assert!(!draft.ready("fix-ci", true));
    }

    #[test]
    fn the_agent_runs_with_its_permission_flags_then_the_prompt() {
        let argv = |provider, perm| Draft { provider, perm, ..Draft::default() }.argv("  Fix CI  ").join(" ");
        let got = [("claude", Perm::Ask), ("claude", Perm::AutoEdit), ("claude", Perm::Plan), ("codex", Perm::Ask), ("codex", Perm::AutoEdit), ("codex", Perm::Plan)].map(|(p, m)| argv(p, m));
        let want = [
            "claude Fix CI",
            "claude --permission-mode acceptEdits Fix CI",
            "claude --permission-mode plan Fix CI",
            "codex Fix CI",
            "codex --full-auto Fix CI",
            "codex -s read-only Fix CI",
        ];
        assert_eq!(got, want.map(String::from));
        assert_eq!(Draft::default().argv(" \n "), vec!["claude".to_string()]);
    }

    #[test]
    fn the_default_base_leads_the_recent_branches() {
        let mut branches: Vec<(String, Option<i64>)> = ["feat", "fix", "main", "old"].map(|b| (b.to_string(), None)).to_vec();
        default_first(&mut branches, "", "fix");
        assert_eq!(branches.iter().map(|(b, _)| b.as_str()).collect::<Vec<_>>(), ["main", "feat", "fix", "old"]);
        let mut none: Vec<(String, Option<i64>)> = Vec::new();
        default_first(&mut none, "main", "main");
        assert_eq!(none, []);
    }

    #[test]
    fn slug_names_a_worktree_after_the_prompt() {
        assert_eq!(slug("The RestoreView snapshot fails on CI"), "the-restoreview-snapshot-fails");
        assert_eq!(slug("fix: flaky!"), "fix-flaky");
        assert_eq!(slug("  …  "), "");
    }

    #[test]
    fn an_empty_name_falls_back_to_a_free_one() {
        let taken: HashSet<String> = ["fix-flaky", "brave-otter", "Brave-Heron", "Fix-Login", "fix-login-2", "release/1.0"].map(String::from).into();
        assert_eq!(auto_name("fix: flaky!", 0, &taken), "fix-flaky-2");
        assert_eq!(auto_name("Fix login", 0, &taken), "fix-login-3");
        assert_eq!(auto_name("Release", 0, &taken), "release-2");
        assert_eq!(auto_name("Add dark mode", 0, &taken), "add-dark-mode");
        assert_eq!(auto_name("", 0, &taken), "brave-maple");
        assert_eq!(auto_name("", 63, &HashSet::new()), "swift-lynx");
        assert_eq!(auto_name("", 64, &HashSet::new()), "brave-otter");
    }

    #[test]
    fn a_name_must_be_free_and_valid_for_git() {
        let taken: HashSet<String> = ["main", "Foo", "fix/login"].map(String::from).into();
        assert_eq!(name_problem("fix-login_2.0", &taken), None);
        for used in ["main", "foo", "fix"] {
            assert_eq!(name_problem(used, &taken), Some("A worktree or branch with this name already exists"), "{used}");
        }
        for bad in ["", "fix login", "fix/login", "-x", ".x", "x.", "a..b", "x.lock", "tên", "HEAD"] {
            assert_eq!(name_problem(bad, &taken), Some("Use letters, digits, - _ or ."), "{bad}");
        }
    }
}
