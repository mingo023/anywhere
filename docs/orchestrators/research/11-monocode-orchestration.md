# 11 — MonoCode: orchestration, automations, operator CLI, inbox

Date: 2026-09-30.

Sources:
- hardbeat920/monocode@cdc1441dc51e3709cd843e5c316608a123f323c6 (local clone), including `README.md` and `CHANGELOG.md` sections 0.1.46–0.5.0.
- Pocket worktree `orchestrator-research`@86deb13, used for mapping only.
- Cross-references: report 09 (`09-monocode-product.md`) and report 10 (`10-monocode-architecture-harness.md`) in this directory.

Citation legend: `M path:L` is a file and 1-indexed line in the MonoCode clone at the SHA above. `P path:L` is a file and line in this Pocket worktree, relative to its root (`packages/...`, `CONTEXT.md`). `Z` (Zeron) is not used. "CL x.y.z Lnn" is that version's section of `M CHANGELOG.md`, at line nn. A URL is cited as is. Where code and docs disagree, code wins and the disagreement is listed in F11. "Inference" marks claims not read directly in code.

## TL;DR

- **Orchestrator.**
  - A lead agent runs a pool of workers: 1–4 concurrent (default 2), at most 40 tasks per run.
  - Each worker gets its own git worktree `mc/orch-<12 chars>`, seeded with the lead's uncommitted files.
  - The lead drives workers with `<exe> control ACTION --json …`. A `review` copies the worker's changed files into the lead checkout, and refuses any file the lead changed in the meantime (`M src/features/orchestration/model/orchestration.ts:1021-1023`; `M src/features/orchestration/model/orchestrationCatalog.ts:16`; `M src/features/source-control/model/worktrees.ts:78-81`; `M src-tauri/src/checkpoint.rs:252`).
- **Plan first.**
  - The lead's first turn is read-only and must emit `<monocode_proposal>`.
  - The user edits the assignment card and clicks **Confirm & start**. Nothing executes before that; the turn is sent with harness intent `plan` (`M src/app/App.tsx:6792`; `M src/features/orchestration/model/orchestrationPlan.ts:244-265`; `M src/features/orchestration/ui/OrchestrationPreview.tsx:561`).
- **Guardrails.**
  - Per-grant token (two concatenated UUIDv4s: 64 hex chars, ~244 random bits), loopback only.
  - `requestId` idempotency.
  - Overlapping write scopes are queued.
  - An out-of-scope write stops only that worker; the check runs after the fact.
  - Workers are forbidden to spawn agents, use git or switch branches.
  - Worker approvals go to the lead, never to the user.
  - The lead is auto-woken at most 20 times (`M src-tauri/src/control.rs:56-57,183`; `M src/features/orchestration/model/orchestration.ts:174,1482-1487,2018-2021,2152`).
- **`/operator` app CLI.**
  - 13 actions: `models.list`, `sessions.list/read/send/draft/start`, `worktrees.list/create`, `folders.list/move`, `notes.list/read/write`.
  - `sessions.start` places a new session as a tab, a right split or a down split.
  - Usable only in a thread the user opted in with `/operator`, and only during an active turn (`M src-tauri/src/control_cli.rs:85-152`; `M src-tauri/src/control.rs:15`).
- **Automations.**
  - Triggers: schedules (hourly, daily, weekdays, weekly, local time) and "opened/created/appeared" events from GitHub, GitLab, Linear, Jira and Azure DevOps.
  - Storage: a SQLite ledger with compare-and-swap claims.
  - Missed-run grace defaults to 720 min. Each event key fires once per automation. Run history is capped at 100 (`M src-tauri/src/automations.rs:12-15,839-964`; `M src/features/automations/model/automations.ts:347-376`).
- **Automation gap.**
  - The scheduler (30 s) and the Inbox poll (30 s) run in the webview, so the app must be open.
  - Events that arrive while the app is closed never fire: the "primed" tracker is in memory (`M src/app/App.tsx:7286`; `M src/features/inbox/model/inboxNotifications.ts:24-51`).
  - pocketd is always on, so Pocket can do strictly better.
- **Inbox.** It lists external work items: GitHub through the `gh` CLI; Linear, Jira, GitLab and Azure DevOps through REST or GraphQL. Tokens are 0600 plaintext files, not Keychain. Fit for Pocket is low (F5, F8).
- **Notifications and quick composer.**
  - Notifications: off by default; suppressed while the window is focused and the session is visible; a "Show" action; per-project mute for 1/4/8 h.
  - Quick composer: global ⌘⇧Space opens a non-activating NSPanel, 680 px wide, 22% from the top (`M src/features/notifications/model/notifications.ts:13,90-107`; `M src-tauri/src/quick_composer.rs:41-55`).
- **Pocket today.**
  - pocketd's ops socket (0600) already accepts `spawn`, `prompt`, `input`, `screen` and `close`, and `POCKETD_SOCK` is injected into every terminal. Any agent can therefore drive Pocket, with no gating and no CLI verbs.
  - The phone cannot spawn anything, and worktree operations live only in the desktop (`P packages/pocketd/internal/ops/ops.go:67-77,118-158`; `P packages/pocketd/internal/daemon/plugin.go:65-81`; `P packages/pocketd/internal/proto/messages.go:63-80`; `P packages/desktop/crates/git/src/git.rs:166-173`).
- **Ranking for Pocket.**
  1. A scoped agent CLI on pocketd.
  2. Start-session and worktree operations in pocketd, for phone and desktop.
  3. A daemon-side automations scheduler.
  4. The orchestrator, built on 1 and 2.
  5. Notification polish.
  - Won't do: external inbox integrations, notes.

## Findings

### F1. Orchestration: trigger, planning, confirmation

- **Trigger.** The composer "+" popover lists four modes (`M src/features/sessions/ui/Composer.tsx`):

  | Mode | Colour | Subtitle | Line |
  |---|---|---|---|
  | Plan | yellow-300 | "Review a plan before building" | `:2194-2198` |
  | Operator | sky-300 | "Give this thread access to MonoCode" | `:2221-2225` |
  | Orchestrator | fuchsia-300/65, with a `v1` pill | "Plan and coordinate agent work" | `:2248-2257` |
  | Draft | – | "Save this message without starting the agent" | `:2284` |

  A selected mode shows as a chip (Orchestrator `bg-fuchsia-500/15`, `:2322`).
- **Planning turn.**
  - The lead gets "Investigate and plan only… No execution is authorized until the user confirms the assignment card".
  - It must return a `<monocode_proposal>` block (`M src/features/orchestration/model/orchestrationPlan.ts:244,249,265`).
  - The proposal is validated: "Choose 1 to 4 parallel workers" (`:126`), and dependency cycles are rejected (`:227`).
  - An invalid proposal gets one repair prompt that says "do not inspect the project again" (`:304`).
  - An interrupted plan shows "Planning was interrupted. Generate the assignments again." (`:375`).
- **Proposal card.**
  - Shows 3 tasks, then "Show N more tasks" (`M src/features/orchestration/ui/OrchestrationPreview.tsx:722`).
  - Has a "Parallel workers" radiogroup 1–4 (`:735-763`).
  - Buttons: "Try again" (`:549`), "Confirm & start" / "Starting…" (`:561`), "View agents" (`:578`).
  - Default workers: 2 (`M src/features/orchestration/model/orchestrationCatalog.ts:16`).
