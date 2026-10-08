# Automations (v1)

Contract for three parallel tasks (GO, RUST CLIENT, RUST VIEW). Where this doc and a recon disagree, this doc wins.

## 1. Goals / non-goals

Goals
- An automation is `name, prompt, provider (claude|codex), folder, schedule, enabled`. Triggers: schedule and "run now" only.
- The scheduler lives in pocketd, survives the desktop closing, and survives restarts and in-place upgrades (all state on disk).
- A run launches through `launch.Launcher.Create`. It is a normal PTY session, visible in the desktop session list like any other. It is NOT `claude -p` (no headless-without-terminal type exists; building one is out of scope). `access` is always `"settings"`, `plan` false, checkout is the existing folder (`Checkout.Worktree = folder`).
- Run lifecycle: `pending -> running -> succeeded|failed|cancelled`, `running <-> waiting` while the agent Needs you, `skipped` without a launch.
- Missed run beyond `Grace` (12h, constant) is `skipped` ("Mac asleep"). A due run never backfills: one fire, next time computed after now.
- A due run whose automation already has an active run (`pending|running|waiting`) is `skipped` ("Still running"). Run now on an active automation is refused (`automation_busy`).
- Claim is a compare-and-swap on `nextRunAt`, persisted before launching, so a restart never double-fires.
- History: last 50 runs per automation.
- Desktop screen: a column of Automations and Runs tabs with the selected automation or run on the right, and a full-page editor for New/Edit.

Non-goals: event triggers (shown as disabled "Soon" rows), natural-language composer, multiple projects, worktree-per-run, safety notes, editable grace, stop button, CLI (`pocketd automation`), phone access (owner only), new `automation` session origin, toasts and entry animations, Inbox badge. No new dependency in Go (stdlib only); Rust uses `chrono` (already a workspace dep) in `pocket` only.

## 2. pocketd layout and Go types

Wire types live in `internal/proto/automation.go` (model code imports `proto`, as `launch` does). New package `internal/automation` (no wsserver import).

```go
// internal/proto/automation.go
const CapAutomations = "automations.v1"
type Schedule struct {
    Kind     string `json:"kind"`               // "days" | "interval"
    Days     []int  `json:"days,omitempty"`     // days only: 0=Sun..6=Sat, non-empty, unique
    Time     string `json:"time,omitempty"`     // days only: "HH:MM" 24h, daemon local time
    EveryMin int    `json:"everyMin,omitempty"` // interval only: 5..10080
}
type Automation struct {
    ID, Name, Prompt, Provider, Folder string // json: id name prompt provider folder
    Schedule  Schedule `json:"schedule"`
    Enabled   bool     `json:"enabled"`
    NextRunAt int64    `json:"nextRunAt,omitempty"` // unix ms; omitted when disabled
}
type Run struct {
    ID, AutomationID string // json: id automationId
    Status     string `json:"status"`  // pending running waiting succeeded failed skipped cancelled
    Trigger    string `json:"trigger"` // "schedule" | "manual"
    Why        string `json:"why,omitempty"`      // short cause: "Mac asleep", "Still running", "Interrupted", "Session closed", launch failure message (<=80 runes)
    Summary    string `json:"summary,omitempty"`  // last assistant text, plain, <=240 runes, best effort
    StartedAt  int64  `json:"startedAt"`          // unix ms; for skipped = the due time
    FinishedAt int64  `json:"finishedAt,omitempty"`
    AgentID    string `json:"agentId,omitempty"`
    TerminalID string `json:"terminalId,omitempty"`
}
```

Validation (`proto.Automation.Valid`, used by `DecodeClient`): name 1..80 after trim; prompt 1..`MaxPrompt`; provider claude|codex; folder non-empty; schedule per kind as above. Unknown keys are rejected (same strictness as `decodeSpec`).

