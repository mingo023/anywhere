# pocketd and phone clients: wire map for the native iOS app

Scope: how phones reach pocketd today, what each message does, how terminals work, and what to add for the SwiftUI app. Paths are relative to `packages/pocketd/` unless they say otherwise. Line numbers are from `main` at 1b59160.

## 1. Transport

- **Phone listener.** `internal/reach/reach.go:109` `reach.Listen(port, mode, h)`. Plain HTTP and WebSocket, **no TLS**. It always binds `127.0.0.1:port`. In mode `auto` it also binds every Tailscale address, rechecked every 30 s (`recheck`, :18; `Refresh` :189).
  - A Tailscale address is `fd7a:115c:a1e0::/48`, or `100.64.0.0/10` on a `utun*` or `tailscale0` interface (:28). When no interface matches, it falls back to `tailscale ip` (:77).
  - A Host-header gate (:177, `AllowHost` :259) returns 403 unless the host is loopback, `localhost`, `*.ts.net` or a bound tailnet IP. This guards against DNS rebinding.
  - Ports come from `internal/config/config.go:49`: 4517 for release, 4518 for dev. `Listen` is `auto` or `loopback` (:81).
  - WireGuard is the only encryption. The RN app's `wsURL` (`packages/app/src/pairing.ts:62`) uses `ws://` for loopback, 100.64/10, fd7a:115c:a1e0: and *.ts.net, and `wss://` for anything else. pocketd has no `wss` listener.
- **Wiring.** `cmd/pocketd/serve.go:188` builds `wsserver.Server`. `:218` calls `reach.Listen(cfg.Port, cfg.Listen, ws)`. `:223` runs `watchTailnet`, which feeds `HostState.Tailnet` every 30 s. `PairHost` (`reach.go:248`) is the first tailnet IPv4 `ip:port`.
- **Owner channels** (desktop only):
  - the unix socket `~/<home>/pocketd.sock`, mode 0600 (`internal/ops/ops.go:113`);
  - optional loopback TCP `ownerListen`, default `127.0.0.1:4519` (`serve.go:229`, `config.go:57`).
  - `ops.Server.route` (`ops.go:190`) sniffs the first byte. `'G'` means an HTTP GET, which goes to the same `wsserver` (the desktop speaks the phone protocol over the socket as owner). Anything else is NDJSON `ops.Msg`.
- **WebSocket library.** `github.com/coder/websocket`. Text frames only today (`wsserver.go:207,216`).
  - Read limit is 4 KiB before auth and 1 MiB after (`:47-48`).
  - pocketd pings every 20 s with a 10 s timeout and closes a dead peer (`keepalive` :188).
  - Hello timeout is 10 s (`:39`).
  - At most 16 unauthenticated conns (`maxPreauth` :46). After 3 auth failures in a minute an IP is locked out (`limiter.go`, `reject` :400). The close reason is `rate_limited` with status 1008.
- **Close codes.** `4401` means unpaired or revoked (`statusUnpaired`). `4426` means a version mismatch (`statusVersionMismatch`), with reason `client_too_old` or `server_too_old`.
- **Fan-out.** `internal/hub/hub.go`. Each subscriber gets a `chan []byte` with buffer 1024 (`:9`). `Publish` never blocks: a subscriber that falls a full buffer behind is dropped and its channel closed (`:60-64`). `conn.forward` then ends and `c.cancel()` closes the socket (`wsserver.go:205-212`), so the phone reconnects and re-pages.
  - `SubscribeSubagents` also receives `PublishSubagent` items, for the `subagents.v1` cap.

## 2. Pairing

- **Owner begins.**
  - Over WS: `pair.begin` → `wsserver/pair.go:61` `pairBegin`. It refuses with `tailnet_off` when `Host()` is false (:67).
  - Over ops: `ops/pair.go`, used by `pocketd pair --json` from `packages/desktop/crates/daemon/src/service/bundled.rs:44`.
- **Code.** 16 random bytes, base64url without padding, so 22 chars (`internal/pairing/pairing.go:84-86`).
  - TTL is 5 min (`ttl` :19). Only one code is open at a time: `Begin` voids the previous one with `pair_expired`.
  - 5 wrong codes lock pairing for 1 min (`maxFails`, `lockFor`).
  - Only the SHA-256 of a code is held. A spent code stays remembered for 10 min, so a replay gets `pair_used` or `pair_expired` (:130-151).
- **URL / QR payload.** `anywhere://pair?v=1&h=<ip:port urlescaped>&c=<code>&n=<Mac ComputerName urlescaped>` (`pairing.go:88`). The answer is `pair.offer {id,url,code,expiresAt(ms)}` (`proto/messages.go:199`).
  - Later the begin conn receives either `pair.done {deviceId,name}` or `error{id,code∈pair_expired|pair_locked|pair_failed}` (`pair.go:96`).
  - RN parsing is in `packages/app/src/pairing.ts:35`: code regex `^[A-Za-z0-9_-]{22}$`, host regex `host:port`, and `v` must be `1`.
