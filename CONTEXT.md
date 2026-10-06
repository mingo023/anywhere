# Anywhere

Watch and drive coding agents running on a Mac, from the desktop app and the phone.

## Language

**Terminal**:
One PTY that pocketd owns. A terminal the desktop opens runs the user's login shell, which starts any agent inside it, and ends only when that shell exits. One started with `pocketd run` runs its command directly. It belongs to the worktree holding its launch directory, for life.
_Avoid_: pane, PTY session, pocketd session

**Project**:
A git repository, or any other folder, added to Pocket, listed on the sidebar in the user's order. Selecting it selects its main worktree, the repository's own checkout, which the project's own row stands for. A terminal launched outside every project adds its folder as a project while the folder has terminals, unless the user keeps it; a folder that isn't a repository is its own only worktree. Removing a project closes its terminals and leaves its folder on disk.
_Avoid_: repository, repo, folder

**Worktree**:
A git worktree of a project, listed under it on the sidebar by its name: the name of its folder, which stays when its branch changes. A project lists its main worktree and the ones Pocket created or the user imported; a folder in any other worktree belongs to the main one. One Pocket creates starts on a new branch of the same name. Deleting a worktree closes its terminals and removes its folder but keeps its branch; the main worktree can't be deleted. Its terminals are laid out in tabs and splits; the sessions in them are its session list.
_Avoid_: workspace, branch

**Login shell**:
The shell set on the user's macOS account with `chsh`, such as fish. It stays the same no matter how the app was launched.
_Avoid_: TERM, $SHELL, default shell

**Session**:
What the session list shows as one card: one agent, from launch to exit. Running `claude` then `codex` in one terminal makes two sessions; `/clear` keeps the session. It ends, and leaves the list, when its agent exits. Closing a session closes its terminal. It belongs to its terminal's worktree, wherever the agent `cd`s.
_Avoid_: card, workspace, tab

**Agent**:
A `claude` or `codex` process running in a terminal, from launch to exit. A terminal runs at most one agent at a time and can run several over its life.
_Avoid_: provider session, thread

**Attached**:
An agent is attached when Pocket hears its status from the agent itself. An agent that is not attached is still listed, but shows only terminal activity and never Needs you or Done.
_Avoid_: hooked, bound, connected

**Conversation**:
The Claude session or Codex thread an agent is currently on. `/clear` or `/resume` switches the agent to another conversation; the agent stays the same.
_Avoid_: session id, transcript

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

**Seen**:
An agent is seen while its pane is visible in a focused desktop window, or its timeline is open on the phone. One flag for all clients; a turn that ends while seen never becomes Done.
_Avoid_: read, acknowledged

**Up next**:
The sessions that want a look, in the order to handle them: Needs you, then Failed, then Done, each oldest status change first. Seen sessions drop out because pocketd turns a seen Done into Idle.
_Avoid_: next up, queue