```
internal/automation/
  schedule.go   Next(s proto.Schedule, from time.Time) time.Time   // first match strictly after from, local zone
                Late(due, now) bool                                  // now-due > Grace
  store.go      Store: File{Version:1, Automations, Runs}; Open(home) loads Home/state/automations.json
                (bad/future file -> renamed .bad, empty), save via atomicfile.Write 0o600 in 0o700 dir (as internal/state)
                Save(a, now) (Automation, error)      // assigns ID when empty, recomputes NextRunAt, keeps runs
                Enable(id, on, now), Delete(id)       // delete also drops its runs
                Claim(id, due, now) (Run, bool)       // CAS: ok only if stored NextRunAt == due && enabled; sets
                                                      // NextRunAt = Next(now); appends Run{pending|skipped}; one atomic save
                Begin(id, now) (Run, error)           // manual run; ErrBusy when active; NextRunAt untouched
                Update(runID, func(*Run)), Snapshot() (autos, runs), Changed() <-chan struct{} (closed+replaced on every save)
                Reconcile(): pending -> failed "Interrupted"
  scheduler.go  Scheduler{Store, Now func() time.Time, Tick time.Duration, Start Starter, Watch Watcher}
                Run(ctx): every Tick (10s, wall clock, never one long timer) claim each due automation, launch in a goroutine
                RunNow(id) (Run, error); Resume(): re-observe running|waiting runs that have an AgentID
  observe.go    Step(r Run, seen Seen, present, worked bool) (Run, bool)  // pure status mapping
type Starter func(runID string, a proto.Automation) launch.Result      // closure over Launcher.Create
type Watcher func(agentID string) (Seen, bool)                          // closure over Agents.Get(id).Summary() + timeline
type Seen struct{ Status string; Failed bool; Summary string }
```

Launch: `Starter` calls `l.Create(launch.Who{Owner: true, Key: "automation:"+a.ID}, run.ID, spec, nop, nop)`; `run.ID` is the idempotency request id. On `Result.Err` the run is `failed`, `why = Err.Message`. On success: `agentId`, `terminalId` set, status `running`.

Observation (poll `Watch` every 1s, own goroutine per run): `working -> running`, `needsYou -> waiting`, `done|idle` after having seen `working` (`worked`) -> `succeeded`, or `failed` when `Failed`; agent absent -> `cancelled` ("Session closed") unless already finished. `Summary` read once at finish. After a restart `Resume` sets `worked=true` and tolerates an absent agent for 30s while restore reattaches.

Wiring (`cmd/pocketd/serve.go` only): build `automation.Open(home)`, `Reconcile()`, `Scheduler`, `go sched.Run(ctx)` after `d.Restore/Adopt`, then `sched.Resume()`; hand `Store` + `Scheduler` to `wsserver.Server{Automations: ...}`. Do not touch `Agents.OnStatus`.

Files: `internal/peer/check.go` adds `"ws:automation.save|enable|delete|run": Own`. `proto.ServerCaps` adds `CapAutomations`. `internal/wsserver/automation.go` holds dispatch cases and the feed (copy `conn.withNames`: subscribe on first hello when the client advertised the cap and has `Own`; send a snapshot after `agent.list`; push a snapshot on every `Store.Changed()`).

## 3. Wire protocol (phone protocol over the unix socket; cap `automations.v1`, owner scope only)

Client to server (each carries `id`, the request id):

| type | fields beyond `type`,`id` | reply |
|---|---|---|
| `automation.save` | `automation`: `{id?, name, prompt, provider, folder, schedule, enabled}`; omit `id` to create | `ack`, then snapshot push |
| `automation.enable` | `automationId`, `enabled` | `ack`, then snapshot push |
| `automation.delete` | `automationId` | `ack`, then snapshot push |
| `automation.run` | `automationId` | `ack` once the run is recorded `pending`, then snapshot push |

There is no `automation.list`: the snapshot is pushed on hello and on every change.

Server to client: `ack` (`{type,id}`), `error` (`NewCodedError(id, code, message, "")`), and

```json
{"type":"automations","automations":[Automation...],"runs":[Run...]}
```

`runs` is newest first, all automations mixed. The message is always a full snapshot, never a delta, and has no `id`.

Error codes: `invalid_automation` (also an unusable `folder`), `unknown_automation`, `unknown_project` (folder not in `desktop.json` projects), `automation_busy`. A malformed message gets the existing `Malformed message`. Without the cap an older pocketd answers `Malformed message`, which the desktop already maps to OUTDATED.

Golden fixtures (GO TASK creates them first, with exactly this content; Rust and TS read them):
- `client/automation_save.json`: `{"type":"automation.save","id":"p1","automation":{"id":"au1","name":"Morning review","prompt":"Review new PRs","provider":"claude","folder":"/Users/me/app","schedule":{"kind":"days","days":[1,2,3,4,5],"time":"09:00"},"enabled":true}}`
- `client/automation_save_interval.json`: same without `automation.id`, `"schedule":{"kind":"interval","everyMin":60}`, `"provider":"codex"`, id `p2`
- `client/automation_enable.json`: `{"type":"automation.enable","id":"p3","automationId":"au1","enabled":false}`
- `client/automation_delete.json`: `{"type":"automation.delete","id":"p4","automationId":"au1"}`
- `client/automation_run.json`: `{"type":"automation.run","id":"p5","automationId":"au1"}`
- `server/automations.json`: `{"type":"automations","automations":[{"id":"au1","name":"Morning review","prompt":"Review new PRs","provider":"claude","folder":"/Users/me/app","schedule":{"kind":"days","days":[1,2,3,4,5],"time":"09:00"},"enabled":true,"nextRunAt":1791450000000}],"runs":[{"id":"r2","automationId":"au1","status":"waiting","trigger":"manual","startedAt":1791363600000,"agentId":"a1","terminalId":"t1"},{"id":"r1","automationId":"au1","status":"succeeded","trigger":"schedule","summary":"Reviewed 3 PRs.","startedAt":1791277200000,"finishedAt":1791277500000,"agentId":"a0","terminalId":"t0"}]}`