- **Phone redeems.** It opens a fresh WS and sends `{type:"pair",id,code,name,platform:"ios"|"android",protocol:{min,max}}` (`messages.go:115-117`; the platform must be ios or android). The handler is `wsserver/pair.go:22`:
  - It optionally negotiates the version and closes 4426 if there is no overlap.
  - `Pairing.Redeem` → `devices.Store.Add(name, platform, devices.PhoneScopes)`.
  - It replies `pair.ok {id,deviceId,token}` and **closes 1000**. The phone must reconnect and send `hello` with the token.
- **Device token.** 32 random bytes, base64url (`internal/devices/devices.go:95`). Only `sha256` hex is stored, in `devices.json` (0600, `:101`, `:255`).
  - `Device{id(hex16), name, platform, scopes, createdAt, lastSeenAt, lastAddr}` (:44).
  - `Seen` is saved at most once a minute. Revoke goes through ops `devices.revoke`: `Kick` calls `wsserver.CloseDevice`, which closes 4401 with reason `revoked` (`wsserver.go:389`).
- **Phone keychain (RN).** `packages/app/src/credentials.ts`. Keys: `pocket.macHost`, `pocket.deviceToken`, `pocket.deviceId`, `pocket.macName`, `pocket.clientId`. Accessibility is `WHEN_UNLOCKED_THIS_DEVICE_ONLY` (`keychain.ts:4`). `clientId` is `<os>-<base36 ms>-<rand8>`, persisted. Wipe keeps the host so the code can be re-entered. The SwiftUI app should mirror this with `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`.

## 3. Principals, scopes, authorization

- **Scopes** (`internal/peer/scope.go:12`): `observe`, `drive`, `approve`, `spawn`, `owner`.
  - `PhoneScopes = observe,drive,approve,spawn` (`devices.go:27`).
  - `LegacyScopes` are the same without spawn.
  - `OwnerScopes` are all five. `PTYScopes = observe`.
- **Kinds:**
  - `Owner`: a unix-socket peer outside every Terminal, or a peer that sent `ownerToken` (`owners.Store`).
  - `Device`: a paired phone.
  - `PTY`: a process inside a Pocket Terminal (`peer/classify.go:28`).
  - `None`: fails closed.
- **Every authed verb** goes through `c.who.Check("ws:"+type, asking, text)` (`wsserver.go:465`) against the allowlist `peer.Needs` (`peer/check.go:15-81`). **A verb missing from `Needs` is refused for everyone with `scope_denied`.** For PTY peers the ask guard (`ask_open`) comes first, and so does the refusal of prompts starting with `!` or `/` (`check.go:106-121`).
- **WS verb to required scope:**
  - observe: `agent.list`, `agent.timeline`, `agent.view`, `agent.seen`, `project.list`, `agent.providers`, `agent.commands`, `agent.mcp` (list only)
  - drive: `agent.prompt`, `agent.interrupt`, `agent.send_now`, `agent.compact`, `agent.close`, `agent.settings`, `agent.pin`, `agent.mcp.act` (an mcp with an action, `wsserver.go:461`)
  - approve: `permission.resolve`
  - spawn: `agent.create`
  - owner: `pair.begin`, `config.set`, `worktree.rename`, `local.create|rename|delete`, `automation.save|enable|delete|run`
- **Bug found.** `agent.btw` is decoded (`messages.go:121`) and handled (`wsserver.go:602`), but `"ws:agent.btw"` is **not in `peer.Needs`**. Every principal gets `scope_denied: "agent.btw is not a known verb"`, including the desktop, which sends it from `packages/desktop/crates/agents/src/agents.rs:897`. No test covers it, since the matrix in `peer/check_test.go:16` omits it. The fix is a one-line `"ws:agent.btw": Drive` (or Observe), pinned by a test, done separately.
- **Extra gates beyond scope:**
  - `agent.create` from a non-owner needs cap `launch.v1` (`wsserver/launch.go:27`). A `{local}` checkout needs `locals.v1`.
  - Access is capped by `launch.CheckAccess(who, access, PhoneMaxAccess)`. `PhoneAccesses = ask,edits,auto` (`proto/launch.go:28`), so a phone never gets `full`. The owner sets the cap with `config.set {key:"phone.maxAccess"}`.
  - `local.*` and `automation.*` without their cap return `Malformed message`, as if unknown.
- **Ops verbs** are owner-only for anything touching terminals: `ops:attach|input|resize|close|screen|spawn|prompt|exec|fs.*`. `list`, `status`, `hook` and `pipe` need only observe (`check.go:44-80`). **Phones never reach ops.**

## 4. Handshake: hello, caps, version

- **Client sends** `hello {id, token?, ownerToken?, clientId, protocolVersion, caps?: string[], protocol?: {min,max}}` (`messages.go:113`).
- **Version.** `proto/proto.go:11` has `MinVersion = MaxVersion = 3`. `Negotiate` (`proto/version.go:45`) picks `min(client.Max, 3)` and the intersection of caps, sorted. With no overlap it sends `error{code: client_too_old|server_too_old}` and closes 4426 (`wsserver.go:410`).
  - An old Mac answers a hello it doesn't understand with `error{message:"Malformed message"}` and no code. RN reads that as `updateMac` (`packages/app/src/connection.ts:28`).
