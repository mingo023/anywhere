# 02 — Zeron: architecture, engine, sync, transport

Date: 2026-09-30

Sources:
- Zeron: github.com/zeronsh/comet (aka zeronsh/zeron) @ `ed3b1aae4a5189eef67143db7b8c5c3ee7a933c5` (2026-09-29), MIT, workspace version 0.2.99 (Z Cargo.toml:24). Local clone: /Users/mingo/tmp/orchestrators/zeron.
- Pocket: this worktree @ `86deb13`.
- Expo push API: https://docs.expo.dev/push-notifications/sending-notifications/

Citation legend: `Z path:L` is a file and line in the Zeron clone, relative to its root. `P path:L` is a file and line in this Pocket worktree, relative to its root. `M` (monocode) is not cited in this report. A URL cites an external primary source. Where Zeron's docs and code disagree, the code is treated as truth and the gap is called out (see §F14). Line numbers are for the SHAs above.

## TL;DR

- **One Rust binary.** The engine is a library.
  - The headed app probes `127.0.0.1:27654`. It uses a running daemon if there is one; otherwise it embeds the engine behind an in-memory RPC client that uses the same protocol.
  - `zeron headless` is the daemon, installed as a launchd or systemd unit (Z apps/zeron/src/main.rs:281-288; Z crates/rpc/src/lib.rs:296-298; Z apps/zeron/src/daemon.rs:15,237).
- **Chats are not PTYs.**
  - claude runs as `--print --input-format stream-json ...` and codex as `codex app-server`. PTY terminals are a separate subsystem (Z crates/harness/src/claude/mod.rs:172; Z crates/harness/src/codex/mod.rs:1; Z crates/engine/src/terminals.rs:6,36).
  - Pocket is PTY-first, so Zeron's run-journal and command-ledger machinery only partly transfers.
- **Device-to-device traffic goes through a Cloudflare DeviceRoom Durable Object (DO) byte relay**, one per host device.
  - Each remote client becomes a virtual connection into the same RPC server, so every RPC works remotely (Z crates/rpc/src/device_room.rs:7-36).
  - There is no end-to-end encryption; the relay sees plaintext (Z docs/PARITY.md:101-102).
- **CRDT use is narrow.**
  - Loro 1.13 holds per-chat transcript docs only (Z Cargo.toml:48).
  - The workspace registry was moved off Loro onto per-field hybrid-logical-clock last-writer-wins rows (HLC LWW) after three DO wedges in one week (Z docs/registry-sync.md:8-12).
  - Transcripts needed whale-healing, history retention cut from 30 to 3 days, and tool output cut to 160 chars (Z crates/doc/src/constants.rs:6-9; Z crates/doc/src/parts.rs:18).
- **Durability is local-first.**
  - SQLite in WAL mode (`docs.sqlite3`) holds a durable outbox with stable batch IDs (Z crates/sync/src/store.rs:55,97-99; Z docs/session-publication.md:15-16).
  - A per-chat JSONL run journal enables crash recovery, with up to 3 auto-resumes within 12h (Z crates/engine/src/run_journal.rs:3,8; Z crates/engine/src/sessions.rs:836-837).
- **Auth.**
  - WorkOS AuthKit JWTs are verified at the edge via JWKS. Desktop sign-in uses a loopback callback on port 27641; headless uses paste-code (Z edge/src/auth.ts:50-51; Z crates/engine/src/lib.rs:632-635; Z edge/src/auth-routes.ts:8).
  - Local IPC has no auth token. It listens on loopback only and rejects any handshake that carries an `Origin` header (Z crates/engine/src/lib.rs:980; Z crates/rpc/src/server.rs:156-166).
- **Push.**
  - The edge's RegistryRoom sends APNs alerts on session-row transitions (done / input / failed), with per-device preferences and a collapse id per chat.
  - Delivery is best effort with no retry (Z edge/src/registry-room.ts:503-551; Z edge/src/push-notify.ts:46-99).
- **iOS is a thin client.** A Rust core (`zeron-client` over UniFFI) talks to the edge directly: registry WS, chat2 WS, and the device relay for host RPCs. No agent runs on the phone (Z crates/client/src/lib.rs:1-7; Z crates/client/src/live/urls.rs:33-102).
- **Updates.** The R2 `manifest.json` carries a sha256 per artifact. Checks run hourly. The staged binary must pass a `--version` probe, and the daemon restarts only when quiescent. There is no publisher signature (Z crates/update/src/lib.rs:6-10,51,73).
- **For Pocket:**
  - Now: push notifications on Status transitions, byte-bounded frames, phone liveness and redial, idempotent prompts, a launchd unit.
  - Next: an outbound-only relay with end-to-end encryption, for off-LAN access without Tailscale.
  - Do not adopt a CRDT for transcripts.

## Findings

### F1. Process model

- **Single binary `zeron`** with clap subcommands (Z apps/zeron/src/main.rs:32-60):
  - `headless`, `login`, `logout`, `status`
  - `sync` (per-room introspection)
  - `mcp` (a stdio MCP server that proxies to engine IPC; main.rs:258-260)
  - `daemon {install,uninstall,start,stop,restart,status}`
  - `update {--check}`
  - `appshot` (Linux)
