# Anywhere

Watch and drive coding agents running on a Mac, from the desktop app and the phone.

## Language

**Terminal**:
One PTY that pocketd owns. A terminal the desktop opens runs the user's login shell, which starts any agent inside it, and ends only when that shell exits. One started with `pocketd run` runs its command directly. It belongs to the Local it was opened in, else to the worktree holding its launch directory, for life.
_Avoid_: pane, PTY session, pocketd session

**Project**:
A git repository, or any other folder, added to Pocket, listed on the sidebar in the user's order, by name or by recent activity, under Pinned, a Group or Other. Selecting it selects its main worktree, the repository's own checkout; with nothing open there, the new session page shows instead. A terminal launched outside every project adds its folder as a project while the folder has terminals, unless the user keeps it; a folder that isn't a repository is its own only worktree. Removing a project closes its terminals and leaves its folder on disk.
_Avoid_: repository, repo, folder

**Worktree**:
A git worktree of a project, listed under it on the sidebar by its name: the name of its folder, which stays when its branch changes. A project lists its main worktree and the ones Pocket created or the user imported; a folder in any other worktree belongs to the main one. One Pocket creates starts on a new branch of the same name. Deleting a worktree closes its terminals and removes its folder but keeps its branch; the main worktree can't be deleted. Its terminals are laid out in tabs and splits; the sessions in them are its session list.
_Avoid_: workspace, branch

**Local**:
A named place over a project's own checkout, listed under the project with its worktrees. Its Locals share files, index and branch; each has its own terminals, tabs and sessions. Every project has one over its main worktree, which can't be deleted and is listed only while it has a terminal or tab open; deleting another closes its terminals and leaves the files alone.
_Avoid_: workspace, branch workspace

**Scratch**:
A place with no project: a folder Anywhere makes under `~/.anywhere/sessions`, an empty git repository of its own, with its own terminals, tabs and session list. The sidebar lists them above Projects, under Sessions. pocketd knows each as a Project; only the sidebar and the new session page tell them apart. One is made when a session is sent with No project picked, which is what the Sessions `+` starts on. Deleting one closes its terminals and removes its folder.
_Avoid_: project-less workspace

**Group**:
A named, coloured box of projects on the sidebar that the user makes, folds and drags projects into. Deleting it leaves its projects in Other, the projects in no group.
_Avoid_: folder, section

**Pinned**:
The sidebar's top box of the projects, worktrees and Locals the user pinned. A pinned project leaves its group until it's unpinned or moved.
_Avoid_: favorite, starred

**Login shell**:
The shell set on the user's macOS account with `chsh`, such as fish. It stays the same no matter how the app was launched.
_Avoid_: TERM, $SHELL, default shell

**Session**:
What the session list shows as one card: one agent, from launch to exit. Running `claude` then `codex` in one terminal makes two sessions; `/clear` keeps the session. It ends, and leaves the list, when its agent exits. Closing a session closes its terminal. It belongs to its terminal's Local or worktree, wherever the agent `cd`s.
_Avoid_: card, workspace, tab

**Agent**:
A coding agent's process running in a terminal, from launch to exit: a claude, codex, cursor, grok, opencode, pi, omp, fx, hermes or antigravity. A terminal runs at most one agent at a time and can run several over its life.
_Avoid_: provider session, thread

**Attached**:
An agent is attached when Pocket hears its status from the agent itself. An agent that is not attached is still listed, but shows only terminal activity and never Needs you or Done. A claude is also attached when it reports through OSC 7501 (Program Status Protocol). Only a claude or a codex reports its status from a terminal. Every other agent reports it only in a chat, where pocketd drives it over the Agent Client Protocol or pi's RPC mode; started in a terminal it runs, but is not attached.
_Avoid_: hooked, bound, connected

**Conversation**:
The Claude session, the Codex thread, or the thread another agent's CLI keeps under an id of its own, that an agent is currently on. `/clear` or `/resume` switches the agent to another conversation; the agent stays the same.
_Avoid_: session id, transcript

**Compare**:
The Changes panel's other scope, beside Uncommitted: the files a worktree changed against a branch it is compared **against**: its base (the branch it was created from) or another local or remote branch. It diffs from where the worktree left that branch to HEAD, as a pull request shows the branch. It is read-only, and each worktree remembers its own. A ref that goes away sends it back to the base.
_Avoid_: branch diff, branch scope

### Status

**Status**:
Where an agent stands, from the user's view. One of Needs you, Failed, Done, Working, Idle, in that order of urgency. A session shows its agent's status; terminals without an agent have none.

