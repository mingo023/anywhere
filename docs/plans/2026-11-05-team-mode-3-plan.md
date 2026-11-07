# Team Mode 3: Team Plans Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** `team: true` on a prompt (`agent.prompt`, or the first prompt of an `agent.create` spec) makes the session a team's **lead**: pocketd sends it the team rules, reads the plan block its turn ends with, validates it, and publishes `team.update` (`planning` → `proposed`, with one repair turn, or `failed`); `team.list` lists every team, kept in `state/team.db`.

**Architecture:** A new package `internal/team` holds every decision as pure functions (`Wrap`, `Parse`, `Validate`, `ValidateKept`, `Order`, `Startable`, `Allowed`, `Branch`, `Reply`, `Planned`, `Repair`), a SQLite `Store` (one JSON row per lead), and `Teams`, the **one team engine** (plan 4 adds its running verbs to this same type, not a second one). `Teams.change` is the only writer: under one mutex it reads a team, applies a decision, and saves and publishes `team.update`. Hooks that run under an agent's lock (`Teams.Status`, from `Registry.OnStatus`) only queue an event; `Teams.Run` handles the queue in order on one goroutine: a turn end goes to `Teams.Ended`, which reads the turn off the timeline (retrying while the transcript lags) and applies `Planned`; a lead closing while planning fails its team. `Teams.Prompt` refuses (`ErrBusy`, wire code `team_busy`) while the lead's team is running, in review or merging. `wsserver` routes `team.list` and `agent.prompt` with `team`, `launch` routes a created session's first prompt, and `serve.go` wires it all and starts `Run`. A small leaf package `internal/teamrules` strips the rules from user timeline items, so the chat and the title show only what the user typed. While a team is `proposed`, `change` marks its lead's agent as awaiting the user (`Agent.SetAwaiting`), so pocketd reports the lead `needsYou` everywhere. pocketd advertises `team.v1`. Wire types land in Go (`internal/proto`), TypeScript (`packages/protocol`) and Swift (`packages/ios/.../Wire`, whose golden tests decode every pocketd golden).

**Toolset** (Go commands from `packages/pocketd`; others from the repo root):
- One Go package: `go test ./internal/team`, `go test ./internal/proto`, `go test ./internal/peer`, `go test ./internal/launch`, `go test ./internal/agent`, `go test ./internal/daemon`
- Rewrite server goldens: `go test ./internal/proto -run TestServerGolden -update`
- E2E (builds pocketd + fakeclaude): `go test ./e2e -run Team -count=1`. If any build fails with `ghostty/vt.h: file not found`, run `scripts/build-ghostty.sh` once from the repo root.
- Everything Go: `go vet ./... && go test ./...`; formatting: `gofmt -l .` must print nothing
- TS protocol: `pnpm --filter @pocket/protocol test`
- Swift wire: `cd packages/ios/Packages/AnywhereKit && PATH="$HOME/.swiftly/bin:$PATH" swift test --filter GoldenTests`

**Read first:**
- `docs/plans/2026-11-05-team-mode.md`: the decisions and contracts this plan implements (types, verbs, rules text, repair prompt, `internal/team` API). Don't change a contract here; see "Contract changes needed" at the end.
- `CONTEXT.md`: the glossary. A team's work is never a "run" (a Run is one firing of an automation).
- `packages/pocketd/internal/skills/store.go`: the SQLite pattern the team store copies.
- `packages/pocketd/internal/automation/scheduler.go` (`observe`, `maxSummaryPolls`) and `watch.go`: how pocketd reads an agent's last text after its status says the turn ended, retrying because the transcript lags the status.
- `packages/pocketd/internal/proto/golden_test.go`, `automation_test.go`: goldens and decode tests.
- `packages/pocketd/e2e/chat_test.go`, `e2e/phone_test.go`, `e2e/fakeclaude/chat.go`: chat sessions against the fake claude, which answers every plain prompt with `echo: <prompt>`.

## Decisions taken in this plan

- **The plan is read from the timeline.** The turn's text is every top-level `assistant` item after the last top-level `user` item, joined with `\n`; its end is the `result` item after that user item (`OK: false` is a failed turn, an interrupt included). pocketd remembers the timeline's `maxSeq` when it sends a team or repair prompt and reads only items after it, so an older turn is never taken for the answer.
- **Lag retry:** up to 5 reads, 1s apart (`Teams.Poll`), until the turn's `result` is in the timeline. After that, the text so far is used and the turn counts as failed only if the agent's summary says so.
- **Parse** takes the block whose `</anywhere_team_plan>` comes last, from the last `<anywhere_team_plan>` before it, so a stray mention of the tag earlier doesn't break it. It reads only `id`, `title`, `agent`, `files`, `after`, `allow`, `instructions`; anything else the lead writes (`model`, `state`, …) is dropped.
- **Validate** reports every problem at once, joined with `"; "`, because the repair prompt carries them all. Beyond the index's list it also rejects ids that can't name a branch (`^[A-Za-z0-9][A-Za-z0-9_-]{0,31}$`) and empty titles or instructions. Two files overlap when they are the same cleaned path or one is inside the other's folder (`src/` and `src/a.go`). The lead's plan needs 2 to 4 tasks; **`ValidateKept`** applies the same rules with 1 to 4, for the tasks kept at Start (plan 4).
- **The lead needs you while its plan waits.** Each `Teams.change` that leaves a team `proposed` marks the lead's agent as awaiting (`Agent.SetAwaiting(true)`), and one that moves it to any other state but `planning` clears it. pocketd then reports the lead `needsYou` while it is idle, so the sidebar, banners, the Dock badge and the phone all see it. The lead's next turn (`Agent.Working`) also clears it; that is how a re-plan clears it, because clearing at `planning` would read as a turn end (`needsYou` → `idle`) and `Ended` would take the half-started turn for the plan. The upgrade handoff and the restart file keep the agent's own turn status (`Agent.Turn`), never the wait.
- **Order** places, again and again, the first task in plan order whose `after` are all placed. An `after` naming no task in the list holds nothing back; tasks a cycle holds back come last, in plan order.
- **Startable** counts `running` and `needsYou` as busy, returns `waiting` tasks in `Order` whose `after` are all `done`, up to `Parallel` busy plus started. An `after` naming a task no longer in the team (the user removed it at **Start**) counts as done.
- **Allowed** trims the command and each prefix, refuses any command containing `;`, `&`, `|`, a backtick, `$(`, `<`, `>`, `\n` or `\r`, and then needs a non-blank prefix followed by a space or the end. A tab after the prefix doesn't count as a space.
- **A team prompt replaces the lead's team** when it is `planning`, `proposed`, `merged`, `conflict` or `failed`. While it is `running`, `review` or `merging`, the prompt is refused with `team.ErrBusy`, which goes out as code `team_busy` (`proto.CodeTeamBusy`), and nothing is sent to the lead.
- **A failed planning turn** fails the team with `The lead's turn failed`. A team prompt that can't be sent fails it with the send's error. **A lead that closes while its team is `planning`** fails it with `the lead closed while planning`.
- **One engine, one writer.** Every team change goes through `Teams.change` (one mutex; `Store.Put`, then `team.update`). Hooks only queue; `Teams.Run` handles the queue in order on one goroutine. Plan 4 adds event kinds and verbs to `Teams`.
- **The rules stay before the user's text** (`Wrap` as the index has it; the PR 2 e2e tests rely on the echo's last block being the user's). User timeline items lose a leading `<anywhere_team>…</anywhere_team>` block through `teamrules.Strip`, applied where Claude's transcript and Codex's `userMessage` items become timeline events, so the chat shows only the user's text and `firstLine` titles the session from it.
- **`agent.create` with `team`** needs `chat` and a `prompt`; otherwise `invalid_spec`, "A team starts from a chat session's first prompt".
- **Cap `team.v1`** (`proto.CapTeam`) is in `ServerCaps`, so a client offers Team only to a pocketd that speaks it. `team.update` still goes to every client through the hub, like `agent.update`; the desktop and iOS both ignore message types they don't know.

---

## PR 1: Team wire types and the `internal/team` rules

**Scope:** `proto.Team`/`proto.TeamTask`, `team.list` (verb and reply), `team.update`, `team` on `agent.prompt` and `LaunchSpec`, and the `team_busy` code, mirrored in `packages/protocol` and the iOS `Wire` package with goldens; `internal/team`'s pure functions `Wrap`, `Parse`, `Validate`, `ValidateKept`, `Order`, `Startable`, `Allowed`, `Branch`; and `internal/teamrules.Strip` on Claude and Codex user items. **Inert:** nothing routes `team.list`, and `team: true` is decoded but ignored, until PR 2, which is also where pocketd starts advertising `team.v1`. `ValidateKept`, `Order`, `Startable`, `Allowed` and `Branch` stay unreferenced until plan 4.
**Depends on:** nothing
**Done when:** `go test ./...`, `pnpm --filter @pocket/protocol test` and the Swift `GoldenTests` pass.

### Task 1.1: Go wire types, verbs and goldens

**Files:**
- Create: `packages/pocketd/internal/proto/team.go`
- Create: `packages/pocketd/internal/proto/team_test.go`
- Create: `packages/pocketd/internal/proto/testdata/golden/client/team_list.json`, `agent_prompt_team.json`, `agent_create_team.json`
- Generated: `packages/pocketd/internal/proto/testdata/golden/server/team_update.json`, `team_list.json`, `team_list_empty.json`
- Modify: `packages/pocketd/internal/proto/golden_test.go` (`serverGolden` map, ~line 32)
- Modify: `packages/pocketd/internal/proto/messages.go` (`ClientMessage` ~line 18, `DecodeClient` ~lines 175-180)
- Modify: `packages/pocketd/internal/proto/launch.go` (`LaunchSpec` ~line 45, `decodeSpec` ~line 171)

**Context:** `DecodeClient` mirrors the TS schema: one `case` per verb, `get` for a required field, `optionalString` etc. for optional ones; `fields.get` refuses `null`. `decodeSpec` reads the spec with `strictObject`, whose key list is an allowlist, so `team` must be added there or every spec carrying it is malformed. `orEmpty[T]` already exists in `internal/proto/gh.go`. Server goldens are written from the `serverGolden` map by `-update`; client goldens are hand-written files that `TestClientGolden` decodes. Go's `encoding/json` escapes `<` and `>`, so golden strings here avoid them. Plan 1 adds `parent` to the same `decodeSpec` allowlist; if both land, keep both keys.

**Step 1: Write the failing tests**

Create the three client goldens (one line each, no trailing spaces):

`testdata/golden/client/team_list.json`:
```json
{"type":"team.list","id":"tl1"}
```

`testdata/golden/client/agent_prompt_team.json`:
```json
{"type":"agent.prompt","id":"p2","agentId":"a1","text":"Add truncate and titleCase, each with its own test","team":true}
```

`testdata/golden/client/agent_create_team.json`:
```json
{"type":"agent.create","id":"c10","requestId":"r10","spec":{"project":"/Users/me/pocket","checkout":{"worktree":"/Users/me/pocket"},"provider":"claude","access":"ask","plan":false,"prompt":"Add truncate and titleCase","chat":true,"team":true}}
```

Add to the `serverGolden` map in `golden_test.go`, after `"local_list_empty"`:

```go
	"team_update": NewTeamUpdate(Team{Lead: "a1", State: "proposed", UpdatedAt: 1790000000000, Tasks: []TeamTask{
		{ID: "t1", Title: "Add truncate", Agent: "claude", Files: []string{"src/truncate.js", "test/truncate.test.js"}, After: []string{}, Allow: []string{"npm test"},
			Instructions: "Write truncate(text, n) and its test", State: "planned"},
		{ID: "t2", Title: "Export truncate", Agent: "codex", Model: "gpt-6.1-sol", Files: []string{"src/index.js"}, After: []string{"t1"},
			Instructions: "Export truncate from src/index.js", State: "planned"},
	}}),
	"team_list":       NewTeamList("tl1", []Team{{Lead: "a2", State: "failed", Error: "a plan needs 2 to 4 tasks, not 1", Repaired: true, UpdatedAt: 1790000000000}}),
	"team_list_empty": NewTeamList("tl1", nil),
```

Create `internal/proto/team_test.go`:

```go
package proto

import (
	"encoding/json"
	"testing"
)

func TestTheTeamGoldensDecodeToTheirFields(t *testing.T) {
	m, err := DecodeClient(golden(t, "client", "agent_prompt_team.json"))
	if err != nil || !m.Team || m.AgentID != "a1" || m.Text != "Add truncate and titleCase, each with its own test" {
		t.Fatalf("%+v %v", m, err)
	}
	if m, err = DecodeClient(golden(t, "client", "agent_prompt.json")); err != nil || m.Team {
		t.Fatalf("a prompt without team: %+v %v", m, err)
	}
	if m, err = DecodeClient(golden(t, "client", "agent_create_team.json")); err != nil || !m.Spec.Team || !m.Spec.Chat {
		t.Fatalf("%+v %v", m, err)
	}
	if m, err = DecodeClient(golden(t, "client", "team_list.json")); err != nil || m.Type != "team.list" || m.ID != "tl1" {
		t.Fatalf("%+v %v", m, err)
	}
}

func TestATeamFieldOffTheContractIsMalformed(t *testing.T) {
	for _, raw := range []string{
		`{"type":"agent.prompt","id":"1","agentId":"a","text":"x","team":"yes"}`,
		`{"type":"agent.prompt","id":"1","agentId":"a","text":"x","team":null}`,
		`{"type":"team.list"}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w"},"provider":"claude","access":"ask","plan":false,"team":"yes"}}`,
		`{"type":"agent.create","id":"1","requestId":"r","spec":{"project":"/p","checkout":{"worktree":"/w"},"provider":"claude","access":"ask","plan":false,"Team":true}}`,
	} {
		if _, err := DecodeClient([]byte(raw)); err != ErrMalformed {
			t.Errorf("%s: got %v, want ErrMalformed", raw, err)
		}
	}
}

func TestATeamGoesOutWithEveryListItsSchemaRequires(t *testing.T) {
	got, _ := json.Marshal(NewTeamUpdate(Team{Lead: "a1", State: "planning", Tasks: []TeamTask{{ID: "t1", State: "planned"}}}))
	want := `{"type":"team.update","team":{"lead":"a1","state":"planning","tasks":[{"id":"t1","title":"","agent":"","files":[],"after":[],"allow":[],"instructions":"","state":"planned"}],"updatedAt":0}}`
	if string(got) != want {
		t.Fatalf("got  %s\nwant %s", got, want)
	}
	got, _ = json.Marshal(NewTeamList("tl1", []Team{{Lead: "a1", State: "planning"}}))
	want = `{"type":"team.list","id":"tl1","teams":[{"lead":"a1","state":"planning","tasks":[],"updatedAt":0}]}`
	if string(got) != want {
		t.Fatalf("got  %s\nwant %s", got, want)
	}
}
```

What each proves: the goldens decode to the fields dispatch will read; `team` is optional, typed and never null; a team always goes out with `[]`, never `null`, for the lists the TS schema requires.

**Step 2: Run the tests to verify they fail**

Run: `cd packages/pocketd && go test ./internal/proto`
Expected: FAIL, build errors such as `undefined: NewTeamUpdate`, `undefined: Team`, `m.Team undefined`.

**Step 3: Write the implementation**

Create `internal/proto/team.go`:

```go
package proto

// CodeTeamBusy refuses a team prompt while the lead's team is running, in review or merging.
const CodeTeamBusy = "team_busy"

// Team is a lead's plan and how its tasks stand. pocketd keeps one per lead.
type Team struct {
	Lead      string     `json:"lead"`
	State     string     `json:"state"` // planning | proposed | running | review | merging | merged | conflict | failed
	Error     string     `json:"error,omitempty"`
	Tasks     []TeamTask `json:"tasks"`
	Ready     bool       `json:"ready,omitempty"`    // the lead said "Ready to merge"
	Conflict  []string   `json:"conflict,omitempty"` // files of the failed merge
	Repaired  bool       `json:"repaired,omitempty"` // the one repair turn was used
	UpdatedAt int64      `json:"updatedAt"`
}

// TeamTask is one task of a Team, done by one worker.
type TeamTask struct {
	ID           string   `json:"id"`
	Title        string   `json:"title"`
	Agent        string   `json:"agent"` // claude | codex
	Model        string   `json:"model,omitempty"`
	Files        []string `json:"files"`
	After        []string `json:"after"`
	Allow        []string `json:"allow"`
	Instructions string   `json:"instructions"`
	State        string   `json:"state"`            // planned | waiting | running | needsYou | done | failed | stopped | merged
	Worker       string   `json:"worker,omitempty"` // worker's agent id
	Branch       string   `json:"branch,omitempty"`
	Worktree     string   `json:"worktree,omitempty"`
	Summary      string   `json:"summary,omitempty"` // worker's last text
	Stat         string   `json:"stat,omitempty"`    // git diff --stat against its base
}

// TeamList answers team.list with every lead's team.
type TeamList struct {
	Type  string `json:"type"`
	ID    string `json:"id"`
	Teams []Team `json:"teams"`
}

func NewTeamList(id string, teams []Team) TeamList {
	out := make([]Team, len(teams))
	for i, t := range teams {
		out[i] = t.wire()
	}
	return TeamList{"team.list", id, out}
}

// TeamUpdate is a team after any change, sent to every client.
type TeamUpdate struct {
	Type string `json:"type"`
	Team Team   `json:"team"`
}

func NewTeamUpdate(t Team) TeamUpdate { return TeamUpdate{"team.update", t.wire()} }

