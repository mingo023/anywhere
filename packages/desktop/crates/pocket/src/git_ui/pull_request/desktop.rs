use super::{NoPr, PrEvent, PrFrame, PrSide, PrTab, Undo, conflict_prompt, fix_prompt, from_commits};
use crate::chat::Route;
use crate::desktop::Desktop;
use crate::desktop::chrome::Side;
use crate::git_ui::changes::push;
use crate::git_ui::diff::At;
use crate::git_ui::gh_accounts::Gh;
use crate::git_ui::pull_requests::PrItem;
use crate::removal::Removal;
use crate::util::now_ms;
use git::github::{self, Check, Note, Pr, PrState, Thread};
use gpui_kit::*;
use std::time::Instant;
use theme::{Token, provider_name};
use workspace::Doc;
use workspace::tree::PaneId;

impl Desktop {
    /// Opens the panel on the Pull request side.
    pub(crate) fn show_pr_side(&mut self, cx: &mut Context<Self>) {
        self.side = Side::PullRequest;
        self.sidebar.update(cx, |s, cx| s.set_panel_open(true, cx));
        cx.notify();
    }

    fn pr_frame(&self, cx: &App) -> PrFrame {
        let tree = self.cwd(cx);
        let branch = self.repo(cx).map(|r| super::Branch { name: r.branch.clone(), base: r.base.clone(), ahead: r.ahead });
        let pr = tree.as_deref().and_then(|t| self.prs.handle(t));
        let offer = tree.as_deref().zip(branch.as_ref()).and_then(|(t, b)| match self.prs.item(t, &b.name, b.base.as_deref()) {
            Some(PrItem::Create) => Some(NoPr::Create),
            Some(PrItem::Hint(hint)) => Some(NoPr::Hint(hint)),
            _ => None,
        });
        let shown = pr.is_some().then_some(tree.as_deref()).flatten();
        PrFrame {
            asking: tree.as_deref().is_some_and(|t| self.prs.asking(t)),
            agent: if shown.is_some() { self.fix_agent(cx) } else { "agent" },
            observe_only: self.agents.observe_only(),
            deletable: tree.as_deref().is_some_and(|t| self.deletable(t)),
            me: shown.map(|t| self.login(&self.gh_in(t, cx))).unwrap_or_default(),
            ai: self.store.git.ai,
            draft: self.store.git.draft,
            writer: provider_name(&self.store.git.agent),
            minute: (now_ms() / 60_000) as u64,
            tree,
            branch,
            pr,
            offer,
        }
    }

    /// Hands the side what it draws, before anything draws it or calls into it.
    pub(crate) fn sync_pr_side(&mut self, cx: &mut Context<Self>) {
        let frame = self.pr_frame(cx);
        self.pr.update(cx, |p, cx| p.set(frame, cx));
    }

    pub(crate) fn shown_pr(&self, cx: &App) -> Option<(String, &Pr)> {
        let tree = self.cwd(cx)?;
        let pr = self.prs.get(&tree)?;
        Some((tree, pr))
    }

    /// Where the PR of `tree` stands, in its colour, as its card's header says it.
    pub(crate) fn pr_verdict(&self, tree: &str, pr: &Pr, cx: &App) -> (Token, String) {
        self.pr.read(cx).verdict(tree, pr, self.fix_agent(cx))
    }

