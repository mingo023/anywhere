# 22 — Antigravity (`agy acp`) parity: questions, subagents, steering, folders, quota

Date: 2026-11-25. Worktree `research-antigravity-acp-gaps` @ df4735fd (= `origin/main`).

Question: what would it take to match a competitor's Antigravity support? The competitor ships six things: native question cards, subagents in the transcript, steering a running turn, extra folders for multi-folder projects, account quota in Settings, and native approvals. Antigravity was not installed for this research, so nothing was tested live.

Sources and evidence tiers, strongest first:
- **[B]** The `agy` 1.5.16 darwin-arm64 binary, fetched from the installer's manifest (sha512 checked) and read with `strings` only, never run. Its ACP server is Go package `jetski/cli/acp/server`, and its symbol names are cited as `[B sym]`.
- **[CL]** The CLI's own changelog, which is embedded in the binary. `[CL 1.5.0]` means an entry under that version's heading.
- **[C]** The competitor's open-source adapter code, which has the wire shapes they rely on.
- **[CT]** The competitor's test fixtures. These are their mocks, not captured traffic.
- A zero-hit string grep is weak evidence: `session/set_model` has 0 hits even though `handleSessionSetModel` exists.
- `P path:L` is a location in this repo. `pd/` is `packages/pocketd/internal/` and `dk/` is `packages/desktop/crates/`.
- **LIVE** marks a claim that needs a capture from a real `agy acp` before anyone builds on it.

## 1. TL;DR

| # | Feature | Anywhere today | Gap size | Where the work lands |
|---|---|---|---|---|
| 1 | Questions | Wrong. agy's choice questions probably arrive as permission requests, which get auto-answered under auto and full access (a likely bug, pending LIVE check 3). Form questions are never offered to it. | S–M | pocketd only |
| 2 | Subagents | A launch shows as a generic "other" tool row | S (card) / L (child progress) | pocketd, plus a provider flag |
| 3 | Steering | Follow-ups queue until the turn ends | M | pocketd only. **LIVE** first. |
| 4 | Extra folders | Projects have a single folder | L | product-wide prerequisite |
| 5 | Quota | None; Antigravity has no account row | M | pocketd, plus hiding "Add account" on desktop and iOS (extra pools would need protocol work) |
| 6 | Approvals | Done | — | — |

The headline is a probable bug. Under auto and full access, Anywhere silently picks the first choice of an Antigravity decision question (§2.1). This rests on the competitor's code and fixtures, not on a capture (LIVE check 3).

## 2. Features

### 2.1 Native questions (feature 1)

**Wire.** agy asks questions over two paths:

1. **Form elicitation, `elicitation/create`.** agy uses this when the client advertises `clientCapabilities.elicitation.form`.
   - **[B]** `[B sym]` `askQuestion`, `askQuestionWithForm`, `askQuestionInChat`, `presentAskedQuestionInChat`, `elicitQuestionAnswers`.
   - **[B]** Strings: `"elicitation/create for session %s (%s) step %d ended without an answer"` and `"elicitation/create rejected for session %s … asking in c[hat]"`. The tags `json:"elicitation,omitempty"` and `json:"form,omitempty"` both exist, but `strings` output can't show which struct holds them. Without the capability, or after a rejection, agy writes the question into the chat as plain text.
   - **[C]** The competitor sends `clientCapabilities: { elicitation: { form: {} } }` and gives the reason: "Antigravity asks clarifying questions as ACP form elicitations when the client supports them, and as plain chat text otherwise."
   - **[CT]** Request shape:
     ```json
     {"mode":"form","sessionId":"…","message":"The agent has a question.",
      "requestedSchema":{"type":"object","properties":{
        "q1":{"type":"string","title":"Which fruit should I use?","oneOf":[{"const":"1","title":"Apple"},{"const":"2","title":"Pear"}]},
        "q1_other":{"type":"string","title":"Other","description":"Optional free-form answer to: …"}}}}
     ```
     Questions are numbered `q1`…`qN`. A multi-select question is `type:"array"` whose `items` hold `enum` or `anyOf`. The optional `qN_other` field (title `"Other"`, not in `required`) takes a free-text answer.
   - **[CT]** Reply: `{"action":"accept","content":{"q1":"2"}}` sends the option's `const`, not its title. A custom answer is `{"q1_other":"Peach"}` with no `q1`. A skip is `{"action":"decline"}`.
   - Confidence: high on the method name and capability ([B]); medium on the schema details ([CT]).