- **Headed mode** (no subcommand):
  - The UI probes `ZERON_IPC_PORT` (default 27654) and connects if a daemon answers (main.rs:281-288).
  - Otherwise it runs the engine in-process through `memory_client`: two 256-slot mpsc channels carrying the same ndjson envelopes as the WS path (Z crates/rpc/src/lib.rs:296-298). It also serves that engine on the IPC port, best effort (Z ARCHITECTURE.md:37).
- **Engine boot:**
  - `InstanceLock` on the data dir allows one engine per data dir (Z crates/engine/src/lib.rs:202).
  - The engine binds `127.0.0.1:{port}` only (lib.rs:980).
  - It exits on Ctrl-C, SIGTERM, a `StopEngine` RPC, or definitive sign-out (lib.rs:938-957).
  - macOS `NWPathMonitor` triggers an immediate redial when the network returns (lib.rs:774).
- **Daemon install** (Z apps/zeron/src/daemon.rs):
  - macOS LaunchAgent `sh.zeron.app` (daemon.rs:15) with `RunAtLoad`, `KeepAlive {SuccessfulExit=false}` and `ThrottleInterval 30` (daemon.rs:285-290). Managed with `launchctl bootout`, not `kill`; with KeepAlive a killed job would just respawn (daemon.rs:140).
  - Linux user unit with `Restart=on-failure`, `RestartSec=5` and `EnvironmentFile=-%h/.zeron/env` (daemon.rs:237). A VPS additionally needs `loginctl enable-linger` (daemon.rs:67).
  - The install captures `PATH` and all `ZERON_*` variables so harness CLIs resolve under launchd (daemon.rs:23-36).
- **Allocator:** mimalloc v2 on macOS only (main.rs:110-118). Linux runs a 60s `malloc_trim` loop, because a headless engine once held 6.6GB across 129 arenas after a day (main.rs:122-139).
- **Logs:** `{data_dir}/logs/zeron-{headed|headless}.log` with the previous launch kept as `.old` (main.rs:178). The default filter is `info,loro_internal=warn,loro=warn` (main.rs:168).

### F2. Local storage and profiles

- **Data dir:** `~/.zeron`. Overridden by `ZERON_DATA_DIR`; Windows uses `%LOCALAPPDATA%` (Z apps/zeron/src/paths.rs:11-33). A one-shot migration moves `~/.comet-native` (paths.rs:36).
- **`WorkspaceScope`:** `Local`, `Synced` or `Development`. It is captured once at startup, so login and logout need a restart (Z ARCHITECTURE.md:49).
  - Local store: `profiles/local/` plus `local-profile.json` (Z crates/engine/src/profile.rs:16,334).
  - Synced store: `orgs/{org}/{user}/` plus `uploads/` (profile.rs:59,349-353).
- **Device-scoped state, shared across profiles:** device id, repos, worktrees, agent credentials, and UI settings, including `ui-settings.json` `openTabs` (Z ARCHITECTURE.md:74; `openTabs` at Z ARCHITECTURE.md:27).
- **`docs.sqlite3`:** WAL, `synchronous=NORMAL`, `busy_timeout` 5s (Z crates/sync/src/store.rs:97-99). Tables:
  - `snapshots` (store.rs:38)
  - `processed_commands` (store.rs:43)
  - `chat_outbox(ordinal, doc_id, batch_id UNIQUE, bytes, needs_checkpoint)` (store.rs:55)
  - `chat_sync_jobs` and `sync_job_clock` (store.rs:68-74)

### F3. Session and chat data model

- **Registry entities** (Z crates/proto/src/entities.rs):
  - `Device {id, name, platform, lastSeenAt, version, capabilities[]}` (entities.rs:77-101).
  - `Chat {id, deviceId(host), title, archived, cwd, branch, checkoutId, config{harness, model, ...}, harnessSessionId, spaceId, lastSeenAt, roomGen, parentChatId}` (entities.rs:182-221).
    - An empty `harnessSessionId` is a do-not-resume tombstone (entities.rs:202-206).
    - `lastSeenAt` is the synced seen marker (entities.rs:221).
  - `ChatIndicator` is one of Working, AwaitingInput, Errored, Completed(unseen) or Idle (entities.rs:259-270). A Working or AwaitingInput session older than 45s counts as dead (Z crates/proto/src/view.rs:41-55).
- **Session doc**, one per chat, in Loro:
  - `meta` map; `messages` list whose parts are maps with LoroText bodies; `commands` ledger list; `queue` movable list (Z crates/doc/src/schema.rs:1-16).
  - Part kinds: text, reasoning, tool, input, error, image, fork.
  - Tool output is stored as a summary of at most 160 chars (Z crates/doc/src/parts.rs:18). The full output lives only in the host's run journal, because the R2 sidecar was parked in v0.1.30 (Z docs/chat2-sync.md:50-51).