    pub(crate) fn on_pr_event(&mut self, _: &Entity<PrSide>, ev: &PrEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            PrEvent::Refresh => self.refresh_pr(cx),
            PrEvent::OpenUrl(url) => self.open_pr(url.clone(), window, cx),
            PrEvent::OpenFile(path) => {
                let Some(base) = self.shown_pr(cx).map(|(_, pr)| self.pr_base(pr)) else { return };
                self.open_doc(Doc::PrFile { base, path: path.clone() }, false, cx);
            }
            PrEvent::OpenComposer => self.open_pr_composer(window, cx),
            PrEvent::Write => self.write_pr(window, cx),
            PrEvent::Create => self.create_pr(window, cx),
            PrEvent::SetDraft(draft) => {
                self.store.git.draft = *draft;
                self.save_soon(cx);
                cx.notify();
            }
            PrEvent::Merge => self.merge_pr(cx),
            PrEvent::Ready => self.mark_ready(cx),
            PrEvent::Fix { checks, threads } => self.send_fix(checks, threads, cx),
            PrEvent::Conflict => self.send_conflict(cx),
            PrEvent::Resolve { thread, resolved } => self.resolve_thread(thread.clone(), *resolved, true, cx),
            PrEvent::Reply { thread, body } => self.send_reply(thread.clone(), body.clone(), window, cx),
            PrEvent::Comment { tree, body } => self.send_comment(tree.clone(), body.clone(), window, cx),
            PrEvent::ViewThread(id) => self.view_thread(id, cx),
            PrEvent::DeleteMerged(tree) => self.delete_merged(tree, cx),
            PrEvent::Undo => self.undo_pr(cx),
        }
    }

    /// Opens the composer for the worktree on screen, filled by the agent picked for commit messages or from the commits.
    pub(crate) fn open_pr_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus(cx);
        let (Some(tree), Some(repo)) = (self.cwd(cx), self.repo_handle(cx)) else { return };
        self.show_pr_side(cx);
        let ai = self.store.git.ai;
        self.pr.update(cx, |p, cx| p.open_composer(tree, (!ai).then(|| from_commits(&repo.commits)), window, cx));
        if ai {
            self.write_pr(window, cx);
        }
        cx.notify();
    }

    /// Asks the commit-message agent for a title and body from the branch's commits and diff.
    pub(crate) fn write_pr(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(cwd), Some(repo)) = (self.cwd(cx), self.repo(cx)) else { return };
        let Some(base) = repo.base.clone() else { return };
        let fallback = from_commits(&repo.commits);
        if !self.pr.update(cx, |p, cx| p.begin_write(cx)) {
            return;
        }
        let tree = cwd.clone();
        let argv = self.store.git.pr_argv(&self.store.agents.command(&self.store.git.agent));
        let task = cx.background_executor().spawn(async move {
            let context = git::pr_context(&hosts::runner_for(&cwd), &cwd, &base);
            crate::util::run_login(&argv.iter().map(String::as_str).collect::<Vec<_>>(), &cwd, &context)
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            this.update_in(cx, |d, window, cx| {
                d.pr.update(cx, |p, cx| p.written(&tree, res, fallback, window, cx));
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Pushes, then opens the PR the composer holds; the card takes its place at once.
    pub(crate) fn create_pr(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.pr.read(cx).composing.clone() else { return };
        let Some(base) = self.repos.get(&tree).and_then(|r| r.base.clone()) else { return };
        let draft = self.store.git.draft;
        let Some((title, body)) = self.pr.update(cx, |p, cx| p.begin_create(&tree, draft, window, cx)) else { return };
        let (dir, push_argv) = (tree.clone(), self.push_argv(cx));
        let (argv, gh) = (github::create_argv(&base, &title, draft), self.gh_in(&dir, cx));
        let task = cx.background_executor().spawn(async move {
            push(&dir, &push_argv)?;
            gh.run(&argv, &dir, &body)
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            this.update_in(cx, |d, window, cx| {
                d.pr.update(cx, |p, cx| p.end_gh(cx));
                match res {
                    Ok(out) => {
                        let base = github::base_branch(&base).to_string();
                        let kind = if draft { "draft PR" } else { "PR" };
                        match github::created(&out) {
                            Some(number) => {
                                d.notify_pr(format!("Opened {kind} #{number} into {base}"), None, cx);
                                let url = out.lines().last().unwrap_or_default().trim().to_string();
                                d.prs.put(&tree, Pr { number, title, url, draft, base, ..Pr::default() }, Instant::now());
                            }
                            None => {
                                d.notify_pr(format!("Opened {kind} into {base}"), None, cx);
                                d.prs.expire(&tree, Instant::now());
                            }
                        }
                        d.pr.update(cx, |p, cx| p.close_composer(window, cx));
                        d.poll_prs(cx);
                    }
                    Err(e) => d.pr.update(cx, |p, cx| {
                        p.error = Some(e);
                        cx.notify();
                    }),
                }
                d.refresh_git(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn notify_pr(&mut self, text: String, undo: Option<Undo>, cx: &mut Context<Self>) {
        self.pr.update(cx, |p, cx| p.notify_pr(text, undo, cx));
        cx.notify();
    }

    /// Runs `gh` in the worktree on screen while the card says `busy`, then `done` with what it printed.
    fn run_gh(&mut self, busy: &'static str, argv: Vec<String>, cx: &mut Context<Self>, done: impl FnOnce(&mut Self, String, &mut Context<Self>) + 'static) {
        let Some(dir) = self.cwd(cx) else { return };
        if !self.pr.update(cx, |p, cx| p.begin_gh(&dir, busy, cx)) {
            return;
        }
        let gh = self.gh_in(&dir, cx);
        let task = cx.background_executor().spawn(async move { gh.run(&argv, &dir, "") });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                d.pr.update(cx, |p, cx| p.end_gh(cx));
                match res {
                    Ok(out) => done(d, out, cx),
                    Err(e) => d.fail(e, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Merges the PR on screen, then deletes its worktree if the card says to.
    pub(crate) fn merge_pr(&mut self, cx: &mut Context<Self>) {
        let Some((tree, pr)) = self.shown_pr(cx) else { return };
        if pr.gate() != github::Gate::Ready {
            return;
        }
        let (method, number, base) = (self.pr.read(cx).track.method, pr.number, pr.base.clone());
        self.run_gh("Merging…", github::merge_argv(number, method), cx, move |d, _, cx| {
            d.prs.edit(&tree, |pr| pr.state = PrState::Merged);
            d.prs.expire(&tree, Instant::now());
            d.notify_pr(format!("Merged PR #{number} into {base}"), None, cx);
            if d.pr.read(cx).track.delete_after {
                d.delete_merged(&tree, cx);
            }
        });
    }

    pub(crate) fn mark_ready(&mut self, cx: &mut Context<Self>) {
        let Some((tree, pr)) = self.shown_pr(cx) else { return };
        self.run_gh("Marking ready…", github::ready_argv(pr.number), cx, move |d, _, _| {
            d.prs.edit(&tree, |pr| pr.draft = false);
            d.prs.expire(&tree, Instant::now());
        });
    }

    /// A worktree of its own, unlike the project's checkout, which stays.
    pub(crate) fn deletable(&self, tree: &str) -> bool {
        self.project.as_ref().and_then(|p| self.worktrees.get(p)).is_some_and(|ws| ws.iter().any(|w| w.path == tree && !w.main))
    }

    /// The name of the agent fixes go to.
    pub(crate) fn fix_agent(&self, cx: &App) -> &'static str {
        self.chat_target(&self.chat_choices(cx), cx).and_then(|id| self.agents.get(&id)).map_or("agent", |a| theme::provider_name(&a.provider))
    }

    /// Deletes a merged PR's worktree and its subagents', keeping their branches; asks first if any has changes.
    pub(crate) fn delete_merged(&mut self, tree: &str, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone().filter(|_| self.deletable(tree)) else { return };
        let removal = Removal { project, tree: tree.to_string(), branch: None, delete_branch: false, teardown: true };
        let (mut subagents, on_hosts) = self.subagent_removals(&removal, cx);
        let dir = removal.tree.clone();
        // Untracked files count even when Changes hides them, so this reads the trees afresh.
        let dirty = |dir: &str| git::dirty_files(&hosts::runner_for(dir), dir).is_none_or(|n| n > 0);
        let task = cx.background_executor().spawn(async move {
            subagents.extend(crate::removal::host_removals(&on_hosts, false));
            (dirty(&dir) || subagents.iter().any(|r| dirty(&r.tree) || r.lost() > 0), subagents)
        });
        cx.spawn(async move |this, cx| {
            let (dirty, checked) = task.await;
            this.update(cx, |d, cx| {
                if dirty {
                    d.ask_delete_worktree(removal.project, removal.tree, cx);
                } else {
                    d.delete_worktree(removal, &checked, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Opens the file thread `id` is on as the PR changes it, scrolled to the thread; one whose line is gone shows in Comments instead.
    pub(crate) fn view_thread(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some((_, pr)) = self.shown_pr(cx) else { return };
        let Some(t) = pr.threads.iter().find(|t| t.id == id) else { return };
        if t.outdated || t.line.is_none() {
            self.pr.update(cx, |p, cx| {
                p.track.tab = PrTab::Conversation;
                cx.notify();
            });
            return self.show_pr_side(cx);
        }
        let doc = Doc::PrFile { base: self.pr_base(pr), path: t.path.clone() };
        self.diff.reveal = Some(id.to_string());
        self.open_doc(doc, false, cx);
    }

    /// The remote branch `pr` merges into, as its diffs read it.
    fn pr_base(&self, pr: &Pr) -> String {
        format!("{}/{}", self.store.git.remote, pr.base)
    }

    /// Pins the threads of the PR on screen to the PR diffs.
    pub(crate) fn pin_threads(&mut self, cx: &mut App) {
        let pr = self.cwd(cx).and_then(|tree| self.prs.get(&tree));
        let base = pr.map(|pr| self.pr_base(pr));
        self.diff.pin(pr.map_or(&[][..], |pr| pr.threads.as_slice()), base.as_deref(), cx);
    }

    /// Pins the threads again and reads the PR diffs again, for what was pushed or said since.
    pub(crate) fn refresh_pr_diffs(&mut self, cx: &mut Context<Self>) {
        self.pin_threads(cx);
        let panes: Vec<PaneId> = self.diff.views(cx).filter(|(_, v)| matches!(v.at, At::Base(_))).map(|(p, _)| p).collect();
        for pane in panes {
            self.load_diff(pane, cx);
        }
    }

    /// Sends `prompt` to the agent this worktree's chat goes to; that agent's id once sent.
    fn send_to_agent(&mut self, prompt: &str, cx: &mut Context<Self>) -> Option<String> {
        if self.agents.observe_only() {
            return None;
        }
        let Some(target) = self.chat_target(&self.chat_choices(cx), cx) else {
            self.fail("Start an agent in this worktree to send it this.", cx);
            cx.notify();
            return None;
        };
        let (terminal, provider) = self.agents.get(&target).map(|a| (a.terminal_id.clone(), a.provider.clone()))?;
        match self.route(&terminal, cx) {
            Route::Chat => self.chat_command(&target, chat::Command::Send(prompt.to_string()), cx),
            Route::Paste => {
                self.send_input(&terminal, &term::paste_bytes(prompt, true), cx);
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(crate::add_to_chat::SEND_AFTER).await;
                    this.update(cx, |d, cx| d.send_input(&terminal, b"\r", cx))
                })
                .detach();
            }
            Route::Blocked => {
                self.fail("This agent can't take pasted text right now.", cx);
                cx.notify();
                return None;
            }
        }
        self.notify_pr(format!("Sent to {}", theme::provider_name(&provider)), None, cx);
        Some(target)
    }

    /// Hands failed checks and review threads of the PR on screen to its agent.
    pub(crate) fn send_fix(&mut self, checks: &[String], threads: &[String], cx: &mut Context<Self>) {
        let Some((tree, pr)) = self.shown_pr(cx) else { return };
        let pr = pr.clone();
        let checks: Vec<&Check> = pr.runs.iter().filter(|c| checks.contains(&c.name)).collect();
        let threads: Vec<&Thread> = pr.threads.iter().filter(|t| threads.contains(&t.id)).collect();
        if checks.is_empty() && threads.is_empty() {
            return;
        }
        if let Some(agent) = self.send_to_agent(&fix_prompt(&pr, &checks, &threads), cx) {
            self.pr.update(cx, |p, cx| {
                p.track.send(&tree, &agent, &pr, &threads, &checks, false);
                cx.notify();
            });
        }
    }

    pub(crate) fn send_conflict(&mut self, cx: &mut Context<Self>) {
        let Some((tree, pr)) = self.shown_pr(cx) else { return };
        let (prompt, pr) = (conflict_prompt(&pr.base), pr.clone());
        if let Some(agent) = self.send_to_agent(&prompt, cx) {
            self.pr.update(cx, |p, cx| {
                p.track.send(&tree, &agent, &pr, &[], &[], true);
                cx.notify();
            });
        }
    }

    /// Posts the reply at once and shows it as sent; a failure takes it back out.
    pub(crate) fn send_reply(&mut self, thread: String, body: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = self.cwd(cx) else { return };
        let gh = self.gh_in(&dir, cx);
        let me = self.login(&gh);
        let at = now_ms() as u64;
        self.prs.edit(&dir, |pr| {
            if let Some(t) = pr.thread_mut(&thread) {
                t.comments.push(github::Comment { author: me, body: body.clone(), at, ..github::Comment::default() });
            }
        });
        self.pr.update(cx, |p, cx| p.cancel_reply(window, cx));
        let argv = github::reply_argv(&thread, &body);
        let tree = dir.clone();
        let task = cx.background_executor().spawn(async move { gh.run(&argv, &dir, "") });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                if let Err(e) = res {
                    d.prs.expire(&tree, Instant::now());
                    d.fail(format!("Couldn't post the reply: {e}"), cx);
                    d.poll_prs(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Who `gh` acts as, as a comment it posts names its author.
    pub(crate) fn login(&self, gh: &Gh) -> String {
        match (gh.account(), &self.prs.gh) {
            (Some(account), _) => account.to_string(),
            (None, Some(Some(github::GhStatus::Ready { login, .. }))) => login.clone(),
            _ => "You".into(),
        }
    }

    /// Asks `gh` about the PR on screen now, rather than at its next poll.
    pub(crate) fn refresh_pr(&mut self, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd(cx) else { return };
        self.prs.expire(&tree, Instant::now());
        self.poll_prs(cx);
        cx.notify();
    }

    pub(crate) fn send_comment(&mut self, tree: String, body: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(number) = self.prs.get(&tree).map(|pr| pr.number) else { return };
        let dir = tree.clone();
        let gh = self.gh_in(&dir, cx);
        let note = Note { author: self.login(&gh), body: body.clone(), at: now_ms() as u64, ..Note::default() };
        self.prs.edit(&dir, |pr| pr.notes.push(note.clone()));
        self.pr.update(cx, |p, cx| p.cancel_comment(window, cx));
        let (argv, input) = (github::comment_argv(number), body.clone());
        let task = cx.background_executor().spawn(async move { gh.run(&argv, &dir, &input) });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            this.update_in(cx, |d, window, cx| {
                match res {
                    Ok(out) => {
                        let url = out.lines().last().unwrap_or_default().trim().to_string();
                        d.prs.edit(&tree, |pr| {
                            if let Some(n) = pr.notes.iter_mut().find(|n| **n == note) {
                                (n.id, n.url) = (url.rsplit('#').next().unwrap_or("posted").to_string(), url);
                            }
                        });
                        d.prs.expire(&tree, Instant::now());
                        d.poll_prs(cx);
                    }
                    Err(e) => {
                        d.prs.edit(&tree, |pr| pr.notes.retain(|n| *n != note));
                        d.pr.update(cx, |p, cx| p.comment_failed(tree, body, &e, window, cx));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn resolve_thread(&mut self, thread: String, resolved: bool, undo: bool, cx: &mut Context<Self>) {
        let Some(dir) = self.cwd(cx) else { return };
        self.resolve_in(dir, thread, resolved, undo, cx);
    }

    fn resolve_in(&mut self, dir: String, thread: String, resolved: bool, undo: bool, cx: &mut Context<Self>) {
        self.prs.edit(&dir, |pr| {
            if let Some(t) = pr.thread_mut(&thread) {
                t.resolved = resolved;
            }
        });
        let notice = undo.then(|| {
            let text = if resolved { "Resolved the conversation" } else { "Reopened the conversation" };
            (text.to_string(), Undo { tree: dir.clone(), thread: thread.clone(), resolved: !resolved })
        });
        self.pr.update(cx, |p, cx| p.resolved(&thread, notice, cx));
        let (argv, gh) = (github::resolve_argv(&thread, resolved), self.gh_in(&dir, cx));
        let tree = dir.clone();
        let task = cx.background_executor().spawn(async move { gh.run(&argv, &dir, "") });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                if let Err(e) = res {
                    d.prs.edit(&tree, |pr| {
                        if let Some(t) = pr.thread_mut(&thread) {
                            t.resolved = !resolved;
                        }
                    });
                    d.fail(format!("Couldn't update the conversation: {e}"), cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn undo_pr(&mut self, cx: &mut Context<Self>) {
        let Some(Undo { tree, thread, resolved }) = self.pr.update(cx, |p, cx| p.take_undo(cx)) else { return };
        self.resolve_in(tree, thread, resolved, false, cx);
    }

    pub(crate) fn fetch_avatars(&mut self, tree: &str, cx: &mut Context<Self>) {
        let Some(pr) = self.prs.handle(tree) else { return };
        let me = self.login(&self.gh_in(tree, cx));
        let dir = self.store.home().join("avatars");
        self.pr.update(cx, |p, cx| p.fetch_avatars(&pr, &me, dir, cx));
    }

    pub(crate) fn pinned_thread(&self, k: usize, id: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.pr.update(cx, |p, cx| p.pinned_thread(k, id, cx))
    }
}
