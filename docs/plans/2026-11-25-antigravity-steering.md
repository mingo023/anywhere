# Antigravity steering

**Goal:** While an Antigravity turn runs, a prompt the user sends goes to agy at once as a second `session/prompt`, and agy folds it into the running turn. Today it waits in pocketd's ACP queue until the turn ends. The chat shows one turn that holds both user messages and ends with one result. The change is in pocketd only: no client, protocol or golden-file changes.

**Read first:**
- `docs/orchestrators/research/22-antigravity-parity.md` §2.3 (the design sketch) and `docs/orchestrators/research/23-antigravity-live-checks.md` §1 (the live wire, which corrects the sketch).
- `packages/pocketd/internal/acp/session.go`: `Prompt` :762, `start` :777, `send` :782, `deliver` :804, `next` :814, `finish` :828, `Cancel` :964, `open` :322 (initialize :327), `asks` :108, `chunk` :1290, `flush` :1318.
- Codex steering, the model to follow: `packages/pocketd/e2e/codex_chat_test.go:95` `TestAPromptSentMidTurnSteersTheChatCodexTurn`, and `packages/pocketd/internal/daemon/codex_chat.go:298` `prompt`. codex folds a mid-turn `turn/start` into its turn by itself.
- `packages/pocketd/e2e/fakeacp/main.go`: `initialize` :230, `session/prompt` :311, `turn` :385, `wait` :454, `request` :559.

---

## Live facts (agy 1.5.16, doc 23 §1)

| Fact | What the design does about it |
|---|---|
| A second `session/prompt` B, sent while A is open, makes A reply `end_turn` within ~10 ms. Usage is sometimes absent. | A's reply does not end the turn while B is out. It is the sign that agy took B. |
| B's reply closes the merged turn (`end_turn`, with usage). | B's reply ends the turn. |
| A third prompt C, sent while B is the open one, makes B reply `end_turn` (no usage) 2 ms after C arrives. C's reply closes the turn, and the reply covers all three (`DONE-A\nBANANA\nCHERRY`). Checked for this plan on 2026-11-25 (agy 1.5.16; log in `/tmp/agy-steer3/logs/three.jsonl`, not in the repo). | Each new steer is confirmed by the reply of the prompt before it, so chains work the same way. |
| B is applied after the current step; a long generation runs to its end. | Nothing extra. The client's "Steered" row already reads "takes it at its next step". |
| B, sent while a permission or question is pending, is refused at once with `-32603`. A is unaffected. | Don't steer while a card is open, and put a refused B back in the queue. |
| B, followed by `session/cancel`, makes B reply `cancelled`. | The turn ends interrupted, as for any stop. |
| agy never echoes B as a `user_message_chunk`. | pocketd records B's user message itself. |
| `initialize` returns `agentInfo: {"name":"Antigravity ACP Server","version":"1.5.16"}`. | This is the gate. |

## How the mid-turn send flows today (no change needed outside `acp`)

- **Desktop** (`pocket/src/chat/composer.rs` `chat_send`) and **iOS** (`ChatStore.swift` `send`/`deliver`) decide `steer = busy && !stopping && !queue` for any provider whose `Chat.SendWhileWorking` is set. They send `agent.prompt` without `queue`, and show the send as `Delivery::Steered` until its user item shows in the timeline. Antigravity already has `SendWhileWorking: true` (`providers/antigravity.go:10`).
- **wsserver** (`wsserver/orchestrator.go:168`) turns this into `message.send` with mode `steer`. `orch/decide.go` emits `EffTurnStart`, which calls `pipeDriver.Prompt` (`daemon/pipe_wire.go`), which calls `acp.Session.Prompt`.
- `Session.Prompt` puts it on `s.queue` while `s.running`. **That queue is the only thing this plan changes.**
- **On Stop,** `pipeDriver.Interrupt` drops `s.queue` through `Cancel`. The client puts back any Steered send that never echoed. That covers a steer agy refused and a steer that was dropped at a stop (below).

**Capability flags:** none.
- `sendWhileWorking` is already true.
- `sendNow` stays false. It means "take it before the current step ends", which agy doesn't do.
- No `proto` or `providers.Chat` field changes, so no protocol golden files are touched.
- `orchhost.go:74` `CanSteer` stays claude/codex only. It is fixed per provider, but this gate depends on the version each session reports. An older agy would hold a wake in `s.queue`, and a Stop drops that queue with no client to put the wake back. Wakes keep queuing in the orchestrator. Turning `CanSteer` on is a follow-up, once it can be per session.

## Design (all in `packages/pocketd/internal/acp/session.go`)

### 1. Gate

