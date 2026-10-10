# Locals share the project's checkout, and a terminal carries the Local it was opened in

A project can hold several Locals beside its worktrees: named places over the project's own checkout, each with its own terminals, tabs and sessions, the way Superset's local workspaces work. Two Locals share files, index and branch; only what runs in them differs. This amends ADR 0002: a terminal belongs to the Local it was opened in, else to the worktree holding its launch directory. A folder can't tell two Locals apart, so pocketd stores the Local on the terminal, saves it with the terminal and hands it over on upgrade, and gives the shell `POCKETD_LOCAL` so `pocketd run` inside it stays in it. We chose pocketd over the desktop store so the Local survives a desktop restart with its terminals, and so the phone can target one later. The cost is a new capability and an optional field in the terminal's saved state and handoff.

Every project has one Local without a record: the main worktree's row, whose id is the main worktree's path, so existing layouts, names and selection carry over. Its name stays in `names.json` through `worktree.rename`, and it can't be deleted. Locals the user adds live in `state/locals.json` with ids the client picks. Deleting one closes its terminals and leaves the files alone.

## Rules

- `locals.v1` gates `local.create {localId, project, name}`, `local.rename {localId, title}`, `local.delete {localId}`, the `local.list {locals: [{id, project, name}]}` broadcast and the `{local: id}` checkout of `agent.create`.
- A Local's id never starts with `/`, so it can't collide with a worktree path.
- A terminal's Local is fixed at spawn. A terminal without one belongs to the worktree holding its launch directory, as before.
- A rename trims the title; a blank one changes nothing.