- **Confirm.**
  - `startApproved` refuses with "Return to the proposal's checkout before starting" if the user switched checkout (`M src/features/orchestration/model/orchestration.ts:581,603,651`).
  - It sets `workspacePolicy: "isolated-child"` (`:640`) and posts the confirmed card to the lead as a message (`:660`).
- **Run start guards.**
  - `startRun` requires 1–4 workers (`:695`).
  - It refuses if another session "is still running in this checkout" (`:725`).
  - It resets `continuations` to 0 (`:781`) and pumps (`:792`).
  - Independent turns in a controlled checkout get "This checkout is controlled by an orchestrator. Stop that run before starting independent work." (`M src-tauri/src/control.rs:406-427`).

### F2. Orchestration: lead control loop (`<exe> control`)

- **Binary and syntax.**
  - The desktop executable doubles as the CLI: argv[1] `control` runs the lead CLI, `app` runs the /operator CLI (`M src-tauri/src/main.rs:7,12`).
  - Syntax: `{exe} control ACTION [--json JSON | --input FILE|-] [--request-id ID]` (`M src-tauri/src/control_cli.rs:10`).
- **Actions** (12; `M src-tauri/src/control_cli.rs:80-83`; fields in `M src/features/orchestration/model/orchestration.ts:200-213`). Unknown fields are rejected with the list of accepted fields (`:214-225`).

  | Action | Input | Behaviour and limits |
  |---|---|---|
  | `list` | `{}` | Run, tasks, latest results, allowed harness/model IDs |
  | `delegate` | `title`≤160, `harness`, `model`≤256, `prompt`≤30,000, `files` (write scope, each ≤512), `dependsOn`≤40 | Queues a worker (`:1021-1059`). ≤40 tasks per run (`:1022-1023`). Scope is required: "Declare at least one file/directory scope…" (`:1057`). `["."]` reserves the whole checkout (`M src-tauri/src/control_cli.rs:22-23`) |
  | `get` | `taskId` | One task and its latest result |
  | `wait` | `timeoutSeconds` 0–25, default 20 | Blocks until a task changes. Wakes as soon as a worker asks for input (`M …/orchestration.ts:1420-1427`; CL 0.1.49 L325) |
  | `respond` | `taskId`, `requestId`, `decision` allow\|deny | Answers a worker approval (`:1201`) |
  | `answer` | `taskId`, `requestId`, `answers` \| `skip` | Answers a worker question (`:1225`). A stale id gets "Stale requestId" (`:1210,1234`) |
  | `steer` | `taskId`, `text` | Redirects a running worker mid-turn (`:1181`) |
  | `message` | `taskId`, `text` | New turn for a stopped worker; keeps its session, checkout and history (`:1102`) |
  | `retry` | `taskId`, `text`, `files` | Stopped worker with a corrected scope (`:1135,1155`) |
  | `cancel` | `taskId` | Running or queued |
  | `review` | `taskId` | Only `completed` tasks. Isolated tasks: `integration_started` → `integrateWorker` → `integrated` → cleanup → `cleaned` (`:1249-1340`) |
  | `finish` | `{}` | Requires every task to be accepted or cancelled: "Review all remaining tasks…" (`:1341-1407`) |

- **Lead envelope.** `<monocode_orchestration>` is appended to lead turns (`:833`). The CLI help says: "Agents never prompt the user; list, get and wait report the prompt as that task's needsInput" (`M src-tauri/src/control_cli.rs:31-33`).
  - Documented loop: `list -> delegate ... -> wait or get -> steer … -> respond/answer -> inspect -> message -> review each task -> finish` (`:52-54`).
  - When paused: "Do not keep polling or retry mutations… ask the user to click Resume" (`:56-58`).
- **Idempotency.**
  - `requestId` is ≤128 chars (`M src-tauri/src/control.rs:233`).
  - Each run keeps ≤512 request receipts (`M …/orchestration.ts:873-876`).
  - Reusing an id with different input fails with "Request ID was already used with different input" (`:853,859`).
  - A timeout says "Control request timed out. Retry with the same request ID." (`M src-tauri/src/control.rs:266`).
- **Scheduling (`pump`, `:1454-1530`).** A queued task starts only when all three hold:
  - active tasks < `maxWorkers` (`:1470`);
  - every `dependsOn` task is **accepted**, not merely completed (`:1476-1478`);
  - no active task has an overlapping scope (`:1482-1487`).
- **Auto-wake.**
  - When worker results or blocked requests appear, the lead gets a new turn: "Worker results are ready…" or "These agents are blocked waiting on you…" (`:2060,2063`).
  - Results are truncated to the last 4,000 chars and details to 2,000 (`:2042,2048`). Stored results keep the last 20,000 chars (`:1635,1658,1675`).
  - After 20 continuations the run pauses with "Automatic continuation limit reached. Its agents were stopped; review and resume the run." (`:2018-2021`).
- **Pause, stop, persistence.**
  - `pause` is at `:1818`; `stopRun` at `:1838`. Stopping the lead stops its workers (CL 0.1.46 L386).
  - Runs are saved through `control_save` (≤8,000,000 bytes, `M src-tauri/src/control.rs:518-523`).
  - After a restart the run hydrates as interrupted (`M …/orchestration.ts:529,544`).
  - Resume sends workers `recoveryTurn`: "Continue the existing assignment from its retained worker checkout… do not repeat destructive or external operations" (`:159`).

### F3. Orchestration: workers, isolation, integration

- **Data model** (`M src/features/orchestration/model/orchestrationState.ts`):
  - `WorkspacePolicy`: `shared | isolated-child | isolated-top-level`.
  - `DispatchStage`: `accepted → session_prepared → turn_submitted → settled → integration_started → integrated → cleaned`.
  - UI task labels: Needs input, Saved (not live), Paused, Queued, Working, Stopping, Done, Failed, Needs review, Interrupted, Cancelled (`M src/features/orchestration/model/orchestrationSummary.ts`).
- **Worktree.**
  - Branch `mc/orch-<12 lowercase alnum>` (`M src/features/source-control/model/worktrees.ts:78-81`), under the default root `<repo>-worktrees` (`M src-tauri/src/worktrees.rs:123-125`).
  - Seeded by copying the lead's uncommitted files. Symlinks, non-regular files and directory swaps are refused (`:246-274`). A failed seed is recovered or kept for manual review (`:353-361`).
  - Created by `git_orchestration_worktree_create` (`:392`) and removed by `:724`. Branch removal is guarded (`:752-768`).
- **Scratch directory.** `$TMPDIR/monocode-worker-<uuid>`, mode 0700 (`M src-tauri/src/control.rs:358,386,392`). The worker's TMPDIR, TMP and TEMP point there (`M …/orchestration.ts:172`).
- **Worker envelope `<monocode_assignment>`** (`M …/orchestration.ts:174`). Paraphrase:
  - Work only in this checkout, and edit only the scope.
  - Report a blocker and stop if another file is needed.
  - "Do not spawn agents, create worktrees, switch branches, stage, commit, push, install dependencies or run broad formatters/generators."
  - "Other workers may be working concurrently in separate checkouts."
  - Report focused checks, changed files, remaining issues and a concise final result.
  - The envelope is stripped from the visible transcript (`visibleUserPrompt`, `:178`).
