# 18 — Trust and security model for Pocket: pairing, device tokens, remote reach, agent control

Date: 2026-09-30.

**Sources**

- Pocket @ b9d14a1 (orchestrator-research worktree).
- zeronsh/comet @ ed3b1aae4a5189eef67143db7b8c5c3ee7a933c5 (MIT).
- hardbeat920/monocode @ cdc1441dc51e3709cd843e5c316608a123f323c6 (MIT).
- Installed deps:
  - react-native@0.86.3, from Pocket's pnpm store;
  - github.com/coder/websocket v1.8.15, from the Go module cache;
  - the macOS SDK headers.
- Official docs, fetched 2026-09-30:
  - https://code.claude.com/docs/en/interactive-mode
  - https://code.claude.com/docs/en/security
  - https://tailscale.com/kb/1312/serve
  - https://tailscale.com/kb/1192/acl-samples
  - https://docs.expo.dev/versions/latest/sdk/securestore/
  - https://docs.expo.dev/versions/latest/sdk/notifications/

**Citation legend.**

- `P path:L` is Pocket, relative to the worktree root.
- `Z path:L` is Zeron (comet); `M path:L` is MonoCode. Both are relative to their clone roots.
- `RN path:L` is `node_modules/.pnpm/react-native@0.86.3…/node_modules/react-native/<path>`.
- `WS path:L` is `github.com/coder/websocket@v1.8.15/<path>`.
- `SDK path:L` is `MacOSX.sdk/usr/include/<path>`.
- `rNN:L` is a line in prior report NN under `docs/orchestrators/research/`; `rNN-K` is idea K in that report.
- Bare URLs are official docs.
- Where code and docs disagree, code wins and the text says so.

## TL;DR