- **Doc constants** (Z crates/doc/src/constants.rs):

  | Constant | Value | Line |
  |---|---|---|
  | `MSG_INLINE_MAX` | 256KiB | :5 |
  | `RETAIN_DAYS` | 3 | :9 |
  | `COMPACT_LOG_BYTES` | 8MiB | :11 |
  | `SOFT_CEILING_BYTES` | 25MiB | :13 |
  | `STREAM_COMMIT_MS` | 120 | :15 |
  | `DO_FLUSH_MS` | 5000 | :17 |
  | `DOC_LRU_BYTE_BUDGET` | 80MiB | :19 |
  | `TAIL_MESSAGE_COUNT` | 64 | :21 |
  | `TERMINAL_OUTPUT_BATCH_MS` | 12 | :23 |
  | `COMMAND_DEFAULT_TTL_MS` | 24h | :25 |

- **Command ledger** (Z crates/doc/src/commands.rs):
  - Kinds: Run, Steer, Interrupt, RespondInput.
  - Statuses: Pending, Applied, Rejected, Expired, Superseded, Cancelled (commands.rs:33,118).
  - `evaluate_command` is pure: it handles dedupe, TTL and supersede (commands.rs:9,138-169).
  - Rules: devices append only their own entries, and only the host writes outcomes.
  - The host marks a command processed in `processed_commands` before executing it.
- **Agent events:**
  - `AgentEvent` variants: SessionStarted, TextDelta, GeneratedImage, ReasoningDelta, AssistantMessageCompleted, ToolCall, ToolResult, ContextUsage, Usage, AvailableCommands, Error, InputRequested, InputResolved, Steered, Done, UserMessage and Subagent (Z crates/proto/src/agent.rs:484-599).
  - Harness ids: ClaudeCode, Codex, Cursor, Devin, Grok, Hermes, Pi, Opencode, Antigravity and Mock (Z crates/proto/src/agent.rs:8-27).

### F4. Harness, terminals, run journal

- **Terminals:** portable-pty in the engine; alacritty_terminal is a UI-crate dependency (Z crates/ui/Cargo.toml:66). Output is batched every 12ms and sent base64-encoded, with a 1MiB replay buffer (Z crates/engine/src/terminals.rs:6,22-36).
- **Run journal:** `{data_dir}/journals/{chat_id}.jsonl`, one line per `{seq, event}` (Z crates/engine/src/run_journal.rs:3,68).
  - A journal without a final `Done` marks a crashed run. Recovery stamps it `aborted` and appends a synthetic `Done` (run_journal.rs:8,174).
  - A `.resume` file counts resume attempts (run_journal.rs:72,86).
  - At most 16 journal files are open at once (run_journal.rs:107).
  - Auto-resume is capped at 3 consecutive attempts, and only for crashes less than 12h old (Z crates/engine/src/sessions.rs:834-837).
  - Subscribers replay the journal, then tail it.

### F5. UI↔engine RPC

- **Envelope** (Z crates/rpc/src/lib.rs:7-9,246-267): ndjson over WS text frames.
  - Client sends `{id,method,params}`, or `{id,cancel:true}` to stop a stream.
  - Server replies `{id,ok}` or `{id,err}`; streams send `{id,item}*` and then `{id,done:true}`.
- **Service shape:** `RpcService::handle(method, params) -> Value | Stream` (lib.rs:264-282).
- **Surface:** about 100 methods (107 `pub const` names in Z crates/rpc/src/lib.rs) covering chats, queue, sync, auth, repos and git, workspace files, worktrees, terminals, diffs, uploads and updates. Most accept `targetDeviceId` and can be forwarded over the relay.
- **Security:**
  - Loopback-only bind with no auth token (Z crates/engine/src/lib.rs:980).
  - The server rejects any handshake carrying `Origin`, because browsers always send it and WS is exempt from the same-origin policy (Z crates/rpc/src/server.rs:156-166).
  - Any local process can drive the engine.

### F6. Device↔device relay (DeviceRoom)

- **Frame format:** `uleb128(header_len) ‖ JSON header {s,k,to?,from?} ‖ payload` (Z crates/rpc/src/device_room.rs:7).
  - Kinds: `" relay"` (with a leading space), `nudge`, `rpc` (device_room.rs:32-36).
  - The DO stamps `from=connId`. The host demuxes each connId into a virtual connection that feeds the normal RPC server (device_room.rs:8).
- **Ownership** (Z edge/src/device-room.ts):
  - The first host join claims `owner=userId`. Clients must be the same user, otherwise 403 (device-room.ts:149-159).
  - A new host socket supersedes the old one with close code 4409 (device-room.ts:167).
- **Liveness:**
  - Host and client ping every 10s (device_room.rs:56); the edge comment says 15s, and the code wins.
  - Silence lease 25s (device_room.rs:66).
  - End-to-end echo through the host within 20s. This is needed because the DO's hibernation auto-pong proves only the client↔edge leg (device_room.rs:68-78).
  - The edge treats a host as alive for 75s (device-room.ts:78).
- **Durable nudges:** an SQLite `pending_nudges` table, paged 64 at a time, capped at 4096 (above which it returns 503 plus a reconcile marker), with token-fenced ACKs (Z edge/src/device-nudges.ts:2-32).
- **Sidecar JSON slots** (for example a repos snapshot) serve instant pickers (Z edge/src/index.ts:26-29).
- **Phone client** (Z crates/client/src/live/relay.rs:20-30):
  - One cached link per target device with a 30s unary timeout.
  - Uploads are split into 510,000-byte slices, 3 in parallel, to stay under Cloudflare's 1MiB WS message cap.
  - A peer that has been silent for more than 5 minutes is marked Dark and not dialed.