- **Write-scope check** (`observe`, `:2094-2160`).
  - It runs on `tool.started`/`tool.updated` events with `preview.kind === "write"`, then canonicalises each path (`resolvePath`).
  - A path outside `writeScopes`+`scratchDir` blocks only that worker: "{title} reported a write outside its assignment… Only this worker was stopped and its checkout was retained" (`:2152`).
  - This is detection, not prevention. The tool has already started, and Bash writes that report no `write` preview go unchecked (inference from the `preview.kind` filter).
- **Approvals.** The host maps worker permission modes: "auto stays auto, supervised asks the lead" (`M src/app/App.tsx:8693`). Worker approvals are hidden from user toasts and notifications: "An orchestrated worker answers to its lead, never to the user directly" (`M src/features/notifications/model/approvalToast.ts:65-66`).
- **Integration** (`M src/app/App.tsx:8799-8838` → `M src-tauri/src/checkpoint.rs:202`).
  - Copies worker files into the lead checkout from the worker's session checkpoint (≤500 files, `:18`). Symlinks are refused (`:232`).
  - A file the lead changed since the worker started aborts with "Cannot integrate {relative}: the lead checkout changed since this worker started. The worker worktree was kept." (`:252`).
  - If either branch HEAD moved: "The worker or lead branch moved… The worker worktree was kept for manual review." (`M src/app/App.tsx:8799,8829`).
  - Git finalisation (commit, push) stays with the lead after integration (`M …/orchestration.ts:174`).
- **Sidebar agents card** (`M src/features/orchestration/ui/OrchestrationSidebarAgents.tsx`):
  - Header "N agents" and "x/N done" (`:88-91`).
  - Status colours: amber-400 needs input; `text-accent` + TerminalSpinner working; emerald-400 done (`:165-179`).
  - Row actions: "Open this agent beside the orchestrator" (`:206`), "See details" (`:217`), "Cancel task" (`:231`).
  - Paused state: "Resume continues interrupted workers…" (`:255`), an amber warning (`:258`), "Open blocker" (`:271`), "Wait for interrupted agents to stop" (`:280`).

### F4. `/operator` app CLI (agent access to MonoCode)

- **Opt-in.** A message starting with `/operator`, `/mono` or `/monocode` (`M src/features/sessions/model/operatorCommand.ts:20`) turns on app access for that thread. The flag is stored on the block (`block.monocode === true`, `:34,48`).
  - An empty command becomes "Explain what you can do in MonoCode with the app CLI." (`M src/app/App.tsx:6030`).
  - Not allowed inside an orchestration run: "Use /operator from a regular session turn, outside an orchestration run." (`:6021`).
  - During a busy turn: "/operator starts a new turn after the current turn finishes." (`:6053`).
- **Envelope `<monocode_app>`** (`M src/app/App.tsx:6820`). It lists the capabilities:
  - tabs, and splits right or down;
  - worktrees;
  - read and continue sessions;
  - drafts, folders, reading and writing notes.

  It instructs: "start with its latest two or three user/assistant exchanges" and "inherit permission mode". `sessions.start` returns after acceptance, not completion.
- **Gating.** Outside an active opted-in turn, the CLI fails with `APP_TURN_INACTIVE`: "…Use /operator once in this thread to enable it, then call the CLI during an active agent turn…" (`M src-tauri/src/control.rs:15`). Other refusals:
  - Orchestration workers are refused: "This session cannot use the MonoCode app CLI" (`M src/app/App.tsx:9024`).
  - App calls cannot enable /operator elsewhere (`M src/features/agent-app/model/agentApp.ts:130-132`).
- **Actions** (`M src-tauri/src/control_cli.rs:85-152`):

  | Action | Fields and limits |
  |---|---|
  | `models.list` | Providers, models, settings, permission modes |
  | `sessions.list` | IDs, busy, `hasDraft` |
  | `sessions.read` | `sessionId`, `before`, `limit`≤3 exchanges, `maxChars` 200–6000 (default 1200). Tools and reasoning omitted; pages via `nextBefore` (`M src/features/agent-app/model/sessionConversation.ts:21,65,67`) |
  | `sessions.send` | Idle sessions only; busy is rejected |
  | `sessions.draft` | Idle only; never overwrites an existing draft |
  | `sessions.start` | `prompt`≤240,000, `harness`, `model`, `effort`, `reveal` (default false, `agentApp.ts:251`), `workspaceMode` current\|worktree, `worktreeCwd`, `worktreeBase`, `draft`, `placement` tab\|right\|down, `besideSessionId` (right/down only, `agentApp.ts:377-390`), optional `runtimeMode` (defaults to inherit) |
  | `worktrees.list` / `worktrees.create` | `branch`, `base` (default HEAD), `existing` |
  | `folders.list` / `folders.move` | `folderId` or `newFolderName` |
  | `notes.list` / `notes.read` / `notes.write` | `limit` 30 / `offset`; `title`, `body`≤240,000, `tags` |

  - New session IDs are deterministic, `app-${source.id}-${requestId}`, so a retry cannot create a duplicate (`agentApp.ts:337,352,387`).
- **UI.**
  - Tool calls render as `monocode app <action>` rows (`M src/features/sessions/ui/AgentTranscript.tsx:3085,3167,3181,3211`).
  - The operator user bubble keeps its neutral `bg-content/10` (`:1685-1692`). On send it plays a one-shot amber halo and sparkle (`.monocode-sparkles`: halo 2400 ms, sheen 1100 ms), hidden under reduced motion (`M src/styles/index.css:586-731`).
  - The Operator chip is `bg-sky-500/15` (`M …/Composer.tsx:2305`).

### F5. Control transport and security (shared by F2 and F4)

- **Server.**
  - TCP on `127.0.0.1:0` (`M src-tauri/src/control.rs:183`), with 8 reader threads and a 32-slot queue that also covers unauthenticated sockets (`:196-198`).
  - Per socket: 3 s read/write timeouts (`:219-220`); one request line ≤256 KiB (`:228`); ≤24 pending requests (`:242`); 35 s wait for the owning window's reply (`:266`).
- **Tokens.** Two concatenated simple UUIDv4s per grant (64 hex chars; ~244 random bits, since each v4 UUID has 122) (`:56-57,325-326`), passed as `MONOCODE_CONTROL_ENDPOINT/TOKEN` and `MONOCODE_APP_ENDPOINT/TOKEN` in child env (`:465-479`). The request is forwarded to the owning webview as the `monocode-control-request` event (`M src/app/App.tsx:9003`).
- **Client.**
  - Refuses non-loopback endpoints (`M src-tauri/src/control_cli.rs:308`).
  - Timeouts: connect 3 s, read 40 s, write 5 s (`:311-321`).
  - Response caps: 4,000,000 bytes (app) and 2,000,000 (control) (`:330`).
  - Errors name the fix: "No MonoCode connection. Confirm the Orchestrator proposal in MonoCode first." (`:300`), "…this agent's sandbox may be blocking localhost." (`:314`).
- **Filesystem.** `control_write_path` and `control_scopes` canonicalise paths (`M src-tauri/src/control.rs:581,609`). Scopes are rebased from absolute to project-relative, and paths outside the project are rejected (CL 0.1.47 L377).

### F6. Automations