TS mirror: `packages/protocol/src/automation.ts` plus the unions in `messages.ts` and the export in `index.ts`; `packages/protocol/test/golden.test.mjs` must pass unchanged.

## 4. Rust layout (ADR 0003)

Client and model, crate `agents` (no `ui`/`theme`/`pocket` dependency, no `chrono`):
- `crates/agents/src/agents/automations.rs`: serde types `Schedule` (enum, `kind` tag), `Automation`, `RunStatus`, `Run`, `AutomationDraft{id: Option, name, prompt, provider, folder, schedule, enabled}` with `problem(&self) -> Option<&'static str>` (mirrors Go validation), and state `Automations{items, runs, loaded}` with `apply(items, runs)`, `waiting()`, `last_run(id)`, `runs_of(id)`, `active(id)`.
- `crates/agents/src/agents.rs`: `CAPS` gains `automations.v1`; `Frame` gains `automations`/`runs`; `Event::Automations{automations, runs}` and `Event::AutomationError{id, code, message}` (error frames whose `id` starts `auto-`); `Agents.automations: Automations` filled by `Agents::apply`; `Agents::automations_offered()`; `Outbox::automation_save(AutomationDraft) -> String`, `automation_enable(&str, bool)`, `automation_delete(&str)`, `automation_run(&str)` (request ids `auto-N`). Reconnect needs nothing: the server re-sends the snapshot on hello.

View, crate `pocket`, module `automations.rs` + `automations/` (views only; no IO in render):
- State `AutomationsState { tab: Tab (Automations|Runs), segment: Segment (All|Active|Paused), run_filter: Option<String>, selected: Option<String>, list_scroll, runs_list: ListState }`; `new(cx) -> (Self, Vec<Subscription>)`; `CONTEXT = "Automations"`; `Default` used by `capture::reset`.
- Pure logic (free functions over plain data, sentence-named tests at the bottom of `automations.rs`; generic over `chrono::TimeZone` so tests pin the zone): `stats(autos, runs, now)` (7-day window: `startedAt >= now-7d`), `sections(runs, now, tz)` (Needs you, Running, Today, Yesterday, Earlier this week; empty omitted), `up_next(autos)` (enabled, by `nextRunAt`, top 3), `schedule_text(&Schedule)`, `when_text(ms, now, tz)`, `reselect(selected, autos)`, `visible(autos, segment)`.
- One file per component under `automations/`: `page.rs` (header with subline "next <name>, <Day HH:MM>", tab bar with amber Runs badge), `stats.rs`, `needs_card.rs` (first waiting run, Answer), `table.rs` (segmented control, header, rows with enable switch; `uniform_list` + `UniformListScrollHandle`, rows `w_full()`), `drawer.rs` (372w: sentence, Run now / Answer in session, Edit, detail list, last-12 strip, up to 6 recent runs), `runs.rs` (chips, Up next, virtualized `list` feed), `glyph.rs` (six status shapes per look.md D1), `editor.rs` (modal `Overlay::AutomationEditor`: name, prompt, provider, folder picker over registered projects, schedule, Save/Delete via `Confirm`).
- `impl Desktop` stays thin: `open_automations`, `select_automation`, `open_run_session` (existing session-open path; Answer/Open), `save_automation`, `toggle_automation`, `run_automation`. Empty states: "No automations yet. Create one with New automation.", "Nothing active.", "Nothing paused.", "No runs yet".
- Sidebar entry: "Automations" with the `bolt` icon in the expanded sidebar and the rail `nav()`, `Screen::Automations`, `main_view` arm, `.key_context(automations::CONTEXT)`; no list column (`column_shown` false). Palette `Pick::Automations` ("Go to Automations"). Action `OpenAutomations` with a free key (collision test decides). Esc closes the drawer, then the screen's selection.

## 5. Test plan