- **No end-to-end encryption** (Z docs/PARITY.md:101-102). Preview traffic still uses WebRTC with STUN and has no HTTPS byte relay (Z docs/transport-reliability.md:152-153).

### F7. Sync

- **Edge Worker** (Z edge/wrangler.jsonc:23-62):
  - DOs: SessionRoom (legacy), DeviceRoom, PreviewRoom, RegistryRoom, ChatRoom — all SQLite-backed.
  - R2 buckets: blobs and releases.
  - Routes: `edge.zeron.sh`, `zeron.sh/install.sh`, `zeron.sh/releases/*`.
- **chat2 (session docs):** the DO is a dumb log relay that runs no Loro wasm (Z docs/chat2-sync.md; Z edge/src/chat-room.ts).
  - Storage: rows `(seq, device, batch_id UNIQUE, bytes)` plus a client-built checkpoint blob.
  - Binary frames: `[type u8][headerLen u32 LE][JSON][payload]`, 11 types from hello 0x01 to error 0x0b, with a 4096-byte header cap (Z edge/src/chat-frames.ts:21-61).
  - Caps (Z edge/src/chat-room.ts:40-207; Z edge/src/chat-log.ts:18):
    - Row: 1MiB
    - Checkpoint: 32MiB
    - Sidecar: 4MiB
    - Presence TTL: 30s
    - 300 pushes and 8MiB per 60s per device
    - Rows body: 4MiB
  - A checkpoint is posted when `rowBytes > 512KB` or there are more than 200 rows. `GET /checkpoint` supports Range resume, and the log is backed up to R2 nightly (Z docs/chat2-sync.md:110-132).
  - Whale-healing: a 1,079,986-byte doc was rebuilt into a thin ~160KB doc at `epoch=2` (chat2-sync.md:12,162-163; Z crates/doc/src/rebuild.rs:22).
- **Registry:** per-field HLC LWW rows, not a CRDT (Z docs/registry-sync.md:39,78-85). The Rust doc id is still `registry1` (Z crates/doc/src/registry.rs:42).
  - HLC format: `"{ms:013}-{counter:06}-{device}"`.
  - Ops: upsert, update, delete.
  - Tombstones are garbage-collected after 30 days; `gcFloor` forces a full resync for older cursors.
  - Delta sync uses a `seq` cursor (registry-sync.md:67).
  - Presence beats every 15s with a 45s freshness window (registry-sync.md:111).
  - Rationale: the CRDT keeps history, the DO had to replay it in wasm under a CPU limit, and Loro's full-snapshot export threw RangeErrors on workerd (registry-sync.md:8-12).
- **Publication:**
  - Every local update is journaled into `chat_outbox` before network admission, so stable batch IDs make lost-ACK replay idempotent (Z docs/session-publication.md:15-16).
- **Transport reliability** (Z docs/transport-reliability.md):
  - Messages over 16KiB are sent as 4KiB fragments with pings between them (:16).
  - The dial deadline is 20s for DNS, TCP, TLS and the upgrade combined (:26).
  - Measured at 2KiB/s with 600ms delay: outbound bytes fell 29.5%, and completion took 7.28s instead of 9.84s (:49-53).
  - Docs have an HTTP fallback (:71). Generic RPC still needs WS (:153).
- **Capacity:**
  - Current caps: 28 clients per profile, 32 chat sockets per process, 4 concurrent dials, 8 HTTP requests, 50ms dial spacing (Z docs/sync-capacity-calibration.md:3).
  - Recommended load: 20 agent chats plus 8 viewed. Measured cohort p95 was 435-450ms at `RLIMIT_NOFILE` 256; 32 or more agents hit EMFILE (calibration.md:24-30).
  - Admission control: at most 4 per 100ms tick (Z docs/sync-resource-resilience.md:29). On EMFILE, admission pauses 2s (resilience.md:105), with a 128-request queue (resilience.md:107).

### F8. Auth

- **Token check:** WorkOS AuthKit JWT verified with jose against the JWKS at `https://api.workos.com/sso/jwks/{clientId}`, with issuer `.../user_management/{clientId}` (Z edge/src/auth.ts:21-51).
  - The bearer comes from a header, or from `?token=` for WS.
  - Dev mode is available (auth.ts:40).
- **Server-side exchange:** `/auth/exchange`, `/auth/refresh` and `/auth/orgs` keep the WorkOS API key on the edge. The user id is always the token's `sub` (Z edge/src/auth-routes.ts:1-14).
- **Sign-in flows:**
  - Desktop: loopback callback on port 27641 (Z crates/engine/src/lib.rs:632-635).
  - Headless: the page `/auth/cli/callback` shows a paste-able `state.code`, which is CSRF-checked (auth-routes.ts:162-187).
- **Room naming:** rooms are per user, e.g. `reg1/{org}/{user}` (Z ARCHITECTURE.md:107). An org member cannot see another member's rows.

### F9. Push