- In `open()`, after `initialize`, set a new field `steers bool` from a free function `steers(init json.RawMessage) bool`. It returns true when `agentInfo.name == "Antigravity ACP Server"` and `agentInfo.version` is at least 1.5.0.
- The version check is a small `atLeast(version string, major, minor int) bool`. It splits on `.` and reads the first two parts with `strconv.Atoi`. Anything unparseable or a prerelease (a `-` in the string) is false, so the session queues as today. pocketd has no semver helper.
- Gate on the name, not on a `flavor` field. The agent identifies itself, so nothing else needs to know.

### 2. When to steer

`Prompt` gets a new case before the queueing case:

```
case s.running && s.steerable(): s.steer(text)
```

`steerable()` is true only when all of these hold:
- `s.steers`;
- `!s.stopping`;
- `!s.adopted`: a turn a previous pocketd started ends through `Stray()`, which has no reply goroutine to count;
- `len(s.asks) == 0`: no permission or question card is open, since agy would refuse;
- `len(s.queue) == 0`: a prompt already waiting must not be overtaken;
- `len(s.steered) == 0`: the last steer is confirmed, which takes ~10 ms. So at most one steer is unconfirmed, and a refused one goes back to the front of the queue in send order. A prompt sent inside that window queues behind it.

Otherwise the prompt queues exactly as today. When a card closes, prompts that queued behind it are **not** sent as steers. They run after the turn, as today. This keeps the change out of `ask()` in `request.go`, which the in-flight `fix/antigravity-followups` work touches. That branch has no commits beyond main yet; this plan avoids `request.go` by design.

**Compaction:** nothing to do. The Antigravity flavor has no `compactMethod`/`compactPrompt`, so `Compact()` refuses it. `running` and `compacting` never hold together, and `Prompt`'s `!s.ready || s.compacting` case still queues first.

### 3. One turn across several replies

- **New fields:**
  - `out int`: the turn's prompts agy has not answered yet.
  - `steered []string`: steers sent but not yet known taken, so not yet shown.
- **Split `send()`.**
  - `send(text)` keeps the turn start: `running`, `began`, `turn++`, `Working()`.
  - It then calls a new `prompt(text string, steer bool)`. That method does `s.out++`, makes its own `sent` channel and stores it in `s.sent`, and runs the existing goroutine: `deliver`, then lock, then the guard `s.running && s.turn == turn`.
  - Under that guard the goroutine calls `s.promptReplied(text, steer, res, err)` instead of `finish`. Don't name it `answered`, which is already a field (:111).
- **`steer(text)`** appends to `s.steered` and calls `prompt(text, true)`. It does not do `turn++` or `Working()`.
- **Stop ordering:** the steer must install its own `s.sent`. `Cancel()` waits on `s.sent` before it sends `session/cancel`. Without that, a cancel could overtake B, and agy ≥ 1.5.1 would then run B as a fresh turn after the Stop.
- **`promptReplied(text, steer, res, err)`** decrements `s.out`, then takes the first case that matches:
  1. **Refused:** `steer && err != nil && s.out > 0`, meaning an error on a steer while an earlier prompt is still out. agy refused it; `-32603` is generic, so the code itself is not checked.
     - Clear `s.steered`; it held only this steer.
     - Unless `s.stopping`, put it at the **front** of `s.queue`. Everything queued behind it was sent later. It runs through `next()` → `start()` after the turn, which records its user message then.
     - The turn goes on.
  2. **Early reply:** `s.out > 0`. A reply arrived while a later prompt is still out: the previous prompt's early `end_turn`, the sign agy took the steer. Call `showSteered()`; the turn goes on. An error from A here is not shown; B's reply decides how the turn ends.
  3. **Last reply:** `showSteered()`, then `finish(res, err)`, as today. The turn's usage is this last reply's, as for any turn. A's early reply, often without usage, is dropped.
- **`showSteered()`** calls `s.flush()` first, so the reply being streamed closes as its own message and does not straddle the user bubble. It then applies `timeline.Event{Kind: "user", Text: t}` for each entry of `s.steered`, and clears the list.
  - **Why record on A's reply and not when B is sent:** a steer recorded at send and then refused would show twice, once now and again when `start()` runs it. Clients settle a Steered send on its echo, so a refused steer that already echoed would be lost on Stop. A's reply comes ~10 ms after B, so the delay can't be seen.
- **`finish()`** also sets `s.out = 0` and `s.steered = nil`. The `StopWait` timer in `Cancel()` force-finishes with replies still out, and their late answers already fail the guard: `running` is false, or `turn` moved on.
  - A steer never shown at a forced finish is dropped. Its client row never echoed, so the client puts it back in its queue, as for any prompt dropped at a stop.
- **Stop:** `Cancel()` is unchanged.
  - B replies `cancelled`. A has already replied, or replies too.
  - `s.stopping` makes the last reply's `finish` end the turn `interrupted` and `Stopped()`, exactly once.

### 4. Comments

- Update `queue`'s comment (:98) to say a steering agent's mid-turn prompts skip it.
- Update `Prompt`'s doc (:761) to say "or steers it into the turn under way, where the agent takes that".