- **Today a paired phone is a remote shell running as you.** Four things combine:
  - `agent.prompt` types raw text into the PTY (P packages/pocketd/internal/terminal/terminal.go:278-287).
  - In Claude, a leading `!` runs a shell command directly (https://code.claude.com/docs/en/interactive-mode, "Shell mode with `!` prefix").
  - The phone can answer "Yes, and switch to auto mode" (P packages/pocketd/internal/daemon/permission.go:15,24-26).
  - Every future rule must assume "device token = shell". MonoCode says the same: "Tokens grant control of the host as its OS user" (M docs/remote-access.md:55).
- **Biggest current gap: an agent can approve its own permission requests.**
  - The phone-grade token sits in plaintext in `~/.coding-pocket/config.json` (P packages/pocketd/internal/config/config.go:52-60). The desktop reads it too (P packages/desktop/crates/agents/src/agents.rs:198-200,248).
  - Claude runs read-only Bash such as `cat` without asking (https://code.claude.com/docs/en/security, "Permission-based architecture").
  - Any authenticated socket may call `permission.resolve` (P packages/pocketd/internal/wsserver/wsserver.go:156-161).
- **The ops socket is fully open to every process inside a Pocket terminal.**
  - `POCKETD_SOCK` and `POCKETD_PTY` are injected into every PTY (P packages/pocketd/internal/daemon/plugin.go:81).
  - `spawn` takes arbitrary cmd, args, cwd and env; `input` writes raw bytes to any terminal (P packages/pocketd/internal/ops/ops.go:95-100,121-128,150-151).
  - So an agent can type `!<cmd>` into a sibling Claude and skip that sibling's permission checks.
- **Reach is too wide.**
  - pocketd listens on `:4517` on every interface over plain `ws://` (P packages/pocketd/cmd/pocketd/serve.go:46).
  - It accepts any Origin (`InsecureSkipVerify: true`, P …/wsserver.go:53).
  - It uses one shared token with no device identity (`clientId` is hardcoded "pocket-app", P packages/app/src/session.tsx:76).
- **Fix order (all S/M):**
  1. Bind to loopback and Tailscale only (18-1).
  2. Host allowlist and pre-auth limits (18-2).
  3. Hashed per-device tokens with revoke (18-3).
  4. QR pairing with a one-time code, never the long-lived token (18-4).
  5. Move the desktop off the phone token (18-6).
  6. Per-terminal agent grants plus a peer-PID check, so no approving or pairing from inside a PTY (18-8).
- **Copy MonoCode's credential model:**
  - 32-byte base64url tokens, shown once, stored as sha256 (M host/store.ts:278-313);
  - the 403 and 401 handshake checks (M host/server.ts:203-225);
  - Revoke copy (M src/features/connections/ui/ConnectionsSettings.tsx:222-231).
- **Copy Zeron's Devices page and file-path grammar:**
  - the page: sections, last-seen format, Rename (Z crates/ui/src/settings/devices.rs:32-46,390-396);
  - the paths: `.git` blocked, symlinks rejected per component, canonical path must stay inside the root (Z crates/engine/src/workspace_files.rs:150-196,1512-1556).
- **Do not copy from Zeron:**
  - "every signed-in device is fully trusted" (r01:335);
  - `AbsoluteRead::Outside`, which lets a device read any host file by absolute path (Z …/workspace_files.rs:428-436,1360-1372,1420-1443);
  - TOFU ownership of the host room (Z edge/src/device-room.ts:150-159);
  - the lack of E2EE (Z docs/PARITY.md:101-102).
- **Off the tailnet, E2EE is mandatory.** Use Noise with per-device static keys pinned via the QR. This replaces r15-12's single shared seed with PBKDF2, which cannot revoke one device.
- **Repo-supplied commands** (r06-15, r01-7) run only after an explicit desktop import, as in Zeron (Z docs/reference/project-actions.md:5-9). **Phone spawn** (r08-21) is limited to claude or codex in a registered project, with no cmd, env or cwd from the phone.

## Findings

### F1. Pocket today: code facts

- **Listener:**
  - Binds `net.Listen("tcp", ":4517")` on all interfaces (P packages/pocketd/cmd/pocketd/serve.go:46).
  - Prints `phone: ws://<tailscale ip>:<port>` and the token to stdout (:58-59).
  - Falls back to `localhost` when `tailscale ip -4` fails (:63-69).
- **Token:**
  - 24 random bytes as base64url, so 32 chars and 192 bits. Port 4517; dir 0700, file 0600 (P packages/pocketd/internal/config/config.go:52-60).
  - One token for every client.
- **Handshake:**
  - `InsecureSkipVerify: true`, so any Origin is accepted (P packages/pocketd/internal/wsserver/wsserver.go:53).
  - Read limit is 1 MiB even before auth (:57).
  - hello uses plain `!=`, and bad token and protocol mismatch get the same "Rejected" (:122-127).
  - No rate limit and no device record.
  - After hello, the server sends every open permission request to the client (:135-139).
- **Authorization:** none past hello.
  - Every authenticated client can call `permission.resolve`, `agent.prompt`, `interrupt`, `compact` and `close` on any agent (P …/wsserver.go:156-200).
  - The desktop only uses view, seen and close (P packages/desktop/crates/agents/src/agents.rs:209-217), yet holds the same power.
- **Prompt path:**
  - `Prompt` writes the text as-is, then sends `\r` after 150 ms (P packages/pocketd/internal/terminal/terminal.go:278-287).
  - Only guard: the Claude pid must be in the foreground (P packages/pocketd/internal/daemon/presence.go:123-158).
  - ESC sequences such as Shift+Tab pass through. Shift+Tab cycles permission modes up to `bypassPermissions` and `auto`, "when available" (https://code.claude.com/docs/en/interactive-mode, "Cycle permission modes").
- **Permissions:**
  - IDs are sequential `perm-N` (P packages/pocketd/internal/broker/broker.go:47) and are published to every phone (:52).
  - Unanswered requests are denied after 10 min (:16).
  - Allow can carry `setMode auto` (P packages/pocketd/internal/daemon/permission.go:15,24-26).
- **Ops socket:**
  - Socket 0600 in a 0700 dir (P packages/pocketd/internal/ops/ops.go:67-77); no auth.
  - `spawn` accepts any Cmd/Args/Cwd/Env (:95-100,121-128).
  - `input` takes raw bytes, `screen` returns plain text, `close` works on any terminal ID (:150-158).
- **Hooks:**
  - The caller supplies the `Pid` itself, as its nearest Claude ancestor (P packages/pocketd/cmd/pocketd/hook.go:14-37). pocketd checks only that the pid is the Claude in that terminal (P packages/pocketd/internal/daemon/claude.go:14-24).
  - A same-terminal process can forge hook events, including fake PermissionRequests that appear on the phone (P packages/pocketd/internal/daemon/daemon.go:92-120).
- **Phone at rest:**
  - SecureStore keys `pocket.host` and `pocket.token` use default options (P packages/app/src/screens/ConnectScreen.tsx:7-8).
  - The default is `WHEN_UNLOCKED`, with `requireAuthentication` off (https://docs.expo.dev/versions/latest/sdk/securestore/).
  - No app lock; the token field uses `secureTextEntry` (ConnectScreen.tsx:50).
- **Setup commands and copy lists:**
  - The setup command is typed by the user on the desktop and stored in `desktop.json` (P packages/desktop/crates/store/src/store.rs:8-15). It is device-private.
  - The file is written with default permissions (:38-41).
  - It runs in the login shell before the agent starts (P packages/desktop/crates/daemon/src/daemon.rs:101-122; P packages/desktop/crates/pocket/src/forms.rs:427-448).
  - The default copy list takes any `.env*` entry in the repo root (forms.rs:284-291). `std::fs::copy` then follows symlinks (forms.rs:436).
- **Not built yet:** push, phone spawn and the file viewer. Their rules can be set before any code exists.

### F2. The same-UID boundary

- **Same-UID is not a security boundary.**
  - Same-UID malware can already edit shell rc files.
  - The goal is narrower: no single low-friction agent tool call should grant approval or remote-shell powers.
  - MonoCode takes the same stance: "Treat a bad prompt like running those CLIs yourself" (M SECURITY.md:7).
- **Zero-prompt reads:**
  - Claude, in Manual mode, runs a built-in set of read-only commands without asking, including `cat` (https://code.claude.com/docs/en/security).
  - Read/Grep/Glob outside the working directory prompt, but read-only Bash is bounded only by opt-in sandbox `denyRead` (same page, "Working directory boundary").
  - So the plaintext token in config.json is one zero-prompt call away from an injected agent.
- **Network step:**
  - `curl`/`wget` are not auto-approved (same page, "Network command approval").
  - Self-approval therefore needs one approved or allow-listed network-capable command. An allow-listed `Bash(node:*)` would be enough (how common that is: unverified).
- **What pocketd can check:**
  - It can learn the peer PID of a unix-socket client with `LOCAL_PEERPID` (SDK sys/un.h:89), or its audit token with `LOCAL_PEERTOKEN` (:93).
  - pocketd owns every PTY, so it can tell whether a client descends from a Pocket terminal.
  - Double-fork or `launchctl` reparenting escapes this check; treat it as raising the bar, not a boundary.

### F3. MonoCode model (copy most of it)

- **Loopback only:**
  - The host binds only `127.0.0.1:3774` (M docs/remote-access.md:57).
  - Remote desktops reach it through an SSH local forward bound to loopback (:24).
  - Pairing runs `monocode-host pair --name <quoted> --json` over the SSH-authenticated channel (M src-tauri/src/remote_ssh.rs:417-420).
- **SSH hardening** (M src-tauri/src/remote_ssh.rs:165-215):
  - `ForwardAgent=no`, `ForwardX11=no`, `ControlMaster=no`, `PermitLocalCommand=no`;
  - `StrictHostKeyChecking=ask`, `ExitOnForwardFailure=yes`, `ConnectTimeout=15`;
  - the askpass secret travels over a nonce-authenticated loopback socket, "never argv/files" (M src-tauri/src/ssh_askpass.rs:1-2).
- **Tokens** (M host/store.ts:36,278-313; M host/cli.ts:79-89,185-215):
  - `randomBytes(32)` base64url, 43 chars;
  - only the sha256 is stored, in `devices(id, name, hash UNIQUE)`;
  - shown once;
  - `devices` and `revoke` CLI commands.
  - No last-seen, created-at or platform fields exist in code. The docs don't claim them either.
- **Handshake** (M host/server.ts):
  - 403 if an Origin is present, the method isn't POST, or the path isn't `/rpc` (:203-213).
  - Bearer must match `[A-Za-z0-9_-]{43}`, else 401 "Device credential is invalid or revoked" (:215-225).
  - The token is checked again after the body is read (:227-234).
  - The environment ID is pinned: "Host identity changed; reconnect this machine explicitly" (:237-243).
  - Limits: `requestTimeout 20000`, `headersTimeout 10000`, `maxHeaderSize 8192` (:191).
  - Other: `devices.revokeSelf` (:415-418); a separate lifecycle secret (M host/cli.ts:241).
- **Agent grants** (M src-tauri/src/control.rs):
  - Per-grant token of 2 UUIDv4s, about 244 bits (r11 corrected the "256-bit" claim).
  - The token is put into the child env only while the grant exists, and removed otherwise (:464-486).
  - The app namespace works only during an opted-in turn, else `APP_TURN_INACTIVE` (:15).
  - Listener on `127.0.0.1:0` (:183); 8 workers, queue 32 (:196-201); 3 s I/O (:219-220); at most 24 pending (:242); 35 s reply (:266).
- **Workspace commands and paths** (M host/workspace-commands.ts:37-79,84-87,111-113; M host/workspace.ts:28-61):
  - an allowlist of workspace commands;
  - paths: ≤4096 chars, no NUL; resolved against the root, rejected if the result escapes it or has a `.git` component, then a realpath recheck. Absolute inputs inside the root are accepted ("Paths are absolute host paths", M host/workspace-commands.ts:35);
  - caps: 1 MiB text, 10 MiB preview.
- **Endpoint rule:** `https`, or `http` on loopback only; no userinfo, query, fragment or path (M src-tauri/src/remote.rs:72-91; M docs/remote-access.md:69).

### F4. Zeron model (copy the UI and paths, not the trust)

- **Trust:**
  - Online only after WorkOS sign-in, and then every device is fully trusted (r01:332-344).
  - No E2EE: "transport is TLS + WorkOS bearers" (Z docs/PARITY.md:101-102).
- **Relay room:**
  - The first host to claim the room becomes its owner (TOFU). A later host from the same user supersedes it with close code 4409 (Z edge/src/device-room.ts:150-170).
  - The JWT may be passed as `?token=` (Z edge/src/auth.ts:32-37).
- **Local IPC:**
  - Loopback TCP with no token (Z crates/engine/src/lib.rs:966-985).
  - It rejects any handshake that carries an Origin, because browsers can't suppress Origin (Z crates/rpc/src/server.rs:154-183).
  - Pocket's 0600 unix socket is stricter against other local users.
- **Paths** (Z crates/engine/src/workspace_files.rs):
  - relative ≤4096 bytes, ≤256 components, no `.`/`..`, `.git` blocked case-insensitively (:29-44,150-196);
  - symlinks rejected per component, regular files only, canonical path must start with the root, else "file escaped workspace" (:1512-1556);
  - caps: editable 1 MiB, preview 8 MiB, 500 entries per page, 6 s RPC timeout (:29-44).
- **The hole:** a chat target may read by absolute path, and anything outside the workspace comes back read-only as "a host file", e.g. `~/.ssh/id_ed25519` (:428-436,1360-1372,1420-1443).
- **Stated stance:** "Ignored-file visibility is not an authorization boundary"; no filename denylist (Z ARCHITECTURE.md:84; Z README.md:42).
- **Repo commands:**
  - "Opening a repository never authorizes a command"; `zeron.json` candidates must be imported first, and imports are device-private (Z docs/reference/project-actions.md:5-9).
  - Invalid JSON or more than 50 entries invalidates the whole file (:30).
- **Push:** fixed body copy: "Run finished" / "Waiting on your input" / "Run failed" (Z edge/src/push-notify.ts:59-60,91-99). The title is the chat title, else "New session" (:62-69), so it can carry prompt-derived text.
- **Devices page:**
  - name, platform, last-seen, "This device", Copy ID, Rename; **no revoke** (Z crates/ui/src/settings/devices.rs:1-3,333-357).
  - Online means a heartbeat within 70 s (:23).

### F5. Threat table: actor by asset

**Assets**

| Code | Asset |
|---|---|
| A1 | Mac shell as the user |
| A2 | Permission decisions |
| A3 | Repo files and secrets (`.env`, keys) |
| A4 | Transcript and prompt text |
| A5 | Device credential |
| A6 | Availability |

**Scale:** L = likelihood, I = impact; each is H, M or L.

| Actor | Path today (cite) | Assets | L / I | Mitigation (rule) |
|---|---|---|---|---|
| Lost phone, unlocked | App opens straight into sessions. The token is in SecureStore with default options (P ConnectScreen.tsx:7-8). A `!cmd` prompt runs a shell command. | A1 A2 A3 A4 | M / H | Revoke that phone alone from the desktop (R1). Optional Face ID app lock (R13). |
| Lost phone, locked | Keychain `WHEN_UNLOCKED` protects the token. Lock-screen pushes do not exist yet. | A4 (future push) | M / L | Fixed push copy; Allow/Deny require device auth (R12). Keychain entry is `THIS_DEVICE_ONLY` (R13). |
| Phone backup / migration | `WHEN_UNLOCKED`, not `…THIS_DEVICE_ONLY` (Expo SecureStore docs), so the token can move with a backup to a new device. | A5, then A1 | L / H | `WHEN_UNLOCKED_THIS_DEVICE_ONLY` (R13). |
| LAN attacker (café Wi-Fi) | Port 4517 is open on every interface (P serve.go:46). If the user typed a LAN IP, the token crosses the network in cleartext on each hello (P packages/app/src/client.ts:33). Pre-auth frames up to 1 MiB (P wsserver.go:57). | A5, then A1; A6 | M / H | Bind loopback plus Tailscale only (R3). LAN only as an opt-in with TLS (R4). Pre-auth limits (R5). |
| Tailnet peer | The default policy "lets all devices in the tailnet access all other devices" (https://tailscale.com/kb/1192/acl-samples). On a shared work tailnet, coworkers can reach 4517. They still need the token, and WireGuard keeps it confidential. | A6; A1 only if the token leaks | L / H | Per-device tokens and failed-hello limits (R1, R5). Optional Tailscale ACL snippet (R4). Never trust `Tailscale-User-*` headers for auth (R4). |
| Malicious web page (CSWSH / DNS rebinding) | Any Origin is accepted (P wsserver.go:53). The token is sent in hello, not a cookie, so today this means pre-auth DoS and guessing. It becomes real once pairing codes exist, because they are short-lived and guessable from a browser. | A6; A5 once pairing exists | M / M | Host allowlist, no `InsecureSkipVerify`, pairing attempt limit (R2, R5). |
| Malicious repo config | Today nothing repo-supplied runs; setup is typed on the desktop (P forms.rs:246,340). But the default copy list picks up committed `.env*` symlinks (P forms.rs:284-291,436). Future `pocket.json`, `.worktreeinclude` and phone-triggered setup would widen this. | A1 A3 | M / H | Import gating, copy-list validation, desktop.json 0600 (R11). Phone runs imported actions only (R7). |
| Prompt-injected agent on the ops socket | Any PTY process can `spawn` any cmd, `input` raw bytes or read `screen` of any terminal (P ops.go:121-158). It can type `!cmd` into a sibling (bypassing the sibling's permission checks), cycle modes with ESC[Z, forge hook events (P hook.go:14-37), or read config.json and resolve its own permission requests (F2). | A1 A2 A3 A4 | M / H | Per-terminal grants, peer-PID ancestry, no approve or pair from inside a PTY, sanitized cross-session prompts, hooks bound to the peer PID (R6, R8, R9). |
| Lock-screen push content (bystander; APNs/relay operator) | Not built. r08 asked whether to include the tool name and command, proposing fixed copy by default with details opt-in (r08:615). | A4 | M / M | Fixed copy by default, title = project name, details opt-in, random requestId (R12). |
| Relay operator (future 02-1/08-22) | A relay without E2EE sees every frame, which would include tokens and prompts; Zeron has no E2EE (Z docs/PARITY.md:101). | A1 A2 A4 A5 | L / H | Noise with per-device keys (R4). |

### F6. Recommended Pocket trust spec

**R1. Per-device tokens**

- **Store:** `~/.coding-pocket/devices.json`, file 0600 in the existing 0700 dir (P config.go:57-60). Each entry holds:
  - `id` (16 random bytes, hex);
  - `name` (≤64 chars);
  - `platform`: `ios`, `android` or `watch`;
  - `scopes`: a list;
  - `tokenHash`: sha256 hex;
  - `createdAt`, `lastSeenAt`, `lastAddr`.
- **Token:** 32 random bytes, base64url, 43 chars. Accept only `^[A-Za-z0-9_-]{43}$`. Look it up by sha256; the plaintext never touches the Mac's disk (M host/store.ts:278-313; M host/server.ts:215-225).
- **Last seen:**
  - Update `lastSeenAt` and `lastAddr` on hello, and at most once per 60 s while connected.
  - "Online" means a live socket. pocketd knows this directly, so Zeron's 70 s heartbeat window is not needed.
- **Revoke:**
  - Delete the entry and close that device's live sockets with close code `4401` "revoked".
  - Drop its push token.
  - The phone clears SecureStore and shows the Revoked state.
  - `devices.revokeSelf` powers the phone's "Unpair" (M host/server.ts:415-418).
- **CLI:** `pocketd devices [--json]`, `pocketd devices rename <id> <name>`, `pocketd devices revoke <id>`. All are refused from PTY descendants (R9).
- **Migration:**
  - The config.json `Token` becomes a device named "Shared token (legacy)".
  - The desktop offers "Re-pair and remove".
  - Once no legacy connection has been seen for 7 days, delete it. pocketd stops printing `token:` (P serve.go:59).

**R2. Pairing and QR**

- **Minting:**
  - Desktop "Pair phone", or `pocketd pair`. The CLI requires a TTY on stdin and is refused from PTY descendants (R9).
  - The pairing code is 16 random bytes, base64url, 22 chars.
  - TTL is 5 min and each code is single-use. Minting a new code voids the old one.
  - 5 failed redemptions void every code and lock pairing for 60 s.
  - Zeron's CLI code likewise "expires in a few minutes and only works on the device that started sign-in" (Z edge/src/auth-routes.ts:161-192).
- **QR:** `anywhere://pair?v=1&h=<host:port>&c=<code>&n=<mac name>`, plus `&fp=<base64url sha256(Mac static pubkey)>` only when relay or LAN TLS is enabled.
  - The scheme is already registered (P packages/app/app.json:7).
  - `h` is the MagicDNS name in Serve mode, else the Tailscale IPv4.
  - **The QR never carries the long-lived token.** r08-18 (r08:489) proposed `&token=` and is superseded here.
- **Redemption:**
  - The phone sends a pre-auth message `pair {code, name, platform, protocolVersion}`.
  - The reply is `paired {deviceId, token}`. The phone stores the token, reconnects, and sends a normal hello.
  - The desktop sheet shows "Paired {name}" and closes.
- **Manual fallback:** host plus code, typed. The typed code must never be the long-lived token.

**R3. Bind policy**

- **Default:** listen on `127.0.0.1:4517` plus each `tailscale ip -4` and `tailscale ip -6` address. The Tailscale lookup already exists (P serve.go:63-69).
  - Re-check every 30 s, because Tailscale may come up after pocketd; add or drop listeners as it changes.
- **No Tailscale:** loopback only. Print "Phone access needs Tailscale", replacing today's `ws://localhost` hint (P serve.go:68).
- **Serve mode (15-13):** loopback only. `tailscale serve` exposes it to the tailnet over HTTPS and needs HTTPS certificates enabled in the tailnet (https://tailscale.com/kb/1312/serve).
- **`--listen lan`:** refused unless TLS is configured (R4).

**R4. Transport**

- **On the tailnet (default):** `ws://100.x` is fine because WireGuard encrypts it. The token is still required.
  - Docs should carry an optional tailnet ACL snippet limiting 4517 to the user's own devices, because the default policy allows every device (https://tailscale.com/kb/1192/acl-samples).
- **Serve TLS:**
  - `wss://<mac>.<tailnet>.ts.net` uses a public-CA certificate, so no pinning is needed.
  - Serve adds `Tailscale-User-Login` and `Tailscale-User-Name` headers (https://tailscale.com/kb/1312/serve). Record them as device metadata only: any local process can send them to loopback.
- **Phone manual entry:**
  - Allow `ws://` only for loopback and the Tailscale IP ranges.
  - Everything else must be `wss://`. This follows MonoCode's HTTPS-or-loopback rule (M src-tauri/src/remote.rs:72-91).
- **Off the tailnet (02-1, 08-22, 14-8):**
  - E2EE is mandatory, and the relay sees only ciphertext.
  - Use Noise IK: the phone pins the Mac's static key from the QR `fp`, and the Mac records the phone's static key at pairing (Noise patterns: https://noiseprotocol.org/noise.html). This gives per-device revoke and forward secrecy.
  - The device token still goes inside the channel.
  - Reject r15-12's design (r15:358). Its single 32-byte seed shared by every phone means no per-device revoke. PBKDF2 at 100k iterations adds nothing to a 256-bit random seed. A static symmetric key has no forward secrecy.
- **LAN TLS with a pinned self-signed certificate:** `wont` for v1.
  - RN's WebSocket exposes only headers (RN Libraries/WebSocket/WebSocket.js:101), with no trust hook, so pinning needs native code.

**R5. Handshake hardening**

- **Origin and Host:** drop `InsecureSkipVerify` (P wsserver.go:53).
  - coder/websocket then allows an absent Origin, or an Origin whose host equals the request Host (WS accept.go:228-241).
  - Also require `Host` to be one of: loopback, the bound Tailscale IPs, or the MagicDNS name. The same-host check alone lets a DNS-rebinding page through.
- **Why not reject every Origin (Zeron/MonoCode style):**
  - Zeron and MonoCode reject any Origin (Z crates/rpc/src/server.rs:154-183; M host/server.ts:203-213).
  - Pocket can't: RN Android adds a default `origin: http(s)://host:port` (RN ReactAndroid/…/websocket/WebSocketModule.kt:104-127,387-407).
- **Pre-auth limits:**
  - Read limit 4 KiB until authenticated (today 1 MiB, P wsserver.go:57). Keep the 10 s hello deadline (:22-27).
  - At most 16 pre-auth sockets.
  - 3 failed hello/pair attempts per source address per minute, then close.
- **Errors:**
  - Protocol mismatch returns "Update Pocket on your Mac or phone".
  - A bad or revoked token returns "Not paired", with `4401` on live revoke.
  - Today both return "Rejected" (P wsserver.go:122-127).
- **Hygiene:**
  - Random `requestId` instead of `perm-N` (P broker.go:47). Not an exploit, since resolve still requires auth, but IDs leak into push payloads.
  - Constant-time compare for the legacy token.

**R6. Capability scopes**

Scopes: `observe`, `drive` (prompt, interrupt, compact, close), `approve`, `spawn` (R7), `files` (R10), `owner` (raw spawn, attach, input, resize, pair, devices).

| Principal | Credential | Scopes | Never |
|---|---|---|---|
| Paired phone | device token (R1) | observe, drive, approve, spawn, files | owner |
| Observe device (watch or second phone, optional) | device token | observe, plus approve if chosen | drive, spawn, files |
| Desktop app | ops socket, not a PTY descendant | owner, observe; closes agents via ws or ops | reading any phone token (it stops reading config.json; P agents.rs:198-200) |
| `pocketd run` / `attach` CLI outside Pocket | ops socket, not a PTY descendant | owner | — |
| Agent in a Pocket PTY, no grant | ops, peer PID descends from a PTY | `hook` for its own PTY only; the hook PID comes from `LOCAL_PEERPID`, not the payload | approve, pair, devices, owner, reading other terminals |
| Agent with grant (desktop toggle, 11-2) | per-terminal grant token in env only while granted (M control.rs:464-486) | observe siblings; drive siblings with sanitized prompts (R8); spawn from the allowlist (R7) | approve, pair, devices, raw `input`/`attach` to others |

- `permission.resolve` requires `approve`, which only device tokens held off the Mac carry. That alone ends self-approval once the desktop no longer holds a phone-grade token and no plaintext token sits on disk.

**R7. What the phone may spawn (08-21, 11-3)**

- **Message:** `agent.spawn {target: {projectId} | {worktreeId} | {newWorktree: {projectId, base}}, agent: "claude" | "codex", prompt?, mode?: "default" | "plan"}`.
- **cwd:** resolved only from roots the desktop registered.
  - Needs pocketd to own or mirror the project registry, which today lives only in `desktop.json` (P store.rs:17-28).
- **argv:** fixed server-side and run through the login shell like `agent_op` (P daemon.rs:99-101).
  - No cmd, args, env or cwd from the phone.
  - No `bypassPermissions` / auto mode unless the desktop setting "Let phone start agents in auto mode" is on.
- **Setup:** only the device-private `RepoConfig.setup` (P store.rs:13) or an imported action (R11).
- **Raw terminals:** `terminal.spawn` of arbitrary commands stays owner-only, for the desktop.
- **Folder trust:** Claude requires trust verification for first-time codebases (https://code.claude.com/docs/en/security). pocketd must not type the first prompt while that dialog is in the foreground; show "Trust this folder on your Mac first". See the open questions.

**R8. What the phone may type**

- **All sources:** strip C0 control characters except `\n` and `\t`, plus ESC and DEL. Interrupt and compact keep their own messages (P packages/app/src/session.tsx:108-119). Cap at 64 KiB.
- **Phone:** a leading `!` is allowed, since the phone is the owner. The composer shows a "Runs in your shell" chip.
- **Agent-sourced cross-session prompts:** reject a leading `!` (shell mode) or `/` (slash commands). Prefix the timeline entry with "From agent {name}".

**R9. Agents driving other sessions (11-1, 11-2)**

- **Scope assignment:**
  - Scope is set by `LOCAL_PEERPID` ancestry (SDK sys/un.h:89), not by whether a token is present. An agent that omits its token still gets terminal scope.
  - Reparenting (e.g. `nohup` or `launchctl`) escapes the check; this is a residual risk.
- **Grants:**
  - Off by default. Enable one per terminal from the desktop tab menu, or with a MonoCode-style opt-in turn (M control.rs:15).
  - The grant token exists in the child env only while granted.
- **Hooks:** accept only from a peer PID that descends from the Claude PID the hook claims.
  - Closes forged hook events and fake permission prompts (P hook.go:14-37; P claude.go:14-24).
- **Limits (copy MonoCode, M control.rs:196-266):** 8 workers, queue 32, ≤24 pending, 3 s I/O, 35 s reply.
- **UI:** a "Driven by {agent}" badge on sessions an agent is driving.

**R10. File-viewer path rules (02-16)**

- **Request:** `{rootId, relPath}`. Roots are registered project and worktree roots only; never absolute paths. Do not copy Zeron's `AbsoluteRead::Outside`.
- **Grammar** (Z workspace_files.rs:29-44,150-196; MonoCode only checks length, NUL, escape and `.git`, M host/workspace.ts:28-46):
  - ≤4096 bytes, ≤256 components;
  - no empty, `.` or `..` components, no NUL or control characters;
  - `.git` blocked in any case.
- **Resolution:**
  - lstat each component and reject symlinks;
  - regular files only;
  - canonical path must start with the canonical root (Z :1512-1556; M host/workspace.ts:48-61).
- **Ignored files:** hidden in listings and refused on read unless the per-project toggle "Show ignored files on phone" is on.
  - This is leak hygiene (screenshots, shoulder-surfing), not authorization. Zeron says the same (Z ARCHITECTURE.md:84).
- **Caps:** 1 MiB text, 8 MiB preview, 500 entries per page, 6 s timeout (Z :29-44). Read-only in v1.

**R11. Repo-supplied commands (06-15, 01-7, 15-9)**

- Opening or cloning a repo never runs anything.
- A committed `pocket.json` offers **candidates** only. The desktop shows each command verbatim with an Import button.
  - Imported copies are device-private, in `desktop.json`. Change its write to 0600 (today default permissions, P store.rs:38-41).
  - Model: Z docs/reference/project-actions.md:5-9; Z crates/engine/src/project_actions.rs:1,507-517.
- Later edits to `pocket.json` never change an imported command. A changed candidate shows as "Updated in pocket.json" and needs a fresh import.
- Invalid JSON or more than 50 entries: ignore the whole file (Z project-actions.md:30).
- The phone may run imported actions and the device-private setup only. It can never import.
- **Copy lists** (`RepoConfig.copy`, `.worktreeinclude`):
  - relative, no `..`, inside the repo root;
  - regular files only via lstat, so symlinks are rejected;
  - applies to today's default `.env*` scan too (P forms.rs:284-291,436).

**R12. Push and lock screen (08-19, 14-9)**

- **Body:** fixed copy "Run finished" / "Waiting on your input" / "Run failed" (Z edge/src/push-notify.ts:59-60).
- **Title:** project name only; agent titles can contain prompt text.
- **Details:** opt-in "Show details on lock screen" adds the tool name and a command truncated to 80 chars.
- **Payload:** `agentId`, random `requestId`, `category`, and nothing else by default.
- **Lock-screen actions:** Allow and Deny both set `isAuthenticationRequired: true`, and Deny also sets `isDestructive` (https://docs.expo.dev/versions/latest/sdk/notifications/).
  - "Always allow" and "switch to auto mode" (P permission.go:18-27) are in-app only.

**R13. Phone at rest**

- **Keychain accessibility:** SecureStore `keychainAccessible: WHEN_UNLOCKED_THIS_DEVICE_ONLY` (https://docs.expo.dev/versions/latest/sdk/securestore/).
- **Face ID lock (optional):**
  - "Require Face ID" sets `requireAuthentication` on the token entry.
  - Changing biometrics invalidates the key, which forces a re-pair (same page).
- **Unpair:** calls `devices.revokeSelf`, then wipes both keys.

### F7. Which rules block or unblock existing ideas

| Idea | Rules | Effect |
|---|---|---|
| r01 F7 (Zeron: every device trusted) | R1, R6 | Do not copy. Scoped device tokens instead. |
| r02-1 relay, r02 risks (r02:372-374) | R2 `fp`, R4 | **Blocked** until R2 carries the Mac key; E2EE uses Noise, not TLS termination at the relay. |
| r02-16 phone file viewer | R10 | **Unblocked:** the `.env` question is answered (hidden by default; not a boundary). |
| r08-18 QR pairing | R2, R5 | **Changed:** the QR carries a one-time code, not the token. R5 is required first. |
| r08-19 lock-screen Allow/Deny | R12 | **Unblocked**, with auth on both actions and no escalation options. |
| r08-21 phone spawn ("needs security review") | R7, R11 | **Unblocked**, allowlisted: claude/codex, registered roots, no cmd/env/cwd. Needs a project registry in pocketd. |
| r08-22 hosted relay; r08 push-privacy risk (r08:615) | R4, R12 | Relay blocked without E2EE; push privacy settled as fixed copy by default. |
| r10-19 SSH remote host | — | Out of scope. MonoCode's SSH-channel pairing is its analogue of R2. |
| r11-1 agent CLI verbs | R8, R9 | **Unblocked** behind grants; `sessions.send` sanitized. |
| r11-2 per-terminal token plus scopes | R6, R9 | Adopted, plus the peer-PID ancestry check that makes it enforceable. |
| r11-3 `agent.start` / `terminal.spawn` over ws | R7, R6 | Phone: `agent.spawn` allowlist only. `terminal.spawn`: owner-only. |
| r14-8 relay + TLS + QR + per-device tokens | R1–R4 | **Split:** R1–R3 now (S/M); relay later (L). |
| r14-9 push | R12 | Payload and copy fixed. |
| r15-12 E2EE seed | R4 | **Replaced** by Noise with per-device keys. |
| r15-13 Serve TLS + bind | R3, R4 | Adopt as-is and do first. |
| r06-15 committed setup file | R11 | Only via import. |
| r01-7 project actions (phone can trigger) | R11, R7 | Phone runs imported actions only. |

## Ideas to clone into Pocket

| ID | Idea | User value | Evidence | Pocket mapping | Effort | Prerequisites |
|---|---|---|---|---|---|---|
| 18-1 | Bind to loopback plus Tailscale IPs, re-checked every 30 s. Loopback only without Tailscale. `--listen lan` needs TLS. | No exposure on café Wi-Fi | P serve.go:46,63-69; M docs/remote-access.md:57; r15-13 | `pocketd/cmd/pocketd/serve.go`; adapt | S | none |
| 18-2 | Handshake hardening: Host allowlist, no `InsecureSkipVerify`, 4 KiB pre-auth read limit, ≤16 pre-auth sockets, 3 failures/min, split error reasons, random requestIds | Closes CSWSH/rebinding; needed before pairing codes | P wsserver.go:53,57,122-127; WS accept.go:228-260; M host/server.ts:203-225; RN WebSocketModule.kt:104-127 | `pocketd/internal/wsserver`, `internal/broker`; adapt | S | none |
| 18-3 | Per-device tokens: 32 B, sha256-hashed `devices.json` (id, name, platform, scopes, createdAt, lastSeenAt, lastAddr); revoke closes sockets with 4401; `pocketd devices` CLI; legacy token migration | Revoke one lost phone; see what's connected | M host/store.ts:36,278-313; M host/cli.ts:79-89,185-215; P config.go:52-60 | new `pocketd/internal/devices`; `wsserver` hello; `packages/protocol`; port | M | 18-2 |
| 18-4 | QR pairing with a one-time 128-bit code (5 min TTL, single use, 5 failures lock), `pair` pre-auth message, phone scanner and confirm | Zero typing; the long-lived token never shown | Z auth-routes.ts:161-192; M remote_ssh.rs:417-420; P app.json:7; supersedes r08-18 | pocketd `pair` subcommand plus wsserver; `packages/app` new Pair screen (expo-camera); new | M | 18-2, 18-3 |
| 18-5 | Desktop Devices sheet: This Mac, Phones, Rename, inline Remove confirm, Pair phone QR sheet, legacy-token banner | Manage trust without a CLI | Z devices.rs; Z widgets.rs; M ConnectionsSettings.tsx:222-385 | `packages/desktop/crates/pocket` new sheet; new | M | 18-3, 18-4 |
| 18-6 | Desktop off the phone token: talk to pocketd over the ops socket (owner scope by peer check) and stop reading config.json | Removes the plaintext approve-grade secret from disk | P agents.rs:198-200,248; F2 | `desktop/crates/agents`, pocketd `internal/ops` agent namespace; adapt | M | 18-3 |
| 18-7 | Scope enforcement in ws dispatch (observe/drive/approve/spawn/files) plus observe-only devices | Watch or second-phone pairing without shell power | P wsserver.go:156-200 | `pocketd/internal/wsserver`; new | S | 18-3 |
| 18-8 | Agent grants on ops: peer-PID ancestry (`LOCAL_PEERPID`), per-terminal grant token in env only while granted, hook PID verification; no approve/pair/devices from PTY descendants | Ends self-approval and silent cross-session shell | SDK sys/un.h:89; M control.rs:15,196-266,464-486; P ops.go:95-158; P hook.go:14-37 | `pocketd/internal/ops`, `internal/daemon/plugin.go`; new (subsumes r11-2) | M | 18-6 |
| 18-9 | Prompt sanitation: strip C0/ESC/DEL, 64 KiB cap; agent-sourced prompts reject a leading `!` or `/`; "Runs in your shell" chip on the phone | No mode cycling via ESC[Z; agents can't open shell mode in siblings | P terminal.go:278-287; interactive-mode docs (Shift+Tab, `!`) | `pocketd/internal/terminal` or `daemon`; app composer; new | S | none |
| 18-10 | Phone `agent.spawn` allowlist: registered roots, claude/codex, prompt, default/plan mode; no cmd/env/cwd; auto mode behind a desktop setting | Start work from the phone safely | r08-21; P daemon.rs:99-101; P store.rs:13-28 | `packages/protocol`, pocketd wsserver plus project registry; new | M | 18-7; project registry in pocketd |
| 18-11 | File-path guard: Zeron grammar plus per-component lstat plus canonical root check; no absolute reads; ignored files hidden unless toggled; 1 MiB/8 MiB/500/6 s | Safe read-only viewer for r02-16 | Z workspace_files.rs:29-44,150-196,1512-1556; M workspace.ts:28-61 | new `pocketd/internal/files`; port | M | 18-7 |
| 18-12 | Repo-command import gating (`pocket.json` candidates, verbatim import, device-private copies, ≤50 entries, whole-file invalidation); copy lists must be regular files; `desktop.json` 0600 | Cloning a repo can't run or exfiltrate anything | Z project-actions.md:5-9,30; P forms.rs:284-291,436; P store.rs:38-41 | `desktop/crates/store`, `crates/pocket/forms.rs`; port | M | none (0600 and symlink check are S alone) |
| 18-13 | Lock-screen privacy: fixed copy, project-name title, details opt-in, Allow/Deny require auth, no escalation options on the lock screen | No command text on a locked phone | Z push-notify.ts:59-60,91-99; Expo NotificationAction docs; P permission.go:18-27 | `packages/app` notifications; pocketd push payload; adapt | S | r08-1..r08-3 |
| 18-14 | Phone at rest: `WHEN_UNLOCKED_THIS_DEVICE_ONLY`, optional Face ID lock, Unpair (`revokeSelf` plus wipe) | Lost or backed-up phone can't silently keep access | P ConnectScreen.tsx:7-8; Expo SecureStore docs; M host/server.ts:415-418 | `packages/app`; adapt | S | 18-3 |
| 18-15 | Tailscale Serve mode: `wss://<mac>.<tailnet>.ts.net`, loopback-only bind, MagicDNS host in the QR; identity headers recorded, not trusted | TLS and a stable hostname for free | https://tailscale.com/kb/1312/serve; r15-13 | pocketd `serve --tailscale-serve`; app URL rules; adapt | S | 18-1, 18-2 |
| 18-16 | Relay E2EE with Noise IK, per-device static keys pinned via the QR `fp` | Off-tailnet reach without trusting the relay | Z docs/PARITY.md:101; r02-1; r15-12 (rejected design) | new `pocketd/internal/relay` plus app crypto; new | L | 18-4, r02-1 |
| 18-17 | LAN mode with a pinned self-signed certificate | Works without Tailscale on home Wi-Fi | RN WebSocket.js:101 (headers only, no trust hook) | native module; new | L | 18-4 |
| 18-18 | Plain-language trust copy in onboarding and the Devices sheet: "A paired phone can run commands on this Mac as you." | Honest mental model | M SECURITY.md:7; M docs/remote-access.md:55 | desktop plus app copy; port | S | none |

## UI/UX spec to copy

**Desktop: Devices sheet**

This is new. Cmd-, is already taken by Project settings (P packages/desktop/crates/pocket/src/main.rs:1070), so open it from the palette ("Devices…") and from the phone chip in the top bar.

- **Page frame** (Z crates/ui/src/settings/widgets.rs):
  - max width 760, horizontal padding 40 (:308-309);
  - top padding = titlebar 38 + 16, bottom 48 (:312-324).
- **Header:**
  - "Devices" at 20/26 medium (:328-350).
  - Subtitle 13/17 muted, mt 4 (:354-363): "Phones that can reach this Mac. A paired phone can run commands here as you."
  - Right side: "Pair phone" button with a Plus icon (Filled; gap 6, radius 8, min height 32, px 10; :1514-1522).
- **Sections:**
  - "This Mac" and "Phones", renamed from Zeron's "This device" and "Other devices" (Z devices.rs:390-396).
  - Label 13/17 muted, inset 8; section gap 32 (widgets.rs:455-468).
  - Card: mt 24, radius 12, `wash(0.045)` (:502-522).
  - Zeron's own Devices page overrides these: label mt 28, card mt 8 (Z devices.rs:369-392).
- **This Mac row:**
  - Title: Mac name.
  - Meta: `mac-mini.tail1234.ts.net · port 4517 · Tailscale` or `Tailscale off · phones can't connect` (danger).
  - Button: "Copy address".
- **Phone row** (card_row: mx 16, py 12, min height 60, gap 16, hairline between rows; :527-540):
  - Title at 13/17 medium (:561-569). Zeron's device row has no leading icon; if one is wanted, `row_tile` is a 24×32 tile with a 16 px glyph (:544-557).
  - Meta line 12/16 muted, "·" at 0.3 opacity (:573-595).
  - Meta pattern: `{platform} · paired {Sep 30} · {Online | Last seen …} · {lastAddr}`. Use the success tint for Online.
  - Last-seen strings copy Zeron: "never seen", "just now" (<60 s), "Nm ago", "Nh ago", "Nd ago" (Z devices.rs:32-46).
  - Buttons: "Rename" (Quiet) and "Remove…" (Quiet, danger text).
  - "Rename device" dialog with Cancel and Rename (Z devices.rs:187).
- **Remove confirm:** an inline panel under the row, adapted from M ConnectionsSettings.tsx:331-368 (border-top, bg wash, px 16, py 16, 12 px).
  - "Remove {name}? It disconnects now and needs a new pairing to come back."
  - Buttons "Remove" (danger) and "Cancel".
  - MonoCode's three-way choice (:361-368) doesn't apply, because Pocket's desktop is the host.
- **Notices:**
  - Emerald, after removal: "{name} was removed. Your agents keep running."
  - Error strip on failure (Z widgets.rs:1562-1584; mt 16, px 16, py 12, radius 12, red border @0.2, bg @0.06, 12.5 px, DangerTriangle 16): "Could not remove {name}: {reason}".
- **Legacy row:**
  - "Shared token (legacy)", with meta "Used by phones paired before device tokens · Last seen …".
  - Button "Re-pair and remove": opens the Pair sheet, then revokes on success.
- **Empty state:** dashed border, px 20, py 32 (M ConnectionsSettings.tsx:385 pattern). "No phones yet. Pair one to follow agents and answer requests from anywhere on your tailnet."
- **Agent access section** (R9):
  - One row per terminal holding a grant: `{agent} in {worktree} · can drive other sessions` with a "Revoke" button.
  - The tab context menu toggles grants with "Let this agent drive other sessions".

**Desktop: Pair phone sheet**

- **Layout:** a 240 px QR on white with 16 px quiet zone. Below it, "Scan with your phone's camera." and a mono countdown "Expires in 4:59".
- **Host line:** "via mac-mini.tail1234.ts.net" (Serve) or "via 100.x.y.z:4517".
- **Manual fallback:** "Can't scan?" reveals the host and code with Copy buttons.
- **States:**
  - redeemed: "Paired {name}" (emerald), then close after 1.5 s (Zeron uses 1500 ms for its "Copied" feedback, Z devices.rs:153-165);
  - expired: "Code expired" plus "New code";
  - Tailscale off: error strip "Tailscale isn't running. Phones can't reach this Mac."

**Phone**

- **Pair screen** (replaces ConnectScreen):
  - Keep the existing styles: padding 24, heading 28/700, input padding 12, font 15, button paddingVertical 14 (P ConnectScreen.tsx).
  - Heading "Pair with your Mac". Body: "On your Mac, open Devices → Pair phone, then scan the code."
  - Primary action "Scan QR code". Secondary action "Enter code instead" shows host and code fields.
  - The Camera app opening `anywhere://pair?...` lands on the same confirm step.
- **Confirm card:** "Pair with {n}?" and `{h}`, then "Pair" / "Cancel".
  - Name field prefilled with the device name: "Name on your Mac".
- **Errors:**
  - "Code expired. Make a new one on your Mac."
  - "This code was already used."
  - "Can't reach {host}. Is Tailscale on?"
  - "Update Pocket on your Mac."
- **Revoked state:** full screen "This Mac removed this phone." with "Pair again". SecureStore is already wiped.
- **Phone settings, "This Mac" section:**
  - name, host, "Paired Sep 30";
  - toggles "Require Face ID" and "Show details on lock screen" (off by default);
  - destructive "Unpair this phone", with the alert "Unpair? You'll need your Mac to pair again."
- **Composer:** when the text starts with `!`, show a chip "Runs in your shell".
- **Lock screen:** title = project name. Body = fixed copy. Actions "Allow" and "Deny", both requiring auth.

## Open questions / risks

- **Reparenting escape:** PTY-ancestry scope (R9) is bypassed by double-fork or `launchctl`. Is a code-signature check on privileged ops (`LOCAL_PEERTOKEN` via Security.framework, needing cgo) worth it for pair and devices?
- **Project registry owner:** is it pocketd or the desktop? R7 and R10 need pocketd to know registered roots; today only `desktop.json` has them (P store.rs:17-28).
- **iOS Origin:** RN iOS WebSocket Origin behaviour was not verified; only Android's default origin was read. R5's Host-plus-same-origin rule should pass either way; test on a device.
- **Claude folder trust dialog:** does a phone-typed `\r` accept Claude's first-run trust dialog in a phone-spawned session? The presence foreground check (P presence.go:123-158) doesn't tell the dialog from the prompt. Test before shipping 18-10.
- **Codex shell escape:** R8's `!` guard is Claude-specific. Does Codex's TUI have an equivalent that R8 must also block for agent-sourced prompts? Not verified.
- **Default mode:** Serve mode vs direct bind. Serve needs tailnet HTTPS certs enabled (https://tailscale.com/kb/1312/serve) and hides client IPs, so `lastAddr` becomes 127.0.0.1.
- **Legacy migration:** grace length (7 days proposed). Breaking existing phones immediately is safer but harsher.
- **Desktop confirm on redemption:** does it add real protection? A same-UID attacker can forge the desktop's ops call too (F2), so the value is UX only.
- **Copy-list symlinks:** `std::fs::copy` behaviour when both source and destination are committed symlinks (P forms.rs:436) was not tested. R11 rejects non-regular files regardless.
- **Lock-screen action timing:** background actions may not get enough time to reconnect and resolve (r08:619). A small authenticated HTTP endpoint may be needed; it must carry the same R1/R5 rules.
- **Shared work tailnets:** Pocket can't enforce ACLs. Should `pocketd serve` warn when `tailscale status` shows other users' devices?
- **Code vs docs:** r08-18 (r08:489) put the token in the QR, and r15-12 (r15:358) used PBKDF2 on a random seed. This report overrides both. r11's "256-bit" grant token is about 244 bits in code (2 UUIDv4, M control.rs).

## Verification

Date: 2026-09-30. Claims checked: 40. Corrected: 11.

Confirmed against source: pocketd `:4517` all-interface bind, 24-byte token, 0700/0600 config, `InsecureSkipVerify`, 1 MiB read limit, shared "Rejected", open-request replay on hello, no per-message authz, `perm-N` IDs, 10 min timeout, `setMode auto` option, ops socket 0600 with unrestricted spawn/input/screen/close, `POCKETD_SOCK`/`POCKETD_PTY` injection, desktop reading config.json and using only view/seen/close, `clientId "pocket-app"`, SecureStore default options, `.env*` copy list with `std::fs::copy`, `desktop.json` default perms, `cmd-,` = Project settings, scheme `anywhere`; MonoCode tokens, hash store, 403/401 checks, revokeSelf, SSH options, askpass, endpoint rule, control-socket limits and env-only grants; Zeron Origin rejection, loopback IPC, TOFU room, `?token=`, `AbsoluteRead::Outside`, path grammar and caps, project-action import gating and 50-entry rule, 70 s online window, last-seen strings, widget metrics; RN Android default Origin; coder/websocket same-host Origin rule; `LOCAL_PEERPID`/`LOCAL_PEERTOKEN`; Claude `!` shell mode, Shift+Tab, read-only `cat`, curl/wget approval, trust verification; Expo SecureStore and NotificationAction options; Tailscale Serve certs and identity headers; default ACL wording.

- F1 hooks: the hook caller supplies the Claude ancestor's pid, not "its own" pid.
- F1 prompt path: Shift+Tab reaches `bypassPermissions`/`auto` only "when available".
- F2: "Many users allow-list `Bash(node:*)`" was unsupported; reworded and marked unverified.
- F3: MonoCode does not reject absolute paths; it accepts absolute inputs inside the root and rejects escapes.
- F3: lifecycle secret is M host/cli.ts:241, not :244.
- F4: 4409 supersede is at Z device-room.ts:163-170; citation widened to 150-170.
- F4: Zeron push body is fixed, but the title is the chat title (Z push-notify.ts:62-69), not fixed copy.
- F5: r08:615 raised tool/command in push as a question and proposed fixed copy by default; it did not propose including them.
- UI spec: Zeron device rows have no leading icon; "icon size 20" was unsupported. `row_tile` is 24×32 with a 16 px glyph.
- UI spec: Zeron's Devices page uses label mt 28 and card mt 8, overriding the generic section metrics; section names are Zeron's "This device"/"Other devices".
- R10: MonoCode's grammar lacks the `.`/`..`-component and component-count rules; citation qualified.