- **Trigger:** push is sent by the edge, not the device. `RegistryRoom.notifySessions` compares each `sessions` row before and after a push batch (Z edge/src/registry-room.ts:503-551).
- **Rules** (Z edge/src/push-notify.ts:46-56):
  - `failed` fires when the row becomes Errored.
  - `input` fires when it becomes AwaitingInput.
  - `done` fires on a new `lastCompletedTurn`, but only while fresh (≤45s).
  - The first sighting of a row only sets the baseline.
  - Working→Idle without a completion is silent.
- **Exclusions:** side chats (`parentChatId`) and archived chats never notify (push-notify.ts:62-69).
- **Preferences:** per device, all on by default (push-notify.ts:71-87).
- **APNs call** (Z edge/src/apns.ts:31-83):
  - ES256 provider JWT, cached for 50 minutes.
  - `apns-priority 10`, 24h expiration, `collapse-id=chatId`, `thread-id=chatId`.
  - Dead tokens (410, BadDeviceToken, Unregistered) are deleted.
  - Failures are logged, never retried (registry-room.ts:504-505).
- **Registration:** the phone registers via `POST /registry/:org/push-target?device=` (Z crates/client/src/live/urls.rs:66). Defaults are team `5XY3M483YQ` and topic `sh.zeron.ios` (Z edge/src/env.ts:36-41).

### F10. Mobile

- **App:** `apps/ios` is a SwiftUI app. `crates/mobile` is a UniFFI facade over `zeron-client` (Z crates/mobile/src/client_ffi/mod.rs:1-12).
- **Client:** the phone is an engine-free peer. It mirrors the registry, joins chat2 rooms, sends commands through the ledger, and makes host RPCs over the relay (Z crates/client/src/lib.rs:1-7).
- **Caps:** `WARM_SESSION_CAP 6`, `PRELOAD_CAP 4`, and a 48MiB attachment cache (Z crates/client/src/client.rs:30-38).
- **Connectivity states:** Disabled, Offline, Reconnecting, Connected, with a 4s degrade grace (Z crates/client/src/connectivity.rs:14,23-31).
- **Send states:** Sending, Queued, Failed. Failed means unadopted after 120s (connectivity.rs:17,48-56).

### F11. Remote workspace file access

- **Trust model:** any device signed into the same account can list, read and write files. `Show ignored files` also exposes `.env`. `.git` is always blocked (Z README.md:42; Z crates/engine/src/workspace_files.rs:163-195).
- **Limits** (workspace_files.rs:29-44):

  | Limit | Value |
  |---|---|
  | Relative path | 4096 bytes / 256 components |
  | Directory page | 500 |
  | Directory entries | 50,000 |
  | Search results | 200 |
  | RPC timeout | 6s |
  | Editable file | 1MiB |
  | Preview file | 8MiB |
  | Watch debounce | 100ms |
  | Watched dirs | 8,000 |

- **Path handling:** the root is canonicalized before resolution (workspace_files.rs:291).

### F12. Updates

- **Feed:** `{edge}/releases/manifest.json` carries a version and a sha256 per artifact. `latest.txt` is a fallback that has no checksums, so downloads from it skip verification (Z crates/update/src/lib.rs:6-10,85-98).
- **Install kinds:** Managed (versioned dir plus a `current` symlink flip), MacApp (bundle swap), WindowsPortable, and Unmanaged (report only) (lib.rs:12-23).
- **Schedule:**
  - Checks every hour. Failures back off 1m → 5m → 15m → 30m.
  - Deadlines are wall-clock, so time asleep counts.
  - The engine's first check runs at +20s (lib.rs:51-65).
- **Apply gate:** the staged binary must answer `--version` within 30s (lib.rs:73; lib.rs:660). An auto-apply blocked by active sessions re-checks every 5 minutes (lib.rs:70).
- **UX:** "Update ready — restart to apply"; otherwise the update installs at quit. `ZERON_AUTO_UPDATE=0` notifies only (Z README.md:60).
- **No signing:** integrity comes only from a sha256 served by the same origin.

### F13. Performance budgets

- **Memory target:** 150-250MB steady-state RSS, flat over a workday (Z docs/memory-plan.md:5).
- **Feel budgets** (memory-plan.md:40-47):
  - Reopening an evicted chat: first paint under 100ms.
  - Live deltas may land up to about 200ms after paint.
  - Warm chat switches must not change.
- **Measured** on 2026-09-05 with Linux software Vulkan, not Metal (Z docs/performance-resource-usage.md:23-31):
  - Idle CPU 7.49%
  - Streaming CPU 158.57%
  - Idle peak RSS 206MiB
- **Sync:** cohort p95 435-450ms at 20 agents (F7).

### F14. Doc vs code disagreements (code wins)

| Topic | Doc says | Code/newer doc says |
|---|---|---|
| Registry | "Data model — all Loro" (Z ARCHITECTURE.md:94) | HLC LWW rows (Z docs/registry-sync.md:39; Z edge/src/registry-room.ts) |
| Retention | "retain 30d" (Z ARCHITECTURE.md:105) | `RETAIN_DAYS = 3` since 2026-08-04 (Z crates/doc/src/constants.rs:6-9) |
| chat2 | "Status: PLANNED" (Z docs/chat2-sync.md:3) | shipped: ChatRoom DO, migration v3 (Z edge/wrangler.jsonc:62) |
| Mobile | "out of scope" (Z ARCHITECTURE.md:261) | `apps/ios` + `crates/mobile` exist |
| Local→synced import | "does not upload, import, link…" on sign-in (Z ARCHITECTURE.md:78) | one-time `LocalImporter` wired (Z crates/engine/src/lib.rs:146,291; Z crates/engine/src/local_import.rs:1) |
| Relay ping | edge comment 15s (Z edge/src/device-room.ts:74) | Rust 10s (Z crates/rpc/src/device_room.rs:56) |