// wire fills the lists the schema requires, which a team read back from JSON may hold as null.
func (t Team) wire() Team {
	tasks := make([]TeamTask, len(t.Tasks))
	for i, k := range t.Tasks {
		k.Files, k.After, k.Allow = orEmpty(k.Files), orEmpty(k.After), orEmpty(k.Allow)
		tasks[i] = k
	}
	t.Tasks = tasks
	return t
}
```

In `messages.go`, add to `ClientMessage` right after `Text string`:

```go
	// Team makes agent.prompt's agent plan Text as a team's lead.
	Team bool
```

In `DecodeClient`, add `team.list` to the verbs without fields:

```go
	case m.Type == "agent.list", m.Type == "pair.begin", m.Type == "project.list", m.Type == "agent.providers", m.Type == "team.list":
```

and replace the `agent.prompt` case with:

```go
	case m.Type == "agent.prompt":
		ok = get("agentId", &m.AgentID) && get("text", &m.Text) && (!f.has("team") || get("team", &m.Team))
```

In `launch.go`, add to `LaunchSpec` after `Chat`:

```go
	// Team makes the first prompt ask the agent for a team's plan.
	Team bool `json:"team,omitempty"`
```

and add `"team"` to the `decodeSpec` allowlist:

```go
	spec, ok := strictObject(raw, "project", "checkout", "provider", "model", "effort", "access", "plan", "prompt", "fork", "account", "chat", "team")
```

Run `gofmt -w internal/proto`.

**Step 4: Write the server goldens, then run the tests to verify they pass**

Run: `go test ./internal/proto -run TestServerGolden -update`
Then: `git status --short internal/proto/testdata`
Expected: only the three new server files (`team_update.json`, `team_list.json`, `team_list_empty.json`) plus the three client files you created. No existing golden changes.

`server/team_update.json` must read:

```json
{
  "type": "team.update",
  "team": {
    "lead": "a1",
    "state": "proposed",
    "tasks": [
      {
        "id": "t1",
        "title": "Add truncate",
        "agent": "claude",
        "files": [
          "src/truncate.js",
          "test/truncate.test.js"
        ],
        "after": [],
        "allow": [
          "npm test"
        ],
        "instructions": "Write truncate(text, n) and its test",
        "state": "planned"
      },
      {
        "id": "t2",
        "title": "Export truncate",
        "agent": "codex",
        "model": "gpt-6.1-sol",
        "files": [
          "src/index.js"
        ],
        "after": [
          "t1"
        ],
        "allow": [],
        "instructions": "Export truncate from src/index.js",
        "state": "planned"
      }
    ],
    "updatedAt": 1790000000000
  }
}
```

`server/team_list.json`:

```json
{
  "type": "team.list",
  "id": "tl1",
  "teams": [
    {
      "lead": "a2",
      "state": "failed",
      "error": "a plan needs 2 to 4 tasks, not 1",
      "tasks": [],
      "repaired": true,
      "updatedAt": 1790000000000
    }
  ]
}
```

`server/team_list_empty.json`:

```json
{
  "type": "team.list",
  "id": "tl1",
  "teams": []
}
```

Run: `go test ./internal/proto`
Expected: `ok  	pocketd/internal/proto`

`pnpm --filter @pocket/protocol test` and the Swift goldens now fail on the new files; Tasks 1.2 and 1.3 fix them, in this PR.

### Task 1.2: TypeScript mirror in `packages/protocol`

**Files:**
- Create: `packages/protocol/src/team.ts`
- Modify: `packages/protocol/src/messages.ts` (imports; `agent.prompt` struct ~line 57; end of `ClientMessage` ~line 166; end of `ServerMessage` ~line 266)
- Modify: `packages/protocol/src/constants.ts` (`CAP_TEAM`, after `CAP_SKILLS`)
- Modify: `packages/protocol/src/launch.ts` (`LaunchSpec` ~line 32)
- Modify: `packages/protocol/src/index.ts`
- Test: `packages/protocol/test/golden.test.mjs` (unchanged; it decodes every golden with `onExcessProperty: "error"`)

**Context:** Each area keeps its schemas in its own file and exports `XClient`/`XServer` arrays that `messages.ts` spreads into the unions (see `terminal.ts`). `ProviderId` (`"claude" | "codex"`) is in `launch.ts`. The test builds with `tsc -b` first, so a type error fails it too.

**Step 1: The failing test already exists**

Run (repo root): `pnpm --filter @pocket/protocol test`
Expected: FAIL with `# fail 6`: `server/team_update.json`, `server/team_list.json`, `server/team_list_empty.json` (no such type), `client/team_list.json` (no such type), `client/agent_prompt_team.json` and `client/agent_create_team.json` (`team` is an excess property).

**Step 2: Write the implementation**

Create `packages/protocol/src/team.ts`:

```ts
import { Schema } from "effect";
import { ProviderId } from "./launch.js";

export const TeamState = Schema.Literal("planning", "proposed", "running", "review", "merging", "merged", "conflict", "failed");
export type TeamState = typeof TeamState.Type;

export const TeamTaskState = Schema.Literal("planned", "waiting", "running", "needsYou", "done", "failed", "stopped", "merged");
export type TeamTaskState = typeof TeamTaskState.Type;

export const TeamTask = Schema.Struct({
  id: Schema.String,
  title: Schema.String,
  agent: ProviderId,
  model: Schema.optional(Schema.String),
  files: Schema.Array(Schema.String),
  after: Schema.Array(Schema.String),
  allow: Schema.Array(Schema.String),
  instructions: Schema.String,
  state: TeamTaskState,
  worker: Schema.optional(Schema.String),
  branch: Schema.optional(Schema.String),
  worktree: Schema.optional(Schema.String),
  summary: Schema.optional(Schema.String),
  stat: Schema.optional(Schema.String),
});
export type TeamTask = typeof TeamTask.Type;

export const Team = Schema.Struct({
  lead: Schema.String,
  state: TeamState,
  error: Schema.optional(Schema.String),
  tasks: Schema.Array(TeamTask),
  ready: Schema.optional(Schema.Boolean),
  conflict: Schema.optional(Schema.Array(Schema.String)),
  repaired: Schema.optional(Schema.Boolean),
  updatedAt: Schema.Number,
});
export type Team = typeof Team.Type;

export const TeamClient = [Schema.Struct({ type: Schema.Literal("team.list"), id: Schema.String })] as const;

export const TeamServer = [
  Schema.Struct({ type: Schema.Literal("team.list"), id: Schema.String, teams: Schema.Array(Team) }),
  Schema.Struct({ type: Schema.Literal("team.update"), team: Team }),
] as const;
```

In `messages.ts`, add the import after the `PushClient` import:

```ts
import { TeamClient, TeamServer } from "./team.js";
```

replace the `agent.prompt` struct with:

```ts
  Schema.Struct({
    type: Schema.Literal("agent.prompt"),
    id: Schema.String,
    agentId: Schema.String,
    text: Schema.String,
    team: Schema.optional(Schema.Boolean),
  }),
```

add `...TeamClient,` after `...PushClient,` in `ClientMessage`, and `...TeamServer,` after `...FsServer,` in `ServerMessage`.

In `constants.ts`, after `export const CAP_SKILLS = "skills.v1";` (pocketd advertises it from PR 2):

```ts
export const CAP_TEAM = "team.v1";
```

In `launch.ts`, add to `LaunchSpec` after `chat`:

```ts
  team: Schema.optional(Schema.Boolean),
```

In `index.ts`, add:

```ts
export * from "./team.js";
```

**Step 3: Run the test to verify it passes**

Run: `pnpm --filter @pocket/protocol test`
Expected: PASS, `# fail 0`.

### Task 1.3: Swift mirror in the iOS `Wire` package

**Files:**
- Create: `packages/ios/Packages/AnywhereKit/Sources/Wire/Team.swift`
- Modify: `packages/ios/Packages/AnywhereKit/Sources/Wire/ServerMessage.swift` (the enum cases, `init(from:)`, `encode(to:)`, `type`)
- Modify: `packages/ios/Packages/AnywhereKit/Sources/Wire/ClientMessage.swift` (the same four places)
- Modify: `packages/ios/Packages/AnywhereKit/Sources/Wire/Agents.swift` (`ClientMessage.AgentText` ~line 249)
- Modify: `packages/ios/Packages/AnywhereKit/Sources/Wire/Launch.swift` (`LaunchSpec` ~line 73)
- Test: `packages/ios/Packages/AnywhereKit/Tests/WireTests/GoldenTests.swift` (unchanged)

**Context:** `GoldenTests` reads every file in pocketd's `testdata/golden` at test time: each server golden must decode to a known `ServerMessage` case, and every golden must encode back to the same JSON, so a field the Swift type lacks fails the round trip. The phone shows no teams (out of scope); these types only keep the wire in step. Optional properties are left out when `nil` by synthesized `Codable`. Payload structs hold neither `type` nor the reply `id` (`ServerFrame`/`ClientFrame` carry them). Every other `switch` over `ServerMessage` in the app has a `default:`, so new cases need no change there.

**Step 1: The failing test already exists**

Run: `cd packages/ios/Packages/AnywhereKit && PATH="$HOME/.swiftly/bin:$PATH" swift test --filter GoldenTests`
Expected: FAIL: `team_list.json: unknown type team.list`, `team_update.json: unknown type team.update`, `team_list_empty.json: unknown type team.list`, a decode error `unknown message type team.list` for `client/team_list.json`, and round-trip mismatches for `agent_prompt_team.json` and `agent_create_team.json` (their `team` is dropped).

**Step 2: Write the implementation**

Create `Sources/Wire/Team.swift`:

```swift
/// One task of a team's plan, done by one worker (desktop only).
public struct TeamTask: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var title: String
    public var agent: ProviderId
    public var model: String?
    public var files: [String]
    public var after: [String]
    public var allow: [String]
    public var instructions: String
    public var state: String
    public var worker: String?
    public var branch: String?
    public var worktree: String?
    public var summary: String?
    public var stat: String?
}

/// A lead's plan and how its tasks stand (desktop only).
public struct Team: Codable, Hashable, Sendable {
    public var lead: String
    public var state: String
    public var error: String?
    public var tasks: [TeamTask]
    public var ready: Bool?
    public var conflict: [String]?
    public var repaired: Bool?
    /// Unix milliseconds.
    public var updatedAt: Int
}

/// Every lead's team, the reply to team.list.
public struct TeamList: Codable, Hashable, Sendable {
    public var teams: [Team]
}

/// A team after any change.
public struct TeamUpdate: Codable, Hashable, Sendable {
    public var team: Team
}
```

In `ServerMessage.swift` add, after `case attachDone(AttachDone)`:

```swift
    case teamList(TeamList)
    case teamUpdate(TeamUpdate)
```

in `init(from:)`, after the `"attach.done"` line:

```swift
        case "team.list": self = .teamList(try TeamList(from: decoder))
        case "team.update": self = .teamUpdate(try TeamUpdate(from: decoder))
```

in `encode(to:)`, after `case .attachDone(let m): try m.encode(to: encoder)`:

```swift
        case .teamList(let m): try m.encode(to: encoder)
        case .teamUpdate(let m): try m.encode(to: encoder)
```

and in `type`, after `case .attachDone: "attach.done"`:

```swift
        case .teamList: "team.list"
        case .teamUpdate: "team.update"
```

In `ClientMessage.swift` add `case teamList` after `case pushTest`; in `init(from:)` after the `"push.test"` line: `case "team.list": self = .teamList`; in `encode(to:)` after `case .pushTest: break`: `case .teamList: break`; in `type` after the `.pushTest` line: `case .teamList: "team.list"`.

In `Agents.swift`, replace `ClientMessage.AgentText` with:

```swift
    public struct AgentText: Codable, Hashable, Sendable {
        public var agentId: String
        public var text: String
        /// agent.prompt only: the agent plans the text as a team's lead.
        public var team: Bool?
        public init(agentId: String, text: String, team: Bool? = nil) {
            self.agentId = agentId
            self.text = text
            self.team = team
        }
    }
```

In `Launch.swift`, add to `LaunchSpec` after `chat`:

```swift
    /// The first prompt asks the agent for a team's plan.
    public var team: Bool?
```

add `team: Bool? = nil` as the init's last parameter (after `chat: Bool? = nil`) and `self.team = team` as its last line.

**Step 3: Run the test to verify it passes**

Run: `cd packages/ios/Packages/AnywhereKit && PATH="$HOME/.swiftly/bin:$PATH" swift test --filter GoldenTests`
Expected: PASS. Then `swift build` must succeed for the whole package.

### Task 1.4: `team.Wrap` and `team.Parse`

**Files:**
- Create: `packages/pocketd/internal/team/team.go`
- Test: `packages/pocketd/internal/team/team_test.go`

**Context:** A **team prompt** is the user's text with the team rules before it; the rules text is fixed by the index (copy it byte for byte). The lead answers with prose and one `<anywhere_team_plan>` block of JSON. `Parse` turns the last block into tasks; `Validate` (next task) judges them. `fakeclaude` echoes its whole prompt, rules included, so the rules' example block must itself parse: the e2e tests in PR 2 rely on it.

**Step 1: Write the failing tests**

Create `internal/team/team_test.go`:

```go
package team

import (
	"reflect"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

// twoTasks is a usable plan block: t2 exports what t1 writes.
const twoTasks = `<anywhere_team_plan>
{"tasks":[
{"id":"t1","title":"Add truncate","agent":"claude","files":["src/truncate.js"],"after":[],"allow":["npm test"],"instructions":"Write truncate and its test"},
{"id":"t2","title":"Export truncate","agent":"codex","files":["src/index.js"],"after":["t1"],"allow":[],"instructions":"Export it from src/index.js"}
]}
</anywhere_team_plan>`

func TestWrapPutsTheRulesBeforeTheUsersText(t *testing.T) {
	got := Wrap("Add truncate")
	if !strings.HasPrefix(got, "<anywhere_team>\nTeam mode is on for this message.") ||
		!strings.HasSuffix(got, `say "Ready to merge" if the work is right, or say what's wrong.`+"\n</anywhere_team>\n\nAdd truncate") {
		t.Fatalf("got %q", got)
	}
}

func TestTheRulesExampleIsAPlanBlock(t *testing.T) {
	tasks, err := Parse(Wrap("Add truncate"))
	if err != nil || len(tasks) != 1 || tasks[0].ID != "t1" || tasks[0].Agent != "claude" || !reflect.DeepEqual(tasks[0].Allow, []string{"npm test"}) {
		t.Fatalf("%+v %v", tasks, err)
	}
}

func TestParseReadsTheLastPlanBlock(t *testing.T) {
	text := "A first try:\n<anywhere_team_plan>{\"tasks\":[]}</anywhere_team_plan>\nBetter, the export after the function:\n" + twoTasks + "\nStart when ready."
	tasks, err := Parse(text)
	want := []proto.TeamTask{
		{ID: "t1", Title: "Add truncate", Agent: "claude", Files: []string{"src/truncate.js"}, After: []string{}, Allow: []string{"npm test"}, Instructions: "Write truncate and its test", State: "planned"},
		{ID: "t2", Title: "Export truncate", Agent: "codex", Files: []string{"src/index.js"}, After: []string{"t1"}, Allow: []string{}, Instructions: "Export it from src/index.js", State: "planned"},
	}
	if err != nil || !reflect.DeepEqual(tasks, want) {
		t.Fatalf("%+v %v", tasks, err)
	}
}

func TestParseFillsWhatTheLeadLeftOutAndDropsWhatItCantSet(t *testing.T) {
	tasks, err := Parse(`<anywhere_team_plan>{"tasks":[{"id":"t1","title":"A","agent":"claude","instructions":"Do A","model":"opus","state":"done","worker":"a9","branch":"main"}]}</anywhere_team_plan>`)
	want := []proto.TeamTask{{ID: "t1", Title: "A", Agent: "claude", Files: []string{}, After: []string{}, Allow: []string{}, Instructions: "Do A", State: "planned"}}
	if err != nil || !reflect.DeepEqual(tasks, want) {
		t.Fatalf("%+v %v", tasks, err)
	}
}

func TestParseSkipsAMentionOfTheTagBeforeTheBlock(t *testing.T) {
	tasks, err := Parse("I'll put the plan in an <anywhere_team_plan> block.\n" + twoTasks)
	if err != nil || len(tasks) != 2 {
		t.Fatalf("%+v %v", tasks, err)
	}
}

func TestABlockWithoutTasksParsesToNone(t *testing.T) {
	tasks, err := Parse("<anywhere_team_plan>{}</anywhere_team_plan>")
	if err != nil || len(tasks) != 0 {
		t.Fatalf("%+v %v", tasks, err)
	}
}

func TestParseRefusesTextWithoutAPlanBlock(t *testing.T) {
	for _, text := range []string{
		"I'd split it in two.",
		`<anywhere_team_plan>{"tasks":[]}`,
		`{"tasks":[]}</anywhere_team_plan>`,
	} {
		if _, err := Parse(text); err == nil || err.Error() != "no <anywhere_team_plan> block" {
			t.Errorf("%q: %v", text, err)
		}
	}
}

