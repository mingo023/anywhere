---
id: 5
title: How terminal, agent and card relate when an agent runs in a shell
labels: [wayfinder:grilling]
status: closed
assignee: mingo023
blocked_by: []
---

## Question

When the user types `claude` in a Pocket terminal, what is the "session" on the list? Today each agent gets a card, an agentless PTY gets its own card, and child shells (tab/split) are hidden (`packages/desktop/crates/pocket/src/main.rs:327-363`). Settle:

- whether the terminal's card becomes the agent's card, or an agent card is added and tied to the terminal;
- what the card shows when the agent exits but the shell lives, and when one terminal runs `claude` then `codex`;
- where an agent started in a child shell (another session's split or tab) appears;
- which worktree owns the session when the user `cd`s to another worktree before starting the agent;
- the names (Session, Terminal, Agent, Card), recorded in `CONTEXT.md`.

## Resolution

Terms are in `CONTEXT.md`: Terminal, Session, Agent, Conversation.

- **Card = session.** A session is a top-level terminal plus its tab and split terminals. Agents run inside terminals, at most one per terminal at a time. There is no card per agent.
- **Several agents in one session:** one aggregate card.
  - Status: the most urgent across the session's agents.
  - Title: from the top-level terminal's agent, else the most recently active agent.
  - One provider badge per agent.
  - Opening the card focuses the pane that needs attention.
- **Agent exits, shell lives:** the card keeps the last agent's title and a faded provider badge until a new agent runs in the session. A session that never ran an agent shows its command line, as today.
- **Worktree:** fixed by the top-level terminal's launch cwd, for life. `cd` elsewhere doesn't move the card; branch and Changes follow the session's worktree.
- **Agent = process.** `/clear` or `/resume` switches the agent's conversation, and the timeline and title follow it. The phone keeps one entry per agent.
