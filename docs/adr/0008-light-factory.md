# Light Factory: one engine owns a Factory ticket, and one pure function decides it

A Ticket of a project whose Factory is on moves through fixed stations: Spec, Plan, Implement, Checks, PR review, Merged. pocketd's `factory.Engine` owns such a Ticket's `Status`, `Station` and `Gate`, and the Board's scheduler leaves it alone. Every decision is made by one pure function, `factory.Next(Facts)`, which returns the Ticket as it should read now and the actions to do. We chose this because the Board derives a Ticket's status from its one session, and a Factory ticket has several: while workers run, the lead is idle, so the lead's status would free the slot and show the Ticket as finished. A pure function also lets every rule of the pipeline be tested without a session. The cost: two writers of `board.json` exist, split by `Scheduler.Owns`, and a Factory ticket's status no longer follows its lead.

## Rules

- A Ticket with a `Station` is a Factory ticket. The engine sets the station when the scheduler claims a Ticket of a project whose Factory is on. Turning the Factory off later changes nothing for a Ticket that has a station.
- Hooks only queue. `Registry.OnStatus`, `Registry.OnTurnEnd` and `Teams.OnChange` run under a lock, so each appends one event. `Engine.Run` handles events in order on one goroutine, and `Engine.step` is the one place that calls `Next` and writes the result.
- `Next` reads nothing but its `Facts`. A new rule is a new field of `Facts` and a new case, with a test in `station_test.go`.
- Team mode is not changed. The lead is a chat session, its plan is a Team, a `proposed` Team is the Plan gate, and `team.start` is the approval.
- No agent advances a gate. The verbs that do (`team.start`, `permission.resolve`, `gh.pr.merge`, `factory.advance`, `factory.hint`) need a scope that a terminal peer does not hold, and `scripts/red-team.sh` proves it.
- What a Ticket has done is on disk, not in memory: `Station`, `Gate`, `Fix` and `Hint` in `board.json`, the spec and the activity in `state/factory/<ticket>/`. A restart grants nothing: `Engine.Recover` steps every live Factory ticket again from those facts.

## Considered

- **Let the scheduler keep the status, and add a "has workers" flag.** Rejected: the rule for each station would then live in two packages.
- **One goroutine per Ticket.** Rejected: a Ticket's facts come from three sources (the agent registry, Team mode, GitHub), and a queue gives one order for all of them with no lock around a Ticket.
- **A new verb to approve a plan.** Rejected: `team.start` already carries the kept tasks and already has its scope row and its red-team probe.