No other files change in `pd/acp`. `request.go` and `flavor.go` stay untouched.

## Fake agent (`packages/pocketd/e2e/fakeacp/main.go`)

As `agy` only:
- **`initialize`** answers `agentInfo: {"name":"Antigravity ACP Server","version":"1.5.16"}`. Other names keep `{a.name, "0"}`. Nothing in `packages/pocketd` reads an ACP `agentInfo` today, and no Antigravity e2e test sends mid-turn, so existing tests are unaffected.
- **`session/prompt` while a turn runs** steers instead of refusing `busy`:
  - `note("steer: %s")`;
  - reply the open prompt's id `{"stopReason":"end_turn"}` with no usage;
  - make B's id the one the turn answers at its end;
  - add B's text to a `steered` list.

  `turn()` then says `echo: <text>` for each steered text after its current step, before its final reply. Keep the reply id and the list on `agent`, under `mu`. `end()` clears both. The fake has no refusal branch: pocketd never steers with a card open, and unit test 4 covers the refusal.
- Update the header's prompt list with one line about steering.

## Tests

**Unit**, in `packages/pocketd/internal/acp/session_test.go`, beside `TestAPromptSentMidTurnWaitsForTheTurnToEnd` (:207). Answer `initialize` with an `agyInit` const holding the live `agentInfo` (the `script()` helper answers it `{}` today). The fake's `serve()` already answers calls off its loop, so it can hold A while it takes B.

1. `TestAPromptSentMidAntigravityTurnSteersItAndTheTurnEndsOnce`
   - Setup: A is held; B arrives, A replies `end_turn`, and B replies `end_turn` after `re: two`.
   - Expect: `session/prompt` is called twice before any `result`.
   - Expect these lines, in order: `user:one, working, …, user:two, assistant_draft:re: two, …, result:true:, ended:false`.
   - Expect exactly one `result` and one `ended`.
2. `TestAPromptSentMidTurnQueuesForAnAntigravityThatCantSteer`
   - A table of agentInfo values: `1.4.9`; `1.5.0-rc.1`; `""`; and a different name with version `1.5.16`.
   - Expect: one `session/prompt` until A's turn ends, then B runs as its own turn (as test :207 does).
3. `TestAPromptSentWhileAntigravityAsksWaitsForTheTurnToEnd`
   - Setup: A's turn asks a permission (as `asking()` :784 sets up) that stays unanswered; B is sent.
   - Expect: B is not sent. After the answer and A's end, B runs as its own turn.
4. `TestASteerAntigravityRefusesRunsAfterTheTurnAndShowsOnce`
   - Setup: the fake refuses B with `-32603`. This is the race the card check can't close; pocketd answers Full-access permissions itself, so one can be pending on the wire.
   - Expect: A ends `result:true`, then B runs as its own turn.
   - Expect: `user:two` appears exactly once, right before B's turn.
5. `TestAStopRightAfterASteerReachesTheAgentAfterItAndEndsTheOneTurnInterrupted`
   - Setup: A is held. Call `Cancel()` right after the steering `Prompt`, as test :272 does, so the `s.sent` swap is what orders them. The fake answers A and B `cancelled` once the cancel arrives.
   - Expect: the call order `session/prompt, session/prompt, session/cancel`, using the check from `TestAStopSentRightAfterAPromptReachesTheAgentAfterIt` (:272).
   - Expect: `result:false:interrupted` and `stopped`, once each.

**e2e**, in `packages/pocketd/e2e/acp_chat_test.go`, next to `TestAPromptSentMidTurnWaitsForTheChatHermesTurnToEnd` (:237), in the Codex test's shape:

```
TestAPromptSentMidTurnSteersTheChatAntigravityTurn
  h, phone, term, id := chatACP(t, "antigravity", nil)
  phone.Prompt(id, "slow first"); phone.WaitStatus(id, "working")
  phone.Prompt(id, "second"); h.WaitScreen(term, "steer: second")
  phone.WaitAll("one turn with both", streamOf(id, "user", "second"),
      streamOf(id, "assistant", "echo: first"), streamOf(id, "assistant", "echo: second"), <one done>)
  the screen holds no "busy"
```

The Hermes test stays as the regression for agents that don't steer.

## Checks before commit

From `packages/pocketd`:
- `gofmt -l .`, `go vet ./...` and `staticcheck ./...`;
- `go test ./internal/acp/`, then `go test ./e2e/ -run 'Antigravity|MidTurn'`, then `go test ./...`.

No desktop, iOS or protocol changes, so the Rust, Swift and golden suites are untouched.

**Optional live check:** re-run doc 23's `s1_steer.py` against agy 1.5.16 through a local pocketd chat. Don't change agy's settings.

## Out of scope

- Steering when a card closes; turning on `CanSteer` for orchestrator wakes; Stop's handling of `s.queue`.
- Any change to how the client words the Steer button. agy applies a steer at the next step, which `Delivery::Steered` already says.
