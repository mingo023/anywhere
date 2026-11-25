# Antigravity subagent launch card

**Goal:** An Antigravity `invoke_subagent` call shows as the sub-agent card (`task`) on desktop and iOS, not as a generic "other" row. pocketd only. Child progress stays out of scope, and `Chat.SubAgents` stays off.

**Read first:**
- `docs/orchestrators/research/22-antigravity-parity.md` §2.2 and §4 item 2.
- `packages/pocketd/internal/acp/tool.go:216` `describe()`, `:251` `delegation()` (Hermes's brief, the closest precedent), `:294` `subAgent()`, `:328` `report()`.
- `packages/pocketd/internal/acp/session.go:1408` `show()`: it sets `t.task = name == "task"`. `:1354-1406` `tool()`: a task call ends through the sub-agent path.
- `packages/pocketd/internal/acp/session_test.go:361` `TestACallShowsByItsKind`, and `:1001` for a session-level task test.

---

## Wire shape (live, agy 1.5.16)

Captured in `docs/orchestrators/research/23-antigravity-live-checks.md` §4. agy sends a standard ACP `tool_call` / `tool_call_update` with a **non-standard top-level `name`**. `rawInput` is flat, one sub-agent per call:

```json
{"sessionUpdate":"tool_call","toolCallId":"call_8494…","name":"invoke_subagent","status":"in_progress","kind":"other","title":"Run invoke_subagent",
 "rawInput":{"Model":"inherit","Prompt":"Run the shell command `echo child-step-ok` and report its output.","Role":"Command Runner","TypeName":"self"}}
{"sessionUpdate":"tool_call_update","toolCallId":"call_8494…","name":"invoke_subagent","status":"completed"}
```

- There is **no `Subagents[]` array**. Several sub-agents come as several calls, about 30 ms apart.
- The title is always `"Run invoke_subagent"`, so it never names the card.
- The call goes `completed` about 25 ms after launch, with no `content` and no `rawOutput`, while the child is still working.
- `start_subagent` was never seen; it is kept as a name to match ([C] only).

The guessed shape this plan first held (`rawInput.Subagents[]` of `{Role, Prompt}`, from the [B] schema tag) was wrong for 1.5.16.

## Design

1. **Match by `name` only.** A call whose `Name` is `invoke_subagent` or `start_subagent` is a task. Check it **first** in `describe()`, before the `t.Kind == "task"` branch. If agy also sends `kind: "task"`, that branch would otherwise take it and read `description`, which agy's input doesn't have. Keep `start_subagent`: it costs one string, and [C] still sees it.
2. **Parse with a typed struct, not `t.input()`.** Unmarshal the flat `RawInput` into `struct{ Role, TypeName, Prompt string }` with no JSON tags. `encoding/json` matches keys case-insensitively, so `Prompt` and `prompt` both match. `Model` (`"inherit"`) is not read.
3. **Everything goes in `Description`.** The first return value must stay `"task"`, because `show()` derives `t.task` from it. A task detail marshals only `description`, `model` and `tokens` (`proto/proto.go:80`), so there is no field for an agent name.
   - `Description` is `<label>: <Prompt>`, where `<label>` is `cmp.Or(Role, TypeName)`: `"Command Runner: Run the shell command…"`.
   - With no label it is the `Prompt` alone; with no `Prompt` it is the label alone.
   - `Model` stays empty.
4. **No title fallback.** The title is always `"Run invoke_subagent"`, which says nothing. A call that shows before its input has arrived has an empty description, and both clients name the card "Subagent".

   If the real `name` differs from both strings, nothing matches, and the call stays the "other" row it was before.
5. **Shape of the code.** Add one helper method next to `delegation()`:
   ```
   func (t *toolCall) launch() (string, bool)
   ```
   It returns the description, and `ok` when the `Name` matches. `describe()` calls it first and returns `"task", proto.ToolDetail{Kind: "task", Description: brief}`.
6. **Comments to update.**
   - The `Name` field comment `// OpenCode's` (`tool.go:16`) becomes `// OpenCode's and Antigravity's`.
   - The `describe()` comment at `tool.go:217` adds "Antigravity's invoke_subagent".
   - Write a doc comment on `launch()` that states the arg shape.

## What else this touches (verified, no change needed)

- **Permission requests.** `request.go:86` also calls `describe()`. `auto()` (`flavor.go:172`) decides on the ACP `kind`, not on `describe()`, so what gets auto-answered under each access level is unchanged. The only change: when the user is asked, the card is a task card instead of "other / invoke_subagent". On iOS that reads "Start agent <description>" (`Reading.swift:180`), and on desktop "Wants to start <description>" (`agents/src/ask.rs:45`). This is an improvement, and needs no test.
- **End of the call.** A task call ends through `session.go:1372-1406`:
  - `flushKid` does nothing, because there are no kids.
  - `subAgent()` sniffs the output for an OpenCode `metadata.sessionID`, a Hermes `dispatched` status, or a Grok `subagent_id: ` line. agy's call ends with no output, which matches none of them, so `sub.session == ""`, and pocketd records `tool_end` with no report.
  - Risk: if agy's real output ever contained one of those markers, pocketd would try to `adopt()` (call `session/load` on) an id agy doesn't own. The session test below pins "no `session/load`".
  - `spawning("")` runs only for Grok, Cursor and OpenCode's child updates (`subagent_spawned`, `_meta.child`). agy sends neither, so a task with no `description` input can't be adopted by mistake.
- **`SubAgents` flag.** Neither client's render path reads it:
  - desktop: only `chat/src/adapter.rs:27` plumbs it;
  - iOS: only `ChatModel/Adapter.swift:24` and `Wire/Launch.swift` plumb it.

  `providers/antigravity.go` stays as it is, and `proto/testdata/golden/server/agent_providers.json` is **unchanged**.
- **Desktop.** `chat/src/timeline.rs:266` maps `task` to `ToolKind::Agent`.
  - `pocket/src/chat/sub_agent.rs`: `name()` takes the description's first non-empty line, cut at 55 chars, with "Subagent" when empty; `tool_row.rs:89` shows the description on one line.
  - While the call runs with no child steps, `mark()` is `Starting`, and `meta` reads "starting". Once it ends, `meta` reads "done" or the duration, and `report()` is the call's output.
  - `agent_detail.rs` hides the brief and steps when the transcript is empty, and shows the report.

  **No change.**
- **iOS.** `ChatModel/Timeline.swift:248` maps `task` to `.agent`.
  - `FeatureChat/SubAgentRow.swift` and `Agent.name`, `mark` and `meta` in `ChatModel/Reading.swift:250-300` mirror the desktop.
  - `SubAgentScreen.swift` skips an empty brief and shows the report once the call is done.
  - The `background-agents-done` fixture (`Fixtures/SampleChat.swift:8`) already renders task cards that have no children.

  **No change.**
## Lifecycle (live)

**The call ends at launch.** It goes `completed` about 25 ms after it starts, with no output, so the card flips to "done" while the child is still working and has no report. The child's own tool calls arrive on the parent session with a `"<childConversationId>:"` prefix on their `toolCallId`, and show as ordinary rows; grouping them under the card is child progress, out of scope here. The turn itself stays open until the children finish.

**Known limitation, accepted.** The full prompt shows only as the row's one-line description. The detail view takes its brief from child items, and this PR adds none. Child progress (research §4 item 6) fills both the brief and the steps. A Hermes-style `user` event with `ParentToolUseID` would add a brief; it is **out of scope** here.

## Tests

All tests go in `packages/pocketd/internal/acp/session_test.go`, beside the existing ones. There is no `tool_test.go`, so don't create one.

1. **Rows in `TestACallShowsByItsKind`**. Each row's expected value is `name:kind:` followed by `Command+Path+Query+Description`.
   - `"an Antigravity sub-agent by its role and prompt"`: `Name: "invoke_subagent"`, `Kind: "other"`, `Title: "Run invoke_subagent"`, input `{"Model":"inherit","Prompt":"Fix the flaky test","Role":"Test Fixer","TypeName":"self"}` → `task:task:Test Fixer: Fix the flaky test`
   - `"an Antigravity sub-agent with no role by its type"`: input `{"Prompt":"Look around","TypeName":"self"}` → `task:task:self: Look around`
   - `"an Antigravity sub-agent told in lower case"`: `Name: "start_subagent"`, input `{"prompt":"Look around"}` → `task:task:Look around`. This row pins both the legacy name and case-insensitive keys.
   - `"an Antigravity sub-agent with a kind of task by its role"`: `Kind: "task"` → `task:task:Test Fixer: Fix the flaky test`. This pins the order of the checks in `describe()`.
   - `"an Antigravity sub-agent before its input not by its title"`: no input, `Title: "Run invoke_subagent"` → `task:task:`
2. **Session test**, `TestAnAntigravitySubAgentShowsAsATaskThatEndsAsItLaunchesWithNoReport`: the fake sends the live `tool_call` above (`toolCallId:"sa1"`), then `{"sessionUpdate":"tool_call_update","toolCallId":"sa1","name":"invoke_subagent","status":"completed"}`. After `s.Prompt("go")` the sink reads `tool_start:sa1:task:task:Test Fixer: Fix the flaky test`, then `tool_end:sa1:true`, and `a.called("session/load", 0)` is nil.

## Verify

From `packages/pocketd`:

```
gofmt -l .
go vet ./...
go run honnef.co/go/tools/cmd/staticcheck@v0.8.2 ./...
go test ./internal/acp/ ./internal/proto/
```

The proto golden tests must pass with no golden changed. Desktop and iOS are untouched, so there is no screen capture to compare.

## Out of scope

- Child progress, and flipping `SubAgents` (research §4 item 6, after LIVE check 4).
- Normalising agy's other quirks: `run_command` / `CommandLine`, `toolSummary`, and the `mcp_<server>_<tool>` and `call_mcp_tool` names.
- Showing the prompt as a brief under the card.