- **ServerCaps** (`version.go:33`): `pair.v1, scopes.v1, host.v1, registry.v1, summary.v2, launch.v1, restore.v1, open.v1, names.v1, automations.v1, locals.v1, fork.v1, accounts.v1, subagents.v1, settings.v1, commands.v1, access.v1`. Cap naming is `<area>.v<n>`.
  - The RN app sends only `pair.v1, summary.v2, registry.v1, launch.v1` (`packages/app/src/client.ts:50`).
  - The desktop sends 14 caps (`desktop/crates/agents/src/agents.rs:1023`).
- **Auth** (`wsserver.go:232-270`):
  - A local, owner-classified peer is owner. A local TCP peer must send a valid `ownerToken`.
  - A non-local peer must send a device `token` (`Devices.Lookup`), otherwise `error{code:not_paired}` and close 4401.
  - A second hello on the same conn is allowed. It must be the same device, and it re-sends the snapshot (`TestSecondHelloResendsSnapshot`).
- **Order after a successful hello** (`wsserver.go:274-316`). The hub subscription is created *before* the snapshot so nothing is lost; updates buffer until the snapshot is out.
  1. `hello.ok {id, serverId(=hostname), hostname, protocolVersion, caps, protocol{min,max}, scopes? (if scopes.v1), host? {tailnet, keepingAwake, upgradeFailed} (if host.v1), version}` (`messages.go:168`)
  2. `agent.list {agents: AgentSummary[]}` (no id)
  3. `automations` snapshot (owner + `automations.v1` only)
  4. every open `permission.request`
  5. `worktree.names {names: {path: title}}` (names.v1)
  6. `local.list {locals}` (locals.v1)
  7. then live pushes: `agent.update`, `agent.stream`, `permission.request`, `permission.resolved`, `host.changed`, `worktree.names`, `naming.failed`, `local.list`
- **Request/response correlation.** Every client message carries `id` (the RN app uses `c1`, `c2`, …). Replies echo `id`: `ack {id}`, `error {id?, message, code?, detail?}`, or a typed reply. Pushes carry no `id`.

## 5. Every ClientMessage: decode and handler

Decode is in `internal/proto/messages.go:58-166`. Dispatch is in `internal/wsserver/wsserver.go:459-627`.

| type | fields (decode line) | handler | reply |
|---|---|---|---|
| hello | :113 | wsserver.go:225-318 | hello.ok + snapshot |
| pair | :115 | wsserver/pair.go:22 | pair.ok then close 1000 |
| pair.begin | :118 | wsserver.go:469 → pair.go:61 | pair.offer, later pair.done/error |
| agent.list | :118 | :479 | agent.list{id} |
| project.list | :118 | :482 `Projects()` = `worktree.Projects(reg.Load())` (serve.go:188) | project.list{projects:[{path,name,worktrees:[{name,path,branch,isMain}]}]} |
| agent.providers | :118 | :498 → wsserver/launch.go:49 | agent.providers{providers,maxAccess,phoneMaxAccess} |
| agent.prompt | :119 agentId,text | :540 `Driver().Prompt` | ack |
| agent.btw | :121 | :602 (goroutine) | agent.btw{answer}; refused today, see section 3 |
| agent.interrupt / send_now / compact / close / commands | :123 agentId | :542 / :544 / :550 / :617 / :568 | ack (commands → agent.commands{commands}) |
| agent.mcp | :125 action∈auth,reconnect,enable + server | :579 (goroutine) | agent.mcp{servers,authUrl?} |
| agent.settings | :128 model/effort/access | :552 | ack or coded error |
| agent.pin | :131 | :619 | ack (+ agent.update to all) |
| agent.view / agent.seen | :133 agentIds[] | :485 SetView(conn,…) / :490 MarkSeen | ack |
| agent.timeline | :135 sinceSeq?, limit 1..500 | :524-539 | agent.timeline{epoch,items,hasOlder,maxSeq} |
| permission.resolve | :138 requestId, decision allow/deny, option?, message?, answers? | :471-478 → broker.Resolve | ack, or error "no longer open" |
| agent.create | :141 requestId ≤64, spec (strict `decodeSpec`, proto/launch.go:166) | :495 → wsserver/launch.go:25 (goroutine) | agent.progress{step,note}*, agent.creating{terminalId,cwd,setup}, agent.created{agentId,terminalId} or coded error |
| config.set | :143 key="phone.maxAccess" | :501 → launch.go:54 | ack / invalid_config |
| worktree.rename | :145 path,title | :508-517 (Names.Rename + retitle Claude convos) | ack |
| local.create/rename/delete | :147-152 | :506 → wsserver/locals.go:24 | ack, or error then a corrective local.list |
| automation.save/enable/delete/run | :153-158 | :504 → wsserver/automation.go:48 | ack then automations snapshot |

Notes:

- Unknown types decode as `ErrMalformed` ("Malformed message").
- `project.list` runs `git worktree list` per project **on the conn's read loop**: `List` in `internal/worktree/list.go:24` uses `exec.Command` with no cache.
- `agent.commands`, `agent.mcp`, `agent.btw` and `agent.create` run in goroutines. Everything else blocks the conn's read loop.

## 6. Timeline, epoch, seq, maxSeq

`internal/timeline/timeline.go`.