### F15. Pocket today (comparison)

- **Processes:**
  - pocketd owns the PTYs.
  - The desktop and the `pocketd run` CLI talk JSON lines over a unix socket at `~/.coding-pocket/pocketd.sock`, mode 0600 (P packages/pocketd/internal/ops/ops.go:67-76; P packages/pocketd/internal/config/config.go:14-26; P packages/desktop/crates/daemon/src/daemon.rs:48-54,136).
  - This is stronger than Zeron's token-less loopback TCP.
- **Phone transport:**
  - Plain `ws://` on all interfaces, port 4517 (P packages/pocketd/cmd/pocketd/serve.go:46-50; P packages/pocketd/internal/config/config.go:55).
  - The token is sent in `hello` and compared with `!=` (P packages/pocketd/internal/wsserver/wsserver.go:123).
  - Origin checks are disabled (`InsecureSkipVerify`, wsserver.go:53).
  - Off-LAN access means Tailscale; the daemon prints its Tailscale IP (serve.go:58-69).
- **Version check:** strict equality, `protocolVersion != 3`, is rejected (wsserver.go:123; P packages/pocketd/internal/proto/proto.go:11).
- **Liveness:**
  - Server pings every 20s with a 10s timeout; the hello timeout is 10s (wsserver.go:22-27).
  - The phone has no watchdog or dial deadline. Reconnect backoff runs from 1s to 30s (P packages/app/src/client.ts:7,51-55).
  - `send` while disconnected is silently dropped (client.ts:59).
- **Identity:** `clientId` is the constant `"pocket-app"` (P packages/app/src/session.tsx:76). Request ids are `c{n}` and restart on every client instance (client.ts:58).
- **Fan-out:** the hub buffers 1024 messages per subscriber and drops a slow subscriber, which then reconnects and re-pages (P packages/pocketd/internal/hub/hub.go:9,35-51).
- **Timeline:**
  - Held in memory, not persisted (P packages/pocketd/internal/timeline/timeline.go:27-35).
  - `Clear` keeps `seq` running across conversations (timeline.go:58-65).
  - It is rebuilt by tailing Claude's own JSONL (P packages/pocketd/internal/claude/transcript.go:1,136).
- **Page size:**
  - Default page is 200 items, maximum 500 (wsserver.go:23; P packages/protocol/src/messages.ts:24).
  - A tool output can be up to 64KiB (proto.go:12).
  - So one default-size `agent.timeline` reply can reach about 12.5MiB (about 31MiB at `limit` 500), far above Cloudflare's 1MiB WS message cap (Z crates/client/src/live/relay.rs:22-24).
- **Seen:**
  - One server flag. An agent is seen while any connection views it (P packages/pocketd/internal/agent/agent.go:117-150).
  - Keepalive drops a sleeping phone so it can't mark turns seen (wsserver.go:84).