2. **`session/request_permission` with an `interaction_…` tool call id.** agy sends a single-choice decision this way even in full access.
   - **[C]** `params.toolCall.toolCallId.startsWith("interaction_")`. The question text is `toolCall.title`, and each choice is an option (`optionId`, `name`).
   - **[CT]** The fixture gives every option `kind: "allow_once"`, so a choice is indistinguishable from an approval except by the id prefix.
   - Reply: `{"outcome":{"outcome":"selected","optionId":<chosen>}}`. A skip is `cancelled`.
   - The binary has no literal `interaction_` id prefix or format string; its hits are proto names like `interaction_id`. Evidence is [C]/[CT] only, so confidence is medium-low. The prefix and the option `kind` values are **LIVE** (check 3).

**Anywhere today.**
- `pd/acp/request.go:35-48` answers only `session/request_permission`, `cursor/ask_question` and `x.ai/ask_user_question`. An `elicitation/create` request gets "Method not found" (`request.go:26`), but agy never sends one, because the Antigravity flavor advertises no `elicitation` capability (`pd/acp/flavor.go:80`, and the caps are merged into initialize at `pd/acp/session.go:323-328`). Form questions therefore show up as plain assistant text.
- An `interaction_*` request goes through `permission()` (`request.go:51`):
  - The fixture's `toolCall` has no `kind`, so `request.go:72` makes it `other`.
  - In auto or full access, `auto()` (`flavor.go:172-179`, called at `request.go:82`) returns `pick(options, "allow_once", "allow")`, which is the **first choice**. The agent's question is answered without the user ever seeing it. **This is a bug** if the prefix and the `allow_once` kinds hold.
  - In ask access, or in edits access (where `flavor.go:173` returns `""` for `other`), the user gets an Allow/Deny card (`request.go:86-104`) instead of the choices. That is wrong too: Allow takes the first choice.
- The question card already exists and accepts typed answers. `question()` (`request.go:215-238`) shows Claude's AskUserQuestion card. The desktop wizard keeps typed text per page (`dk/pocket/src/chat/question.rs:36`), the iOS wizard has a free-text field (`packages/ios/Packages/AnywhereKit/Sources/FeatureChat/QuestionWizard.swift:56`), and `broker.Answer.Answers` is a `map[string]string` keyed by question text (`pd/broker/broker.go:31`). So `qN_other` needs **no protocol, desktop or iOS change**.

**Implementation.** pocketd only.
1. `flavor.go:80`: add `"elicitation": {"form": {}}` to Antigravity's `caps`.
2. `request.go` `answerer`: add `case "elicitation/create"`. Parse `requestedSchema.properties` into `[]question`:
   - each option's ID is its `const` and its label is its `title`;
   - `type:"array"` sets `Multi`;
   - the `qN_other` companion is dropped from the list.

   Then reuse `s.question()` with a new `elicitationAnswers(qs, answers)` that does three things:
   - maps each picked label back to its `const`;
   - sends a typed answer that matches no label as `qN_other`;
   - sends `{"action":"decline"}` when there are no answers.