- **Data model** (`M src/features/automations/model/automations.ts:8-94`):
  - `Automation{name, prompt, harness, model, modelSettings, cwd, workspaceMode current|worktree|existing, worktreeCwd, sessionFolderId, reuseSession, runtimeMode, triggers[], missedRunGraceMinutes, enabled, nextRunAt, lastRun*, lastSessionId}`.
  - `AutomationTrigger{kind time|github|linear|jira|gitlab|azuredevops, event, scheduleKind hourly|daily|weekdays|weekly, minute, time, dayOfWeek, repos, branch, actor}`.
  - `AutomationRun{trigger scheduled|manual|event, status pending|running|succeeded|failed|skipped|cancelled, scheduledFor, sessionId, eventKey, prompt}`.
- **Storage** (SQLite, `M src-tauri/src/automations.rs:189-213`):
  - `automations(id, definition_json, enabled, next_run_at, updated_at)` with index `(enabled, next_run_at)`;
  - `automation_runs(id, automation_id FK cascade, created_at, run_json)`;
  - `automation_event_claims(automation_id, event_key, created_at, PK(automation_id, event_key))`.
- **Validation limits** (`:12-15,247-315`):
  - name ≤200; prompt ≤1,000,000; ≤20 triggers; ≤50 repos; folder id ≤80;
  - `runtime_mode` supervised|auto-accept-edits|auto|full-access;
  - grace 0–43,200 min;
  - `next_run_at` must be in the future.
- **History.** Keeps 100 runs per automation and never trims non-terminal runs (`:591-612`).
- **Defaults** (`newAutomationDraft`, `M …/automations.ts:347-376`): workspace `worktree`, runtime `auto`, Weekdays 09:00, grace 720 min, enabled.
- **Schedule math** (local time, `:294-331`): hourly at `:MM`, daily, weekdays (skips Sat/Sun), weekly by `dayOfWeek`. `nextRunAt` is the minimum over time triggers; with no time trigger it is now + 1 year (`:237-245`). The preview reads "Next run Wed 1 Oct, 09:00 <zone>" (`:276-292`).
- **Due claim (CAS).**
  - The renderer runs `evaluate()` on mount, every 30 s, and when the tab becomes visible (`M src/app/App.tsx:7264-7296`).
  - It walks up to 100 overdue occurrences (`M …/automations.ts:247-262`) and calls `automations_claim_due`, which runs `UPDATE automations SET next_run_at=?1 WHERE id=?2 AND enabled=1 AND next_run_at=?3 AND next_run_at<=?4` (`M src-tauri/src/automations.rs:839-862`).
  - Lateness beyond grace → a `skipped` run with "Missed the scheduled run beyond its grace period." (`:879-885`).
- **Event claim.**
  - The Inbox poll's `onAppeared` calls `claimInboxAutomationRuns` (`M src/app/App.tsx:7298-7315,9463`), which does `INSERT OR IGNORE` into claims, so each event key fires once per automation (`M src-tauri/src/automations.rs:900-964`).
  - Event key: `<provider>:<kind>:<repo>:<number>` or `<provider>:issue:<id>`, sanitised, ≤400 chars (`M src/features/automations/model/automationEvents.ts:71-82`).
  - Supported events (`:34-40`):
    - GitHub: `draft_opened`, `pull_request_opened`, `issue_opened`
    - GitLab: `merge_request_opened`, `issue_opened`
    - Linear: `issue_created`
    - Jira: `issue_created`
    - Azure DevOps: `pull_request_appeared`, `work_item_appeared`
  - Prompt = automation prompt + blank line + `inboxStartDraft(item)`, for example "Work on this GitHub issue:\n\n#N title\nurl" (`:113-143`; `M src/features/inbox/model/githubTasks.ts:1175-1205`).
  - Failed launches retry from localStorage `monocode.automation-inbox-retries.v1`, capped at 500 (`automationEvents.ts:30-31`).
- **Launch** (`M src/app/App.tsx:7017-7155`).
  - Reuses `lastSessionId` if it has the same harness, is idle, is not reserved, and is in the same checkout. Otherwise it creates a new session titled with the automation name (or the harness label for events), carrying `automationId` and `linkedWorkItem`.
  - Worktree mode passes `workspaceMode:"worktree", worktreeBase:"HEAD"`. The session goes into `sessionFolderId`.
  - Run status: `running` → `submitWithSettlement` → `succeeded|cancelled|failed`.
- **Recovery** (`M src-tauri/src/automations.rs:737-811`; `M src/app/App.tsx:7015,7240-7262`):
  - runs still `running` → `cancelled` "Interrupted when MonoCode last stopped.";
  - event runs without a stored prompt → `failed` "The Inbox event prompt was not available after restart.";
  - `pending` runs relaunch.
  - "Run now" creates a `manual` run (`automation_run_now`, `:814-836`).
- **UI** (`M src/features/automations/ui/AutomationsView.tsx`, 1965 lines):
  - List: header "Automations", "Filter automations", "New automation"; empty states "No automations yet" / "No matching automations".
  - Row actions: Run now (`:1002`), Pause/Enable (`:1019`), Delete automation (`:1083`).
  - Editor sections:
    - Triggers (`:1134`, with "Search triggers").
    - Instructions (`:1314`).
    - Session (`:1366`): Working copy "This repo, or a fresh worktree"; Conversation "New chat, or continue the last run" ("Continue last" is disabled in worktree mode, `:1397`); Session folder; Missed-run grace, with options Do not catch up / 30 min / 2 h / 12 h / 24 h (`:1628-1644`).
    - Run history (`:1452`): Trigger, Triggered, Duration.
  - Trigger categories: Scheduled, GitHub, Linear, Jira, GitLab, Azure DevOps (`:1663-1696`). Schedule labels: "Every hour at", "Every day at", "Every weekday at", "Every week on" (`:1832-1837`).
- **Templates** (`M src/features/automations/model/automationTemplates.ts:52-337`), 14 in total:
  - Find critical bugs
  - Scan codebase for vulnerabilities
  - Generate docs
  - Add test coverage
  - Review pull requests
  - Review draft PRs
  - Audit dependencies
  - Scan for secrets
  - Triage GitHub issues
  - Triage new issues (Linear)
  - Watch failing checks
  - Weekly changelog
  - Repo health check
  - Environment doctor

### F7. Inbox, inbox_media, Ask, CI repair

- **Polling** (`M src/features/inbox/hooks/useInboxUnseen.ts:70-73,240-253,348`):
  - `POLL_MS` 30,000; fallback refresh 60,000; ≤3 concurrent lookups.
  - One chime per batch (`playCue("inboxUnseen")`). Self-authored changes are suppressed and marked seen (pending age 10 min, `M src/features/inbox/model/inboxSelfActivity.ts`).
  - Seen state lives in localStorage `monocode.inboxSeen`.
- **Tracker.** `InboxNotificationTracker` keeps revisions and `primed` providers in memory. The first poll per provider only primes (`M src/features/inbox/model/inboxNotifications.ts:22-69`).
- **Items** (`M src/features/inbox/model/githubTasks.ts:44-46,75-92,177-180`):
  - The kind drives the item model. PR actions: merge, squash, rebase, draft, ready, close, reopen.
  - Cache is fresh for 30 s; "All" is capped at 100 items.
  - GitHub goes through `gh issue|pr list --state --limit 1..100 --repo --json … [--assignee @me] [--search]` (`M src-tauri/src/fs.rs:2721-2770`).
  - Filters: assignedToMe, hidden projects/kinds, time, status open/draft/closed/merged.
