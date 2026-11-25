# Antigravity subagent launch card

**Goal:** An Antigravity `invoke_subagent` call shows as the sub-agent card (`task`) on desktop and iOS, not as a generic "other" row. pocketd only. Child progress stays out of scope, and `Chat.SubAgents` stays off.

**Read first:**
- `docs/orchestrators/research/22-antigravity-parity.md` §2.2 and §4 item 2.
- `packages/pocketd/internal/acp/tool.go:216` `describe()`, `:251` `delegation()` (Hermes's brief, the closest precedent), `:294` `subAgent()`, `:328` `report()`.
- `packages/pocketd/internal/acp/session.go:1408` `show()`: it sets `t.task = name == "task"`. `:1354-1406` `tool()`: a task call ends through the sub-agent path.
- `packages/pocketd/internal/acp/session_test.go:361` `TestACallShowsByItsKind`, and `:1001` for a session-level task test.

---

## Wire shape and confidence

agy sends a standard ACP `tool_call` / `tool_call_update`, plus a **non-standard top-level `name`**:

```json
{"sessionUpdate":"tool_call","toolCallId":"…","name":"invoke_subagent","title":"…","kind":"…","status":"pending",
 "rawInput":{"Subagents":[{"Role":"…","Prompt":"…"}]}}
```

| Fact | Evidence | Confidence |
|---|---|---|
| The tool name `invoke_subagent` | [B] 98 hits in the binary | high |
| `name` as a top-level field of `tool_call` | [C] | medium |
| `rawInput.Subagents` is a required array | [B] schema tag `Subagents json:"Subagents" jsonschema:"required"` | high |
| Each item has `Role` and `Prompt` | [C] | medium |
| Each item may have `TypeName` (e.g. `'DeepCoder'`) | [B] prompt text only | low |
| `start_subagent` is a legacy name | [C] matches it; [B] has 0 hits | low |
| `kind`, `title`, and the shape of the output | unknown | — |

There is no live `agy` here, so nothing in the table has been captured (research §3 LIVE check 4).

## Design

1. **Match by `name` only.** A call whose `Name` is `invoke_subagent` or `start_subagent` is a task. Check it **first** in `describe()`, before the `t.Kind == "task"` branch. If agy also sends `kind: "task"`, that branch would otherwise take it and read `description`, which agy's input doesn't have. Keep `start_subagent`: it costs one string, and [C] still sees it.
2. **Parse with a typed struct, not `t.input()`.** `input()` reads only strings and string lists, so it can't see `Subagents`. Unmarshal `RawInput` into:
   ```
   struct { Subagents []struct{ Role, TypeName, Prompt string } }
   ```
   with no JSON tags. `encoding/json` matches keys case-insensitively, so `Subagents`, `subagents`, `Prompt` and `prompt` all match. This guards the medium-confidence casing for free. A `type_name` key would not match, which only loses the role prefix.
3. **Everything goes in `Description`.** The first return value must stay `"task"`, because `show()` derives `t.task` from it. A task detail marshals only `description`, `model` and `tokens` (`proto/proto.go:80`), so there is no field for an agent name.
   - First drop entries whose `Prompt` is empty, then count what is left.
   - **One subagent:** `Description` is its `Prompt` and nothing else. This is the research doc's rule. The prompt's first line names the card better than a role does, and it matches Claude's Task card, which doesn't show the subagent type either. As a result, a single subagent's `Role` is not shown anywhere.
   - **Several subagents:** one entry per subagent, joined with `\n`. Each entry is `<label>: <Prompt>`, where `<label>` is `cmp.Or(Role, TypeName)`. A subagent with neither shows its `Prompt` alone. Keep multi-line prompts as they are. The card's name is the first line of the first entry either way, and the row folds everything onto one line.
   - `Model` stays empty. The args carry no model.
4. **Degrade chain.** The detail is computed once, in `show()`. If the call shows before its input has arrived, the card keeps whatever was known at that moment.
   - The prompts, as in step 3. If none are found (no `Subagents`, the wrong type, or every `Prompt` empty), use:
   - `t.Title`. If that is empty too, use:
   - `""`. Both clients then name the card "Subagent".

   If the real `name` differs from both strings, nothing matches, and the call stays the "other" row it is today. No mismatch in the args or the name leaves the chat worse than it is now. For the one lifecycle case that would, see Lifecycle.
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
  - `subAgent()` sniffs the output for an OpenCode `metadata.sessionID`, a Hermes `dispatched` status, or a Grok `subagent_id: ` line. Plain text matches none of them, so `sub.session == ""`, and pocketd records `tool_end` with `report()`, which is the trimmed output.
  - Risk: if agy's real output ever contained one of those markers, pocketd would try to `adopt()` (call `session/load` on) an id agy doesn't own. The session test below pins "no `session/load`" for a plain output.
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
## Lifecycle (unverified, LIVE check 4)

The table above lists the shape of the output as unknown, so when the call ends is unknown too. The plan has to hold under all three cases.

- **The call ends when the subagents finish.** This is the likely case: [CL 1.5.15] says agy keeps `session/prompt` open until its subagents finish. The card shows a dotted "starting" ring for the whole run, then "done" and the report.
- **The call ends at launch.** The descriptors' ALIVE and KILLED states and the background wake-ups hint at this. The card flips to "done" at once, and its report is whatever agy returns. That may be a launch acknowledgement, or raw JSON through `output()`'s fallback to `string(RawOutput)`. This looks odd but is harmless, and it is no worse than the "other" row's output today.
- **agy never sends `completed`.** `finish()` (`session.go:828`) closes open calls only on a stop (`interrupt()`). After a normal `end_turn` the call stays running. As `other`, it is just a stale spinner. As a task, both clients count it as a sub-agent working in the background: desktop `chat/src/rows.rs` `in_background()` feeds the agent dock, and iOS does the same in `ChatModel/Rows.swift` `inBackground()`. This is the one degrade the change makes worse. It is unlikely, because ACP agents end every call they start, and [CL 1.5.15] keeps the turn open until the children end. Don't add a guard for it now. If the LIVE check shows it, end open Antigravity task calls in `finish()` in a follow-up.

**Known limitation, accepted.** The full prompt shows only as the row's one-line description. The detail view takes its brief from child items, and this PR adds none. Child progress (research §4 item 6) fills both the brief and the steps. A Hermes-style `user` event with `ParentToolUseID` would add a brief; it is **out of scope** here.

## Tests

All tests go in `packages/pocketd/internal/acp/session_test.go`, beside the existing ones. There is no `tool_test.go`, so don't create one.

1. **Rows in `TestACallShowsByItsKind`** (`:361`). Each row's expected value is `name:kind:` followed by `Command+Path+Query+Description`.
   - `"an Antigravity sub-agent by its prompt"`:
     `toolCall{Name: "invoke_subagent", Title: "Subagent", RawInput: {"Subagents":[{"Role":"DeepCoder","Prompt":"Fix the flaky test"}]}}`
     → `task:task:Fix the flaky test`
   - `"several Antigravity sub-agents each by its role"`: two entries, one with `Role: "Reviewer"` and one with only `TypeName: "Tester"`, prompts `"Read the diff"` and `"Run the tests"`
     → `task:task:Reviewer: Read the diff\nTester: Run the tests`
   - `"an Antigravity sub-agent told in lower case"`: `Name: "start_subagent"`, input `{"subagents":[{"prompt":"Look around"}]}`
     → `task:task:Look around`. This row pins both the legacy name and case-insensitive keys.
   - `"an Antigravity sub-agent whose input it can't read by its title"`: `Name: "invoke_subagent"`, `Title: "Fix the test"`, input `{"Subagents":"oops"}`
     → `task:task:Fix the test`
   - `"an Antigravity sub-agent with a kind of task by its prompt"`: as the first row but with `Kind: "task"`
     → `task:task:Fix the flaky test`. This pins the order of the checks in `describe()`.
2. **Session test**, `TestAnAntigravitySubAgentShowsAsATaskThatEndsWithWhatItReported`, with `start(t, opts{provider: "antigravity", answer: script(`{"sessionId":"s1"}`, says(…))})`:
   - The fake sends `{"sessionUpdate":"tool_call","toolCallId":"sa1","name":"invoke_subagent","title":"invoke_subagent","status":"pending","rawInput":{"Subagents":[{"Role":"DeepCoder","Prompt":"Fix the flaky test"}]}}`.
   - It then sends `{"sessionUpdate":"tool_call_update","toolCallId":"sa1","status":"completed","content":[{"type":"content","content":{"type":"text","text":"Fixed it"}}]}`.
   - After `s.Prompt("go")`, the sink lines after `working` include, in order, `tool_start:sa1:task:task:Fix the flaky test` and `tool_end:sa1:true:Fixed it`.
   - Wait for `ended:false` first, because `adopt()` loads in a goroutine. Then `a.called("session/load", 0)` is nil.

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