- **seq.** A monotonic per-agent counter. Item `id = "i"+seq` (`append` :132). **An item mutates in place under the same id and seq.** A tool goes `running → ok` and keeps its seq, and drafts grow (golden `stream_tool_running` and `stream_tool_done` both use seq 2). So `sinceSeq` cannot detect an *update* to an item you already hold.
- **epoch.** Incremented by `StartEpoch` on each user prompt (`agent.go:642`, so a new run doesn't merge into the last one's open text), by `Clear` (`:102`), and by `Continue` after an upgrade handoff (`:120`).
  - `agent.stream {agentId, epoch, item}` carries it.
  - **`AgentSummary.Resets`** is the authoritative "drop what you hold" signal. The desktop compares `resets` (`desktop/crates/agents/src/agents.rs:554`).
  - Neither the desktop nor RN uses `epoch` beyond decoding it.
- **maxSeq** is the last seq assigned (`State()` :126). It is also on `AgentSummary.MaxSeq`, so the list shows whether a timeline moved.
- **Page** (`:462`) returns the **newest** `limit` items with `seq > sinceSeq`. `hasOlder = len(items) < len(newer) || sinceSeq > 0`. **There is no `beforeSeq`.** A client cannot page back past the latest 500 items, yet `hasOlder` says older items exist. Long chats on the phone need `beforeSeq` added.
- Without `subagents.v1`, items with `parentToolUseId` are filtered out of pages (`Page` vs `PageWithSubagents`) and of the stream (`hub.PublishSubagent`).
- Drafts (`*_draft` events) are batched every `draftEvery` before publishing (`agent.go:678-711`).
- **What the RN app does on reconnect** (`packages/app/src/session.tsx:123-141`): on `agent.list` after a resync it sends `agent.view` with the visible ids, plus `agent.timeline` per id **without `sinceSeq`**, and replaces the whole timeline. `agent.stream` merges by item id. This is the only correct resume strategy given in-place updates.
- The Item union (`proto/proto.go:124-191`) has kinds `user`, `assistant`, `thinking`, `plan` (text), `tool` (call: ToolCall{toolUseId,name,detail,status,output?,durationMs?}), `tasks`, `compact`, `result`, `notice`. `ToolDetail` kinds are `shell`, `read`, `edit/write` (with FileDiff), `search`, `task`, `other`. Tool output is capped at 64 KiB (`ToolOutputLimit`), and the diff preview at 24 lines × 160 chars.

## 7. Permission flow

1. **Claude (TUI):** the hook `PermissionRequest` goes to `daemon.permission` (`internal/daemon/daemon.go:245`), then to `decide` (:255). **Claude (chat/pipe):** `daemon/stream.go:381`. **Codex:** `codex.Open/NewSession(..., d.Broker)` (`daemon/codex.go:223`, `codex_chat.go:197`).
2. `decide` builds `PermissionRequest{agentId, toolName, detail, options[{id,label}], feedback:true}`, calls `ag.NeedsYou()`, then `Broker.Ask(ctx, req, key)` (`internal/broker/broker.go:54`).
3. `Ask` assigns a random 16-byte hex `requestId` and publishes `permission.request {request}` to all subscribers under the lock. It then waits for the answer, ctx cancellation, or `Timeout = 10 min` (auto-deny).
4. The phone sends `permission.resolve {requestId, decision, option?, message?, answers?}`. `Broker.Resolve` → `finish` publishes `permission.resolved {requestId, decision}` to everyone.
   - `allow` means `updatedInput` with AskUserQuestion answers plus `updatedPermissions` for the option.
   - `deny` means interrupt. A `message` is typed as the next prompt after the turn ends (`promptAfterTurn`, 30 s wait).
5. If the desktop or TUI answers first, `Broker.Dismiss(key, decision)` closes the request with `permission.resolved` and `Ask` returns `""` (`daemon.go:139`). `DenyAll(agentID)` runs when the presence ends (`presence.go:416`).
6. A new connection gets every open request in its hello snapshot (`Broker.Open()`).

## 8. Terminals

### 8.1 How the desktop does it (ops NDJSON over the unix socket)

- Client: `packages/desktop/crates/daemon/src/daemon.rs`. `Daemon::spawn_to` (:497) keeps one long-lived socket that reconnects with backoff from 200 ms to 2 s. Each line is a `Msg` (:30). `request()` uses a fresh connection for one-shot ops.
- Wire format (`internal/ops/ops.go:25` `Msg`). JSON lines: `{"op":…}` from the client, `{"ev":…}` from the server. `[]byte` fields (`data`, `stdin`, `stdout`) are **base64 standard** because Go's encoding/json does it. The desktop decodes them with `STANDARD.decode` (`daemon.rs:270`).
- **Ops:**
  - `list` → `{ev:"terminals", items:[Info{id,cmd,args,cwd,cols,rows,foreground?,local?}]}` (`terminal.go:17`). The desktop polls this every tick (`desktop/crates/pocket/src/main.rs:133`).
  - `spawn {cmd,args,cwd,env,cols,rows,local?,login?,argv?,agent?}` → `{ev:"spawned",id}`. The desktop sends cols 120, rows 36 (`daemon.rs:482`). The handler is `daemon.Spawn` (`internal/daemon/daemon.go:64`), which adds `loginSpec`, `AgentTab` and the terminal env.
  - `attach {id, tty?}` → `{ev:"snapshot",id,cols,rows,data}`, then a stream of `{ev:"output",id,data}`, `{ev:"resize",id,cols,rows}`, `{ev:"foreground",id,text}`, `{ev:"exit",id,code}`. The handler is `ops.go:379` and `attach()` :406. It buffers events that arrive during the snapshot, so ordering is exact. `tty:true` is only for `pocketd run`: a real terminal answers queries itself (`terminal.go:218`). The desktop and a phone must use `tty:false`.
  - `input {id,data(b64)}` → `t.Write`, no reply.
  - `resize {id,cols,rows}` → resizes the PTY **and the VT, broadcasting to every attached client** (`terminal.go:539`).
  - `screen {id}` → `{ev:"screen",text}`, plain visible text (`vt.Plain`).
  - `close {id}` → SIGTERM, then SIGKILL after 2 s (`CloseGrace`).
  - `prompt {id,text}` types a prompt safely (`terminal.go:494`).