- **Media proxy** (`M src-tauri/src/inbox_media.rs:8-11`):
  - HTTPS only; 20 s timeout; 5 redirects; URL ≤8192.
  - Host allowlist: GitHub user-attachments/assets, `*.githubusercontent.com`, `uploads.linear.app`, GitHub S3.
  - HTML/JS MIME types are rejected. Size ≤25 MiB (`M src-tauri/src/fs.rs:18`).
  - A `gh auth token` bearer is sent to GitHub hosts only. Blobs are served locally so the webview CSP stays closed.
- **Ask about an item.** `inboxAskPrompt` wraps `src/instructions/inbox.md`: a read-only remote review, with no clone, checkout or worktrees, where content is treated as untrusted. It adds "INBOX ITEM (reference data)" JSON and "USER MESSAGE" (`M src/features/inbox/model/inboxAsk.ts`). Ask sessions are excluded from approval toasts (`M src/features/notifications/model/approvalToast.ts:64`).
- **CI repair** (`M src/features/inbox/model/ciRepair.ts`):
  - Limits: prompt ≤12,000 chars; check list ≤3,000; failed steps ≤8; annotations ≤5.
  - Prompt text: "Fix the selected failed CI checks for <repo> PR #N… Do not commit or push unless asked… untrusted CI data".
  - Tracked in `monocode.ciRepairs.v1` with phases running, completed, failed, cancelled, interrupted.
  - Same as 09-17.

### F8. Integrations (issue trackers)

- **Common.** 20 s HTTP timeout, default limit 40. Credentials are plaintext files, mode 0600, in `app_data_dir`, not Keychain.

  | Provider | API | Query | Auth / storage |
  |---|---|---|---|
  | Jira (`M src-tauri/src/jira.rs`) | REST v3 `/rest/api/3/search/jql` | `assignee = currentUser()`, `statusCategory != Done`. Comments ≤50, projects ≤100 | Basic email+API token; `jira-config.json` (`:885-944`) |
  | Linear (`M src-tauri/src/linear.rs`) | GraphQL `https://api.linear.app/graphql` | `assignee.isMe`. Teams and comments first 50 | `linear-token` (`:668-722`) |
  | GitLab (`M src-tauri/src/gitlab.rs`) | REST, default `https://gitlab.com` | `/todos?state=pending`. Diff ≤2 MiB | `PRIVATE-TOKEN`; `gitlab-config.json` |
  | Azure DevOps (`M src-tauri/src/azure_devops.rs`) | API 7.1, cloud and on-prem | WIQL `$top`. Diff ≤2 MiB total, 512 KiB/file, 40 files | PAT Basic `:token`; `azure-devops-config.json` |

- `src/features/connections` is about remote machines, not these integrations (see report 10 §8).

### F9. Notifications, approval toasts, dock badge, quick composer

- **Notification rules** (`M src/features/notifications/model/notifications.ts`):
  - Off by default (`:13`).
  - `shouldNotify` = enabled && !(window focused && session visible) && permission granted|prompt (`:90-107`).
  - Focus comes from the Tauri focus event, because WKWebView keeps `hasFocus()` true in the background (`:80-81`).
  - One notification per pending input `requestId` (`:118-155`).
  - Content: title "MonoCode"; subtitle = session title; body = the question, "Approve: <tool>", or the last assistant paragraph, ≤240 chars (`:165-217`).
- **Preferences** (`M src/features/notifications/model/notificationPreferences.ts:1-26`): categories pullRequests, issues, agentFinished, agentInput, reminders. Mute for 1/4/8 h, or `mutedUntil null` = until resumed. Stored in `monocode.projectNotifications.v1`.
- **Native layer** (`M src-tauri/src/notifications.rs`):
  - UNUserNotificationCenter on macOS (`:1-9`).
  - id `session:<id>/<nanos>` (`:102-119`).
  - Category `monocode.session` with a foreground "Show" action (`:345-357`).
  - Presents Banner, List and Sound in the foreground (`:302-312`).
  - A click emits `monocode:notification-click`, which jumps to the session.
- **Dock badge** = number of sessions needing input (`M src/features/notifications/model/dockBadge.ts:14`). Same as 09-1.
- **Approval toasts** (`M src/features/sessions/ui/ApprovalToasts.tsx:43-135`):
  - Stack: fixed `right-3`, width `min(360px, 100vw-24px)`, gap 8.
  - Card: `rounded-xl border-dashed border-content/20 bg-content/10 backdrop-blur-xl shadow-xl`.
  - Content: 13 px semibold title; amber-400 CircleAlert label at 11 px; body `line-clamp-3` at 12 px.
  - Buttons: Allow `bg-content`, Deny `bg-content/10`.
  - Entry animation: 180 ms `cubic-bezier(0.22,1,0.36,1)` from `translateY(-8px) scale(0.98)` (`M src/styles/index.css:1930-1942`).
- **Quick composer: yes, it has a global hotkey** (`M src-tauri/src/quick_composer.rs`).
  - Panel: non-activating NSPanel subclass `MonoCodeQuickComposerPanel`, with canBecomeKey YES and canBecomeMain NO (`:1-9,580-600`).
  - Size: width 680, initial height 128, max height 520, radius 16, positioned 22% from the top of the monitor under the cursor (`:41-55,584-586`).
  - Window: transparent, `Effect::Popover`, always on top, visible on all workspaces, skip taskbar (`:505-540`).
  - Shortcut: default `Command+Shift+Space` via tauri-plugin-global-shortcut (`:55,118,191`). It must include Cmd or Ctrl: "Use Command or Control with another key." (`:57-66`).
  - Prompt ≤256 KiB. The screenshot button runs `/usr/sbin/screencapture -i -x -t png` after a 150 ms sleep (`:395-414`).
  - Launches go through a queue of 64 and are claimed until acknowledged (`M src-tauri/src/quick_composer/delivery.rs`).
  - Keys (`M src/features/quick-composer/ui/QuickComposer.tsx:295-316`): Esc dismisses; Enter starts in background; ⌘Enter starts and reveals; ⌘P project picker; ⌘. model picker.
  - Placeholder: "Start a <Harness> session in <project>…" (`:431-439`).
  - macOS only, opt-in in Settings → General (CL 0.1.56); rebindable (CL 0.2.0 L121). Same as 09-6.

### F10. Notes, search, skills

- **Notes** (`M src-tauri/src/notes.rs:11-70`):
  - Table `notes(id, slug UNIQUE, title, body, tags_json, source_session_id, source_cwd, created_at, updated_at)`.
  - Limits: title ≤200; body ≤1,000,000 (≤240,000 through the app CLI); tag ≤48 chars, ≤20 tags.
  - Images ≤20 MiB (png/jpg/jpeg/gif/webp/svg) in `note-assets`.
  - Agents write notes linked to the calling session (`notes.write`, CL 0.4.0 L58).
- **Search.**
  - Backend: ≤500 matches; files ≤512 KiB; `git grep` output ≤4 MiB; case, whole-word, regex, include and exclude options; cancellable by `searchId` (`M src-tauri/src/search.rs:10-13`).
  - App search scopes: all, conversations, files, projects (`M src/features/search/model/appSearch.ts:19`).
