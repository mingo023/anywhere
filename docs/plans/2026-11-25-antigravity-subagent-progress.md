# Antigravity subagent progress

**Goal:** each Antigravity sub-agent's tool calls, and the permission requests they raise, show under the sub-agent card that launched them. That card shows working until its turn ends. Today the calls show inline as the session's own, and #383's card says done 25 ms after launch. pocketd only, plus flipping `Chat.SubAgents`.

**Read first:**
- `docs/orchestrators/research/23-antigravity-live-checks.md` §4. Raw logs: `/tmp/agy-live/logs/4-sub-a.jsonl` (one sub-agent) and `4-sub-b.jsonl` (two).
- `packages/pocketd/internal/acp/session.go`:
  - `tool()` `:1354`, `show()` `:1413`, `child()` `:1244`;
  - the `children`, `kids` and `calls` fields `:116-124`;
  - `finish()` `:828`, `interrupt()` `:430`, `loaded()` `:400`.
- `internal/acp/tool.go`: `launch()` `:271`, `describe()` `:216`.
- `internal/acp/request.go`: `permission()` `:55`.
- `internal/acp/flavor.go`: the `flavor` struct and Antigravity's entry, `:91`.
- The test `TestAnAntigravitySubAgentShowsAsATaskThatEndsAsItLaunchesWithNoReport` (`session_test.go:1226`), and `fakeacp` `launch()` (`e2e/fakeacp/main.go:503`).

---

## What agy 1.5.16 sends

Live (research 23 §4), plus a re-check made for this plan:

- Each `invoke_subagent` launches one sub-agent. Its `tool_call` has `status:"in_progress"`, `title:"Run invoke_subagent"`, and flat `rawInput` `{Role, Prompt, TypeName, Model}`. It goes `completed` about 25 ms later, with no content.
- A child's `tool_call` and `tool_call_update` frames, and its `session/request_permission`, come on the **parent** `sessionId`. The only tag is the `toolCallId`: `"<childConversationId>:call_…"`. In every capture the child's `tool_call` came before its permission request (by 1–5 ms).
- Child text and thoughts never come. Nothing comes after the prompt's reply. The reply waits until the sub-agents finish: 79 s for the two-agent run, 55 s after the last frame.
- **First-seen order is wrong in the only parallel capture.** Alpha (`…080392`) launched before Beta (`…079449`). At t=7.082, Beta's child `8b9d983b` sent its first frame before Alpha's `5a433588`.
- **The descriptor.** `~/.gemini/antigravity-cli/brain/<parentSessionId>/.system_generated/subagents/<childId>.json` holds:
  ```json
  {"conversationId":"<childId>","subagentDescriptor":{"typeName":"self","role":"Alpha Runner"},"state":"SUBAGENT_STATE_ALIVE","spawnStepIndex":3,…}
  ```
  Timing was measured from the file birth times against each log's start, for 3 children over 2 runs:

  | Child | Launch frame | Descriptor written | First child frame |
  |---|---|---|---|
  | `49b4f6e1` | 2.962 | 2.975 | 6.087 |
  | `5a433588` (Alpha) | 4.461 | 4.470 | 7.082 |
  | `8b9d983b` (Beta) | 4.494 | 4.484 | 7.082 |

  The file is written within about 15 ms of the launch, and at least 2.6 s before the child's first frame. It is written once: mtime equals birth time, and `state` stays `ALIVE` after the child is done.
- **`session/load` replays no children.** Re-checked by loading the two-agent session (`/tmp/agy-load/load-b.jsonl`). Each launch comes back as one `tool_call` that is already `completed` and has its args. The rest is the parent's own text and `update_steps` calls. No `"<childId>:"` frame comes.

## Design

### 1. Flavor flag

Add one field to `flavor` and set it in Antigravity's entry:

```
// childCalls tells its sub-agents' calls on its own session, each id "<sub-agent>:<call>".
childCalls bool
```

Every rule below is gated on it. Other agents' ids may contain `:`, and agy's `invoke_subagent` is the only launch this applies to.

### 2. Which card a child belongs to: the descriptor's role, then launch order

**Decision.** On the **first** frame of a new child id, read that child's descriptor once, then pick a card:

1. Read exactly `<brain>/<s.id>/.system_generated/subagents/<childId>.json`. No directory scan.
2. Take the oldest **unclaimed** held launch whose `rawInput.Role` equals `subagentDescriptor.role`.
3. If the file is missing or unreadable, or no role matches, take the oldest unclaimed held launch.
4. With no held launch at all, the call shows inline, as today.