- **Snapshot.** `internal/vt/vt.go:106`: libghostty-vt, a headless VT per terminal fed every PTY chunk (`terminal.go:251`). `Snapshot()` is a VT byte stream that redraws the current screen plus modes, cursor, style and kitty keyboard state. It is not obviously scrollback: the formatter is called with default screen extras, so **scrollback in the snapshot is unverified**. The ghostty formatter options need checking before relying on it. `Plain()` is the trimmed visible text.
- **Output chunks.** The pump reads 32 KiB at a time (`terminal.go:229`). `broadcast` calls every subscriber's fn **synchronously while holding `s.mu`** (`:368`). The ops attach fn calls `c.Send`, a blocking `json.Encoder.Encode` on the socket (`ops.go:99`). **A slow subscriber stalls the pump, and so the PTY and every other viewer.** That is acceptable on a local unix socket but not over a phone link.
- **Pipe** (`internal/pipe/pipe.go`, `daemon/stream.go:247`) relays JSON lines for chat-only agents (`ChatOnly`). It has nothing to do with screen bytes.
- **Desktop adoption.** The desktop attaches every listed terminal (`terminals.rs:76` `listed` → `sessions.sync`). It places a terminal in a tab only for its own spawns (`intents`) or creates it tracked by `requestId` (`terminals.rs:125` `created` via `agent.creating`). A terminal a phone spawned would be attached but **placement in a desktop tab is unverified**. It probably needs a broadcast the desktop handles, as `agent.creating` is today.

### 8.2 Can a phone reach terminals today?

No. The phone WS has no terminal verbs. `AgentSummary.terminalId` is the only link. All `ops:*` terminal verbs need `owner`, and phones cannot open ops anyway: TCP `ownerListen` needs an owner token, and it is loopback only.

### 8.3 What exposing terminals to phones takes

- **New cap `terminal.v1`.** Handlers go in a new `internal/wsserver/terminal.go`, gated like `locals.v1` (unknown without the cap).
- **Scopes.** `terminal.list` and `terminal.attach`/`detach` read only, so `observe` (tool output is already visible to observe). `terminal.input`, `terminal.resize`, `terminal.create` and `terminal.close` give an arbitrary shell, which **bypasses `phone.maxAccess`** (phones are capped at `auto`). Add a new scope, say `shell`. It should not be in `PhoneScopes` by default; the owner grants it per device (new ops `devices.scopes`, or `config.set phone.terminals=on`). Register every verb in `peer.Needs` and in the matrix test.
- **Per-attach queue.** The phone attach callback must not write the socket. It should append to a per-conn, per-terminal bounded queue (for example 512 KiB) and return. A writer goroutine coalesces output (flush every ~16 ms or 32 KiB).
  - On overflow, drop the queue and re-send a fresh snapshot (`terminal.snapshot`) instead of disconnecting. This works because the VT holds the truth.
  - The same applies to `Hub` semantics: never block the pump.
- **Framing.** Use binary WS frames for output to avoid base64's 33% overhead and JSON escaping. `coder/websocket` supports `MessageBinary`. Proposed frame: `[u8 kind=1 output | 2 snapshot][u8 idLen][id ascii][u64 BE byteOffset][payload]`.
  - `byteOffset` counts bytes the PTY has emitted since terminal start, which pocketd would add as a counter per terminal. It lets a client detect gaps and lets `terminal.attach {sinceOffset}` resume cheaply when pocketd keeps a small ring buffer (for example 256 KiB). Otherwise the client falls back to a snapshot.
  - JSON control messages stay text frames.
  - If binary is rejected, use base64 `data` in JSON, which matches ops.
- **Resize policy.** A resize from a phone reflows the desktop's pane (shared PTY). The phone should by default render at the terminal's own `cols×rows` (scale it, or scroll horizontally). Resize only on an explicit "fit to phone", with the desktop re-asserting its size when it regains focus.
- **Input safety.** For PTY principals there is already an `ask_open` guard on `ops:input`. For phones, consider refusing `terminal.input` to a terminal whose agent has an open permission dialog unless it resolves through `permission.resolve`, so a phone keystroke can't answer Claude's TUI dialog by accident.

## 9. Projects, worktrees, locals, names: what pocketd knows

- **Registry.** `internal/registry/registry.go` reads `desktop.json` next to the socket. **The desktop is its only writer** (:1-2).
  - `File{projects []path, repos{path:{name,setup,gh_account,accounts}}, worktree{root}}`. `Load()` re-reads on stat change.
  - Pinned, groups and order live in the desktop's own store (`packages/desktop/crates/store`), and pocketd doesn't see them.