- **Skills** (`M src-tauri/src/skills.rs`):
  - ≤300 skills; frontmatter ≤16 KiB.
  - Roots: `.agents/skills` (project, then user); `.claude`, `.cursor`, `.codex`, `.opencode`, `.pi` skills; `~/.pi/agent/skills`.
  - Claude plugin skills are found via `installed_plugins.json` plus settings enablement. `/operator` is a builtin skill (`M src/features/sessions/model/operatorCommand.ts:4`).

### F11. Code vs docs disagreements

1. **Isolation copy is stale.** The proposal help says "Every worker edits this same project folder…" (`M src/features/orchestration/ui/OrchestrationPreview.tsx:399-405`), and the footer says "Awaiting confirmation · Shared project folder" (`:777-779`).
   - Code sets `isolated-child` (`M …/orchestration.ts:640,1088`); a run with `version === 1` maps to `shared` and later runs to `isolated-child` (`M src/features/orchestration/model/orchestrationState.ts:181`). CL 0.1.52 L242 says workers use seeded worktrees. Only v1 runs share the folder.
   - The composer pill still says `v1` (`M …/Composer.tsx:2253`).
   - Report 09's "workers share one checkout" reflects the stale copy.
2. **Operator bubble colour.** README says the bubble is a "translucent amber bubble" (`M README.md:46-55`). Code: the bubble is neutral `bg-content/10`; amber is only a one-shot 2.4 s halo and sparkle (`M src/styles/index.css:586-731`).
3. **README omissions.** README lists notes.list/read only, but code and CL 0.4.0 L58 have `notes.write`. README also omits `reveal` and `worktreeBase` (`M src-tauri/src/control_cli.rs:108-140`).
4. **"Without the foreground window."** CL 0.1.52 L228 says automations run "without keeping the window in the foreground". True, but the scheduler and the Inbox poll run in the webview, so the app must be running. Events that occur while it is closed are never fired (in-memory primed set, `M src/features/inbox/model/inboxNotifications.ts:24-51`).
5. **Trigger filters.** Trigger `actor` accepts only "" or "anyone" (`M src/features/automations/model/automationEvents.ts:234`). `branch` is validated but unused in matching (`:206-231`).
6. **Hidden workspace mode.** `workspaceMode:"existing"` exists in the model and backend (`M …/automations.ts:8`) but not in the UI, which offers Current or Fresh worktree (`M …/AutomationsView.tsx:1628-1644`).
7. **Wrong path in report 09.** Report 09 cites the scheduler as `M src/App.tsx`. The correct path is `M src/app/App.tsx:7286`.

### F12. Pocket today vs MonoCode, and ranking

| Concern | Pocket | MonoCode |
|---|---|---|
| Agent → host channel | Unix socket at `POCKETD_SOCK` (dir 0700, sock 0600), JSON ops `list/spawn/hook/attach/input/prompt/resize/screen/close` (`P packages/pocketd/internal/ops/ops.go:15-32,67-77,118-158`) | Loopback TCP plus a per-grant token (F5) |
| Who can call it | Every terminal: `POCKETD_SOCK` and `POCKETD_PTY` are in each terminal's env (`P packages/pocketd/internal/daemon/plugin.go:65-81`). No opt-in, no scopes | Only an opted-in thread during an active turn; workers refused |
| CLI verbs | `pocketd serve \| run \| attach \| hook` (`P packages/pocketd/cmd/pocketd/main.go:10-29`) | `control` (12) and `app` (13) actions with `--help` text |
| Spawn | Desktop and `pocketd run` send `spawn` (`P packages/desktop/crates/daemon/src/daemon.rs:126`; `P packages/pocketd/cmd/pocketd/run.go:23`) | `sessions.start` with placement |
| Placement | New terminals get a new tab automatically (`P packages/desktop/crates/workspace/src/workspace.rs:19-37`) | tab / right / down / besideSessionId |
| Worktrees | Desktop only: `git worktree add -b` / `remove --force` (`P packages/desktop/crates/git/src/git.rs:141,166-173`) | Rust backend plus an agent CLI |
| Phone | WS `agent.list/prompt/interrupt/compact/close/view/seen/timeline`, `permission.resolve`. No spawn or worktree message (`P packages/pocketd/internal/proto/messages.go:63-80`; `P packages/protocol/src/messages.ts:13-58`) | Not applicable (desktop app) |
| Driving an agent | `Driver{Prompt, Interrupt, Compact, Close}`, prompt typed into the PTY (`P packages/pocketd/internal/agent/agent.go:16-21`) | Structured protocol |
| Tool events | Claude hooks `PreToolUse` (matcher `AskUserQuestion\|ExitPlanMode` only), `PermissionRequest`, `Stop`…; only `PermissionRequest` gets a reply (`P packages/pocketd/internal/daemon/plugin.go:13`; `P packages/pocketd/internal/daemon/daemon.go:90-120`) | `tool.started` events |
| "Inbox" | Attention list of Needs you / Failed / unseen Done agents, type `Note` (`P packages/desktop/crates/pocket/src/inbox.rs:11-40`) | External work items; "Notes" is a markdown store |
| Scheduler | None | Renderer-side, 30 s |

