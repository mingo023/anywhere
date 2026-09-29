# Coding Pocket

Watch and drive coding agents running on a Mac, from the desktop app and the phone.

## Language

**Terminal**:
One PTY that pocketd owns, running a shell or an agent directly.
_Avoid_: pane, PTY session, pocketd session

**Session**:
What the session list shows as one card: a top-level terminal plus the terminals opened as its tabs and splits. It belongs to the worktree holding its top-level terminal's launch directory, for life.
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
Where an agent stands, from the user's view. One of Needs you, Done, Working, Idle, in that order of urgency. A session shows its most urgent agent's status; terminals without an agent have none.

**Needs you**:
The agent can't continue until the user answers something: a permission, a question, a dialog. Clears only when answered.
_Avoid_: blocked, waiting, attention

**Working**:
The agent is running a turn.
_Avoid_: running, busy

**Done**:
A turn ended and nobody has seen it yet. Marked failed when the turn errored; a user interrupt is not a failure.
_Avoid_: finished, unread

**Idle**:
At the prompt, with nothing unseen.
_Avoid_: waiting, ready

**Seen**:
An agent is seen while its pane is visible in a focused desktop window, or its timeline is open on the phone. One flag for all clients; a turn that ends while seen never becomes Done.
_Avoid_: read, acknowledged