**Needs you**:
The agent can't continue until the user answers something: a permission, a question, a dialog. Clears only when answered.
_Avoid_: blocked, waiting, attention

**Working**:
The agent is running a turn.
_Avoid_: running, busy

**Done**:
A turn ended and nobody has seen it yet.
_Avoid_: finished, unread

**Failed**:
A Done turn that errored. It asks for a look before plain Done. A user interrupt is not a failure; the agent goes Idle.

**Idle**:
At the prompt, with nothing unseen.
_Avoid_: waiting, ready

**Program status**:
What an agent says about itself on its terminal: idle, working, blocked, done, error. Claude 2.1.295 and later send it once pocketd answers its query; it drives that claude's status instead of the hooks. Its `blocked` is Needs you.

**Seen**:
An agent is seen while its pane is visible in a focused desktop window, or its timeline is open on the phone. One flag for all clients; a turn that ends while seen never becomes Done.
_Avoid_: read, acknowledged

**Up next**:
The sessions that want a look, in the order to handle them: Needs you, then Failed, then Done, each oldest status change first. Seen sessions drop out because pocketd turns a seen Done into Idle.
_Avoid_: next up, queue

### Automations

**Automation**:
A prompt pocketd starts in an agent on a schedule: a name, the prompt, which agent, a folder, and either chosen weekdays at a time or every few minutes. It is enabled or paused, and can also be started by hand with Run now. pocketd owns it and keeps firing while the desktop is closed. Each run is a normal session in its folder; an agent that reports its status only in a chat runs as a chat.
_Avoid_: job, cron, task, routine

**Run**:
One firing of an automation, from its start to its end. It shows the status of its session: Starting, Working, Needs you, Done, Failed, or Stopped when its session closed first. A run that never launched is Skipped, because the Mac was asleep past the grace window or the automation was still working from the last run. An automation has one active run at a time.
_Avoid_: execution, job run, trigger, running (use Working)

### Activity

**Activity**:
One screen of every session across projects, in columns by where it stands: Idle, Working, Needs you, Done and In review. A session sits in the column its Status says, Failed with Needs you. An Idle session whose worktree's branch has an open or draft PR is In review instead: it waits on other people. That differs from the Board's In review, which holds every idle ticket. Opening a card previews the session and marks a Done or Failed one seen; Open session goes to its pane.
_Avoid_: Sessions page, Agents, dashboard

### Board

**Board**:
One screen of every project's Tickets in columns: To do, Queued, Working, Needs you, Done, In review, Merged and, when shown, Closed. A card is in the column its session's status says; it is only dragged while it has no session. pocketd owns it.
_Avoid_: kanban, task board

**Ticket**:
A piece of work on the Board: a title, a prompt, an agent, a project and the branch it starts from. Started, it gets its own Worktree and session. Closing it keeps both; a merged PR or a deleted Worktree closes it too.
_Avoid_: task, issue

**Queued**:
Started and waiting for a slot. pocketd starts the first in line while fewer Tickets than the limit are Starting, Working or Needs you.
_Avoid_: pending, backlog

### Teams

**Team**:
A request a lead split into tasks that workers do side by side, each in its own worktree, and that the user merges back. A team prompt starts it: the lead answers with a plan, which pocketd checks and proposes. The user edits and starts it, answers what workers ask beyond their allowlist, and merges. A lead has one team; a new team prompt replaces it, unless that team is running, in review or merging.
_Avoid_: swarm, crew, run (a Run is one firing of an automation)

**Lead**:
The chat session a team prompt went to. It plans the team, reviews each handback, and says "Ready to merge" when the work is right. It has no tools for the team; pocketd does what its plan says.
_Avoid_: orchestrator, manager, parent

**Worker**:
A chat session pocketd starts for one task of a team, in a new worktree on its own branch, nested under its lead. It changes only its task's files, and each of its turns that ends normally becomes a handback. It is never a pi or an omp: they run every tool without asking, so no allowlist can hold them.
_Avoid_: sub-agent (a sub-agent runs inside one session), child

**Handback**:
What pocketd mails a lead when a worker's turn ends normally: the worker's summary and the `git diff --stat` of the commit pocketd made of its worktree.
_Avoid_: report, result

**Mailbox**:
A session's queue of messages from pocketd, such as handbacks and notes. A message stays queued until a turn that showed it ends normally; one shown in a turn that failed or was stopped is sent again.
_Avoid_: inbox, queue