- **Consequence.** Pocket already has the transport, but with no authorisation. Any process in a Pocket terminal (including a prompt-injected agent) can spawn commands, type into other terminals (`input`/`prompt`) and read their screens (`screen`). Fixing that comes before any agent CLI.
- **Pocket advantage (potential).** Claude's `PreToolUse` hook fires **before** the tool runs. Today pocketd registers it only for `AskUserQuestion|ExitPlanMode` and never replies to it (`P packages/pocketd/internal/daemon/plugin.go:13`; `P packages/pocketd/internal/daemon/daemon.go:108-109`). Widening the matcher and returning a deny would make write-scope enforcement preventive, where MonoCode's is after the fact (F3).
- **Ranking for Pocket** (value to "terminals + agents on a Mac, driven from desktop and phone" ÷ cost):
  1. Scoped ops socket plus an agent CLI (11-1, 11-2): unlocks everything below.
  2. Start-session and worktree operations in pocketd, over WS for the phone (11-3, 11-4).
  3. Daemon-side automations: time triggers first, runnable with no UI open (11-9, 11-10).
  4. Orchestrator lead/worker: highest ceiling, highest cost (11-5 to 11-8).
  5. Notification polish and phone push (11-12); quick composer (11-13).
  6. Last or never: external Inbox integrations, Notes, app-wide search (11-16, 11-17).

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 11-1 | Agent CLI verbs `pocketd app sessions.list/read/send/start`, `worktrees.list/create`, `--json`/`--input -`/`--request-id`, self-describing `--help`, errors that name the fix | An agent in one terminal can fan out, read or continue sibling agents without the user | `M src-tauri/src/control_cli.rs:85-152,300,314`; `M src/features/agent-app/model/agentApp.ts:337-390` | `packages/pocketd/cmd/pocketd` new subcommand plus `internal/ops` new ops; adapt | M | 11-2 |
| 11-2 | Per-terminal token for the ops socket plus capability scopes (observe / drive-self / drive-others / spawn), granted on opt-in (for example `/pocket` in the prompt or a desktop toggle) | Closes the current ungated `input`/`screen`/`spawn` path for any process in a terminal | `P packages/pocketd/internal/daemon/plugin.go:65-81`; `M src-tauri/src/control.rs:15,56-57,465-479` | `packages/pocketd/internal/ops`, `internal/daemon/plugin.go`; new | M | none |
| 11-3 | `agent.start` / `terminal.spawn` WS message (cwd or worktree, command claude\|codex, prompt, draft flag) and desktop placement hint tab\|right\|down | Start an agent from the phone; agents open splits beside the caller | `M src-tauri/src/control_cli.rs:117-140`; `P packages/pocketd/internal/proto/messages.go:63-80`; `P packages/desktop/crates/workspace/src/workspace.rs:31-37` | `packages/pocketd/internal/proto`, `packages/protocol`, `packages/app`, `packages/desktop/crates/workspace`; new (see 10-3) | M | 11-4 for the worktree option |
| 11-4 | Move worktree create/list/remove into pocketd (branch from base, `existing` branch mode, auto name) | Phone and agents can create isolated checkouts; one code path | `M src-tauri/src/worktrees.rs:123-125,392,724-768`; `P packages/desktop/crates/git/src/git.rs:141,166-173` | `packages/pocketd` new `internal/git`; desktop calls pocketd; port | M | none |
| 11-5 | Orchestrator run: plan-only lead turn → editable assignment card → Confirm & start → 1–4 concurrent workers (default 2), ≤40 tasks, `dependsOn` gated on acceptance, overlapping scopes queued | Parallel agents on one goal with a human gate before any edit | `M …/orchestrationPlan.ts:126,227,244-304`; `M …/orchestration.ts:581-792,1454-1530` | `packages/pocketd` new `internal/orch`; desktop card in `crates/pocket`; phone card in `packages/app`; new | XL | 11-1, 11-2, 11-4; headless or PTY prompt drivers (10-1, 10-2) |
| 11-6 | Worker isolation: `pocket/orch-<id>` worktree seeded with the lead's uncommitted files (refuse symlinks and special files), private 0700 scratch TMPDIR, envelope banning git, spawning and broad formatters | Workers cannot trample each other or the lead | `M src-tauri/src/worktrees.rs:246-274,353-361`; `M src-tauri/src/control.rs:386,392`; `M …/orchestration.ts:172-174` | `packages/pocketd/internal/orch`; port | L | 11-4 |
| 11-7 | Preventive write-scope check: pocketd denies `Edit`/`Write`/`MultiEdit` outside scope in the `PreToolUse` hook (canonical paths), blocks only that worker, reports to the lead | Stronger than MonoCode (detection only) | `M …/orchestration.ts:2094-2160`; `P packages/pocketd/internal/daemon/daemon.go:92-121`; `P packages/pocketd/internal/daemon/plugin.go:13` | `packages/pocketd/internal/daemon`: widen the `PreToolUse` matcher and add a reply path (today only `PermissionRequest` replies); new | M | 11-5 |
| 11-8 | Integrate on review: per-file copy from worker to lead, refusing files the lead changed since seeding and symlinks; keep the worktree on conflict; branch-moved guard; lead owns commit | Safe merge-back without git merge conflicts mid-run | `M src-tauri/src/checkpoint.rs:18,202-252`; `M src/app/App.tsx:8799-8838` | `packages/pocketd/internal/orch`; port | L | 11-6 |
| 11-9 | Daemon automations: schedules hourly/daily/weekdays/weekly (local time), Run now, pause, missed-run grace (default 720 min), CAS claim on `next_run_at`, 100-run history, restart recovery | Recurring agent jobs that run with the desktop and phone closed (MonoCode cannot) | `M src-tauri/src/automations.rs:12-15,189-213,591-612,737-897`; `M …/automations.ts:294-372` | `packages/pocketd` new `internal/automation` (JSON or SQLite), WS list/run; desktop editor; new | L | 11-3 |
| 11-10 | Automation launch policy: new terminal in a fresh worktree by default, or reuse the last session if it is idle, has the same agent and is in the same checkout; runs grouped in a sidebar folder | Clean isolation, or continuity for "continue last run" | `M src/app/App.tsx:7017-7155` | `packages/pocketd/internal/automation`; adapt | S | 11-9, 11-4 |
| 11-11 | Event triggers via `gh` polling (PR opened, draft opened, issue opened), event key `<provider>:<kind>:<repo>:<n>` claimed once, **persistent** seen set | "Review every new PR" without webhooks; no lost events across restarts | `M …/automationEvents.ts:34-40,71-82`; `M src-tauri/src/automations.rs:900-964`; `M src/features/inbox/model/inboxNotifications.ts:24-51` | `packages/pocketd/internal/automation`; new | M | 11-9 |
| 11-12 | Notification polish: suppress while the agent is visible and the window is key, one notification per request id, "Show" action, per-project mute 1/4/8 h, dock badge; APNs push on phone | Fewer duplicate pings; reach the user away from the Mac | `M src/features/notifications/model/notifications.ts:90-155`; `M src-tauri/src/notifications.rs:345-357`; `P packages/app/package.json` | `packages/desktop/crates/pocket` (adapt 09-1, 09-2); `packages/app` expo-notifications (new) | M | none |
| 11-13 | Quick composer: global ⌘⇧Space non-activating panel, 680 px wide, 22% from the top; Enter = start in background, ⌘Enter = reveal; ⌘P project, ⌘. agent | Start an agent from any app | `M src-tauri/src/quick_composer.rs:41-66,505-600`; `M src/features/quick-composer/ui/QuickComposer.tsx:295-316` | `packages/desktop/crates/pocket` new window; GPUI panel; new (09-6) | L | 11-3 |
| 11-14 | Prompt templates: 14 automation presets plus the CI-repair prompt (≤12,000 chars, failed steps ≤8, "untrusted CI data") | One-click useful jobs | `M src/features/automations/model/automationTemplates.ts:52-337`; `M src/features/inbox/model/ciRepair.ts` | `packages/pocketd/internal/automation` assets; port text (09-17) | S | 11-9 |
| 11-15 | Idempotent CLI mutations: `requestId` receipts (≤512 per run, mismatched reuse rejected), deterministic new-session id from caller+requestId | Agent retries after timeouts never double-spawn | `M …/orchestration.ts:853-876`; `M …/agentApp.ts:337,387`; `M src-tauri/src/control.rs:266` | `packages/pocketd/internal/ops`; port (pairs with 10-13) | S | 11-1 |
| 11-16 | External Inbox (GitHub/Linear/Jira/GitLab/ADO work items with PR merge actions) | Triage without a browser | F7, F8 | new; out of scope for a terminals app | XL | Keychain storage |
| 11-17 | Agent-writable Notes store | Persist agent findings | `M src-tauri/src/notes.rs:11-70` | Skip; the repo's files serve this | M | vocabulary decision |
| 11-18 | CONTEXT.md terms before building: Run, Lead, Worker, Assignment, Automation; keep "Inbox" meaning Pocket's attention list and do not introduce a MonoCode-style Inbox or Notes | Avoids clashes with `inbox.rs` `Note` | `P packages/desktop/crates/pocket/src/inbox.rs:11-40`; `P CONTEXT.md:5-60` | `CONTEXT.md`; new | S | none |

## UI/UX spec to copy