3. `permission()`: before `auto()`, branch on `strings.HasPrefix(asked.ID, "interaction_")` and route the request to `s.question()` with one single-choice question. Its options are `{ID: optionId, Label: name}` and the reply is `selected(id, optionId)`. Answers come back keyed by label (`question.picked`, `request.go:203-211`), and the fixture has two options both named "Continue". When labels repeat, add the `optionId` to the label to tell them apart, or reply `cancelled` if the answer stays ambiguous.
4. Tests in `pd/acp/session_test.go` beside the Cursor and Grok question tests:
   - an elicitation round trip, with a custom answer and a decline;
   - an `interaction_` request that is **not** auto-answered under `full`.

   Add `elicitation/create` to `packages/pocketd/e2e/fakeacp` if an e2e test is wanted.

### 2.2 Subagents in the transcript (feature 2)

**Wire.**
- **[B]** The tool is `invoke_subagent`, with 98 hits in the binary. `start_subagent` has none, but the competitor still matches it as a legacy name, so the absence is weak evidence.
- **[B]** Its schema tag reads `Subagents json:"Subagents" jsonschema:"required" … "Array of subagents to invoke…"`. The prompt text uses `TypeName` (e.g. `'DeepCoder'`) and `Workspace='inherit'`.
- **[C]** Each `tool_call` or `tool_call_update` carries a **non-standard top-level `name`** field (`"invoke_subagent"`). `rawInput.Subagents[]` holds `{Role, Prompt}`. Other agy quirks the competitor normalises:
  - `name` is `run_command` with `rawInput.CommandLine`;
  - `toolSummary` is a one-line description of the call;
  - eager MCP tools are named `mcp_<server>_<tool>`, and lazy ones go through `call_mcp_tool` with `ServerName` and `ToolName`.
- **Child progress.** `[B sym]` has `watchKnownSubagents`, `sendChildStepUpdatesForGen` and `childStepPromptsClient`, which suggests agy may send child steps over ACP. The competitor does not rely on that. It reads agy's on-disk state instead:
  - descriptors at `~/.gemini/antigravity-cli/brain/<sessionId>/.system_generated/subagents/<childId>.json`, with `subagentDescriptor.role` and `state`;
  - `state` is `SUBAGENT_STATE_ALIVE` or `SUBAGENT_STATE_KILLED` (both [B] strings);
  - inbox messages under `…/messages/*.json`.

  **LIVE:** check whether `session/update` ever carries child steps, and under which `sessionId` or `_meta`.
- **[CL 1.5.15]** agy now keeps `session/prompt` open "across background wake-ups until [subagents/background tasks] finish". On ≥1.5.15 a turn should not end while children run, so the competitor's file-polling "background wait" is mostly for older versions.

**Anywhere today.**
- `toolCall` already parses `name` (`pd/acp/tool.go:16`).
- `describe()` (`tool.go:216-223`) recognises a sub-agent call only by kind `task`, `Name == "subagent"`, `subagent_type`, `variant == "Task"`, `_toolName == "task"`, or Hermes's `delegate_task`. An agy launch therefore falls through to `Kind: "other"`, named by its title (`tool.go:243-248`).
- `providers/antigravity.go:10` leaves `Chat.SubAgents` unset (false), so clients don't expect sub-agent items.
- The machinery for child sessions (`children`, `kids` at `session.go:117-122`) exists for Cursor, Grok and Hermes.

**Implementation.**
1. **Launch card (S).** In `describe()`, treat `t.Name == "invoke_subagent" || "start_subagent"` as `task`. Build `Description` by joining each `Subagents[i].Prompt`, prefixed by its `Role` when there are several. Use `Role` (or `TypeName`) as the name. Add a `tool_test.go` case.
2. **Flag.** Set `SubAgents: true` only once child items flow. Clients read it from the provider list pocketd sends, so the flag changes the golden `pd/proto/testdata/golden/server/agent_providers.json`.
3. **Child progress (L).** After a live capture: if agy streams child steps, route them into `kids` like Cursor's. If it doesn't, decide whether to read `~/.gemini/…` from pocketd, which is fragile and undocumented internal state. Recommendation: ship 1 alone and defer this.

### 2.3 Steering a running turn (feature 3)