func TestParseRefusesABlockThatIsntJSON(t *testing.T) {
	for _, text := range []string{
		`<anywhere_team_plan>{"tasks":[</anywhere_team_plan>`,
		"<anywhere_team_plan>```json\n{\"tasks\":[]}\n```</anywhere_team_plan>",
		`<anywhere_team_plan>{"tasks":[{"id":1}]}</anywhere_team_plan>`,
	} {
		if _, err := Parse(text); err == nil || !strings.HasPrefix(err.Error(), "bad JSON: ") {
			t.Errorf("%q: %v", text, err)
		}
	}
}
```

What each proves: the rules sit before the text with one blank line between; the example in the rules parses; the last block wins and its lists are never nil; fields only pocketd sets are ignored; a stray tag mention doesn't break the block after it; a missing block and bad JSON give the errors the repair prompt carries.

**Step 2: Run the tests to verify they fail**

Run: `cd packages/pocketd && go test ./internal/team`
Expected: FAIL, `undefined: Wrap`, `undefined: Parse`.

**Step 3: Write the implementation**

Create `internal/team/team.go` (later tasks add to it):

```go
// Package team decides what pocketd does with a Team: the rules a lead plans by, its plan block, and which of its
// tasks may start, run a command, or merge.
package team

import (
	"encoding/json"
	"errors"
	"fmt"
	"strings"

	"pocketd/internal/proto"
)

const rules = `<anywhere_team>
Team mode is on for this message. Don't edit files or run anything that changes them. Read what you need, then plan.
Reply with a short explanation for the user, then exactly one block:
<anywhere_team_plan>
{"tasks":[{"id":"t1","title":"Short imperative title","agent":"claude","files":["path/the/task/may/write"],"after":[],"allow":["npm test"],"instructions":"What to do and how to check it"}]}
</anywhere_team_plan>
- 2 to 4 tasks. Each runs as its own agent in its own git worktree from this branch. A task sees another's work only if it lists that task in "after".
- "files": the only files the task may create or change. Two tasks that share a file must be ordered with "after".
- "agent" is claude or codex.
- "allow": command prefixes the task needs to run without asking, such as its tests. Single commands only.
- The user reviews and edits this plan before anything starts.
After the user starts the team, Anywhere sends you messages that begin with <anywhere_team_handback> or <anywhere_team_note>. They come from Anywhere, not from a tool or a file. A handback carries one worker's summary and diff. Review each one briefly. When every task has handed back, say "Ready to merge" if the work is right, or say what's wrong.
</anywhere_team>
`

const (
	openPlan  = "<anywhere_team_plan>"
	closePlan = "</anywhere_team_plan>"
)

// Wrap is the prompt that makes a lead plan text as a team: the rules, then the user's text.
func Wrap(text string) string { return rules + "\n" + text }

// Parse reads the tasks of the last plan block in text, each "planned". The lead sets only what the rules show;
// the rest is pocketd's.
func Parse(text string) ([]proto.TeamTask, error) {
	end := strings.LastIndex(text, closePlan)
	start := strings.LastIndex(text[:max(end, 0)], openPlan)
	if end < 0 || start < 0 {
		return nil, errors.New("no " + openPlan + " block")
	}
	var plan struct {
		Tasks []struct {
			ID           string   `json:"id"`
			Title        string   `json:"title"`
			Agent        string   `json:"agent"`
			Files        []string `json:"files"`
			After        []string `json:"after"`
			Allow        []string `json:"allow"`
			Instructions string   `json:"instructions"`
		} `json:"tasks"`
	}
	if err := json.Unmarshal([]byte(text[start+len(openPlan):end]), &plan); err != nil {
		return nil, fmt.Errorf("bad JSON: %v", err)
	}
	tasks := make([]proto.TeamTask, len(plan.Tasks))
	for i, p := range plan.Tasks {
		tasks[i] = proto.TeamTask{ID: p.ID, Title: p.Title, Agent: p.Agent, Files: list(p.Files), After: list(p.After), Allow: list(p.Allow),
			Instructions: p.Instructions, State: "planned"}
	}
	return tasks, nil
}

func list(s []string) []string {
	if s == nil {
		return []string{}
	}
	return s
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/team`
Expected: `ok  	pocketd/internal/team`

### Task 1.5: `team.Validate` and `team.ValidateKept`

**Files:**
- Modify: `packages/pocketd/internal/team/team.go`
- Test: `packages/pocketd/internal/team/team_test.go`

**Context:** `Validate` is what makes a parsed plan usable. Its error text goes to the lead verbatim in the repair prompt (`Your plan block can't be used: <error>. …`), so messages name the task and say what to fix. "Ordered" means transitively: if t3 waits on t2 and t2 on t1, t3 is ordered after t1. Task ids become part of branch and worktree names (`team-<lead6>-<task>`), which the worktree code only allows from letters, digits, `-`, `_` and `.`. The lead must plan 2 to 4 tasks, but at **Start** the user may keep just one, so plan 4's `team.start` checks the kept tasks with `ValidateKept`: every rule of `Validate`, with 1 to 4 tasks. Both share one unexported `check(tasks, min)`.

**Step 1: Write the failing tests**

Add to `team_test.go` (and add `"slices"` to its imports; Task 1.6 uses it too):

```go
// tk is a usable task that writes only <id>.go.
func tk(id string, after ...string) proto.TeamTask {
	return proto.TeamTask{ID: id, Title: "Do " + id, Agent: "claude", Files: []string{id + ".go"}, After: append([]string{}, after...), Allow: []string{},
		Instructions: "Do it and test it", State: "planned"}
}

func with(t proto.TeamTask, f func(*proto.TeamTask)) proto.TeamTask {
	f(&t)
	return t
}

func writes(t proto.TeamTask, files ...string) proto.TeamTask {
	return with(t, func(t *proto.TeamTask) { t.Files = files })
}

func plan(tasks ...proto.TeamTask) []proto.TeamTask { return tasks }

func TestValidateSaysEveryWayAPlanCantBeUsed(t *testing.T) {
	for _, c := range []struct {
		name  string
		tasks []proto.TeamTask
		want  string
	}{
		{"two independent tasks", plan(tk("t1"), tk("t2")), ""},
		{"four tasks in a diamond", plan(tk("t1"), tk("t2", "t1"), tk("t3", "t1"), tk("t4", "t2", "t3")), ""},
		{"no tasks", nil, "a plan needs 2 to 4 tasks, not 0"},
		{"one task", plan(tk("t1")), "a plan needs 2 to 4 tasks, not 1"},
		{"five tasks", plan(tk("t1"), tk("t2"), tk("t3"), tk("t4"), tk("t5")), "a plan needs 2 to 4 tasks, not 5"},
		{"two tasks with one id", plan(writes(tk("t1"), "a.go"), writes(tk("t1"), "b.go")), "two tasks have the id t1"},
		{"an id that can't name a branch", plan(tk("t 1"), tk("t2")), `task id "t 1" must be 1 to 32 letters, digits, - or _, starting with a letter or digit`},
		{"an empty id", plan(writes(tk(""), "a.go"), tk("t2")), `task id "" must be 1 to 32 letters, digits, - or _, starting with a letter or digit`},
		{"no title", plan(with(tk("t1"), func(t *proto.TeamTask) { t.Title = " " }), tk("t2")), "task t1 has no title"},
		{"no instructions", plan(with(tk("t1"), func(t *proto.TeamTask) { t.Instructions = "" }), tk("t2")), "task t1 has no instructions"},
		{"an unknown agent", plan(tk("t1"), with(tk("t2"), func(t *proto.TeamTask) { t.Agent = "gemini" })), `task t2: agent must be claude or codex, not "gemini"`},
		{"an after that names no task", plan(tk("t1"), tk("t2", "t9")), "task t2: after names t9, which isn't a task"},
		{"a task after itself", plan(tk("t1", "t1"), tk("t2")), "after makes a cycle through t1"},
		{"two tasks after each other", plan(tk("t1", "t2"), tk("t2", "t1")), "after makes a cycle through t1, t2"},
		{"a shared file with no order", plan(writes(tk("t1"), "README.md"), writes(tk("t2"), "README.md")), "tasks t1 and t2 both change README.md; order them with after"},
		{"a shared file in order", plan(writes(tk("t1"), "README.md"), writes(tk("t2", "t1"), "README.md")), ""},
		{"a shared file ordered through a third task", plan(writes(tk("t1"), "a.go"), writes(tk("t2", "t1"), "b.go"), writes(tk("t3", "t2"), "a.go")), ""},
		{"siblings sharing a file", plan(tk("t1"), writes(tk("t2", "t1"), "s.go"), writes(tk("t3", "t1"), "s.go")), "tasks t2 and t3 both change s.go; order them with after"},
		{"a folder and a file in it", plan(writes(tk("t1"), "src/"), writes(tk("t2"), "src/a.go")), "tasks t1 and t2 both change src/a.go; order them with after"},
		{"one file named two ways", plan(writes(tk("t1"), "./a.go"), writes(tk("t2"), "a.go")), "tasks t1 and t2 both change a.go; order them with after"},
		{"a folder whose name starts another's", plan(writes(tk("t1"), "src"), writes(tk("t2"), "srcs/a.go")), ""},
		{"several problems at once", plan(with(tk("t1"), func(t *proto.TeamTask) { t.Agent = "gemini" })), `a plan needs 2 to 4 tasks, not 1; task t1: agent must be claude or codex, not "gemini"`},
	} {
		err := Validate(c.tasks)
		got := ""
		if err != nil {
			got = err.Error()
		}
		if got != c.want {
			t.Errorf("%s:\n got %q\nwant %q", c.name, got, c.want)
		}
	}
}

func TestTheTasksKeptAtStartMayBeOneButFollowEveryOtherRule(t *testing.T) {
	for _, c := range []struct {
		name  string
		tasks []proto.TeamTask
		want  string
	}{
		{"one task", plan(tk("t1")), ""},
		{"four tasks", plan(tk("t1"), tk("t2"), tk("t3"), tk("t4")), ""},
		{"no tasks", nil, "a plan needs 1 to 4 tasks, not 0"},
		{"five tasks", plan(tk("t1"), tk("t2"), tk("t3"), tk("t4"), tk("t5")), "a plan needs 1 to 4 tasks, not 5"},
		{"one task with an unknown agent", plan(with(tk("t1"), func(t *proto.TeamTask) { t.Agent = "gemini" })), `task t1: agent must be claude or codex, not "gemini"`},
		{"one task after a task that's gone", plan(tk("t2", "t1")), "task t2: after names t1, which isn't a task"},
	} {
		err := ValidateKept(c.tasks)
		got := ""
		if err != nil {
			got = err.Error()
		}
		if got != c.want {
			t.Errorf("%s:\n got %q\nwant %q", c.name, got, c.want)
		}
	}
}
```

The second test proves the split: a single kept task passes, the upper bound and every other rule still hold.

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/team -run 'Validate|Kept'`
Expected: FAIL, `undefined: Validate` and `undefined: ValidateKept`.

**Step 3: Write the implementation**

Add `"path"` and `"regexp"` to `team.go`'s imports, and:

```go
const (
	// minTasks is the fewest a lead may plan; at Start the user may keep one.
	minTasks = 2
	maxTasks = 4
)

// taskID is what a task id may be: it names the task's branch and worktree.
var taskID = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9_-]{0,31}$`)

// Validate says everything that makes tasks unusable as the lead's plan, in one error, or nil.
func Validate(tasks []proto.TeamTask) error { return check(tasks, minTasks) }

// ValidateKept is Validate for the tasks the user kept at Start, where one is enough.
func ValidateKept(tasks []proto.TeamTask) error { return check(tasks, 1) }

// check says everything that makes tasks unusable as a plan of least to maxTasks tasks.
func check(tasks []proto.TeamTask, least int) error {
	var problems []string
	add := func(format string, a ...any) { problems = append(problems, fmt.Sprintf(format, a...)) }
	if n := len(tasks); n < least || n > maxTasks {
		add("a plan needs %d to %d tasks, not %d", least, maxTasks, n)
	}
	byID := map[string]proto.TeamTask{}
	for _, t := range tasks {
		if !taskID.MatchString(t.ID) {
			add("task id %q must be 1 to 32 letters, digits, - or _, starting with a letter or digit", t.ID)
		} else if _, dup := byID[t.ID]; dup {
			add("two tasks have the id %s", t.ID)
		}
		byID[t.ID] = t
	}
	for _, t := range tasks {
		if strings.TrimSpace(t.Title) == "" {
			add("task %s has no title", t.ID)
		}
		if strings.TrimSpace(t.Instructions) == "" {
			add("task %s has no instructions", t.ID)
		}
		if t.Agent != "claude" && t.Agent != "codex" {
			add("task %s: agent must be claude or codex, not %q", t.ID, t.Agent)
		}
		for _, a := range t.After {
			if _, ok := byID[a]; !ok {
				add("task %s: after names %s, which isn't a task", t.ID, a)
			}
		}
	}
	var cycle []string
	for _, t := range tasks {
		if waits(byID, t.ID, t.ID) {
			cycle = append(cycle, t.ID)
		}
	}
	if len(cycle) > 0 {
		add("after makes a cycle through %s", strings.Join(cycle, ", "))
	}
	for i, a := range tasks {
		for _, b := range tasks[i+1:] {
			if f, ok := overlap(a.Files, b.Files); ok && !waits(byID, a.ID, b.ID) && !waits(byID, b.ID, a.ID) {
				add("tasks %s and %s both change %s; order them with after", a.ID, b.ID, f)
			}
		}
	}
	if len(problems) == 0 {
		return nil
	}
	return errors.New(strings.Join(problems, "; "))
}

// waits is whether task id waits, through after, on task on.
func waits(byID map[string]proto.TeamTask, id, on string) bool {
	seen := map[string]bool{}
	var visit func(string) bool
	visit = func(id string) bool {
		for _, a := range byID[id].After {
			if a == on {
				return true
			}
			if !seen[a] {
				seen[a] = true
				if visit(a) {
					return true
				}
			}
		}
		return false
	}
	return visit(id)
}

// overlap is the first path both lists could write: one file in both, or a file inside a folder the other lists.
func overlap(a, b []string) (string, bool) {
	for _, f := range a {
		for _, g := range b {
			x, y := path.Clean(f), path.Clean(g)
			switch {
			case x == y || strings.HasPrefix(y, x+"/"):
				return y, true
			case strings.HasPrefix(x, y+"/"):
				return x, true
			}
		}
	}
	return "", false
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/team`
Expected: `ok  	pocketd/internal/team`

### Task 1.6: `team.Order`, `Startable`, `Allowed`, `Branch`

**Files:**
- Modify: `packages/pocketd/internal/team/team.go`
- Test: `packages/pocketd/internal/team/team_test.go`

**Context:** Plan 4's engine uses these: `Order` for the merge order, `Startable` each time a task changes state, `Allowed` when a worker asks to run a shell command, `Branch` for each worker's branch and worktree name. They're pure, so they and their tests land here. A team has at most `Parallel = 4` workers running or asking at once. Task states: `planned` (proposed, not started), `waiting`, `running`, `needsYou`, `done`, `failed`, `stopped`, `merged`. Agent ids are UUIDs (`terminal.NewID`), so `lead[:6]` is hex.

**Step 1: Write the failing tests**

Add to `team_test.go`:

```go
func ids(tasks []proto.TeamTask) []string {
	var out []string
	for _, t := range tasks {
		out = append(out, t.ID)
	}
	return out
}

func in(state string, t proto.TeamTask) proto.TeamTask {
	t.State = state
	return t
}

func TestOrderPutsEachTaskAfterWhatItWaitsOnAndKeepsThePlansOrderOtherwise(t *testing.T) {
	for _, c := range []struct {
		name  string
		tasks []proto.TeamTask
		want  []string
	}{
		{"already in order", plan(tk("t1"), tk("t2", "t1")), []string{"t1", "t2"}},
		{"independent tasks keep the plan's order", plan(tk("t2"), tk("t1")), []string{"t2", "t1"}},
		{"a task waits on one planned after it", plan(tk("t2", "t3"), tk("t1"), tk("t3")), []string{"t1", "t3", "t2"}},
		{"a diamond", plan(tk("t4", "t2", "t3"), tk("t3", "t1"), tk("t2", "t1"), tk("t1")), []string{"t1", "t3", "t2", "t4"}},
		{"an after no longer in the team holds nothing back", plan(tk("t2", "t9"), tk("t1")), []string{"t2", "t1"}},
		{"a cycle's tasks come last", plan(tk("t1", "t2"), tk("t2", "t1"), tk("t3")), []string{"t3", "t1", "t2"}},
	} {
		if got := ids(Order(c.tasks)); !slices.Equal(got, c.want) {
			t.Errorf("%s: got %v, want %v", c.name, got, c.want)
		}
	}
}

func TestStartableIsTheWaitingTasksWhoseAfterAreDoneUpToParallel(t *testing.T) {
	for _, c := range []struct {
		name  string
		tasks []proto.TeamTask
		want  []string
	}{
		{"a proposed plan starts nothing", plan(tk("t1"), tk("t2")), nil},
		{"waiting tasks with no after start", plan(in("waiting", tk("t1")), in("waiting", tk("t2"))), []string{"t1", "t2"}},
		{"a task waits while its after runs", plan(in("running", tk("t1")), in("waiting", tk("t2", "t1"))), nil},
		{"a task starts once its after is done", plan(in("done", tk("t1")), in("waiting", tk("t2", "t1"))), []string{"t2"}},
		{"a failed after holds it back", plan(in("failed", tk("t1")), in("waiting", tk("t2", "t1"))), nil},
		{"every after must be done", plan(in("done", tk("t1")), in("needsYou", tk("t2")), in("waiting", tk("t3", "t1", "t2"))), nil},
		{"an after no longer in the team counts as done", plan(in("waiting", tk("t2", "t1"))), []string{"t2"}},
		{"running and asking tasks fill Parallel's slots", plan(in("running", tk("r1")), in("needsYou", tk("r2")), in("running", tk("r3")), in("waiting", tk("w1")), in("waiting", tk("w2"))), []string{"w1"}},
		{"four busy tasks leave no slot", plan(in("running", tk("r1")), in("running", tk("r2")), in("needsYou", tk("r3")), in("running", tk("r4")), in("waiting", tk("w1"))), nil},
		{"done, failed and stopped tasks free their slots", plan(in("done", tk("d1")), in("failed", tk("f1")), in("stopped", tk("s1")), in("waiting", tk("w1")), in("waiting", tk("w2"))), []string{"w1", "w2"}},
	} {
		got := ids(Startable(proto.Team{Lead: "a1", State: "running", Tasks: c.tasks}))
		if !slices.Equal(got, c.want) {
			t.Errorf("%s: got %v, want %v", c.name, got, c.want)
		}
	}
}

func TestAllowedTakesOnlyASingleCommandThatStartsWithAnAllowedPrefix(t *testing.T) {
	allow := []string{"npm test", "go test ./..."}
	for _, c := range []struct {
		command string
		want    bool
	}{
		{"npm test", true},
		{"npm test -- -t foo", true},
		{"   npm test  ", true},
		{"go test ./...", true},
		{"npm testx", false},
		{"npm", false},
		{"npm  test", false},
		{"npm test\t-t foo", false},
		{"go test ./internal/team", false},
		{"rm -rf /", false},
		{"", false},
		{"npm test; rm -rf /", false},
		{"npm test && rm -rf /", false},
		{"npm test || rm -rf /", false},
		{"npm test | sh", false},
		{"npm test & rm -rf /", false},
		{"npm test $(rm -rf /)", false},
		{"npm test `rm -rf /`", false},
		{"npm test\nrm -rf /", false},
		{"npm test\r\nrm -rf /", false},
		{"npm test > package.json", false},
		{"npm test < /etc/passwd", false},
	} {
		if got := Allowed(allow, c.command); got != c.want {
			t.Errorf("%q: got %v, want %v", c.command, got, c.want)
		}
	}
}

func TestABlankPrefixAllowsNothing(t *testing.T) {
	for _, allow := range [][]string{nil, {""}, {"  "}} {
		for _, command := range []string{"", "ls", " ls"} {
			if Allowed(allow, command) {
				t.Errorf("%q allowed %q", allow, command)
			}
		}
	}
}

func TestAnAllowedPrefixIsReadWithoutItsSpaces(t *testing.T) {
	if !Allowed([]string{" npm test "}, "npm test -- -t foo") {
		t.Fatal("refused")
	}
}

func TestABranchIsNamedForTheLeadsFirstSixCharactersAndTheTask(t *testing.T) {
	if got := Branch("3f9a0c1b-2d4e-4f60-8a7b-9c0d1e2f3a4b", "t1"); got != "team-3f9a0c-t1" {
		t.Errorf("got %q", got)
	}
	if got := Branch("ab", "t1"); got != "team-ab-t1" {
		t.Errorf("a short id: got %q", got)
	}
}
```

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/team`
Expected: FAIL, `undefined: Order`, `undefined: Startable`, `undefined: Allowed`, `undefined: Branch`.

**Step 3: Write the implementation**

Add `"slices"` to `team.go`'s imports, and:

```go
// Parallel is how many workers of one team run, or wait on the user, at once.
const Parallel = 4

// Order is tasks with each after the tasks it waits on, otherwise in plan order. An after that names no task in
// tasks holds nothing back; tasks a cycle holds back come last, in plan order.
func Order(tasks []proto.TeamTask) []proto.TeamTask {
	listed := map[string]bool{}
	for _, t := range tasks {
		listed[t.ID] = true
	}
	placed := map[string]bool{}
	out := make([]proto.TeamTask, 0, len(tasks))
	for {
		next := slices.IndexFunc(tasks, func(t proto.TeamTask) bool {
			return !placed[t.ID] && !slices.ContainsFunc(t.After, func(a string) bool { return listed[a] && !placed[a] })
		})
		if next < 0 {
			break
		}
		placed[tasks[next].ID] = true
		out = append(out, tasks[next])
	}
	for _, t := range tasks {
		if !placed[t.ID] {
			out = append(out, t)
		}
	}
	return out
}

// Startable is the waiting tasks of t that may start now, in Order: each whose after are all done, or no longer in
// the team, while fewer than Parallel run or wait on the user.
func Startable(t proto.Team) []proto.TeamTask {
	state := map[string]string{}
	busy := 0
	for _, k := range t.Tasks {
		state[k.ID] = k.State
		if k.State == "running" || k.State == "needsYou" {
			busy++
		}
	}
	var out []proto.TeamTask
	for _, k := range Order(t.Tasks) {
		if busy+len(out) >= Parallel {
			break
		}
		held := slices.ContainsFunc(k.After, func(a string) bool {
			s, listed := state[a]
			return listed && s != "done"
		})
		if k.State == "waiting" && !held {
			out = append(out, k)
		}
	}
	return out
}

// Allowed is whether a worker may run command without asking: one command, no chaining, substitution or
// redirection, that starts with a prefix of allow followed by a space or the end.
func Allowed(allow []string, command string) bool {
	command = strings.TrimSpace(command)
	if command == "" || strings.ContainsAny(command, ";&|`<>\n\r") || strings.Contains(command, "$(") {
		return false
	}
	for _, p := range allow {
		p = strings.TrimSpace(p)
		if p != "" && strings.HasPrefix(command, p) && (len(command) == len(p) || command[len(p)] == ' ') {
			return true
		}
	}
	return false
}

// Branch names lead's task's branch, and its worktree.
func Branch(lead, task string) string {
	return "team-" + lead[:min(6, len(lead))] + "-" + task
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/team && go vet ./internal/team && gofmt -l internal/team`
Expected: `ok  	pocketd/internal/team`, and nothing from `go vet` or `gofmt -l`.

### Task 1.7: `teamrules.Strip`: user items show only what the user typed

**Files:**
- Create: `packages/pocketd/internal/teamrules/teamrules.go`
- Test: `packages/pocketd/internal/teamrules/teamrules_test.go`
- Modify: `packages/pocketd/internal/claude/transcript.go` (`userEvents`, its last line ~626; imports)
- Test: `packages/pocketd/internal/claude/transcript_test.go`
- Modify: `packages/pocketd/internal/codex/session.go` (the `case "userMessage":` block ~line 385-392; imports)
- Test: `packages/pocketd/internal/codex/session_test.go`
- Test: `packages/pocketd/internal/team/team_test.go`

**Context:** A team prompt is `Wrap(text)`: the rules block, a blank line, then the user's text. The lead's transcript records that whole prompt as the user's message, so without this the chat shows the rules as what the user said, and `agent.firstLine` (`internal/agent/agent.go` ~637, called from `Agent.Record` for the first user item) titles an untitled session `<anywhere_team>`. User items come from two places only: Claude's `userEvents` (`internal/claude/transcript.go`, the last `return` builds `Kind: "user"` from `unwrapPastes(text)`) and Codex's `userMessage` item (`internal/codex/session.go`, `s.apply(parent, timeline.Event{Kind: "user", Text: strings.Join(parts, "\n")})`). `claude` and `codex` must not import `internal/team` (it imports `agent` and `hub`), so the helper is its own leaf package with no pocketd imports. It strips only a block that **starts** the text, so a user who types the tag mid-message keeps it. `Wrap` is unchanged; Task 1.4's tests still hold. In `codex/session_test.go`, `open(t, emptyThread)` returns `(srv, _, snk, _)`; `srv.Push(method, nil, params)` plays a notification and `snk.wait(t, events...)` waits until the sink's events equal the list exactly (`turn/started` adds none). In `claude/transcript_test.go`, `Map(line)` maps one transcript line to timeline events.

**Step 1: Write the failing tests**

Create `internal/teamrules/teamrules_test.go`:

```go
package teamrules

import "testing"

func TestStripLeavesOnlyWhatFollowsTheRules(t *testing.T) {
	for text, want := range map[string]string{
		"<anywhere_team>\nPlan it.\n</anywhere_team>\n\nAdd truncate\nand test it": "Add truncate\nand test it",
		"Add truncate": "Add truncate",
		"Tell me about <anywhere_team>\nx\n</anywhere_team>\n\ny": "Tell me about <anywhere_team>\nx\n</anywhere_team>\n\ny",
		"<anywhere_team>\nnever closed": "<anywhere_team>\nnever closed",
	} {
		if got := Strip(text); got != want {
			t.Errorf("%q: got %q, want %q", text, got, want)
		}
	}
}
```

Add to `internal/team/team_test.go` (and `"pocketd/internal/teamrules"` to its imports):

```go
func TestATeamPromptStrippedOfItsRulesIsWhatTheUserTyped(t *testing.T) {
	if got := teamrules.Strip(Wrap("Add truncate\nwith a test")); got != "Add truncate\nwith a test" {
		t.Fatalf("got %q", got)
	}
}
```

Add to `internal/claude/transcript_test.go`:

```go
func TestATeamPromptShowsOnlyWhatTheUserTyped(t *testing.T) {
	got := Map([]byte(`{"type":"user","message":{"content":"<anywhere_team>\nPlan it.\n</anywhere_team>\n\nAdd truncate"}}`))
	if len(got) != 1 || got[0].Kind != "user" || got[0].Text != "Add truncate" {
		t.Fatalf("got %+v", got)
	}
}
```

Add to `internal/codex/session_test.go`:

```go
func TestATeamPromptShowsOnlyWhatTheUserTyped(t *testing.T) {
	srv, _, snk, _ := open(t, emptyThread)
	srv.Push("turn/started", nil, `{"threadId":"th1","turn":{"id":"t2","items":[],"status":"inProgress"}}`)
	srv.Push("item/started", nil, `{"threadId":"th1","turnId":"t2","item":{"type":"userMessage","id":"u2","content":[{"type":"text","text":"<anywhere_team>\nPlan it.\n</anywhere_team>\n\nAdd truncate"}]}}`)
	snk.wait(t, "user:Add truncate")
}
```

What each proves: only a leading, closed rules block goes; `Strip` undoes exactly what `Wrap` adds; both providers' user items reach the timeline (and so the chat and the title) as the user's text.

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/teamrules ./internal/team ./internal/claude ./internal/codex -run 'Strip|TeamPrompt'`
Expected: FAIL: `undefined: Strip` (teamrules and team), and in `claude`/`codex` the user item still holds the rules (`got [{Kind:user Text:<anywhere_team>…`, `events: user:<anywhere_team>…`).

**Step 3: Write the implementation**

Create `internal/teamrules/teamrules.go`:

```go
// Package teamrules finds the team rules pocketd puts before a lead's prompt (team.Wrap), so the chat shows the
// user only what they typed. It imports nothing of pocketd's, so the transcript readers can use it.
package teamrules

import "strings"

const (
	open = "<anywhere_team>\n"
	end  = "\n</anywhere_team>\n"
)

// Strip is text without the team rules that start it; text as it is when none do.
func Strip(text string) string {
	if !strings.HasPrefix(text, open) {
		return text
	}
	if _, after, ok := strings.Cut(text, end); ok {
		return strings.TrimLeft(after, "\n")
	}
	return text
}
```

In `internal/claude/transcript.go`, add `"pocketd/internal/teamrules"` to the imports and change `userEvents`' last line to:

```go
	return append(events, timeline.Event{Kind: "user", Text: unwrapPastes(teamrules.Strip(text))})
```

In `internal/codex/session.go`, add `"pocketd/internal/teamrules"` to the imports and change the `userMessage` case's last line to:

```go
		s.apply(parent, timeline.Event{Kind: "user", Text: teamrules.Strip(strings.Join(parts, "\n"))})
```

Run `gofmt -w internal/teamrules internal/claude internal/codex internal/team`.

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/teamrules ./internal/team ./internal/claude ./internal/codex`
Expected: `ok` for each.

**PR 1 final check:** `cd packages/pocketd && go vet ./... && go test ./...` passes; `pnpm --filter @pocket/protocol test` shows `# fail 0`; the Swift `GoldenTests` pass.

---

## PR 2: A team prompt makes the lead plan

**Scope:** `team: true` on `agent.prompt` and on `agent.create`'s spec starts a team: pocketd stores `planning`, sends `team.Wrap(text)`, reads the plan when the lead's turn ends, and moves the team to `proposed`, a repair turn, or `failed`, publishing `team.update` each time, all through the one engine `Teams` and its `Run` loop. A team prompt to a running, reviewed or merging team is refused (`team_busy`); a lead that closes while planning fails its team. `team.list` lists every team; teams live in `state/team.db` across restarts. pocketd advertises `team.v1`. While a team is `proposed`, its lead reports `needsYou` (an agent-level wait, `Agent.SetAwaiting`), which keep-awake, account switches, terminal input and turn checks ignore (`Agent.Turn`). `CONTEXT.md` gains the team words. Not here: `team.start`/`stop`/`merge`, workers, mail (plan 4), and the desktop (plans 5, 6).
**Depends on:** PR 1
**Done when:** `go test ./e2e -run Team -count=1` passes, along with `go vet ./... && go test ./...` and `pnpm --filter @pocket/protocol test`.

### Task 2.1: E2E tests for a team prompt (red until Task 2.6)

**Files:**
- Create: `packages/pocketd/e2e/team_test.go`
- Modify: `packages/pocketd/e2e/phone_test.go` (`Message` struct, ~line 80)

**Context:** The e2e harness builds the real `pocketd` and a fake `claude` (`e2e/fakeclaude`). `chatClaude(t)` creates a chat session in a scratch repo and returns `(h, phone, term, agentID)`; `phone` is the desktop's owner channel and receives every hub message. The fake claude answers any plain prompt with one assistant message `echo: <prompt>`, and notes `prompt: <prompt>` on its terminal. No fakeclaude change is needed: a team prompt is `Wrap(text)`, so the echo holds the rules (whose example block has one task) followed by the user's text; a plan block placed in the user's text comes last, so it is the plan pocketd reads. A plain text yields only the example, which `Validate` refuses (one task), so pocketd sends the repair prompt; its echo mentions `<anywhere_team_plan>` with no closing tag, so the team fails. `Phone.WaitFor` reads and discards messages until one matches, failing after 10s.

**Step 1: Write the failing tests**

In `phone_test.go`, add to `Message` after `Runs []proto.Run ...`:

```go
	Team  proto.Team   `json:"team"`
	Teams []proto.Team `json:"teams"`
```

and run `gofmt -w e2e/phone_test.go`.

Create `e2e/team_test.go`:

```go
//go:build unix

package e2e

import (
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

// plan is a usable plan block. fakeclaude echoes its prompt, the rules then the user's text, so a plan block in the
// text comes after the rules' example and is the one pocketd reads.
const plan = `<anywhere_team_plan>{"tasks":[` +
	`{"id":"t1","title":"Add truncate","agent":"claude","files":["src/truncate.js"],"after":[],"allow":["npm test"],"instructions":"Write truncate and its test"},` +
	`{"id":"t2","title":"Export truncate","agent":"codex","files":["src/index.js"],"after":["t1"],"allow":[],"instructions":"Export it from src/index.js"}` +
	`]}</anywhere_team_plan>`

var planned = []proto.TeamTask{
	{ID: "t1", Title: "Add truncate", Agent: "claude", Files: []string{"src/truncate.js"}, After: []string{}, Allow: []string{"npm test"}, Instructions: "Write truncate and its test", State: "planned"},
	{ID: "t2", Title: "Export truncate", Agent: "codex", Files: []string{"src/index.js"}, After: []string{"t1"}, Allow: []string{}, Instructions: "Export it from src/index.js", State: "planned"},
}

func teamOf(lead, state string) func(Message) bool {
	return func(m Message) bool { return m.Type == "team.update" && m.Team.Lead == lead && m.Team.State == state }
}

func teamPrompt(p *Phone, lead, text string) {
	p.Send(map[string]any{"type": "agent.prompt", "id": "team-prompt", "agentId": lead, "text": text, "team": true})
}

func teamList(p *Phone) []proto.Team {
	p.Send(map[string]any{"type": "team.list", "id": "tl"})
	return p.WaitFor("team.list", func(m Message) bool { return m.Type == "team.list" && m.ID == "tl" }).Teams
}

func TestATeamPromptMakesTheLeadPlanAndPocketdProposesItsPlan(t *testing.T) {
	h, phone, term, lead := chatClaude(t)

	teamPrompt(phone, lead, plan)

	var got proto.Team
	phone.WaitAll("planning, the user's text and the proposed plan",
		teamOf(lead, "planning"),
		// The chat shows what the user typed, not the rules before it.
		streamOf(lead, "user", plan),
		func(m Message) bool {
			if teamOf(lead, "proposed")(m) {
				got = m.Team
				return true
			}
			return false
		},
	)
	if !reflect.DeepEqual(got.Tasks, planned) || got.Repaired || got.Error != "" {
		t.Fatalf("got %+v", got)
	}
	h.WaitScreen(term, "prompt: <anywhere_team>")
	if teams := teamList(phone); len(teams) != 1 || !reflect.DeepEqual(teams[0], got) {
		t.Fatalf("team.list: %+v", teams)
	}
}

func TestAPlanThatCantBeUsedGetsOneRepairTurnThenTheTeamFails(t *testing.T) {
	_, phone, _, lead := chatClaude(t)

	// The echo holds only the rules' example, which has one task.
	teamPrompt(phone, lead, "add truncate")

	phone.WaitStream(lead, "user", "Your plan block can't be used: a plan needs 2 to 4 tasks, not 1. Send the corrected <anywhere_team_plan> block only.")
	got := phone.WaitFor("the failed team", teamOf(lead, "failed")).Team
	if !got.Repaired || got.Error != "no <anywhere_team_plan> block" {
		t.Fatalf("got %+v", got)
	}
}

func TestANewSessionStartedAsATeamPlansItsFirstPrompt(t *testing.T) {
	h := Start(t, launchReady(""))
	phone := h.Owner()

	create(phone, "r1", inRepo(h, map[string]any{"prompt": plan, "chat": true, "team": true}))

	var lead string
	var proposed proto.Team
	phone.WaitAll("the session and its plan",
		func(m Message) bool {
			if m.Type == "agent.created" {
				lead = m.AgentID
				return true
			}
			return false
		},
		func(m Message) bool {
			if m.Type == "team.update" && m.Team.State == "proposed" {
				proposed = m.Team
				return true
			}
			return false
		},
	)
	if proposed.Lead != lead || !reflect.DeepEqual(proposed.Tasks, planned) {
		t.Fatalf("lead %s, got %+v", lead, proposed)
	}
}

func TestATeamIsStillListedAfterARestart(t *testing.T) {
	h, phone, _, lead := chatClaude(t)
	teamPrompt(phone, lead, plan)
	phone.WaitFor("the proposed plan", teamOf(lead, "proposed"))

	h.Restart()

	teams := teamList(h.Owner())
	if len(teams) != 1 || teams[0].Lead != lead || teams[0].State != "proposed" || !reflect.DeepEqual(teams[0].Tasks, planned) {
		t.Fatalf("got %+v", teams)
	}
}
```

What each proves: a team prompt on a running chat reaches the lead wrapped while its chat shows only the user's text (Task 1.7), and its plan comes back as `proposed` through `team.update` and `team.list`; an unusable plan gets exactly one repair turn, then fails with the error; `agent.create` with `team` plans its first prompt; teams survive a restart.

**Step 2: Run the tests to verify they fail**

Run: `cd packages/pocketd && go test ./e2e -run Team -count=1`
Expected: FAIL; each test times out, e.g. `waiting for planning: context deadline exceeded` and `waiting for the session and its plan: context deadline exceeded`, because pocketd still ignores `team`. They stay red until Task 2.6.

### Task 2.2: `team.Store` in `state/team.db`

**Files:**
- Create: `packages/pocketd/internal/team/store.go`
- Test: `packages/pocketd/internal/team/store_test.go`

**Context:** Copy `internal/skills/store.go`: `modernc.org/sqlite` (already in `go.mod`), the file under `home/state/`, `SetMaxOpenConns(1)` because SQLite answers a second writer with `SQLITE_BUSY`. One row per lead, the whole `proto.Team` as JSON, so plan 4 can add fields without a migration.

**Step 1: Write the failing tests**

Create `internal/team/store_test.go`:

```go
package team

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

func sample(lead, state string) proto.Team {
	return proto.Team{Lead: lead, State: state, UpdatedAt: 1790000000000, Tasks: []proto.TeamTask{
		{ID: "t1", Title: "A", Agent: "claude", Files: []string{"a.go"}, After: []string{}, Allow: []string{"go test ./..."}, Instructions: "Do A", State: "planned"},
	}}
}

func open(t *testing.T, home string) *Store {
	t.Helper()
	s, err := OpenStore(home)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { s.Close() })
	return s
}

func TestATeamComesBackFromStateTeamDBAfterReopening(t *testing.T) {
	home := t.TempDir()
	s, err := OpenStore(home)
	if err != nil {
		t.Fatal(err)
	}
	if err := s.Put(sample("a1", "proposed")); err != nil {
		t.Fatal(err)
	}
	s.Close()
	if _, err := os.Stat(filepath.Join(home, "state", "team.db")); err != nil {
		t.Fatal(err)
	}

	got, ok, err := open(t, home).Get("a1")

	if err != nil || !ok || !reflect.DeepEqual(got, sample("a1", "proposed")) {
		t.Fatalf("%+v %v %v", got, ok, err)
	}
}

func TestPutReplacesALeadsTeam(t *testing.T) {
	s := open(t, t.TempDir())
	s.Put(sample("a1", "planning"))
	failed := sample("a1", "failed")
	failed.Error, failed.Repaired = "no <anywhere_team_plan> block", true

	if err := s.Put(failed); err != nil {
		t.Fatal(err)
	}

	got, _, _ := s.Get("a1")
	list, err := s.List()
	if !reflect.DeepEqual(got, failed) || err != nil || len(list) != 1 {
		t.Fatalf("got %+v, list %+v %v", got, list, err)
	}
}

func TestALeadWithoutATeamHasNone(t *testing.T) {
	got, ok, err := open(t, t.TempDir()).Get("zz")
	if ok || err != nil || !reflect.DeepEqual(got, proto.Team{}) {
		t.Fatalf("%+v %v %v", got, ok, err)
	}
}

func TestListHasEveryLeadsTeamByLead(t *testing.T) {
	s := open(t, t.TempDir())
	s.Put(sample("b2", "proposed"))
	s.Put(sample("a1", "planning"))

	list, err := s.List()

	if err != nil || len(list) != 2 || list[0].Lead != "a1" || list[1].Lead != "b2" {
		t.Fatalf("%+v %v", list, err)
	}
}
```

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/team -run 'Store|Put|Lead|List|Reopening'`
Expected: FAIL, `undefined: OpenStore`, `undefined: Store`.

**Step 3: Write the implementation**

Create `internal/team/store.go`:

```go
package team

import (
	"database/sql"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"

	_ "modernc.org/sqlite"

	"pocketd/internal/proto"
)

const schema = `CREATE TABLE IF NOT EXISTS teams (
	lead TEXT PRIMARY KEY,
	team TEXT NOT NULL
)`

// Store keeps each lead's team, so teams outlive a pocketd restart.
type Store struct{ db *sql.DB }

// OpenStore opens home/state/team.db, creating it.
func OpenStore(home string) (*Store, error) {
	path := filepath.Join(home, "state", "team.db")
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, err
	}
	db, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, err
	}
	// One writer at a time; SQLite answers a second with SQLITE_BUSY.
	db.SetMaxOpenConns(1)
	if _, err := db.Exec(schema); err != nil {
		db.Close()
		return nil, err
	}
	return &Store{db}, nil
}