- **Status:** closed, then phase, then done (unseen end), otherwise idle (agent.go:174-184).
- **No push.** No relay. No launchd unit, although pocketd already resolves `PATH` for launchd (P packages/pocketd/internal/terminal/terminal.go:87-89).

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 02-1 | Outbound-only relay: pocketd dials a per-Mac relay room; the phone dials the same room. Byte relay with connId virtual connections into the existing wsserver handler, plus end-to-end encryption keyed at pairing (Zeron has no end-to-end encryption). | Off-LAN phone access without Tailscale | Z crates/rpc/src/device_room.rs:7-78; Z edge/src/device-room.ts:78-167; Z docs/PARITY.md:101 | new `packages/relay` (CF Worker+DO, port device-room.ts); new `pocketd/internal/relay` (Go dialer+mux); adapt `packages/app/src/client.ts` transport | L | 02-3, 02-5, 02-9; pairing key exchange (QR); decision on hosting/cost/accounts |
| 02-2 | Push on Status transitions: Needs you, Done, Done+failed. Port `notificationFor`: first sight only sets a baseline; 45s staleness; Working→Idle stays silent; collapse/thread id = session id. Deliver via the Expo Push API from pocketd, so no server is needed. | Know when an agent needs you with the app closed | Z edge/src/push-notify.ts:46-99; Z edge/src/apns.ts:31-83; Z edge/src/registry-room.ts:503-551; Expo push docs URL | adapt `pocketd/internal/agent` (transition hook) + new `pocketd/internal/push`; new `push.register` in `packages/protocol/src/messages.ts`; `packages/app` expo-notifications | M | per-device clientId (02-5); APNs key uploaded to Expo/EAS; `aps-environment` entitlement and `expo-notifications` (both absent today: P packages/app/ios/Anywhere/Anywhere.entitlements, P packages/app/package.json); Expo access token only if push security is enabled (unverified); per-device prefs |
| 02-3 | Byte-bounded frames: cap each `agent.timeline` reply by bytes (e.g. ≤512KiB), not item count; clamp stream items. | No stalls or disconnects on big tool outputs; required for any relay | P packages/pocketd/internal/wsserver/wsserver.go:23; P packages/pocketd/internal/proto/proto.go:12; Z crates/client/src/live/relay.rs:22-24; Z edge/src/chat-room.ts:207 | adapt `pocketd/internal/timeline` Page + `wsserver` | S | none |
| 02-4 | Phone liveness: client silence lease (~25s), dial deadline (20s), redial on network change (NetInfo), 4s degrade grace before showing Offline. | Faster reconnect after Wi-Fi/cell switch; honest connection pill | Z crates/rpc/src/device_room.rs:56-78; Z docs/transport-reliability.md:26; Z crates/client/src/connectivity.rs:14; Z crates/engine/src/lib.rs:774 | adapt `packages/app/src/client.ts`; `pocketd/internal/wsserver` ping 20s→10s | S | none |
| 02-5 | Idempotent prompts: unique per-install clientId; pocketd dedupes `(clientId,id)` for the TTL window and re-acks duplicates (a small version of the command ledger). | A retry after reconnect never sends a prompt twice | Z crates/doc/src/commands.rs:9,138-169; Z crates/sync/src/store.rs:43; P packages/app/src/session.tsx:76; P packages/app/src/client.ts:58 | adapt `packages/app` (persisted clientId, monotonic ids) + `pocketd/internal/wsserver` dedupe map | S | none |
| 02-6 | Phone outbox with Sending/Queued/Failed ("Not delivered — retry" after 120s). | Type while offline; nothing silently lost | Z crates/client/src/connectivity.rs:17,48-56; Z docs/session-publication.md:15-16; P packages/app/src/client.ts:59 | new outbox in `packages/app` (AsyncStorage) | M | 02-5 |
| 02-7 | `pocketd daemon install`: LaunchAgent with KeepAlive{SuccessfulExit=false}, ThrottleInterval 30, captured PATH/env; `bootout` to stop. | pocketd survives logout/crash without a manual start | Z apps/zeron/src/daemon.rs:15,23-36,140,285-290 | new cmd in `packages/pocketd/cmd/pocketd` | S | decide whether desktop app or CLI owns the install |
| 02-8 | Log file under `~/.coding-pocket/logs` with `.old` rotation and flock; `pocketd status` (conns, agents, relay, push log). | Debuggable field issues | Z apps/zeron/src/main.rs:168-178,523-532 (flock); Z edge/src/registry-room.ts:218-227 (`/stats`: sockets, push targets, push log) | adapt `packages/pocketd/cmd/pocketd` | S | none |
| 02-9 | Capability negotiation in `hello`/`hello.ok` (a `capabilities[]` list) instead of rejecting on any version mismatch. | Phone and Mac upgrade independently (App Store lag) | Z crates/proto/src/entities.rs:96-101; P packages/pocketd/internal/wsserver/wsserver.go:123 | adapt `packages/protocol/src/messages.ts`, `pocketd/internal/proto`, app | S | protocol v4 bump once |
| 02-10 | Phone tail cache: persist the last ~64 items per Session; paint from cache, then page `sinceSeq`. Budget: first paint <100ms. | Instant reopen, including offline | Z crates/doc/src/constants.rs:21; Z edge/src/index.ts:34; Z docs/memory-plan.md:40-47 | new cache in `packages/app` | M | epoch/seq semantics stable (P packages/pocketd/internal/timeline/timeline.go:58-65) |
| 02-11 | Timeline journal: append items to `~/.coding-pocket/timelines/{agent}.jsonl` with seq; reload on pocketd start for ended Sessions. | History survives pocketd restart/upgrade | Z crates/engine/src/run_journal.rs:3-8,107 | adapt `pocketd/internal/timeline` | M | retention policy; PTYs still die on restart |
| 02-12 | Self-update: manifest + sha256 + signature (Zeron lacks one) + staged `--version` probe, applied only when no Terminals are open. | Painless upgrades without killing Sessions | Z crates/update/src/lib.rs:6-10,51-73,660; Z README.md:60 | new `pocketd/internal/update`; desktop update strip | M | release hosting; signing key; quiescence definition |
| 02-13 | Multiple hosts on the phone: host list with 15s/45s presence, Dark after 5min (no dial), capabilities per host. | One phone for Mac + VPS | Z docs/registry-sync.md:111; Z crates/client/src/live/relay.rs:27-30 | adapt `packages/app` host store + protocol | L | 02-1 or multiple direct URLs |
| 02-14 | HLC per-field LWW registry for Seen/titles across hosts. | Consistent state with several writers | Z docs/registry-sync.md:39,78-85 | new, `packages/protocol` + pocketd | L | only if 02-13 lands and phone edits offline |
| 02-15 | CRDT (Loro) transcripts. | Offline multi-writer transcripts | Counter-evidence: Z docs/registry-sync.md:8-12; Z crates/doc/src/constants.rs:6-9; Z docs/chat2-sync.md:12,50-51,162 | none: keep the host-authoritative timeline + seq paging | XL | none |
| 02-16 | Read-only workspace file/diff viewer on the phone, with Zeron's path rules (.git blocked, gitignored hidden, 1MiB/8MiB caps, 6s timeout). | Inspect what the agent changed while away | Z crates/engine/src/workspace_files.rs:29-44,163-195,291; Z README.md:42 | new RPCs in `pocketd` + `packages/protocol`; app screen | M | 02-3; threat model for `.env` |