- `project.list` = `worktree.Projects(f)` (`internal/worktree/list.go:61`): `git worktree list --porcelain` per project, sorted by folder birth time. There is no dirty count, ahead/behind, PR or last activity.
- **Worktree create** exists only inside `agent.create`'s `checkout.new {name, base|branch|pr, copy?, setup?, autoName?}` (`proto/launch.go:55`; `launch.checkout` at `internal/launch/launch.go:226`; `worktree.Prepare/Add/PrepareBranch/PreparePR`). It reports steps `prepare, verify, fetch, worktree, copy, setup, agent`.
- **Worktree remove.** `worktree.Remove` (`create.go:238`, `git worktree remove --force`, branch kept, main refused) is used only by the CLI `pocketd worktree remove` (`cmd/pocketd/worktree.go:128`). There is no WS verb.
- **Names.** `internal/names`: display titles by worktree path. Sent whole as `worktree.names` and changed by `worktree.rename` (owner). `naming.failed {agentId}` is pushed when auto-naming fails.
- **Locals.** `internal/locals`: `Local{id,project,name}`. `local.create/rename/delete` are owner only. Delete closes the Local's terminals.
- **Missing for full phone workspace control** (the phone has no `owner` scope, so each needs a decision on whether it is `spawn` or a new `workspace` scope):
  - `worktree.create` without an agent, and `worktree.delete`, which closes its terminals, then runs `Remove` and `names.Forget`.
  - `local.*` opened to phones. They are owner only today, so a phone can't make a Local.
  - `terminal.create {cwd: worktree path | local id}`: open a login shell in a worktree or Local.
  - `project.add` / `project.remove` need a design change: the registry belongs to the desktop. Either pocketd proxies to the desktop's owner conn, or ownership of `desktop.json` moves.
  - Branches (local and remote, dated), so the phone can offer `checkout.new.branch` and `base`. `worktree.Taken` and `branches` exist internally (`create.go:62,82`).
  - Push `project.list` changes. Today it is pull only, with no subscription, so the phone must poll.

## 10. Git, GitHub, files: where the logic lives today

- **All git reading is in the desktop (Rust).** `packages/desktop/crates/git/src/git.rs`:
  - `read()` :95 (branch, base via the reflog `base_of`, staged/unstaged/numstat/name-status, untracked, ahead/behind, 20 commits, unpushed)
  - `log_graph` :554, `tips` :619, `commit_files` :659, `file_diff` :358, `diff_texts` :386, `branches` :217, `remote_branches` :222
  - For remote hosts the desktop runs git through pocketd `exec` and `fs.*` (`desktop/crates/hosts/src/remote.rs:266`). Those are owner-only ops and arbitrary argv, **so they must not be exposed to phones**.
- **GitHub is in the desktop too.** `desktop/crates/git/src/github.rs`: `gh pr view --json number,title,url,state,isDraft,statusCheckRollup,reviewDecision,latestReviews,reviewRequests,mergeable,mergeStateStatus,headRefOid,baseRefName` (:102), plus review threads via `gh api graphql` (:178), merge, ready, create, reply and resolve.
  - pocketd already runs `gh pr view` for `checkout.new.pr` (`internal/worktree/pr.go:29`) with the per-project gh account env (`daemon.GhEnv`).
- **Files.** `internal/hostop/fs.go`: `List` (with git-ignored flags), `Stat`, `Read(max, default 16 MiB)`. They are reachable only via owner ops. A phone needs a read-only, path-scoped variant: allow only paths inside a registered project or worktree (`worktree.Containing`), cap the size, and flag binary files.
- **Recommendation.** Port the read-only parts of `crates/git` to a new Go package (for example `internal/gitview`) and serve them with typed messages. Run them in goroutines, cache per worktree with change detection, and page or virtualize file lists, since 1000+ changed files is the norm. Compute diffs server-side into `proto.FileDiff`-like lines; `internal/timeline/diff.go` already builds `FileDiff` for edit tools and may be reusable.

## 11. Push notifications and Live Activities

- **None exists in code.** A grep for apns, expo-notifications, devicetoken, pushToken, ActivityKit, UNUserNotification and FCM across `packages/` finds no push code. The RN app has no `expo-notifications` and an empty entitlements file (per `docs/orchestrators/research/19-install-onboarding.md:175`).
- The desktop shows **local** macOS banners only: `packages/desktop/crates/pocket/src/desktop/alerts.rs`. `wants_banner` (:18) fires on Needs you, Failed or Done and is silent for Working/Idle, gated by `store::Notifications {banners, needs_you, failed, done}`. Reuse these rules for the trigger points.
- Prior research chose the Expo Push API from pocketd, so no `.p8` key ships on users' Macs (`docs/orchestrators/research/19-install-onboarding.md:292`, `02-zeron-architecture.md:328`). **That decision assumed the RN app.**
  - A native SwiftUI app gets raw APNs device tokens. Expo Push needs Expo push tokens.
  - Live Activity updates need APNs `apns-push-type: liveactivity` with topic `<bundle>.push-type.liveactivity`. Activities expose per-activity update tokens, and iOS 17.2+ adds push-to-start tokens.
  - Either way, someone has to hold the APNs `.p8`: a small publisher-run relay, or a self-builder's own key in pocketd config. This needs a decision.