Go unit (beside code, sentence names, `t.TempDir()`, injected `Now`/`Starter`/`Watcher` as plain funcs):
- schedule: `weekdays_at_nine_skip_the_weekend`, `an_interval_counts_from_the_given_time`, `next_is_strictly_after_from`, `invalid_schedules_are_refused`.
- store: `what_is_saved_loads_back_unchanged`, `a_bad_file_is_set_aside_and_the_store_starts_empty`, `only_one_of_two_claims_on_the_same_due_time_wins`, `a_claim_later_than_the_grace_window_is_skipped`, `a_claim_while_a_run_is_active_is_skipped`, `history_keeps_the_newest_fifty_runs_per_automation`, `deleting_an_automation_drops_its_runs`, `run_now_is_refused_while_a_run_is_active`, `reconcile_fails_runs_left_pending`.
- observe: table over `Step` (working, needsYou, done and idle after work, failed, absent agent, idle before work stays running).
- scheduler: `a_due_automation_starts_exactly_once_across_two_schedulers`, `a_failed_launch_records_why`.
- proto: golden decode and strict-key rejection in `internal/proto`; `peer` Needs table has all four verbs as `Own`.
- TS: `packages/protocol` golden test.

pocketd e2e, `e2e/automation_test.go` (real `pocketd serve`, fakeclaude replies `echo: <prompt>`, `launchReady("")`, `h.Owner(proto.CapAutomations)`; no new fake):
- `TestRunNowLaunchesASessionAndRecordsItDone`: save, run, wait `waiting|running` then `succeeded` in the snapshot, `h.WaitScreen(term, "echo: ...")`.
- `TestADueRunFiresOnceAndASecondStartDoesNotRepeatIt`: write `state/automations.json` with `nextRunAt` a minute ago, start, one `schedule` run; `h.Restart()`, still one.
- `TestARunDueBeyondTheGraceWindowIsSkipped`: `nextRunAt` 13h ago -> `skipped`, "Mac asleep", no session.
- `TestAutomationsSurviveARestart` and `TestAPhoneCannotSeeOrSaveAutomations` (paired phone gets `error`/refusal, no snapshot).

Rust, beside the logic: `agents` (`a_snapshot_replaces_the_automations`, draft `problem` cases, decoding `server/automations.json` via `include_str!`, `Outbox` frame shapes); `pocket` (the free functions above, plus `no_two_bindings_share_a_keystroke_and_context`).

Capture: `STEPS` gains `automations` (list with drawer, one waiting run), `automations-runs`, `automations-editor`; bump the array length; `capture::reset` restores `AutomationsState::default()`. `fixture.ts` adds `automations.v1` to hello caps and sends the golden-shaped snapshot (2 automations, one paused, runs covering all six statuses); add the three screens to `.ui-review/config.json`. Run `.ui-review/fixture/capture.sh <dir> automations=automations automations-runs=automations-runs automations-editor=automations-editor`, in light and dark.

Gates: `cd packages/pocketd && go vet ./... && go test ./...`; `cd packages/desktop && cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`; `cd packages/protocol` its test script. Do not commit.

## 6. Task split (disjoint file ownership)

GO TASK (first step: write the golden files; then the rest). Owns: `packages/pocketd/**` (new `internal/automation/*`, `internal/proto/automation.go`, `internal/proto/messages.go`, `internal/proto/version.go`, `internal/proto/testdata/golden/**`, `internal/peer/check.go`, `internal/wsserver/*`, `cmd/pocketd/serve.go`, `e2e/**`) and `packages/protocol/src/**`. Done when: Go unit + e2e + TS golden pass.

RUST CLIENT TASK. Owns: `packages/desktop/crates/agents/**` only. Reads (never writes) the goldens. Exposes the API in section 4 exactly (names and signatures) so the view compiles against it; stub-compatible with the golden while GO finishes. Done when: `cargo test -p agents` passes.

RUST VIEW TASK. Owns: `packages/desktop/crates/pocket/**` (`automations.rs`, `automations/*`, and the wiring edits in `main.rs`, `desktop.rs`, `desktop/chrome.rs`, `sidebar.rs`, `sidebar/column.rs`, `sidebar/rail.rs`, `actions.rs`, `palette.rs`, `modals.rs`, `modals/*`, `capture.rs`), `packages/desktop/crates/theme/assets/icons/**` (new svgs only, plus a registration line in `theme.rs` if icons need one), `.ui-review/fixture/**`, `.ui-review/config.json`. Must not edit `crates/agents/**`, `crates/ui/**`, `.ui-review/prototypes/automations.html`, `skills-lock.json`, or skills. Builds against section 4's agents API; if it needs an addition, it asks rather than editing. Done when: build, clippy (no new warnings), tests, and before/after captures are clean.

Order: GO writes goldens (minutes) -> CLIENT and VIEW run in parallel with GO -> integrate with `make pocketd` (an old running pocketd shows OUTDATED) and one manual capture against the real daemon.