Priority guesses:
- must: 02-2
- should: 02-1, 02-3, 02-4, 02-5, 02-7, 02-9
- could: 02-6, 02-8, 02-10, 02-11, 02-12, 02-13, 02-16
- wont: 02-14, 02-15

## UI/UX spec to copy

Transport-driven only; transcript UI is covered by other reports.

- **Connection pill** (Z crates/client/src/connectivity.rs:14,23-38):
  - States: hidden, Offline, Reconnecting, Connected.
  - Show a degraded state only after 4s.
  - Show a countdown to the next redial.
  - Pocket equivalent: `ConnectionState` idle / connecting / online / offline (P packages/app/src/client.ts:5).
- **Per-message send state** (connectivity.rs:48-56):
  - Sending, then Queued (durable, delivers later), then Failed with the "Not delivered — retry" affordance after 120s.
- **Push copy** (Z edge/src/push-notify.ts:58-99):
  - Title: the chat title, or "New session".
  - Body: "Run finished", "Waiting on your input" or "Run failed".
  - One thread per chat; a newer alert replaces the older one; tapping opens the chat.
  - Per-device toggles for done / input / failed, all on by default.
  - Pocket wording: Needs you = input; Done; Done + Failed = failed.
- **Update strip** (Z README.md:60): "Update ready — restart to apply"; if not restarted, install at quit.
- **Trust warning** before enabling remote file access, including ignored files (Z README.md:42).

## Open questions / risks

- **Relay hosting.** Who runs it? Options are a single hosted Worker (needs accounts, abuse controls, cost) or a Worker that each user deploys. Zeron ties rooms to a WorkOS `userId` (Z edge/src/device-room.ts:149-159); Pocket has no account system.
- **End-to-end encryption is mandatory for a Pocket relay.** PTY bytes and transcripts carry secrets, and Zeron never designed E2EE (Z docs/PARITY.md:101-102). Handshake choice (Noise XX vs sealed-box from a QR pairing key) and key rotation are open.
- **Cloudflare's 1MiB WS message cap** (Z crates/client/src/live/relay.rs:22-24) forces 02-3 before 02-1.
- **Push privacy and dependency.** The Expo Push route sends session titles through Expo and APNs. Direct APNs needs the app publisher's `.p8` key on a server, which can't ship inside pocketd. Unverified: Expo push token lifetime when the app is backgrounded for a long time.
- **pocketd restart kills every PTY.** That limits self-update (02-12) and makes KeepAlive (02-7) a crash-recovery path, not a session-preserving one. A separate PTY-holder process is out of scope here.
- **Origin rejection.** Zeron's trick (reject any `Origin`) suits a loopback desktop IPC. It is unverified whether React Native's iOS WebSocket sends `Origin`. If it does, the trick can't be applied to pocketd's phone port. Pocket's unix socket at 0600 already covers the desktop path.
- **Weak updater security in Zeron.** The same-origin sha256 plus the `latest.txt` fallback that skips verification (Z crates/update/src/lib.rs:89-90) are not worth copying as-is.
- **Doc drift in Zeron is heavy** (§F14). Any further porting should read code first.
- **CRDT cost is real.** Zeron needed whale surgery, retention cuts and the removal of tool outputs from docs (Z docs/chat2-sync.md:12-51). Revisit 02-15 only if Pocket ever needs offline multi-writer transcripts.

## Verification

Date: 2026-09-30. Claims checked: 40 (all TL;DR items, F1-F15 spot checks, every Pocket statement, all 16 Ideas rows). Corrected: 11. Everything else matched the cited source. Pocket mappings point at existing packages (`pocketd/internal/{agent,timeline,wsserver,proto}`, `pocketd/cmd/pocketd`, `packages/protocol/src/messages.ts`, `packages/app/src/client.ts`); rows marked "new" (`relay`, `push`, `update`) do not exist yet, as stated.

- F2: device-scoped state list cited ARCHITECTURE.md:27; the list is at :74.
- F3: constant names corrected to `SOFT_CEILING_BYTES`, `COMMAND_DEFAULT_TTL_MS`.
- F3: `AgentEvent` list missed GeneratedImage, AssistantMessageCompleted, AvailableCommands, Error; added. Harness ids got a citation.
- F4: alacritty_terminal is not in `terminals.rs`; it is a UI-crate dependency.
- F5: "about 100 methods" had no source; now cites the 107 method-name constants.
- F6: "no HTTPS byte relay" applies to preview only; reworded, line range fixed.
- F7: chat2 quota also caps 8MiB per 60s; added.
- F14: local-import row cited nothing on the doc side; now cites ARCHITECTURE.md:78. Relay-ping row got the edge citation.
- F15: 12.5MiB bound holds only at the default page of 200; noted the 500 max.
- 02-2: "Expo access token" is not a hard prerequisite; added the real ones (APNs key in EAS, `aps-environment` entitlement, `expo-notifications`, both missing in Pocket).
- 02-8: flock claim had no citation; added main.rs:523-532.
