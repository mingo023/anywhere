# Agent worktrees leave the sidebar and clean up after themselves

## Why

Issue #382: worktrees that pocketd makes for delegated tasks pile up. One Mac had 64 worktrees for one repo, with only one session open. They fill the sidebar and the disk.

- pocketd makes a delegated session's worktree on the same path as a user's (`Launcher.Create` with `Checkout.New`), so it gets the `anywhere` marker and counts as made by Anywhere. `Listed` (`pocketd/internal/worktree/list.go`) shows every made worktree.
- The desktop hides one only while its child session is alive (`without_child_only` in `pocket/src/desktop/project.rs`). Once the child closes, the row comes back. iOS (#333) has the same gap.
- Nothing removes the folder when a task ends. #331 and #332 delete a child's worktree only when the user deletes the parent's worktree. A parent that runs in the project's own checkout never gets deleted, so its children's worktrees stay forever.
- The desktop and pocketd each have their own delete with its own subagent cascade (`pocket/src/removal.rs` and `worktree.delete` in `pocketd/internal/wsserver/workspace.go`).

## Decisions

All of this ships as one PR.

### The sidebar rule

1. The sidebar lists the worktrees a person made: the project's main worktree, worktrees the user created in Pocket, worktrees an automation's run or a Board ticket created, and imported worktrees. It doesn't list a worktree pocketd made for a session an agent started: a delegated session, or a fork an agent called through the MCP tools. `CONTEXT.md` (**Worktree**) states the rule.
2. Remove the "Show worktrees not created by Anywhere" setting (`sidebar.external_worktrees`) from pocketd, the protocol package, the desktop, and the docs. pocketd accepts the field from older clients and ignores it. There is no migration. A user who had it on uses Import to bring a worktree back.

### pocketd records which worktrees an agent made

3. `state/registry.json` gains `agent_worktrees`, a list of paths, beside `tracked`. `Listed` leaves out a path in `agent_worktrees` unless `tracked` also holds it.
4. `NewWorktree` gains `ForAgent`. The orchestrator path (`orchhost.go` `launch` and `launchOn`) sets it, so a host's pocketd records the worktree in its own registry too. `Launcher.Create` adds the path to `agent_worktrees` when it makes the worktree. `worktree.delete` and the cleanup in decision 6 remove it again.
5. pocketd fills `agent_worktrees` once for worktrees agents made before this change. It takes them from the lineage in `orchestrator.db` (`delegations` with `isolation = 'worktree'`, the `lineage` table, and the child sessions' `cwd`), and from `subtrees` in `desktop.json`. It keeps only worktrees that are still on disk, carry the `anywhere` marker, and aren't in `tracked`. `state/agent-worktrees-claimed` makes it run once, like `worktree.Claim` from #223.

### Cleanup

6. When a session an agent started closes, pocketd looks at its worktree. It acts only if the worktree is in `agent_worktrees` and no other session or terminal is in it.
	- If the worktree has no uncommitted changes, pocketd runs the project's teardown script and removes the folder. It deletes the branch only when `LostCommits` is 0, so the parent can still merge a branch that holds commits of its own.
	- If the worktree has uncommitted changes, pocketd keeps it and removes it from `agent_worktrees`, so it shows on the sidebar and the user sees the unfinished work.
	- A child that finished but is still open waits. It closes when its parent closes (#282), and the rule runs then.
7. The one-time fill in decision 5 also applies decision 6 to every worktree whose child session is already closed.
8. A delete skips child worktrees on a host that is offline. When that host's link comes back, pocketd sends again each `EffClose` that failed for it, because the outbox doesn't retry a failed effect today (`orch/worker.go`). The host's pocketd then closes the child and cleans up by decision 6.

### The desktop gets worktrees and deletes from pocketd

9. The desktop speaks `workspace.v1` and takes each project's worktrees from `project.list`, on this Mac and on each host. `project.list` worktrees gain `createdAt`. The desktop stops using `git::worktrees` for the sidebar and drops `project::listed` and `without_child_only`. Changes, diffs and status still read git directly.
10. Deleting a worktree moves to pocketd, in two steps.
	- `worktree.inspect` (new) returns what the confirm sheet shows, for the worktree and each child worktree, including those on hosts: uncommitted files, commits the delete would lose, terminals it would close.
	- `worktree.delete` gains `trash` and `teardownSecs`. Both stay desktop-only settings (decision 9 of `2026-11-24-pocketd-owns-shared-state.md`), so the desktop sends them with each request. A client that omits them gets a plain delete and a 120-second teardown. pocketd runs the teardown script, moves the folder to the Trash or removes it, and deletes the child worktrees.
	- The desktop drops its own delete path: `remove_in_order`, `subagent_removals`, `host_leftovers`, and `sweep_host_leftovers` in `pocket/src/removal.rs`, plus the `subtrees` it writes to `desktop.json`.
11. A host whose pocketd lacks the new capability gets no fallback. A delete there fails with a message that Anywhere on that host needs an update. Nobody uses linked hosts yet.

### iOS

12. `HomeList.build` stops hiding worktrees that hold only delegated sessions, because `project.list` already leaves them out. It keeps hiding the delegated sessions themselves. Update `HomeListTests` to match.

## Out of scope

- "Not delivered" on a chat prompt (#380, #381): the link drop, prompts that the desktop never resends, and refusals that pocketd doesn't log.
- `launch.Slug` drops accented letters: the title "chạy trên" becomes `chy-trn`.
- A "Clean up finished worktrees" button.
- The demo worktree `demo-chy-trn-host` left on the M1 host.