**Wire.**
- **[CL 1.5.0]** "`agy acp` runs the CLI as an Agent Client Protocol (ACP) server … with … **steering a running turn by sending another prompt**, … `session/resume` restoring a session without replaying its history…"
- **[CL 1.5.1]** "cancelling a prompt no longer also stops the next prompt sent right after it; `session/update` notifications no longer arrive after the `session/prompt` reply for that prompt."
- **[C]** The wire is a plain second `session/prompt` on the same `sessionId` while the first is still outstanding. No capability flag exists; the competitor gates on `initialize.agentInfo.name` matching `/antigravity|\bagy\b/i` and `agentInfo.version` ≥ 1.5.0. It treats unparseable or prerelease versions as unsupported and falls back to the queue.
- **[CT]** The second prompt's own reply may be `stopReason:"cancelled"`. The competitor keeps the user-visible turn open until both replies are in, and settles with the steer's outcome when the first ended `end_turn`.
- **Unknown, the top LIVE item:** what the **first** `session/prompt` returns once a second arrives. It could end early with `end_turn`, end with `cancelled`, or stay open until the merged work ends. Confidence in the mechanism is high ([CL]); confidence in the reply ordering is low.

**Anywhere today.**
- `Session.Prompt` (`pd/acp/session.go:762-774`) appends to `s.queue` (`session.go:99`) whenever `s.running`. `next()` (`session.go:814-825`) sends the queued text after `finish()`. `Chat.SendWhileWorking: true` (`providers/antigravity.go:10`) only means "queue".
- `finish()` treats `stopReason == "cancelled"` as **interrupted** (`session.go:839`, `853-854`). Steering that makes agy cancel the first prompt would show a false "interrupted" turn.
- Codex steering is the model to follow: `pd/daemon/codex_chat.go:298`, tested by `packages/pocketd/e2e/codex_chat_test.go:95` (`TestAPromptSentMidTurnSteersTheChatCodexTurn`). It produces one turn holding both user messages and a single done.
- `pd/orchhost/orchhost.go:74` sets `CanSteer` only for claude and codex.

**Implementation.** pocketd only, after a live capture.
1. Add `steer bool` to `flavor` (Antigravity only). In `open()`, read `agentInfo.{name,version}` from the initialize result and set a session flag when the version is ≥ 1.5.0.
2. In `Prompt()`, when `s.running && s.steers && !s.compacting && !s.stopping`:
   - apply the `user` event;
   - send `session/prompt` through `deliver` with **no** `s.turn++`;
   - count it in an outstanding counter.

   Change `finish` so the turn ends only when the counter reaches 0. A `cancelled` reply from a steered prompt (or from the first prompt, if the capture shows that) counts as OK, not as interrupted.
3. Make `CanSteer` true for this flavor (`orchhost.go:74`) so orchestrator wakes steer.
4. Tests:
   - in `session_test.go`, a fake that holds the first prompt and answers the second;
   - an e2e test in the Codex test's shape, using `fakeacp`.

### 2.4 Extra folders for multi-folder projects (feature 4)

**Wire.** This is not an ACP message; it goes on the command line.
- **[C]** `agy acp --add-dir <dir> [--add-dir <dir>…]`, on both new and resumed sessions. The competitor's comment: "ACP workspace roots are process-scoped." The primary folder stays the process `cwd` and the `session/new` `cwd`.
- **[B]** `--add-dir <path>` and `--add-dir=` are flag strings; `[B sym]` has `sessionWorkspaceDirs`, `collectProjectDirs` and `(*serverTurns).AddWorkspaceDir`. Confidence: high.
- **[B]** Two struct tags read `additionalDirectories,omitempty` (camelCase and snake_case variants, next to `client_capabilities`). This hints that `session/new` may also accept `additionalDirectories`. **Unverified**: it is a bare tag, and the competitor chose the flag.

**Anywhere today.** There is no multi-folder project concept:
- `proto.Project` has one `Path` (`pd/proto/messages.go:441-446`);
- `state.Launch` has no folders (`pd/state/state.go:21-28`);
- `acpArgv` appends only `acp` for Antigravity (`pd/launch/agents.go:20-31`).