- **Trigger points in pocketd:**
  - `agent.Registry.OnStatus` (`serve.go:124`) already sees every status transition.
  - `Broker.Ask` (`broker.go:54`) sees every permission ask.
  - Payload privacy rules are in `docs/orchestrators/research/18-trust-model.md:222`: fixed copy by default, project name as title, details opt-in.
- The phone socket dies in the background, so push is the only way to update a Live Activity while the app is suspended.

## 12. RN app wire contract (reference only)

- `packages/app/src/client.ts`: one WS per Mac. On open it sends `hello` with `id c<n>`, `token`, `clientId`, `protocolVersion: PROTOCOL_MAX`, `caps [pair.v1, summary.v2, registry.v1, launch.v1]` and `protocol {min,max}`.
  - Hello timeout is 10 s.
  - Reconnect backoff is 250 ms ×2 up to 16 s, reset after 30 s online (`connectivity.ts:3`).
  - It redials on AppState or NetInfo changes, with 1 s settle (`REDIAL_SETTLE_MS`). `send` drops messages while not online.
- `connection.ts:8` `outcome`: close 4401 or code `not_paired`/`revoked` means wipe the credentials. 4426 means updateMac if `server_too_old`, else updatePhone. Anything else is a retry.
- Resume: see section 6 (re-view, then full timeline re-fetch per viewed agent). Banners: "Reconnecting…" after 4 s, "unreachable" after 30 s (`connectivity.ts:34`).
- Deleting `packages/app` also requires editing `scripts/check.sh:30-31` (the app typecheck and test steps). **Keep `packages/protocol`.** Its `test/golden.test.mjs` decodes `internal/proto/testdata/golden/*` and is half of the wire contract. Alternatively, replace it with Swift Codable tests over the same golden files.

## 13. Test conventions

- **Unit tests** sit beside the code with sentence names (`TestAPairedPhoneGetsInUntilRevoked`).
  - WS handler tests: `internal/wsserver/wsserver_test.go` uses `setup(t, opts...)` :71 for a phone on a real `httptest` server with `fakeDriver`, and `local(t, principal)` :107 for owner/PTY. Helpers are `phone.send(raw)`, `recv()`, `hello()`, `closedWith()`.
  - Scope matrix: `internal/peer/check_test.go:16`. Add every new verb here.
  - Golden wire files: `internal/proto/testdata/golden/{client,server}/*.json`, checked by `TestServerGolden`, `TestClientGolden`, `TestDecodeClientRejects` and `TestDecodeClientAcceptsWhatTheSchemaAccepts` (`internal/proto/golden_test.go:142-259`). Each new message needs a golden file on both sides, plus reject cases.
  - There is one cap test per cap (`internal/proto/registry_caps_test.go`).
- **E2E** (`e2e/`, `//go:build unix`). `TestMain` builds `pocketd`, `fake/claude` (`e2e/fakeclaude`, a TUI plus `chat.go` stream-json), `fake/codex` (`e2e/fakecodex/appserver.go`, app-server JSON-RPC) and `redteam`. It also builds `pocketd-next` for upgrade tests.
  - `Start(t)` gives a scratch `POCKET_HOME` with the fakes first on PATH (`harness_test.go:83`).
  - `h.Paired(caps...)` runs real pairing over ops `pair.begin` and then hello (:348). `h.Owner(caps...)` opens WS over the unix socket (:326). `h.Ops()` is a raw ops conn. `StartClaude(t)` gives a harness, a phone, a terminal and an agent (:258).
  - `launchReady(setup)` makes a temp git repo project (:296).
  - Use `Phone.WaitFor(desc, pred)` for async messages.
- **Running:**
  - First run `scripts/build-ghostty.sh`; the cgo `internal/vt` links libghostty-vt from `third_party/ghostty/zig-out`.
  - Then `cd packages/pocketd && go vet ./... && go test -race -count=1 ./...`.
  - Always use a scratch home: `scripts/check.sh` exports `POCKET_HOME=$(mktemp -d)` and `POCKETD_SOCK` and unsets `POCKETD_PTY`, so tests run from inside a Pocket terminal never touch the live pocketd.
  - Red team: `scripts/red-team.sh`, which runs `TestRedTeamFromInsideATerminal`. Extend `e2e/redteam/main.go` for any new verb a PTY peer must not reach.

## 14. Proposed new protocol (protocol 3, new caps; nothing breaks old clients)

Conventions:

- Dot-separated `area.verb` names. Every request has an `id`, and its reply echoes it as `ack`, a typed reply, or `error{code}`.
- Pushes have no id and are gated by a cap negotiated in hello.
- Snapshots are sent whole on hello and on change, as `local.list` and `worktree.names` are.
- Long work streams `*.progress` and then a final message, as `agent.create` does.

### terminal.v1 (scopes: observe for read; new `shell` for write)

