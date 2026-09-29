---
id: 1
title: How herdr detects agents and their state
labels: [wayfinder:research]
status: closed
assignee:
blocked_by: []
---

## Question

How does herdr tell that a pane runs an agent (claude, codex), and how does it derive that agent's state (idle, working, waiting, attention, done…)? Find out:

- the signals behind detection and state: process tree, screen scraping, hooks, OSC, or transcript files;
- the state set and its transitions;
- when "needs attention" clears (pane focused? new input?);
- how each of claude and codex is handled.

Sources: herdr.dev, upstream source, 0.9.0 installed at `/opt/homebrew/bin/herdr`.

## Resolution

Findings: `docs/research/herdr-agent-state.md` on branch `research/herdr-agent-state` (fce4afc).

- **Detection:** foreground process group (`tcgetpgrp`) plus argv matched against a name table. It sees through node/bun/`sh -c` wrappers, and `HERDR_AGENT` covers sandboxes. tmux inside a pane hides the agent.
- **State (claude, codex):** screen rules only, over the live bottom of the screen plus the OSC title and progress, polled every 300ms. Hooks send just the session id at start; transcripts are never read. Hooks were dropped as a state source in 0.6.7 because stale reports left panes stuck.
- **States:** idle, working, blocked, unknown. `done` is idle the user hasn't seen yet.
- **Clearing:** blocked clears when the prompt leaves the screen. `done` clears on a tab switch, on focus, or when work resumes. A finish watched live never becomes `done`.