The answer is cached in `s.children[childId]`, and the cache holds a miss too: test with comma-ok. Every later frame of that child is a map lookup.

**Why the descriptor wins over first-seen order.**
- First-seen order swaps the two cards in the one parallel capture there is. The children's first frames land in the same millisecond, so their order is a race.
- The descriptor is on disk seconds before the child's first frame, so it is always there when needed.
- Role ties: two launches with the same `Role` fall back to launch order within that role. That is the best the data allows.
- Order stays as the fallback, so a moved or renamed file degrades to "usually right" rather than to inline rows.
- Matching on `TypeName` adds nothing: it is `"self"` in every capture.

**Why read it synchronously.** `Notify` holds `s.mu` on the reader goroutine. The read is one `os.ReadFile` of a file of about 260 bytes, once per sub-agent, which is not a hot path. Reading it in a goroutine instead would mean buffering the child's frames until the read lands, and a permission request (`request.go`, also under `s.mu`) could then be shown before its row. Not worth it.

**Brain root.** Use a package var beside `OpenWait`:

```
var brain = filepath.Join(home, ".gemini", "antigravity-cli", "brain")
```

Tests point it at a temp dir. The package uses no `Setenv`. The e2e harness gives pocketd and the fake the same `HOME`, so the default works there.

**Where it hooks in.** At the top of `tool()`, after the `ask_question` skip:

```
if parent == "" && s.f.childCalls { parent = s.launchOf(u.ID) }
```

- `launchOf` splits the id on its first `:`; no prefix returns `""`. It then resolves the child as above.
- `tool()` already stores `parent` on a new `tool`, and `show()` records `tool_start` with `ParentToolUseID`. Desktop and iOS fold that under the card (`chat/src/timeline.rs` and `ChatModel/Timeline.swift`).
- Routing through `child()` instead is not needed: it only adds `flushKid`, and agy sends no child text.

**Where the code goes.** A new file `internal/acp/launch.go`, like `delegation.go` is for Hermes. It holds `launchOf`, the descriptor read, the role match, and `endLaunches` (§4). `session.go` gets the one line in `tool()`, the hold in §3, one call in `finish()`, and one field:

```
// launches are the Antigravity sub-agent calls held open until the turn ends, oldest first.
launches []string
```

### 3. Hold the launch's `completed`

In `tool()`, once a launch call is shown and `!s.opening`, append its id to `s.launches`. A launch is a call for which `t.launch()` says ok.

When a held launch goes `completed`:
- skip the `delete` and the `tool_end`;
- keep it in `s.tools`, with its status as told.

A launch that comes `failed` ends at once, as today.

Replay is never held (`s.opening`). The history's launches arrive already completed. If they were held, `loaded()` → `interrupt()` would end them "Interrupted" and add an `interrupted` result to a history that ended fine.

### 4. When a child counts as done: the turn's end

Nothing on the wire says a child has finished:
- it sends no text;
- the descriptor stays `ALIVE`;
- a child's last tool call need not be its last step.

The turn's end is the one signal that holds: agy keeps `session/prompt` open until its sub-agents finish, and nothing arrives after the reply.