**Implementation.** This is blocked on a product feature, not on Antigravity.
1. **Prerequisite:** extra folders on a project, owned by pocketd. That means a protocol field on `Project` in `packages/protocol`, its goldens, a desktop Settings › Projects editor, and an iOS mirror.
2. **Then the Antigravity part, small.** `acpArgv` takes the session's extra folders and appends `--add-dir <abs>` for each, for Antigravity only. Resume goes through the same spawn path. A test sits beside `acpArgv`.

Claude Code has `--add-dir` too; check the other CLIs. Design the prerequisite for every provider, not as Antigravity work.

### 2.5 Account quota and reset times (feature 5)

**Wire.** This is not ACP; it is the CLI in print mode.
- **[C]** The command:
  ```
  agy --print /usage --output-format json --print-timeout 10s --settings '{"disableSlashCommands":false}' --disable-slash-commands=false --log-file <tmp>/agy.log
  ```
  It runs in an empty temp cwd with `AGY_CLI_DISABLE_AUTO_UPDATE=true`, after a `agy --version` check (≥ 1.1.11).
- **[CL]** History of the command:
  - "Added non-interactive answers for the read-only slash commands in print mode, so `-p "/usage"`, `/quota`, `/credits`…"
  - "piped and `--json` output still use UTC timestamps."
  - With `GEMINI_API_KEY`, "in `-p` runs they exit with code 2."
- **[B]** The strings `print-timeout`, `output-format`, `disable-slash-commands`, `remaining_fraction`, `reset_time` and `AGY_CLI_DISABLE_AUTO_UPDATE` are all present.
- **[C]/[CT]** Response:
  ```json
  {"status":"SUCCESS","num_turns":0,"command":{"name":"usage","data":{"groups":[
    {"name":"Gemini Models","buckets":[{"id":"gemini-5h","window":"5h","remaining_fraction":0,"reset_time":"2026-11-23T18:00:00Z"},
                                       {"id":"gemini-weekly","window":"weekly","remaining_fraction":0.75}]},
    {"name":"Claude and GPT models","buckets":[{"id":"3p-weekly","window":"weekly","remaining_fraction":1},{"id":"3p-5h","window":"5h","disabled":true,…}]}]}}}
  ```
  Bucket ids are [CT] only (`gemini-5h` has no [B] hit). Confidence: medium.
- **Guards.**
  - Accept the output only when `status=="SUCCESS" && num_turns==0 && command.name=="usage"`. Older CLIs send `/usage` to the *model* as a prompt.
  - Stop on the stderr line `"Authentication required. Please visit the URL to log in"`. Report "needs sign-in", never let it open a browser, and never store stdout or stderr, which can hold login URLs.

**Anywhere today.**
- `accounts.logins` holds only Claude and Codex (`pd/accounts/accounts.go:46`). `Providers` is built from it (`accounts.go:71-79`), so Antigravity has **no account row**, and `Refresh` never fetches for it.
- `Usage` has fixed `Session` and `Week` windows (`pd/accounts/usage.go:20-25`), mapped to `proto.Account.fiveHour` and `.week` (`pd/workspace/accounts.go:21-23`; `packages/protocol/src/workspace.ts:7-18`).
- The desktop renders those two meters (`dk/pocket/src/settings/accounts.rs:63-66`), and the iOS app reads them in `packages/ios/Packages/AnywhereKit/Sources/Wire/Workspace.swift`.

**Implementation.**
1. Add an `antigravity` login in `pd/accounts/antigravity.go`. It is a built-in account only: agy keeps its credential in the OS keyring, so pocketd can't switch accounts.
   - `status` is "signed in unless the last fetch said otherwise";
   - `usage` runs the command above with `exec.CommandContext`, with a timeout of about 12 s;
   - `envKey`, `shared` and `history` stay empty. An empty `envKey` makes `Store.Env` and `Store.Of` no-ops (`pd/accounts/accounts.go:160-185`).
   - Adding the login to `logins` also puts Antigravity in `Providers` (`accounts.go:71-79`). That lets `Store.Add` (`accounts.go:242-245`) create managed accounts it can't use, so refuse providers that have no `envKey`. Also check that the account pickers in new-session and Settings › Projects don't offer a choice.