- `terminal.list {id}` → `terminal.list {id, terminals:[{id,cwd,cols,rows,foreground?,local?,agentId?,title?}]}`. Push `terminal.list` whole on spawn and exit.
- `terminal.attach {id, terminalId, sinceOffset?}` → `terminal.snapshot` (binary kind 2, or JSON `{terminalId,cols,rows,offset,data(b64)}`), then `terminal.output` frames (binary kind 1 with offset) and pushes `terminal.resized {terminalId,cols,rows}`, `terminal.foreground {terminalId,text}`, `terminal.exited {terminalId,code}`. Per-attach bounded queue; overflow means a fresh snapshot.
- `terminal.detach {id, terminalId}` → ack. Detach also happens implicitly on socket close.
- `terminal.input {id?, terminalId, data(b64)}`. Fire-and-forget like ops `input`. A binary frame kind 3 can also carry it for keystroke streams.
- `terminal.resize {id, terminalId, cols, rows}` → ack (see the policy in section 8.3).
- `terminal.create {id, requestId, cwd? | worktree? | local?, cols?, rows?, command?}` → `terminal.created {id, requestId, terminalId}`. It runs the login shell like the desktop's `shell_op`, and the desktop must adopt the new terminal into a tab.
- `terminal.close {id, terminalId}` → ack.
- `terminal.screen {id, terminalId}` → `terminal.screen {id, text}`. Plain text, cheap, good for Live Activity or widget previews.

### workspace.v1 (scope: `spawn`, or a new `workspace` scope; owner can always)

- `worktree.create {id, requestId, project, name, base?|branch?|pr?, copy?, setup?}` → `worktree.progress {requestId, step, note?}`* then `worktree.created {requestId, path, branch}`. Reuses `worktree.Prepare/Add/PrepareBranch/PreparePR`, `CopyInto` and setup.
- `worktree.delete {id, project, path}` → ack. It closes the worktree's terminals, then runs `worktree.Remove` and `names.Forget`. It refuses the main worktree (`ErrMain`).
- `branch.list {id, project}` → `branch.list {id, local:[{name,committedAt}], remote:[name], default}`.
- `project.changed` push: the `project.list` snapshot whenever `desktop.json` or a worktree set changes. Gate it behind `workspace.v1`, run it off the read loop, and cache it.
- Open `local.create/rename/delete` to this scope when the cap is negotiated.

### git.v1 (scope: observe for reads; `drive` for stage/commit/push if wanted later)

- `git.status {id, path}` → `git.status {id, path, branch, base?, ahead, behind, unpushed?, remotes, counts:{files,added,removed}, files:[{path,status,added,removed,staged,unstaged}], truncated, rev}`. Supports `offset/limit` paging for 1000+ files; `rev` is an opaque cache key.
- `git.watch {id, paths:[...]}` → pushes `git.changed {path, rev}`, so the phone re-requests only what's on screen.
- `git.log {id, path, revs?, skip, limit}` → `git.log {id, commits:[{sha,parents,subject,author,at,refs}], hasMore}`, mirroring `log_graph` and `tips`.
- `git.branches {id, path}`.
- `git.diff {id, path, file, staged?|commit?, context?}` → `git.diff {id, file, diff: FileDiff, truncated}`. Computed server-side with a line cap.
- `git.commit.files {id, path, sha}` → `{files:[{path,status,added,removed}]}`.

### github.v1 (scope: observe for reads; `drive` for merge, ready, reply and resolve)

- `github.pr {id, path}` → `github.pr {id, pr?: {number,title,url,state,draft,base,head,checks:{passed,failed,pending}, runs:[{name,outcome,url}], review, reviews:[{author,review}], requested, mergeable, blocked}, problem?: "missing"|"loggedOut"}`.
- `github.threads {id, path, number}` → threads [{id,path,line?,left,outdated,resolved,comments:[{author,body,at,hunk}]}].
- Optional writes: `github.merge {id,path,number,method}`, `github.ready`, `github.reply {threadId, body}`, `github.resolve {threadId, resolved}`.
- `github.watch {id, paths}` → `github.changed {path}`, polled server-side at a back-off.

### files.v1 (scope: observe; paths must be inside a registered project or worktree)

- `fs.list {id, path}` → `fs.list {id, entries:[{name,dir,link,size,mtime,ignored}]}`. Reuses `hostop.List`.
- `fs.read {id, path, max?}` → `fs.read {id, data(b64)|text, size, truncated, binary, language?}`. Default cap is small, for example 1 MiB.
- `fs.search {id, root, query, limit}` (optional, `git ls-files`-based).

### push.v1 (scope: observe; per device)

- `push.register {id, token, environment:"sandbox"|"production", bundleId, prefs:{needsYou,done,failed,details}}` → ack. Stored on the `Device`; revoke deletes it.
- `push.unregister {id}` → ack.
- `push.test {id}` → ack plus a test push.

### activity.v1 (Live Activities; scope: observe)

- `activity.start-token {id, token}` (push-to-start, iOS 17.2+) → ack.
- `activity.register {id, activityId, agentId|"fleet", token}` → ack. pocketd sends `liveactivity` pushes on that agent's status, activity and goal changes, rate-limited for the APNs budget.
- `activity.end {id, activityId}` → ack.
- Needs the APNs relay or key decision (section 11).

### Also

- Fix `ws:agent.btw` missing from `peer.Needs`, as its own change.
- Add `agent.timeline {beforeSeq}` so long chats can page back. `hasOlder` should then be exact.
- Consider `devices.scopes` (owner) to grant `shell` and `workspace` per device. `hello.ok.scopes` (scopes.v1) already tells the phone what it may show.