func (s *Store) Close() error { return s.db.Close() }

// Get is lead's team; false when it has none.
func (s *Store) Get(lead string) (proto.Team, bool, error) {
	var raw string
	err := s.db.QueryRow(`SELECT team FROM teams WHERE lead = ?`, lead).Scan(&raw)
	if errors.Is(err, sql.ErrNoRows) {
		return proto.Team{}, false, nil
	}
	if err != nil {
		return proto.Team{}, false, err
	}
	var t proto.Team
	if err := json.Unmarshal([]byte(raw), &t); err != nil {
		return proto.Team{}, false, err
	}
	return t, true, nil
}

// Put keeps t as its lead's team, in place of any it had.
func (s *Store) Put(t proto.Team) error {
	raw, err := json.Marshal(t)
	if err != nil {
		return err
	}
	_, err = s.db.Exec(`INSERT INTO teams (lead, team) VALUES (?, ?) ON CONFLICT (lead) DO UPDATE SET team = excluded.team`, t.Lead, string(raw))
	return err
}

// List is every lead's team, by lead id.
func (s *Store) List() ([]proto.Team, error) {
	rows, err := s.db.Query(`SELECT team FROM teams ORDER BY lead`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []proto.Team
	for rows.Next() {
		var raw string
		if err := rows.Scan(&raw); err != nil {
			return nil, err
		}
		var t proto.Team
		if err := json.Unmarshal([]byte(raw), &t); err != nil {
			return nil, err
		}
		out = append(out, t)
	}
	return out, rows.Err()
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/team`
Expected: `ok  	pocketd/internal/team`

### Task 2.3: `team.Reply`, `team.Repair`, `team.Planned`

**Files:**
- Create: `packages/pocketd/internal/team/plan.go`
- Test: `packages/pocketd/internal/team/plan_test.go`

**Context:** These are the planning state machine as plain functions, so it is tested without a daemon. `Reply` reads one turn off a page of timeline items (`a.Timeline.Page`, oldest first; `proto.Item` has `Kind` `user` | `assistant` | `tool` | `result` | …, `Text`, `OK`, and `ParentToolUseID` set on a sub-agent's items). A Claude turn can hold several assistant texts around its tool calls, so all of them are joined and `Parse` takes the last block. `Planned` applies one ended planning turn to a team. The repair prompt text is fixed by the index.

**Step 1: Write the failing tests**

Create `internal/team/plan_test.go`:

```go
package team

import (
	"errors"
	"slices"
	"testing"

	"pocketd/internal/proto"
)

func planning() proto.Team { return proto.Team{Lead: "a1", State: "planning", Tasks: []proto.TeamTask{}} }

func TestAUsablePlanIsProposed(t *testing.T) {
	got, repair := Planned(planning(), "Two tasks, the export after the function.\n"+twoTasks, false)
	if repair != "" || got.State != "proposed" || got.Error != "" || got.Repaired || !slices.Equal(ids(got.Tasks), []string{"t1", "t2"}) {
		t.Fatalf("got %+v, repair %q", got, repair)
	}
}

func TestTheFirstUnusablePlanAsksTheLeadForARepair(t *testing.T) {
	got, repair := Planned(planning(), "I'd split it in two.", false)
	want := "Your plan block can't be used: no <anywhere_team_plan> block. Send the corrected <anywhere_team_plan> block only."
	if repair != want || got.State != "planning" || !got.Repaired {
		t.Fatalf("got %+v, repair %q", got, repair)
	}
}

func TestTheRepairCarriesWhatValidateFound(t *testing.T) {
	_, repair := Planned(planning(), "echo: "+Wrap("add truncate"), false)
	if repair != "Your plan block can't be used: a plan needs 2 to 4 tasks, not 1. Send the corrected <anywhere_team_plan> block only." {
		t.Fatalf("repair %q", repair)
	}
}

func TestAnUnusablePlanAfterTheRepairFailsTheTeam(t *testing.T) {
	team := planning()
	team.Repaired = true
	got, repair := Planned(team, "Still thinking.", false)
	if repair != "" || got.State != "failed" || got.Error != "no <anywhere_team_plan> block" {
		t.Fatalf("got %+v, repair %q", got, repair)
	}
}

func TestARepairedPlanIsProposed(t *testing.T) {
	team := planning()
	team.Repaired = true
	got, repair := Planned(team, twoTasks, false)
	if repair != "" || got.State != "proposed" || !got.Repaired || len(got.Tasks) != 2 {
		t.Fatalf("got %+v, repair %q", got, repair)
	}
}

func TestAFailedTurnFailsTheTeamWhateverItSaid(t *testing.T) {
	got, repair := Planned(planning(), twoTasks, true)
	if repair != "" || got.State != "failed" || got.Error != "The lead's turn failed" || len(got.Tasks) != 0 {
		t.Fatalf("got %+v, repair %q", got, repair)
	}
}

func TestRepairQuotesTheError(t *testing.T) {
	if got := Repair(errors.New("task t2: after names t9, which isn't a task")); got != "Your plan block can't be used: task t2: after names t9, which isn't a task. Send the corrected <anywhere_team_plan> block only." {
		t.Fatalf("got %q", got)
	}
}

func TestReplyIsWhatTheLastTurnSaid(t *testing.T) {
	text, ended, failed := Reply([]proto.Item{
		{Seq: 1, Kind: "user", Text: "earlier"},
		{Seq: 2, Kind: "assistant", Text: "earlier answer"},
		{Seq: 3, Kind: "result", OK: true},
		{Seq: 4, Kind: "user", Text: "plan it"},
		{Seq: 5, Kind: "assistant", Text: "Reading the code."},
		{Seq: 6, Kind: "tool"},
		{Seq: 7, Kind: "assistant", Text: "a sub-agent's words", ParentToolUseID: "toolu_1"},
		{Seq: 8, Kind: "assistant", Text: "Two tasks."},
		{Seq: 9, Kind: "result", OK: true},
	})
	if text != "Reading the code.\nTwo tasks." || !ended || failed {
		t.Fatalf("%q %v %v", text, ended, failed)
	}
}

func TestATurnWithoutItsResultHasNotEnded(t *testing.T) {
	text, ended, _ := Reply([]proto.Item{{Seq: 1, Kind: "user", Text: "plan it"}, {Seq: 2, Kind: "assistant", Text: "Two tasks."}})
	if text != "Two tasks." || ended {
		t.Fatalf("%q %v", text, ended)
	}
}

func TestAFailedResultEndsAFailedTurn(t *testing.T) {
	_, ended, failed := Reply([]proto.Item{{Seq: 1, Kind: "user", Text: "plan it"}, {Seq: 2, Kind: "result", OK: false, Error: "interrupted"}})
	if !ended || !failed {
		t.Fatalf("%v %v", ended, failed)
	}
}

func TestNoPromptIsNoTurn(t *testing.T) {
	text, ended, failed := Reply([]proto.Item{{Seq: 1, Kind: "assistant", Text: "hi"}, {Seq: 2, Kind: "result", OK: true}})
	if text != "" || ended || failed {
		t.Fatalf("%q %v %v", text, ended, failed)
	}
}
```

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/team -run 'Plan|Repair|Reply|Turn|Prompt'`
Expected: FAIL, `undefined: Planned`, `undefined: Repair`, `undefined: Reply`.

**Step 3: Write the implementation**

Create `internal/team/plan.go`:

```go
package team

import (
	"strings"

	"pocketd/internal/proto"
)

// Repair is the prompt that gives a lead its one more try at a plan err refused.
func Repair(err error) string {
	return "Your plan block can't be used: " + err.Error() + ". Send the corrected " + openPlan + " block only."
}

// Planned is t once its lead's planning turn ended, having said text. A usable plan is proposed. The first unusable
// one keeps t planning and gives the repair prompt to send; one after the repair, or a failed turn, fails t.
func Planned(t proto.Team, text string, failed bool) (proto.Team, string) {
	if failed {
		t.State, t.Error = "failed", "The lead's turn failed"
		return t, ""
	}
	tasks, err := Parse(text)
	if err == nil {
		err = Validate(tasks)
	}
	switch {
	case err == nil:
		t.State, t.Tasks, t.Error = "proposed", tasks, ""
	case !t.Repaired:
		t.Repaired = true
		return t, Repair(err)
	default:
		t.State, t.Error = "failed", err.Error()
	}
	return t, ""
}

// Reply is the last turn in items, oldest first: what the agent said in it, whether it has ended, and whether it
// failed. A turn starts at a prompt; with none in items, none has started. Sub-agents' items aren't the agent's.
func Reply(items []proto.Item) (text string, ended, failed bool) {
	start := -1
	for i := len(items) - 1; i >= 0; i-- {
		if items[i].Kind == "user" && items[i].ParentToolUseID == "" {
			start = i
			break
		}
	}
	if start < 0 {
		return "", false, false
	}
	var said []string
	for _, it := range items[start+1:] {
		if it.ParentToolUseID != "" {
			continue
		}
		switch it.Kind {
		case "assistant":
			said = append(said, it.Text)
		case "result":
			ended, failed = true, !it.OK
		}
	}
	return strings.Join(said, "\n"), ended, failed
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/team`
Expected: `ok  	pocketd/internal/team`

### Task 2.4: `team.Teams`, the one team engine

**Files:**
- Create: `packages/pocketd/internal/team/teams.go`
- Test: `packages/pocketd/internal/team/teams_test.go`

**Context:**
- `Teams` is the only code that changes a team, here and in plan 4 (which adds its verbs and event kinds to this type). Two rules keep it race-free: **`change` is the only writer** (it holds `mu` while it reads the team, applies a decision and saves and publishes the result; the decision re-checks the state it reads, so a slow read done before it can't apply to a team that moved on), and **hooks only queue** (`Status` appends an `event`; `Run` handles the queue in order on one goroutine).
- `agent.Registry` (`internal/agent/agent.go`) holds every agent; `Get(id)` finds one; `a.Driver().Prompt(text)` sends a chat prompt; `a.Timeline.Page(since, limit)` returns the agent's own last `limit` items newer than `since`, oldest first; `a.Timeline.State()` returns `(epoch, maxSeq)`, and seq never goes back.
- `Registry.OnStatus(id, provider, from, to)` runs **under the agent's lock** on every status change, so it must not read the agent: `Status` only queues. A turn end is `working`/`needsYou` → `done` (unseen) or `idle` (seen, or stopped by the user). A session that ends goes to `closed`.
- The transcript, which the timeline is built from, can land after the status changes (see `automation/scheduler.go`, `maxSummaryPolls`): `read` looks up to `reads` times, `Poll` apart.
- `since` remembers the timeline's `maxSeq` when pocketd last prompted each lead, so only items after it count as the answer. An `Ended` whose read started before a newer prompt is dropped. `since` is guarded by `mu`; `send` runs inside `change`.
- `Prompt` checks the team's state, sends, and stores `planning` in one `change`, so a turn end handled meanwhile waits on `mu` and then sees `planning`. `planning` reads under `mu` for the same reason.
- `hub.Hub.Publish(msg)` sends to every client and never blocks.
- In tests, an agent is added with `reg.Add(id, cwd, provider, driver)` and its timeline fed with `a.Record(timeline.Event{Kind: "user" | "assistant_text" | "result", ...})`. Most tests call `Ended` or `handle` directly, so they run without goroutines or sleeps; one runs `Run` to prove a queued status change reaches `Ended`, and the lag test sleeps.

**Step 1: Write the failing tests**

Create `internal/team/teams_test.go`:

```go
package team

import (
	"context"
	"encoding/json"
	"errors"
	"slices"
	"sync"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

// chat is a chat agent's driver that keeps the prompts pocketd sends it.
type chat struct {
	mu      sync.Mutex
	prompts []string
	err     error
}

func (c *chat) Prompt(text string) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.prompts = append(c.prompts, text)
	return c.err
}
func (c *chat) Interrupt() error                   { return nil }
func (c *chat) Compact() error                     { return nil }
func (c *chat) Settings(string, string) error      { return nil }
func (c *chat) Access(string) error                { return nil }
func (c *chat) Commands() ([]proto.Command, error) { return nil, nil }
func (c *chat) Close()                             {}

func (c *chat) sent() []string {
	c.mu.Lock()
	defer c.mu.Unlock()
	return slices.Clone(c.prompts)
}

type fixture struct {
	teams *Teams
	lead  *agent.Agent
	chat  *chat
	msgs  <-chan []byte
}

func newFixture(t *testing.T) fixture {
	h := hub.New()
	msgs, stop := h.Subscribe()
	t.Cleanup(stop)
	reg := agent.NewRegistry(h)
	c := &chat{}
	lead := reg.Add("3f9a0c1b-2d4e-4f60-8a7b-9c0d1e2f3a4b", "/w", "claude", c)
	return fixture{teams: &Teams{Store: open(t, t.TempDir()), Agents: reg, Hub: h, Poll: time.Millisecond}, lead: lead, chat: c, msgs: msgs}
}

// turn records one turn of the lead in its timeline.
func (f fixture) turn(prompt, reply string, ok bool) {
	f.lead.Record(timeline.Event{Kind: "user", Text: prompt})
	f.lead.Record(timeline.Event{Kind: "assistant_text", Text: reply})
	f.lead.Record(timeline.Event{Kind: "result", OK: ok})
}

// answer is the lead's turn on the last prompt pocketd sent it.
func (f fixture) answer(reply string, ok bool) {
	sent := f.chat.sent()
	f.turn(sent[len(sent)-1], reply, ok)
}

func (f fixture) team(t *testing.T) proto.Team {
	t.Helper()
	team, ok, err := f.teams.Store.Get(f.lead.ID())
	if err != nil || !ok {
		t.Fatalf("no team: %v", err)
	}
	return team
}

// published is the state of each team.update sent so far.
func (f fixture) published() []string {
	var states []string
	for {
		select {
		case raw := <-f.msgs:
			var m struct {
				Type string
				Team proto.Team
			}
			json.Unmarshal(raw, &m)
			if m.Type == "team.update" {
				states = append(states, m.Team.State)
			}
		default:
			return states
		}
	}
}

// await waits for the next team.update in state.
func (f fixture) await(t *testing.T, state string) proto.Team {
	t.Helper()
	timeout := time.After(5 * time.Second)
	for {
		select {
		case raw := <-f.msgs:
			var m struct {
				Type string
				Team proto.Team
			}
			json.Unmarshal(raw, &m)
			if m.Type == "team.update" && m.Team.State == state {
				return m.Team
			}
		case <-timeout:
			t.Fatalf("no team.update %s", state)
		}
	}
}

func TestATeamPromptSendsTheRulesAndProposesThePlanTheLeadAnswers(t *testing.T) {
	f := newFixture(t)
	if err := f.teams.Prompt(f.lead, "Add truncate"); err != nil {
		t.Fatal(err)
	}
	if got := f.chat.sent(); !slices.Equal(got, []string{Wrap("Add truncate")}) {
		t.Fatalf("sent %q", got)
	}
	f.answer("Two tasks, the export after the function.\n"+twoTasks, true)

	f.teams.Ended(f.lead.ID())

	got := f.team(t)
	if got.State != "proposed" || got.Repaired || !slices.Equal(ids(got.Tasks), []string{"t1", "t2"}) || got.UpdatedAt == 0 {
		t.Fatalf("got %+v", got)
	}
	if states := f.published(); !slices.Equal(states, []string{"planning", "proposed"}) {
		t.Fatalf("published %v", states)
	}
	if list, err := f.teams.List(); err != nil || len(list) != 1 || list[0].Lead != f.lead.ID() {
		t.Fatalf("list %+v %v", list, err)
	}
}

func TestAnUnusablePlanGetsOneRepairPromptThenTheTeamFails(t *testing.T) {
	f := newFixture(t)
	f.teams.Prompt(f.lead, "Add truncate")
	f.answer("I'd split it in two.", true)

	f.teams.Ended(f.lead.ID())

	repair := "Your plan block can't be used: no <anywhere_team_plan> block. Send the corrected <anywhere_team_plan> block only."
	if sent := f.chat.sent(); len(sent) != 2 || sent[1] != repair {
		t.Fatalf("sent %q", sent)
	}
	if got := f.team(t); got.State != "planning" || !got.Repaired {
		t.Fatalf("got %+v", got)
	}
	f.answer("Still thinking.", true)

	f.teams.Ended(f.lead.ID())

	if got := f.team(t); got.State != "failed" || got.Error != "no <anywhere_team_plan> block" {
		t.Fatalf("got %+v", got)
	}
	if sent := f.chat.sent(); len(sent) != 2 {
		t.Fatalf("a second repair: %q", sent)
	}
	if states := f.published(); !slices.Equal(states, []string{"planning", "planning", "failed"}) {
		t.Fatalf("published %v", states)
	}
}

func TestAPlanSentAfterTheRepairIsProposed(t *testing.T) {
	f := newFixture(t)
	f.teams.Prompt(f.lead, "Add truncate")
	f.answer("I'd split it in two.", true)
	f.teams.Ended(f.lead.ID())
	f.answer(twoTasks, true)

	f.teams.Ended(f.lead.ID())

	if got := f.team(t); got.State != "proposed" || !got.Repaired || len(got.Tasks) != 2 {
		t.Fatalf("got %+v", got)
	}
}

func TestAFailedPlanningTurnFailsTheTeam(t *testing.T) {
	f := newFixture(t)
	f.teams.Prompt(f.lead, "Add truncate")
	f.answer(twoTasks, false)

	f.teams.Ended(f.lead.ID())

	if got := f.team(t); got.State != "failed" || got.Error != "The lead's turn failed" {
		t.Fatalf("got %+v", got)
	}
	if sent := f.chat.sent(); len(sent) != 1 {
		t.Fatalf("sent %q", sent)
	}
}

func TestATurnEndThatReachesTheTimelineLateIsWaitedFor(t *testing.T) {
	f := newFixture(t)
	f.teams.Poll = 50 * time.Millisecond
	f.teams.Prompt(f.lead, "Add truncate")
	f.lead.Record(timeline.Event{Kind: "user", Text: f.chat.sent()[0]})
	f.lead.Record(timeline.Event{Kind: "assistant_text", Text: twoTasks})
	go func() {
		time.Sleep(10 * time.Millisecond)
		f.lead.Record(timeline.Event{Kind: "result", OK: false})
	}()

	f.teams.Ended(f.lead.ID())

	if got := f.team(t); got.State != "failed" || got.Error != "The lead's turn failed" {
		t.Fatalf("got %+v", got)
	}
}

func TestATurnBeforeTheTeamPromptIsNotReadAsItsAnswer(t *testing.T) {
	f := newFixture(t)
	f.turn("an earlier prompt", twoTasks, true)
	f.teams.Prompt(f.lead, "Add truncate")

	f.teams.Ended(f.lead.ID())

	if got := f.team(t); got.State != "planning" || !got.Repaired {
		t.Fatalf("got %+v", got)
	}
}

func TestALeadsLaterTurnsLeaveItsProposedTeamAsItIs(t *testing.T) {
	f := newFixture(t)
	f.teams.Prompt(f.lead, "Add truncate")
	f.answer(twoTasks, true)
	f.teams.Ended(f.lead.ID())
	f.turn("thanks", "You're welcome.", true)

	f.teams.Ended(f.lead.ID())

	if got := f.team(t); got.State != "proposed" || len(got.Tasks) != 2 {
		t.Fatalf("got %+v", got)
	}
	if sent := f.chat.sent(); len(sent) != 1 {
		t.Fatalf("sent %q", sent)
	}
}

func TestAnAgentWithoutATeamIsLeftAlone(t *testing.T) {
	f := newFixture(t)
	f.turn("hi", twoTasks, true)

	f.teams.Ended(f.lead.ID())

	if list, err := f.teams.List(); err != nil || len(list) != 0 {
		t.Fatalf("list %+v %v", list, err)
	}
	if sent := f.chat.sent(); len(sent) != 0 {
		t.Fatalf("sent %q", sent)
	}
}

func TestATeamPromptThatCantReachTheLeadFailsTheTeam(t *testing.T) {
	f := newFixture(t)
	f.chat.err = errors.New("This session has ended")

	err := f.teams.Prompt(f.lead, "Add truncate")

	if err == nil || err.Error() != "This session has ended" {
		t.Fatalf("got %v", err)
	}
	if got := f.team(t); got.State != "failed" || got.Error != "This session has ended" {
		t.Fatalf("got %+v", got)
	}
}

func TestATeamPromptIsRefusedWhileTheLeadsTeamRunsAndReplacesAnyOtherTeam(t *testing.T) {
	for _, state := range []string{"running", "review", "merging"} {
		f := newFixture(t)
		f.teams.Store.Put(proto.Team{Lead: f.lead.ID(), State: state, Tasks: []proto.TeamTask{}})

		err := f.teams.Prompt(f.lead, "Add truncate")

		if !errors.Is(err, ErrBusy) {
			t.Errorf("%s: got %v", state, err)
		}
		if got := f.team(t); got.State != state {
			t.Errorf("%s: the team became %s", state, got.State)
		}
		if sent := f.chat.sent(); len(sent) != 0 {
			t.Errorf("%s: sent %q", state, sent)
		}
	}
	for _, state := range []string{"planning", "proposed", "merged", "conflict", "failed"} {
		f := newFixture(t)
		f.teams.Store.Put(proto.Team{Lead: f.lead.ID(), State: state, Error: "an old one", Tasks: []proto.TeamTask{}})

		if err := f.teams.Prompt(f.lead, "Add truncate"); err != nil {
			t.Errorf("%s: %v", state, err)
		}
		if got := f.team(t); got.State != "planning" || got.Error != "" {
			t.Errorf("%s: got %+v", state, got)
		}
	}
}

func TestALeadThatClosesWhilePlanningFailsItsTeam(t *testing.T) {
	f := newFixture(t)
	f.teams.Prompt(f.lead, "Add truncate")

	f.teams.handle(event{kind: "status", agent: f.lead.ID(), from: "working", to: "closed"})

	if got := f.team(t); got.State != "failed" || got.Error != "the lead closed while planning" {
		t.Fatalf("got %+v", got)
	}
}

func TestALeadThatClosesLeavesATeamPastPlanningAlone(t *testing.T) {
	f := newFixture(t)
	f.teams.Prompt(f.lead, "Add truncate")
	f.answer(twoTasks, true)
	f.teams.Ended(f.lead.ID())

	f.teams.handle(event{kind: "status", agent: f.lead.ID(), from: "idle", to: "closed"})

	if got := f.team(t); got.State != "proposed" {
		t.Fatalf("got %+v", got)
	}
}

func TestRunHandsATurnEndHeardFromTheStatusToItsTeam(t *testing.T) {
	f := newFixture(t)
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	go f.teams.Run(ctx)
	f.teams.Prompt(f.lead, "Add truncate")
	f.answer(twoTasks, true)

	f.teams.Status(f.lead.ID(), "working", "done")

	if got := f.await(t, "proposed"); len(got.Tasks) != 2 {
		t.Fatalf("got %+v", got)
	}
}

func TestWithoutTeamsTheVerbsAreRefused(t *testing.T) {
	var teams *Teams
	if err := teams.Prompt(nil, "Add truncate"); err == nil || err.Error() != "Teams are off" {
		t.Fatalf("prompt: %v", err)
	}
	if _, err := teams.List(); err == nil || err.Error() != "Teams are off" {
		t.Fatalf("list: %v", err)
	}
	teams.Status("a1", "working", "done")
	teams.Run(context.Background())
}
```

What each proves: the prompt goes out wrapped and the answer is proposed, published at each step; the repair happens once; a repaired plan can still be proposed; a failed turn fails the team; a result that reaches the timeline late is waited for (without the retry the plan would be proposed); an older turn is never the answer; once proposed, the lead's other turns don't touch the team; agents without a team are ignored; an unsendable prompt fails the team; a running, reviewed or merging team refuses a new team prompt and sends nothing, while any other team is replaced; a lead that closes while planning fails its team and only then; a status change queued by `Status` reaches `Ended` through `Run`; a nil `Teams` refuses cleanly and its `Run` returns at once.

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/team -run 'Team|Turn|Lead|Agent|Plan'`
Expected: FAIL, `undefined: Teams`.

**Step 3: Write the implementation**

Create `internal/team/teams.go`:

```go
package team

import (
	"cmp"
	"context"
	"errors"
	"log"
	"sync"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
)

const (
	// reads is how many times a turn's end is looked for: the transcript can reach the timeline after the status.
	reads = 5
	// turnItems bounds the items a turn is read from.
	turnItems = 500
)

var (
	errOff = errors.New("Teams are off")
	// ErrBusy refuses a team prompt while the lead's team has workers at work or work to merge.
	ErrBusy = errors.New("This session's team is still running or waiting to merge")
)

// Teams is the one engine behind every lead's team. change is the only writer: it keeps each team in Store and
// tells every client of each change. Hooks only queue what agents did; Run handles it in order. A nil Teams
// refuses every verb.
type Teams struct {
	Store  *Store
	Agents *agent.Registry
	Hub    *hub.Hub
	// Poll is the wait between reads of a turn whose end isn't in the timeline yet; zero is a second.
	Poll time.Duration

	// mu is held by change, so one team change runs at a time; it also guards since.
	mu sync.Mutex
	// since is where each lead's timeline stood when pocketd last prompted it, so only the answer to that prompt is read.
	since map[string]int64

	qmu   sync.Mutex
	queue []event
	wake  chan struct{}
}

// event is something an agent did, queued by a hook that runs under the agent's lock.
type event struct {
	kind     string // "status"
	agent    string
	from, to string // status: the change
}

// Prompt makes lead plan text as a team, in place of any team it had that isn't running, in review or merging.
func (t *Teams) Prompt(lead *agent.Agent, text string) error {
	if t == nil {
		return errOff
	}
	var err error
	t.change(lead.ID(), func(team proto.Team) (proto.Team, bool) {
		if busy(team.State) {
			err = ErrBusy
			return team, false
		}
		if err = t.send(lead, Wrap(text)); err != nil {
			return proto.Team{Lead: lead.ID(), State: "failed", Error: err.Error()}, true
		}
		return proto.Team{Lead: lead.ID(), State: "planning"}, true
	})
	return err
}

// busy is whether a team in state has workers at work or work to merge.
func busy(state string) bool { return state == "running" || state == "review" || state == "merging" }

// List is every lead's team.
func (t *Teams) List() ([]proto.Team, error) {
	if t == nil {
		return nil, errOff
	}
	return t.Store.List()
}

// Status is Registry.OnStatus's. It runs under the agent's lock, so it only queues the change for Run.
func (t *Teams) Status(id, from, to string) {
	if t != nil {
		t.post(event{kind: "status", agent: id, from: from, to: to})
	}
}

func (t *Teams) post(e event) {
	t.qmu.Lock()
	t.queue = append(t.queue, e)
	t.qmu.Unlock()
	select {
	case t.wakeup() <- struct{}{}:
	default:
	}
}

// wakeup is the channel post wakes Run with.
func (t *Teams) wakeup() chan struct{} {
	t.qmu.Lock()
	defer t.qmu.Unlock()
	if t.wake == nil {
		t.wake = make(chan struct{}, 1)
	}
	return t.wake
}

// Run handles what agents did, in the order they did it, until ctx ends.
func (t *Teams) Run(ctx context.Context) {
	if t == nil {
		return
	}
	wake := t.wakeup()
	for {
		select {
		case <-ctx.Done():
			return
		case <-wake:
		}
		t.qmu.Lock()
		queue := t.queue
		t.queue = nil
		t.qmu.Unlock()
		for _, e := range queue {
			t.handle(e)
		}
	}
}

func (t *Teams) handle(e event) {
	switch {
	case e.kind == "status" && e.to == "closed":
		t.closed(e.agent)
	case e.kind == "status" && turnEnd(e.from, e.to):
		t.Ended(e.agent)
	}
}

// turnEnd is whether a status change ends a turn: working or asking, then done (unseen) or idle.
func turnEnd(from, to string) bool {
	return (from == "working" || from == "needsYou") && (to == "done" || to == "idle")
}

// closed fails lead's team when the lead closed while it planned.
func (t *Teams) closed(lead string) {
	t.change(lead, func(team proto.Team) (proto.Team, bool) {
		if team.State != "planning" {
			return team, false
		}
		team.State, team.Error = "failed", "the lead closed while planning"
		return team, true
	})
}

// Ended moves lead's planning team on from the turn that just ended.
func (t *Teams) Ended(lead string) {
	t.mu.Lock()
	team, ok, err := t.Store.Get(lead)
	since := t.since[lead]
	t.mu.Unlock()
	if err != nil || !ok || team.State != "planning" {
		return
	}
	a, err := t.Agents.Get(lead)
	if err != nil {
		return
	}
	text, failed := t.read(a, since)
	t.change(lead, func(team proto.Team) (proto.Team, bool) {
		// A prompt sent while this turn was read starts a turn of its own.
		if team.State != "planning" || t.since[lead] != since {
			return team, false
		}
		next, repair := Planned(team, text, failed)
		if repair != "" {
			if err := t.send(a, repair); err != nil {
				next.State, next.Error = "failed", err.Error()
			}
		}
		return next, true
	})
}

// read is the text of a's turn after since, and whether it failed, looking again while its end hasn't reached the
// timeline. A turn whose end never shows is taken as it stands.
func (t *Teams) read(a *agent.Agent, since int64) (string, bool) {
	var text string
	for i := range reads {
		if i > 0 {
			time.Sleep(cmp.Or(t.Poll, time.Second))
		}
		items, _ := a.Timeline.Page(since, turnItems)
		var ended, failed bool
		if text, ended, failed = Reply(items); ended {
			return text, failed
		}
	}
	return text, a.Summary().Failed
}

// send prompts lead with text, noting where its timeline stood. It runs inside change, under mu.
func (t *Teams) send(lead *agent.Agent, text string) error {
	_, seq := lead.Timeline.State()
	if t.since == nil {
		t.since = map[string]int64{}
	}
	t.since[lead.ID()] = seq
	return lead.Driver().Prompt(text)
}

// change applies f to lead's team (one with only Lead set when it has none) under mu and, when f says it changed
// it, saves it and tells every client. It is the only writer of teams; f re-checks the state it is given.
func (t *Teams) change(lead string, f func(proto.Team) (proto.Team, bool)) (proto.Team, bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	team, ok, err := t.Store.Get(lead)
	if err != nil {
		log.Printf("teams: %s: %v", lead, err)
		return team, false
	}
	if !ok {
		team = proto.Team{Lead: lead}
	}
	next, changed := f(team)
	if !changed {
		return team, false
	}
	next.UpdatedAt = time.Now().UnixMilli()
	if err := t.Store.Put(next); err != nil {
		log.Printf("teams: %s: %v", lead, err)
		return team, false
	}
	t.Hub.Publish(proto.NewTeamUpdate(next))
	return next, true
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test -race ./internal/team && go vet ./internal/team`
Expected: `ok  	pocketd/internal/team`, nothing from `go vet`.

### Task 2.5: `launch`: a new chat session's first prompt can start a team

**Files:**
- Modify: `packages/pocketd/internal/launch/launch.go` (`Launcher` struct ~line 74; `create` after `Check` ~line 134; the chat prompt block ~lines 201-207)
- Test: `packages/pocketd/internal/launch/launch_test.go`

**Context:** `Launcher.create` sends a chat session's first prompt with `a.Driver().Prompt(s.Prompt)` once the agent attaches; naming still uses the raw `s.Prompt`, which is what the user typed. A team prompt goes through `Teams.Prompt` instead, which stores `planning` before sending, so the turn end is never missed. `Teams` is nil-safe: nil answers "Teams are off". `launcher(t, projects)` and `spec(...)` are test helpers in `launch_test.go` and `argv_test.go`; `quiet` discards progress.

**Step 1: Write the failing test**

Add to `launch_test.go`:

```go
func TestATeamStartsOnlyFromAChatSessionsFirstPrompt(t *testing.T) {
	l := launcher(t, `[]`)
	owner := Who{Owner: true, Key: "owner"}
	s := spec("claude", "ask", false)
	s.Team, s.Prompt = true, "Add truncate"
	if r := l.Create(owner, "r1", s, quiet, func(Creating) { t.Fatal("creating") }); r.Err == nil || r.Err.Code != "invalid_spec" ||
		r.Err.Message != "A team starts from a chat session's first prompt" {
		t.Fatalf("no chat: %+v", r.Err)
	}
	s.Chat, s.Prompt = true, ""
	if r := l.Create(owner, "r2", s, quiet, func(Creating) { t.Fatal("creating") }); r.Err == nil || r.Err.Code != "invalid_spec" {
		t.Fatalf("no prompt: %+v", r.Err)
	}
	s.Prompt = "Add truncate"
	if r := l.Create(owner, "r3", s, quiet, func(Creating) { t.Fatal("creating") }); r.Err == nil || r.Err.Code != "unknown_project" {
		t.Fatalf("a chat with a prompt goes on to the project check: %+v", r.Err)
	}
}
```

**Step 2: Run the test to verify it fails**

Run: `go test ./internal/launch -run TestATeamStartsOnlyFromAChatSessionsFirstPrompt`
Expected: FAIL, `no chat: &{Code:unknown_project ...}`.

**Step 3: Write the implementation**

In `launch.go`, add `"pocketd/internal/team"` to the imports. Add to `Launcher` after `Locals`:

```go
	// Teams starts the team a create's first prompt asks for.
	Teams *team.Teams
```

In `create`, right after the `Check` block:

```go
	if s.Team && (s.Prompt == "" || !Chat(s)) {
		return Result{Err: fail("invalid_spec", "A team starts from a chat session's first prompt")}
	}
```

Replace the chat prompt block:

```go
	if r.Err == nil && s.Prompt != "" && Chat(s) {
		if a, err := l.d.Agents.Get(r.AgentID); err == nil {
			send := a.Driver().Prompt
			if s.Team {
				send = func(text string) error { return l.Teams.Prompt(a, text) }
			}
			if err := send(s.Prompt); err != nil {
				r.Err = &Failure{Code: "spawn_failed", Message: "Couldn't send the prompt", Detail: err.Error()}
			}
		}
	}
```

**Step 4: Run the test to verify it passes**

Run: `go test ./internal/launch`
Expected: `ok  	pocketd/internal/launch`

### Task 2.6: Wire `team.list`, `team` prompts and the store into pocketd

**Files:**
- Modify: `packages/pocketd/internal/peer/check.go` (`Needs`, after `"ws:automation.run"` ~line 45)
- Test: `packages/pocketd/internal/peer/check_test.go` (matrix, after `"ws:automation.run"` ~line 40)
- Modify: `packages/pocketd/internal/wsserver/wsserver.go` (`Server` struct ~line 100; `errorReply` ~line 408; `dispatch` first switch ~line 560 and `agent.prompt` ~line 619)
- Test: `packages/pocketd/internal/wsserver/wsserver_test.go` (after `TestAnOversizedPromptIsRefusedWithItsCode` ~line 840)
- Modify: `packages/pocketd/internal/proto/version.go` (`ServerCaps` ~line 36), `packages/pocketd/internal/proto/team.go` (`CapTeam`)
- Test: `packages/pocketd/internal/proto/team_test.go`
- Modify: `packages/pocketd/cmd/pocketd/serve.go` (after `d := &daemon.Daemon{...}` ~line 127; `d.Agents.OnStatus` ~line 143; after `l := launch.New(...)` ~line 172; the `ws := &wsserver.Server{...}` literal ~line 213)
- Test: `packages/pocketd/e2e/team_test.go` (from Task 2.1)

**Context:** `peer.Needs` is the verb allowlist: a verb missing there is refused for everyone, so `team.list` needs an entry (Observe: reading teams is like `agent.list`). The matrix test lists, per verb, the refusal code for owner, phone, legacy phone and PTY peer; `""` is allowed. `wsserver.dispatch` handles verbs without an agent in its first `switch`, then looks up `m.AgentID` for the rest; a returned error goes out through `errorReply`, which gives it a wire code when it knows one (as `automation.ErrBusy` → `automation_busy` in `wsserver/automation.go`). `proto.ServerCaps` lists what hello negotiates; `Negotiate` returns the caps both sides name, so `team.v1` reaches a client only once both list it. This is the task where team prompts start to work, so the cap lands here. `serve.go` builds every service; `Registry.OnStatus` is one func that fans out to events, the push notifier and the awake keeper. Opening the store can fail; like `skills`, pocketd then serves without teams (`teams` stays nil, and every `Teams` method, `Run` included, is nil-safe).

**Step 1: Write the failing tests**

In `check_test.go`'s matrix add:

```go
		"ws:team.list":          {"", "", "", ""},
```

Add to `internal/proto/team_test.go` (and `"slices"` to its imports):

```go
func TestTheServerOffersTheTeamCap(t *testing.T) {
	_, caps, code := Negotiate(Range{Min: 3, Max: 3}, []string{CapTeam}, ServerCaps)
	if code != "" || !slices.Equal(caps, []string{CapTeam}) {
		t.Fatalf("caps %v, code %q", caps, code)
	}
}
```

Add to `internal/wsserver/wsserver_test.go` (and `"pocketd/internal/team"` to its imports):

```go
func TestATeamPromptRefusedWhileItsTeamRunsSaysTeamBusy(t *testing.T) {
	if e := errorReply("p", team.ErrBusy); e.Code != "team_busy" || e.ID != "p" || e.Message != team.ErrBusy.Error() {
		t.Fatalf("%+v", e)
	}
}
```

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/peer ./internal/proto ./internal/wsserver`
Expected: FAIL: `ws:team.list by principal 0: "scope_denied", want ""` (and principals 1-3), `undefined: CapTeam`, and `TestATeamPromptRefusedWhileItsTeamRunsSaysTeamBusy` with `Code:""`.
The e2e tests from Task 2.1 still fail as described there.

**Step 3: Write the implementation**

`proto/team.go`, after `CodeTeamBusy`:

```go
// CapTeam: team prompts, team.list and team.update (and, with plan 4, team.start, team.stop and team.merge).
const CapTeam = "team.v1"
```

`proto/version.go`: append `CapTeam` to `ServerCaps`, after `CapSkills`:

```go
var ServerCaps = []string{"pair.v1", CapScopes, CapHost, CapRegistry, CapSummaryV2, CapLaunch, CapRestore, CapOpen, CapNames, CapAutomations, CapLocals, CapFork, CapAccounts, CapSubagents, CapSettings, CapCommands, CapAccess, CapTerminal, CapWorkspace, CapGit, CapGitHub, CapFiles, CapPush, CapSkills, CapTeam}
```

`peer/check.go`, in `Needs` after `"ws:automation.run": Own,`:

```go
	"ws:team.list":            Observe,
```

`wsserver/wsserver.go`: add `"pocketd/internal/team"` to the imports; add to `Server` after `Skills`:

```go
	// Teams serves team prompts and team.list; nil refuses them.
	Teams *team.Teams
```

In `errorReply`, before `if code := peer.CodeOf(err); code != "" {`:

```go
	if errors.Is(err, team.ErrBusy) {
		return proto.NewErrorCode(id, proto.CodeTeamBusy, err.Error())
	}
```

In `dispatch`'s first `switch`, before `case "worktree.rename":`:

```go
	case "team.list":
		teams, err := c.s.Teams.List()
		if err != nil {
			return err
		}
		c.send(proto.NewTeamList(m.ID, teams))
		return nil
```

and in the second `switch`, replace `case "agent.prompt": err = a.Driver().Prompt(m.Text)` with:

```go
	case "agent.prompt":
		if m.Team {
			err = c.s.Teams.Prompt(a, m.Text)
		} else {
			err = a.Driver().Prompt(m.Text)
		}
```

`cmd/pocketd/serve.go`: add `"pocketd/internal/team"` to the imports. Right after the `d := &daemon.Daemon{...}` literal:

```go
	teams := &team.Teams{Agents: d.Agents, Hub: h}
	if teams.Store, err = team.OpenStore(home); err != nil {
		log.Printf("teams: %v; serving without them", err)
		teams = nil
	}
```

In `d.Agents.OnStatus = func(id, provider, from, to string) {`, add as its first line:

```go
		teams.Status(id, from, to)
```

After `l.Locals = localStore`:

```go
	l.Teams = teams
	go teams.Run(ctx)
```

`Run` sits before `d.Adopt`/`d.Restore`, so it is draining the queue before the first agent's status changes. Plan 4 sets its own `Teams` fields right before this `go teams.Run(ctx)` line.

and add `Teams: teams` to the `ws := &wsserver.Server{...}` literal (after `Skills: skillStore`).

Run `gofmt -w internal/peer internal/wsserver cmd/pocketd`.

**Step 4: Run the tests to verify they pass**

Run: `go test ./internal/peer ./internal/proto ./internal/wsserver ./cmd/pocketd`
Expected: `ok` for each.

Run: `go test ./e2e -run Team -count=1 -v`
Expected: PASS for `TestATeamPromptMakesTheLeadPlanAndPocketdProposesItsPlan`, `TestAPlanThatCantBeUsedGetsOneRepairTurnThenTheTeamFails`, `TestANewSessionStartedAsATeamPlansItsFirstPrompt`, `TestATeamIsStillListedAfterARestart`.

If the repair test fails because the first `team.update` is already `failed` with `bad JSON`, the echo's last block isn't the rules' example; print `got` and compare it with `Wrap`'s text before changing anything.

### Task 2.7: `CONTEXT.md` gains the team words

**Files:**
- Modify: `CONTEXT.md` (append a section after the `### Automations` section's **Run** entry, at the end of the file)

**Context:** Each entry is `**Word**:` on its own line, a definition paragraph, then `_Avoid_:` with the words not to use. Mailbox is built in plan 2; its word lands here with the rest.

**Step 1: Add the section**

Append:

```markdown

### Teams

**Team**:
A request a lead split into tasks that workers do side by side, each in its own worktree, and that the user merges back. A team prompt starts it: the lead answers with a plan, which pocketd checks and proposes. The user edits and starts it, answers what workers ask beyond their allowlist, and merges. A lead has one team; a new team prompt replaces it, unless that team is running, in review or merging.
_Avoid_: swarm, crew, run (a Run is one firing of an automation)

**Lead**:
The chat session a team prompt went to. It plans the team, reviews each handback, and says "Ready to merge" when the work is right. It has no tools for the team; pocketd does what its plan says.
_Avoid_: orchestrator, manager, parent

**Worker**:
A chat session pocketd starts for one task of a team, in a new worktree on its own branch, nested under its lead. It changes only its task's files, and each of its turns that ends normally becomes a handback.
_Avoid_: sub-agent (a sub-agent runs inside one session), child

**Handback**:
What pocketd mails a lead when a worker's turn ends normally: the worker's summary and the `git diff --stat` of the commit pocketd made of its worktree.
_Avoid_: report, result

**Mailbox**:
A session's queue of messages from pocketd, such as handbacks and notes. A message stays queued until a turn that showed it ends normally; one shown in a turn that failed or was stopped is sent again.
_Avoid_: inbox, queue
```

**Step 2: Check**

Run: `grep -c '^\*\*Team\*\*:$\|^\*\*Lead\*\*:$\|^\*\*Worker\*\*:$\|^\*\*Handback\*\*:$\|^\*\*Mailbox\*\*:$' CONTEXT.md`
Expected: `5`

### Task 2.8: `Agent.SetAwaiting`: an agent between turns can still need you

**Files:**
- Modify: `packages/pocketd/internal/agent/agent.go` (`Agent` struct ~line 95; `status` ~line 293; `Working` ~line 339)
- Modify: `packages/pocketd/internal/daemon/handoff.go` (`carried` ~line 58)
- Modify: `packages/pocketd/internal/daemon/presence.go` (`save` ~line 379)
- Test: `packages/pocketd/internal/agent/agent_test.go`, `packages/pocketd/internal/daemon/handoff_test.go`

**Context:** An agent's status is derived in `status()`: `closed`, else its `phase` (`working`, or `needsYou` while an ask is open: `NeedsYou()` is called when the broker shows one), else `done` for an unseen turn end, else `idle`. A lead whose plan waits for **Start** has no turn and no ask, so a phase is wrong for it: every "is a turn underway" check would think it mid-turn. Instead an `awaiting` flag makes `status()` say `needsYou` only between turns (`idle` or `done`); the agent's next turn (`Working`) clears it. Clients, the push notifier and the phone read `status`, so they all see the wait with no client change. Two readers must not carry the wait, because it isn't a turn: the upgrade handoff's carried phase (`daemon.carried`; a restored phase `needsYou` would never return to `idle`) and the restart file's saved status (`presence.save`; `state.Outcome` would read a saved `needsYou` that came back `idle` as an interrupted turn). They read the new `Turn()`, the status without the wait. Keep-awake, the account switch, terminal input and the daemon's turn checks move to `Turn()` too (Task 2.10). `state(s)` in `agent_test.go` is the status plus ` failed`/` compacting`; `newDaemon`, `claudeIn`, `hookFrom`, `sessionStart`, `transcript` and `eventually` are the daemon tests' helpers (`handoff_test.go` uses them already).

**Step 1: Write the failing tests**

Append to `internal/agent/agent_test.go`:

```go
func TestAnAgentAwaitingTheUserNeedsYouBetweenTurnsUntilItsNextTurn(t *testing.T) {
	for _, tc := range []struct {
		name  string
		steps func(a *Agent)
		want  string
	}{
		{"awaiting while idle", func(a *Agent) { a.SetAwaiting(true) }, "needsYou"},
		{"awaiting outranks done", func(a *Agent) { a.Working(); a.TurnEnded(false); a.SetAwaiting(true) }, "needsYou"},
		{"no longer awaiting", func(a *Agent) { a.Working(); a.TurnEnded(false); a.SetAwaiting(true); a.SetAwaiting(false) }, "done"},
		{"a turn ends the wait", func(a *Agent) { a.SetAwaiting(true); a.Working() }, "working"},
		{"and the wait doesn't come back when the turn ends", func(a *Agent) { a.SetAwaiting(true); a.Working(); a.TurnEnded(false) }, "done"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			a := NewRegistry(hub.New()).Add("a1", "/w", "claude", fakeDriver{})
			tc.steps(a)
			if got := state(a.Summary()); got != tc.want {
				t.Fatalf("got %q, want %q", got, tc.want)
			}
		})
	}
}

func TestTurnIsTheStatusWithoutTheWait(t *testing.T) {
	a := NewRegistry(hub.New()).Add("a1", "/w", "claude", fakeDriver{})
	a.SetAwaiting(true)
	if got := a.Turn(); got != "idle" {
		t.Fatalf("idle: got %q", got)
	}
	a.Working()
	a.TurnEnded(false)
	a.SetAwaiting(true)
	if got := a.Turn(); got != "done" {
		t.Fatalf("done: got %q", got)
	}
}
```

Append to `internal/daemon/handoff_test.go`:

```go
func TestALeadAwaitingStartIsHandedOverAndSavedAsItsOwnTurn(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	hookFrom(d, pr, sessionStart("c1", transcript(t, "hi")))
	eventually(t, "the conversation", func() bool { return pr.a.Summary().ProviderSessionID == "c1" })
	pr.a.SetAwaiting(true)
	if s := pr.a.Summary().Status; s != "needsYou" {
		t.Fatalf("status %q", s)
	}
	if got := d.carried(pr.a.ID()); got == nil || got.Phase != "idle" {
		t.Fatalf("carried %+v", got)
	}
	var e state.Terminal
	pr.save(&e)
	if e.Status == "needsYou" {
		t.Fatalf("saved status %q", e.Status)
	}
}
```

What they prove: the wait reads `needsYou` only between turns, outranks `done`, ends with the next turn and doesn't return with its end; `Turn` leaves it out; an upgrade hands the lead over idle and a restart saves its own status.

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/agent -run 'Awaiting|Turn' -count=1 && go test ./internal/daemon -run TestALeadAwaiting -count=1`
Expected: FAIL to build, `a.SetAwaiting undefined (type *Agent has no field or method SetAwaiting)` and `a.Turn undefined`.

**Step 3: Write the implementation**

In `agent.go`'s `Agent` struct, after `goal            proto.Goal`:

```go
	awaiting        bool // between turns, the user has something to do for it, such as a team plan to start
```

Replace `status` with:

```go
// status is what clients see: the agent's turn, or needsYou while it awaits the user between turns.
func (a *Agent) status() string {
	s := a.turn()
	if a.awaiting && (s == "idle" || s == "done") {
		return "needsYou"
	}
	return s
}

// turn is the agent's own status, without the wait SetAwaiting adds.
func (a *Agent) turn() string {
	switch {
	case a.closed:
		return "closed"
	case a.phase != "idle":
		return a.phase
	case a.unseenEnd:
		return "done"
	}
	return "idle"
}

// Turn is the agent's status without SetAwaiting's wait: what an upgrade or a restart carries.
func (a *Agent) Turn() string {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.turn()
}

// SetAwaiting marks the agent as waiting on the user between turns, so it reads needsYou while idle or done.
// Its next turn clears it.
func (a *Agent) SetAwaiting(awaiting bool) {
	a.update(true, func() { a.awaiting = awaiting })
}
```

In `Working`, the line `a.stopped, a.prompted = false, false` becomes:

```go
		a.stopped, a.prompted, a.awaiting = false, false, false
```

In `daemon/handoff.go`'s `carried`, the phase block becomes:

```go
	phase := "idle"
	if turn := a.Turn(); turn == "working" || turn == "needsYou" {
		phase = turn
	}
```

In `daemon/presence.go`'s `save`, the line `e.AgentID, e.CreatedAt, e.Status, e.Failed, e.Origin, e.Pinned = s.ID, s.CreatedAt, s.Status, s.Failed, s.Origin, s.Pinned` becomes:

```go
	e.AgentID, e.CreatedAt, e.Status, e.Failed, e.Origin, e.Pinned = s.ID, s.CreatedAt, pr.a.Turn(), s.Failed, s.Origin, s.Pinned
```

**Step 4: Run the tests to verify they pass**

Run: `gofmt -l ./internal && go test ./internal/agent ./internal/daemon ./internal/state -count=1`
Expected: nothing listed; `ok` for each (the existing status, handoff and restore tests still pass: without the wait, `Turn` is `status`).

### Task 2.9: The lead needs you while its plan waits for Start

**Files:**
- Modify: `packages/pocketd/internal/team/teams.go` (`change`; new `awaits`)
- Test: `packages/pocketd/internal/team/teams_test.go`

**Context:** `change` is the only writer of teams, so it is the one place that knows a team just became, or stopped being, `proposed`. After each write it calls `awaits`: `proposed` sets the lead's wait (Task 2.8), any other state clears it, except `planning`. A re-plan (a team prompt while proposed) leaves it to the re-plan's own turn, whose `Working` clears it: clearing it here, before that turn starts, would turn `needsYou` into `idle`, which `Status` queues as a turn end, and `Ended` would read the half-started turn as the plan. A plain prompt to the lead ends the wait the same way. `SetAwaiting` runs `OnStatus`, whose `Teams.Status` only queues, so calling it under `mu` is safe. A restart forgets the wait; plan 4's restart recovery sets it again for every proposed team.

**Step 1: Write the failing tests** (append to `teams_test.go`)

```go
// proposedFixture is a fixture whose lead's plan waits for Start.
func proposedFixture(t *testing.T) fixture {
	f := newFixture(t)
	f.teams.Prompt(f.lead, "Add truncate")
	f.answer(twoTasks, true)
	f.teams.Ended(f.lead.ID())
	return f
}

func TestALeadNeedsYouWhileItsPlanWaitsUntilItsNextTurn(t *testing.T) {
	f := proposedFixture(t)
	if s := f.lead.Summary().Status; s != "needsYou" {
		t.Fatalf("proposed: %s", s)
	}
	f.teams.Prompt(f.lead, "Split it differently")
	if s := f.lead.Summary().Status; s != "needsYou" {
		t.Fatalf("a re-plan before its turn starts: %s", s)
	}
	f.lead.Working()
	if s := f.lead.Summary().Status; s != "working" {
		t.Fatalf("the re-plan's turn: %s", s)
	}
}

func TestALeadStopsNeedingYouWhenItsTeamLeavesProposed(t *testing.T) {
	f := proposedFixture(t)
	f.teams.change(f.lead.ID(), func(tm proto.Team) (proto.Team, bool) {
		tm.State = "running"
		return tm, true
	})
	if s := f.lead.Summary().Status; s != "idle" {
		t.Fatalf("started: %s", s)
	}

	g := proposedFixture(t)
	g.chat.err = errors.New("This session has ended")
	g.teams.Prompt(g.lead, "Split it differently")
	if s := g.lead.Summary().Status; s != "idle" {
		t.Fatalf("a re-plan that couldn't be sent: %s", s)
	}
}
```

What they prove: a proposed team's lead reads `needsYou`; a re-plan keeps the wait until its turn starts, which ends it; a team that starts (as plan 4's `Started` does through `change`) or fails ends it.

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/team -run 'NeedsYou|Needing' -count=1`
Expected: FAIL in `TestALeadNeedsYouWhileItsPlanWaitsUntilItsNextTurn` with `proposed: idle`. `TestALeadStopsNeedingYouWhenItsTeamLeavesProposed` already passes; it guards the clearing once the wait exists.

**Step 3: Write the implementation**

In `teams.go`'s `change`, after `t.Hub.Publish(proto.NewTeamUpdate(next))`:

```go
	t.awaits(lead, next.State)
```

and add after `change`:

```go
// awaits shows lead as needing you while its team is proposed, and stops in any other state but planning: a
// re-plan's own turn ends the wait (Agent.Working), as clearing it first would read as a turn end.
func (t *Teams) awaits(lead, state string) {
	if state == "planning" {
		return
	}
	if a, err := t.Agents.Get(lead); err == nil {
		a.SetAwaiting(state == "proposed")
	}
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test -race ./internal/team -count=1 && go test ./e2e -run Team -count=1`
Expected: `ok  	pocketd/internal/team`; the four team e2e tests still PASS.

### Task 2.10: The wait is for the user to see, not for keep-awake, a switch or turn checks

**Files:**
- Modify: `packages/pocketd/internal/agent/agent.go` (new `Registry.Busy`; the free `Busy` goes)
- Modify: `packages/pocketd/cmd/pocketd/serve.go` (the `keeper.Run` line ~156)
- Modify: `packages/pocketd/internal/daemon/switch.go` (new `between`; `switchPending` ~line 84 and 91, `move` ~line 173)
- Modify: `packages/pocketd/internal/daemon/claude.go` (`AskOpen` ~line 157; the `interrupted` check ~line 509)
- Modify: `packages/pocketd/internal/daemon/codex.go` (~line 187), `packages/pocketd/internal/daemon/codex_chat.go` (~line 248), `packages/pocketd/internal/daemon/stream.go` (`Prompt`, ~line 417)
- Test: `packages/pocketd/internal/agent/agent_test.go`, `packages/pocketd/internal/daemon/switch_test.go`, `packages/pocketd/internal/daemon/claude_test.go`

**Context:** Task 2.8 puts the wait in `Summary().Status`, which several pocketd readers take as "a turn is underway or an ask is open". These must not see it:
- **Keep-awake.** `serve.go` runs `keeper.Run(…, func() bool { return agent.Busy(d.Agents.List()) })`, and `Busy` counts `needsYou`: a proposed plan left overnight would keep the Mac awake. `Registry.Busy` reads each agent's `Turn()` instead, and the free `Busy` (its only caller is that line) goes, its test rewritten for the method.
- **Account switch.** `movable` lets an agent move only while `idle` or `done`, so a lead whose plan waits would hold a switch until Start. Its three callers pass `between(pr.a)`: the summary with `Turn()` as its status.
- **Terminal input.** `AskOpen` refuses a phone's or PTY peer's keys while the agent "waits on an ask" (`ask_open`); a waiting plan is no ask.
- **Turn checks**, same one-word change to `Turn()`: `claude.go`'s rejected-dialog `interrupted` check, `codex.go`'s "thread mid-turn" when mapping a new thread, `codex_chat.go`'s catch-up on reconnect, and `stream.go`'s `/goal clear` mid-turn check. Each asks whether a turn is running, which a wait between turns never is.

Readers that keep the wait, because it's what the user should see: clients and the phone (`agent.update`), the push notifier (its Needs-you alert and the Live Activity's count), `anywhere wait` (exits `needsYou`), automations' run status (`waiting`), and plan 2's mail pump (it holds mail while `needsYou`; no mail comes while a plan waits). One harmless quirk: `anywhere session prompt` to a waiting lead returns once the prompt is sent, not once its turn starts, since the CLI sees only the wire status. `newDaemon` and `claudeIn` are the daemon tests' helpers; an agent made with `d.Agents.Add(…, nil)` takes no driver calls here.

**Step 1: Write the failing tests**

In `internal/agent/agent_test.go`, replace `TestBusyCountsWorkingAndNeedsYou` with:

```go
func TestBusyIsATurnOrAnOpenAskButNotAWaitBetweenTurns(t *testing.T) {
	for _, c := range []struct {
		name  string
		steps func(a *Agent)
		busy  bool
	}{
		{"idle", func(*Agent) {}, false},
		{"done", func(a *Agent) { a.Working(); a.TurnEnded(false) }, false},
		{"working", func(a *Agent) { a.Working() }, true},
		{"asking", func(a *Agent) { a.Working(); a.NeedsYou() }, true},
		{"awaiting the user between turns", func(a *Agent) { a.SetAwaiting(true) }, false},
	} {
		t.Run(c.name, func(t *testing.T) {
			reg := NewRegistry(hub.New())
			c.steps(reg.Add("a1", "/w", "claude", fakeDriver{}))
			if got := reg.Busy(); got != c.busy {
				t.Fatalf("busy %t", got)
			}
		})
	}
	if NewRegistry(hub.New()).Busy() {
		t.Fatal("no agents is busy")
	}
}
```

Append to `internal/daemon/switch_test.go`:

```go
func TestAWaitForTheUserBetweenTurnsDoesntHoldASwitchButAnAskDoes(t *testing.T) {
	a := newDaemon(t).Agents.Add("a1", "/w", "claude", nil)
	a.SetAttached(true)
	a.SetConversation("c1")
	a.SetAwaiting(true)
	if movable(a.Summary(), "b") || !movable(between(a), "b") {
		t.Fatalf("awaiting: status %q, turn %q", a.Summary().Status, a.Turn())
	}
	a.Working()
	a.NeedsYou()
	if movable(between(a), "b") {
		t.Fatal("an open ask let the switch through")
	}
}
```

Append to `internal/daemon/claude_test.go`:

```go
func TestAWaitForTheUserBetweenTurnsIsNoOpenAsk(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	pr.a.SetAwaiting(true)
	if s := pr.a.Summary().Status; s != "needsYou" || d.AskOpen(term.Info().ID) {
		t.Fatalf("status %q, ask open %t", s, d.AskOpen(term.Info().ID))
	}
}
```

What they prove: keep-awake counts turns and open asks but not a waiting plan; a waiting plan doesn't hold an account switch, while an open ask still does; a phone or PTY peer can still type into a lead whose plan waits.

**Step 2: Run the tests to verify they fail**

Run: `go test ./internal/agent -run TestBusy -count=1 && go test ./internal/daemon -run 'TestAWaitForTheUser' -count=1`
Expected: FAIL to build, `reg.Busy undefined (type *Registry has no field or method Busy)` and `undefined: between`.

**Step 3: Write the implementation**

In `agent.go`, replace the free `Busy` with:

```go
// Busy is whether any agent is on a turn or has an ask open. A wait for the user between turns (SetAwaiting) is
// neither: it shouldn't keep the Mac awake.
func (r *Registry) Busy() bool {
	r.mu.Lock()
	all := slices.Collect(maps.Values(r.agents))
	r.mu.Unlock()
	return slices.ContainsFunc(all, func(a *Agent) bool {
		turn := a.Turn()
		return turn == "working" || turn == "needsYou"
	})
}
```

In `serve.go`, the keeper line becomes:

```go
	go keeper.Run(ctx, kick, d.Agents.Busy)
```

In `switch.go`, add `"pocketd/internal/agent"` to the imports and, after `movable`, add:

```go
// between is a's summary with its turn as its status, so a wait for the user between turns doesn't hold a switch.
func between(a *agent.Agent) proto.AgentSummary {
	s := a.Summary()
	s.Status = a.Turn()
	return s
}
```

and in `switchPending` (twice) and `move` (once), `movable(pr.a.Summary(), …)` becomes `movable(between(pr.a), …)`.

In `claude.go`, `AskOpen`'s return becomes `return asking || pr.a.Turn() == "needsYou"`, and in the transcript loop `pr.a.Summary().Status == "needsYou"` becomes `pr.a.Turn() == "needsYou"`. In `codex.go`, `status := pr.a.Summary().Status` (before `running :=`) becomes `status := pr.a.Turn()`. In `codex_chat.go`, `if status := pr.a.Summary().Status; st.Type == "active" …` becomes `if status := pr.a.Turn(); st.Type == "active" …`. In `stream.go`'s `Prompt`, `midTurn(c.a.Summary().Status)` becomes `midTurn(c.a.Turn())`.

**Step 4: Run the tests to verify they pass**

Run: `gofmt -l ./internal ./cmd && go vet ./... && go test ./internal/agent ./internal/daemon ./cmd/pocketd -count=1`
Expected: nothing listed; vet clean; `ok` for each.

### PR 2 final checks

From `packages/pocketd`: `gofmt -l .` prints nothing; `go vet ./... && go test ./...` passes (`go test -race -count=1 ./...` is what `scripts/check.sh` runs). From the repo root: `pnpm --filter @pocket/protocol test` shows `# fail 0`.

---

## Contract changes needed

All folded into the index (`docs/plans/2026-11-05-team-mode.md`): the iOS mirror rule, `Teams` as the one engine (plan 4 extends it), `Planned`/`Repair`/`Reply`, `Validate`'s extra checks, `ValidateKept` (1 to 4 tasks, for Start), `agent.create` with `team` needing `chat` and a prompt, `launch.Launcher.Teams` and `wsserver.Server.Teams`, plus `team.v1`, `team_busy`, a lead closing while planning, `teamrules.Strip`, and the lead reading `needsYou` while its team is proposed (`Agent.SetAwaiting`, `Agent.Turn`), which keep-awake, account switches, terminal input and turn checks ignore (`Registry.Busy` replacing `agent.Busy`, `daemon.between`).

## Open questions

- **Codex as lead** is covered only by the unit tests (it records a `result` item from `turn/completed`, as Claude does); there is no fakecodex e2e test for a team prompt.

Resolved since the first draft: the rules no longer show as the user's text or the title (`teamrules.Strip`); a team prompt to a running, reviewed or merging team is refused (`team_busy`), and the desktop sends one only when the lead is idle (plan 5); pocketd advertises `team.v1`; `team.start` refuses to drop a task a kept one lists in `after` (plan 4), so `Startable` never meets an `after` naming a removed task. **Resolved by the UX decisions:** a restart while planning is plan 4's restart recovery (a lead that never comes back fails the team; one that does has its last turn read again); Start may keep one task (`ValidateKept`); the lead reads `needsYou` while its plan waits, in pocketd (Tasks 2.8–2.9), so plan 6 needs no desktop-only override, and the wait neither keeps the Mac awake nor holds an account switch (Task 2.10).