2. Map the bucket with `window=="5h"` in the "Gemini" group to `Session` and the `weekly` one to `Week`, with `percent = (1-remaining_fraction)*100` and `ResetsAt` taken from `reset_time`. This needs **no protocol change**.
3. **Optional:** extra quota pools (third-party models, per-model) need a `pools: [{label, pct, resetsAt}]` field on `Account`. That is a protocol change with goldens, plus desktop and iOS rendering. Defer it.
4. Cost control: a process spawn per refresh is heavy. `UsageTTL` is 1 minute (`pd/workspace/accounts.go:49`), so give Antigravity a longer floor (≥ 5 min) and a single-flight guard.
5. The desktop accounts page loops over every provider id and gives each one an "Add account" action (`dk/pocket/src/settings/accounts.rs:128-135`). Antigravity's new row would get one too, so hide it for providers without managed accounts. That needs a capability bit, or the desktop can key off the provider id. Make the same check on iOS.

### 2.6 Native approvals (feature 6)

Already done.
- Antigravity's `session/request_permission` goes through `permission()` (`pd/acp/request.go:51-105`), with the access → mode mapping in `flavor.go:79-85` and the `auto()` policy that allows once only.
- **[CL 1.5.4]** says agy now sends a new permission request when a tool needs a second approval. Anywhere handles each request independently.
- The only gap is the `interaction_` misrouting in §2.1.

## 3. LIVE checks, before building §2.2 (child progress) and §2.3

Run each against a signed-in `agy acp` ≥ 1.5.15, logging the raw JSON-RPC traffic:
1. Steering: send prompt A (a long task), then prompt B mid-turn, and record the reply order and `stopReason` of A and of B.
2. Ask a question in ask mode, with and without the `elicitation.form` capability, and record the exact `elicitation/create` params.
3. Trigger a decision question and record the `interaction_*` request's option `kind`s.
4. Run `invoke_subagent` and check whether child steps arrive as `session/update`, and if so, how they are tagged.
5. Check whether `session/new` honours `additionalDirectories` as an alternative to `--add-dir`.
6. Capture `agy -p /usage --output-format json` from a real account to pin the bucket ids.

## 4. PRs, by value and risk

1. **Antigravity questions** (pocketd only, S–M).
   - Route `interaction_*` permissions to the question card, which fixes the silent auto-answer.
   - Advertise `elicitation.form` and answer `elicitation/create`.
   - Unit tests in `pd/acp`.
   - Low risk; the bug fix alone justifies it.
2. **Subagent launch card** (pocketd only, S): `invoke_subagent` → `task` in `describe()`. Leave `SubAgents` off.
3. **Antigravity quota on the built-in account** (pocketd, plus a small desktop and iOS tweak, M).
   - `accounts/antigravity.go` with the `/usage` print-mode probe;
   - the 5h and weekly Gemini pools map onto the existing fields, so there is no protocol change;
   - longer TTL and a version gate;
   - hide "Add account" for Antigravity (§2.5 step 5).
4. **Steering** (pocketd only, M; after LIVE check 1).
   - A version-gated second `session/prompt`;
   - one turn across both replies;
   - `CanSteer`;
   - an e2e test in the Codex test's shape.
5. **Extra folders on projects** (cross-cutting, L): the protocol field, desktop and iOS editors, and `--add-dir` (or each CLI's equivalent) at spawn. A separate design note first.
6. **Subagent child progress** (L; after LIVE check 4): stream into `kids` and flip `SubAgents`, or decide against reading `~/.gemini`.
7. **Extra quota pools** (protocol, desktop and iOS, S–M): only if users ask.