`finish()` gets one call, after the `switch` (so after a stop's `interrupt()`) and before `s.sink.Apply(e)`:

```
s.endLaunches(e.OK, adopted)
```

Here `adopted` is captured before the line that resets it, as `runs` is. `endLaunches`:

1. Ends each call still open whose `parent` is a held launch: `tool_end`, not OK, output "Interrupted", as `interrupt()` words it. Nothing more will come for it.
2. Ends each held launch still in `s.tools`: `tool_end`, `OK: e.OK`, no output. There is no report, because child text never streams.
3. Sets `s.launches = nil`.
4. If `adopted` is true, ends the leftovers of a pocketd restart (see Restore).

A stop needs nothing new: `interrupt()` already ends every open call, held launches included, as "Interrupted".

**The card.** No client change.
- While held, the call's status is `running`. `sub_agent.rs` `mark()` shows `Starting` until the first child item, then `Running` with "N steps · time". iOS `Reading.swift` does the same.
- At the turn's end it shows `Done` with that meta. On a failed turn it shows `Failed`.
- A sub-agent that ended early still spins until the turn ends. That is an accepted cost, because there is no earlier signal.

**Why the end lands.** `tool_end` reaches only calls in the timeline's `t.tools`. `StartEpoch` keeps only parented calls and `lasting` calls there, and it runs only when a top-level `user` item is recorded (`agent.Record`). A prompt sent mid-turn waits in `s.queue`, and its `user` item is recorded by `start()` after `finish()`, so after `endLaunches`.

**Steering overlap.** The steering plan reworks `finish()`, `send()` and `next()`. Keep this change to the single `endLaunches` call, so steering only has to keep it where the turn truly ends.

**Steering constraint.** If steering records a steer's `user` item mid-turn, `StartEpoch` drops the held cards from `t.tools`, and their end is lost, so the card spins forever. Whichever lands second must keep running `task` calls across `StartEpoch`, or let `tool_end` fall back to `t.call()` as `tool_model` does. Its tests should pin that.

### 5. Permission requests

No change in `request.go`:
- The child's `tool_call` comes first, and `tool()` creates it with its parent, so `permission()` finds it in `s.tools` with `t.parent` set.
- The ask itself stays session-level: `proto.PermissionRequest` has no parent field. Desktop `permission.rs` and iOS show it as they do today ("Run echo child-step-ok"). While it waits, the row under the card shows running.
- Permission-first has never been seen. If it ever happens, the request is answered as today, and the later `tool_call` still lands under the card.
- `auto()` still decides on `kind`, so each access level answers exactly what it answers today.

Naming the sub-agent on the permission card would need a protocol field. That is **out of scope**.

### 6. Restore and resume

| Path | What happens |
|---|---|
| A session restored from the store (`Shown`) | The stored timeline already has each child under its card. The load's replay is dropped (`loaded()`). Nothing to do. |
| A cold `session/load` of a session pocketd never showed | agy replays launches already completed and no children (live). The cards show done with no steps, as with #383. Unavoidable. |
| pocketd restarted mid-turn (`left == Running`, `adopted`) | The new pocketd lost `s.tools`, `s.children` and `s.launches`, while the stored card shows running. agy sends nothing more for it. In `endLaunches`, when `adopted` and `s.Showing != nil` (the guard `delegation.go` uses), end every `running` item from `s.Showing()` whose call kind is `task` and that has no parent, `OK: e.OK`. For agy those can only be launches. `Showing()` copies the timeline once, only at the end of an adopted turn.

The `tool_end` reaches them:
- `timeline.Load` indexes stored running calls in `t.tools`;
- no `StartEpoch` runs in between, because an adopted turn records no top-level `user` item: prompts queue until it ends.

Test 8 checks the event only. A fake sink can't show that the timeline applies it. **Accepted limit:** child calls seen after the restart have no held launch, so they show inline. |
| pocketd and agy both gone (`left == Cut`) | As for any call left running: `loaded()` → `interrupt()`. No new handling. |
| A later turn messages an old sub-agent | Its id is still mapped in `s.children`, so its calls land under the old card, which already says done. Re-opening the card is **out of scope**. |

### 7. Flip `SubAgents`

In `providers/antigravity.go`, set `Chat{…, SubAgents: true}`, and regenerate the golden `internal/proto/testdata/golden/server/agent_providers.json` with `go test ./internal/proto/ -update`. The only change is `"subAgents": true` for antigravity.
- Desktop `agents/src/event.rs:419` checks only `providers[0]`, which is Claude.
- iOS `CommandsTests` checks the caps used with no pocketd, which don't come from the golden.
- Neither client's render reads the flag.

## Tests

All new tests go in `internal/acp/session_test.go`, using `start(t, opts{provider: "antigravity", …})`. In the sink, a child line is `"<parent>>…"`.

1. **Changed on purpose.** `TestAnAntigravitySubAgentShowsAsATaskThatEndsAsItLaunchesWithNoReport` becomes `…ShowsAsATaskThatEndsWithItsTurn`. The lines of today's no-child turn are unchanged, because `tool_end:sa1:true` still comes right before `result`. Add a child call after the launch's `completed` so the hold shows:
   ```
   tool_start:sa1:task:task:Test Fixer: Fix the flaky test
   sa1>tool_start:c1:call_1:shell:shell:echo ok
   tool_end:c1:call_1:true:ok
   tool_end:sa1:true
   result:true:
   ```
2. **`TestAntigravitySubAgentsLaunchedTogetherKeepTheirOwnCallsByRole`.**
   - Setup: `brain` points at a temp dir with descriptors for `kb` (role Beta) and `ka` (role Alpha).
   - Wire order: launch `A` (Alpha), launch `B` (Beta), both completed, then `kb:` calls **before** `ka:` calls, which is the live order.
   - Expect: `B>…kb:…` and `A>…ka:…`. Then both cards end before `result`, A first.
3. **`…WithNoDescriptorFallBackToLaunchOrder`.** Same as test 2 with no files. The first child seen goes under `A`. This pins the fallback.
4. **`…AsksForItsSubAgentsCallUnderItsCard`.**
   - A child `tool_call` (`kind:"execute"`, command `rm x`), then `a.ask("1","session/request_permission", …)` with the prefixed id.
   - Expect: `asked(t, b)` gives `ToolName:"shell"` and the command, and the row line is `sa1>tool_start:…`.
   - Answer allow; `a.replied("1")` selects `allow`.
5. **`…StoppedEndsItsSubAgentsInterrupted`.** A turn that hangs, then `s.Cancel()`. Expect `tool_end:c1:call_1:false:Interrupted` and `tool_end:sa1:false:Interrupted`, both before `result:false:interrupted`.
   - `interrupt()` walks a map, so don't pin their order.
   - `tool_end` carries no parent, as in the existing `tool_end:kid:c1:…` lines.
6. **`…ReplayedEndAsTheHistoryTells`.** With `saved:"s1"`, the load replays a completed launch. Expect `tool_end` right after its `tool_start`, and no interrupted `result`.
7. **`…FailedLaunchEndsAtOnce`.** A launch that comes `failed` ends before any later frame.
8. **`…OfATurnAPocketdBeforeThisOneStartedEndWithIt`** (`left: Running`). Set `s.Showing` to return one running task item `sa1`. When the stray reply comes, expect `tool_end:sa1:true` before `result`.

`TestACallShowsByItsKind` is unchanged.

**e2e** (`e2e/fakeacp` and `e2e/acp_chat_test.go`):
- `fakeacp` `launch()`, built as agy:
  - after the launch's `completed`, send a child `tool_call` with id `<kid>:call_…` that runs `echo <task>`, then its `completed` with output;
  - then `say("delegated …")`.
  - Update the header comment's `delegate` line.
- Rename `TestAChatAntigravityShowsItsSubAgentAsATaskThatEndsAsItLaunches` to `…WithItsCallsUnderIt`. Expect `withSubAgents` to give:
  ```
  user delegate ls
  tool task ok
  > tool shell ok ls
  assistant echo: delegated ls
  result
  ```
  `withSubAgents` (`e2e/pi_chat_test.go:83`) already marks a child line with `> `.
- The fake writes no descriptor. With a single launch, the file and the fallback pick the same card, so writing it would prove nothing. The e2e covers nesting and the end at turn end; the unit tests cover the mapping.
- The home-derived `brain` default is covered only by the live run in Verify.
- Keep the permission case unit-only. The e2e path for asks is already covered for other agents.

## Verify

From `packages/pocketd`:

```
gofmt -l .
go vet ./...
go run honnef.co/go/tools/cmd/staticcheck@v0.8.2 ./...
go test ./internal/acp/ ./internal/proto/ ./e2e/ -run 'Antigravity|ACP'
go test ./...
```

From `packages/desktop`, `cargo test -p agents`, because the golden it reads changed. No view code changes, so no screen capture is needed. One live run against agy 1.5.16 with two sub-agents, on Gemini 3.8 Flash, should confirm that each card nests its own `run_command` and ends with the turn.

## In-flight work

- **`fix/antigravity-followups`** (`request.go`, timeline): this plan changes neither. It only relies on `permission()` finding the call in `s.tools`.
- **The steering plan** (`session.go` turn handling): this plan touches `finish()` with one call, and does not touch `send()`, `next()` or `wake()`.

Both branches sit at `edfa18bc` with clean trees today, so there is no diff to reconcile yet.

## Out of scope

- Child text and a report on the card: agy sends none.
- A brief under the card (a `user` item with the prompt, as Hermes's `show()` adds).
- Naming the sub-agent on the permission card (a protocol field).
- Re-opening a card when a later turn messages its sub-agent.
- Polling descriptors for `state`: it never changes from `ALIVE`.
- agy's `manage_subagents`, `schedule` and `update_steps` calls, which stay generic rows.