- **Composer mode popover** (`M …/Composer.tsx:2194-2338`):
  - Rows `rounded-lg px-2 py-2 gap-2.5`: 16 px icon, 13 px name, 11 px `text-content/45` one-line subtitle, check on the right when selected.
  - Chips `h-6.5 rounded-md px-1.5 text-[11px]` in the mode colour (sky Operator, fuchsia Orchestrator, yellow Plan).
- **Proposal card** (`M …/OrchestrationPreview.tsx`):
  - 3 tasks visible, then "Show N more tasks".
  - Each task shows title, harness/model and file scope.
  - "Parallel workers" is a segmented 1–4: buttons `size-5 rounded-[5px] text-[11px]`, selected `bg-selection-hover`. A help popover 250 px wide explains concurrency (`:394`).
  - Footer: state text on the left; primary "Confirm & start" (→ "Starting…"), secondary "Try again", "View agents" once running.
  - Pocket copy must say "each worker gets its own worktree" (fixes F11.1).
- **Lead sidebar agents card** (`M …/OrchestrationSidebarAgents.tsx:88-280`):
  - "N agents · x/N done".
  - Rows: status dot (amber needs input, accent spinner working, emerald done); hover actions "Open beside", "See details", "Cancel task".
  - Paused banner in amber: "Resume continues interrupted workers…" plus "Open blocker".
- **Operator send highlight.** One-shot halo 2400 ms, delay 150 ms, ease-out; sheen 1100 ms, delay 300 ms, `cubic-bezier(0.4,0,0.2,1)`. Dark `#fde68a` with glow `rgb(251 191 36/.9)`; light `#f59e0b`. None under reduced motion (`M src/styles/index.css:586-731`). The bubble itself stays neutral.
- **Orchestrator completion celebration.** Hub 600 ms, delay 120 ms; ring 700 ms, delay 220 ms; node 600 ms; `cubic-bezier(0.3,0.6,0.4,1)` (`M src/styles/index.css:1052-1119`; CL 0.4.0 L67).
- **Approval toast.** See F9. Worker approvals never toast; the lead answers them.
- **Automation editor.**
  - Four sections in fixed order: Triggers, Instructions, Session, Run history.
  - Session rows use a label plus one-line helper: "This repo, or a fresh worktree", "New chat, or continue the last run", "Where runs appear in the sidebar", "Catch up if a scheduled run was missed".
  - Grace options: Do not catch up, 30 min, 2 h, 12 h, 24 h.
  - Under the schedule: "Next run Wed 1 Oct, 09:00 <zone>".
  - History columns: Trigger, Triggered, Duration.
- **Quick composer.** See F9. Textarea 16/24 px, padding `pl-5 pr-9 pt-4 pb-2`. The placeholder names the agent and project.
- **CLI help as contract.**
  - Every action lists its JSON shape and limits.
  - Errors carry the next step ("Retry with the same request ID", "Confirm the … proposal first", "sandbox may be blocking localhost").
  - Unknown fields list the accepted ones (`M …/orchestration.ts:214-225`).

## Open questions / risks

- **Scale of Pocket's socket exposure.** Any process in any Pocket terminal can `input`/`prompt`/`screen` other terminals today (`P packages/pocketd/internal/ops/ops.go:118-158`, `P packages/pocketd/internal/daemon/plugin.go:65-81`). Is that intended? It must be settled before exposing CLI verbs (11-2).
- **Result extraction over a PTY.** Pocket prompts by typing (`P packages/pocketd/internal/agent/agent.go:16-21`). A worker's "final result" must come from the Claude transcript or Codex items, not the screen. Is that reliable enough for the lead to review? Or does orchestration require the headless drivers from 10-1 and 10-2?
- **Permission model per worker.** MonoCode maps "auto stays auto, supervised asks the lead" (`M src/app/App.tsx:8693`). In Pocket, a worker's `PermissionRequest` goes to the phone broker (`P packages/pocketd/internal/daemon/daemon.go:123-160`). Rerouting to a lead agent removes the human from the loop; is that acceptable?
- **Detection vs prevention.** MonoCode never checks Bash writes (F3). A future Pocket `PreToolUse` deny (11-7) would cover Edit/Write but not Bash side effects either. Is worktree isolation the real boundary?
- **Integration by file copy.** Changes the lead makes to the same file mid-run block integration; the worktree is kept. What is the UX for resolving that on a phone?
- **Continuation cap.** 20 auto-wakes and a 4,000-char result truncation are MonoCode's cost guards. Pocket needs equivalent caps, and a phone control to raise them.
- **Automations with the Mac asleep.** pocketd runs only while the Mac is awake. Missed-run grace covers sleep. Does pocketd need a `caffeinate`/launchd wake schedule?
- **Event polling.** Event triggers need `gh` logged in on the Mac; `gh` rate limits at a 30 s poll across many repos are unmeasured.
- **Placement from pocketd.** The desktop owns tabs and splits (`P packages/desktop/crates/workspace/src/workspace.rs:19-37`). A "right of caller" hint needs a new daemon→desktop layout message.
- **Credentials.** If any tracker integration is added, use Keychain, not MonoCode's 0600 plaintext files (F8).
- **Unverified.** The "v1" pill vs the isolated code path suggests the orchestration UI copy lags the code. Behaviour of `isolated-top-level` was not traced.

## Verification

Date: 2026-09-30. Claims checked: 15. Corrected: 6.

Confirmed against source: 40-task and 1–4/default-2 worker limits; `mc/orch-<12>` branch; lead-changed-file integration refusal; plan-only proposal prompt and Confirm & start; 12 `control` and 13 `app` actions; `APP_TURN_INACTIVE` gating; loopback TCP, 3 s / 256 KiB / 24 pending / 35 s server limits and client timeouts; pump rules (maxWorkers, accepted deps, scope overlap); 20-continuation cap; CAS `UPDATE … next_run_at` claim, `INSERT OR IGNORE` event claim, 100-run cap, 720-min grace; renderer 30 s scheduler and in-memory `primed` set; quick composer 680/128/520/16/0.22 and ⌘⇧Space, key bindings; notification defaults and 1/4/8 h mute; approval toast tokens; operator bubble neutral (README amber claim is stale); Pocket ops socket ops and 0600, `POCKETD_SOCK`/`POCKETD_PTY` injection, phone WS message set (no spawn/worktree), desktop-only worktree git calls, `pocketd` CLI verbs, `inbox.rs` `Note`.

- Token size: "256-bit" was wrong. Two v4 UUIDs give 64 hex chars but ~244 random bits (TL;DR, F5).
- Event triggers: Azure DevOps events are "appeared", not "opened/created" (TL;DR).
- Plan-first: added evidence that the planning turn runs with harness intent `plan` (`M src/app/App.tsx:6792`); previously only the prompt text was cited.
- Citation: `newAutomationDraft` defaults end at line 376 (grace 720 is line 374), not 372.
- Citation: approval-toast skip for orchestrated workers is `approvalToast.ts:65-66`, not 64-65.
- Pocket `PreToolUse`: report claimed pocketd "can deny" tools in `PreToolUse`. It cannot today: the hook matcher is `AskUserQuestion|ExitPlanMode` only and the handler returns nil (only `PermissionRequest` replies) (`P packages/pocketd/internal/daemon/plugin.go:13`; `P packages/pocketd/internal/daemon/daemon.go:91,108-109`). Fixed in F12 table, "Pocket advantage", idea 11-7 mapping (now "new", needs matcher + reply path), and the Detection vs prevention open question.
