# 20 — Fleet of hosts: watching and driving agents on more than one machine

Date: 2026-10-12. Worktree `lucky-koala` @ a2ee24f.

Question: how can Anywhere watch and drive agents on other Macs, Linux boxes, cloud VMs and dev containers, from both the desktop and the phone? Two goals: (1) a sound architecture, and (2) adding a host as easy as possible.

Citation legend:
- `P path:L` is this repo. Paths are relative to the repo root; `pd/` = `packages/pocketd/`, `dk/` = `packages/desktop/crates/`.
- `[label]` is an external source, listed in §9.
- `rNN:L` is an earlier research note in this folder.
- "Inference" is my reasoning, not something a source states.
- **UNVERIFIED** means I could not confirm it from a primary source.

## 1. TL;DR

Every host runs its own pocketd, and the desktop and the phone talk to each host directly, with no hub and no relay. The desktop reaches a host over SSH. SSH installs pocketd where nothing else manages it (Linux, bare hosts) and then forwards the host's `pocketd.sock` back to the desktop, so the desktop's two existing clients (ops JSON lines and the WS protocol) run unchanged over that forward. The remote pocketd sees the forward as a local unix peer outside any Terminal and treats it as owner (P pd/internal/peer/classify.go:23-40), so no owner token crosses the network. The phone reaches each host over the tailnet, as it does today: because the desktop is already owner on each host, it calls that host's `pair.begin` (P pd/internal/ops/ops.go:270) and shows a QR per host. The smallest slice (M0) is a second Mac that already runs Anywhere, reached over SSH, with agents and terminals only, and it needs **no pocketd change**: the desktop reads the remote `$SHELL` and `$HOME` over the same SSH session and builds spawn ops from them. Linux is the biggest cost, because pocketd does not build there today (`GOOS=linux go build ./internal/proc` fails with `undefined: Proc`; I ran it) and libghostty-vt is linked as a macOS xcframework (P pd/internal/vt/vt.go:5). Next is remote git and file reads (`git.v1`, `fs.v1`): they must run on the host and stream to the desktop, because git runs on the UI's machine today (P dk/git/src/git.rs:48).

Ranked:
1. **B: SSH bootstrap plus socket forward.** The desktop's transport and installer, M0–M1.
2. **A: direct pocketd per host over the tailnet.** The phone's transport from M2. The desktop uses it later for hosts without SSH.
3. **C: embedded tsnet.** Opt-in for hosts without tailscaled or a TUN (containers, Codespaces), M3.
4. **E: hub pocketd federating others.** Rejected: a single point of failure that holds every host's owner credential, and a double hop for terminal bytes.
5. **D: hosted relay.** Rejected until Noise E2EE exists (r18:71, r18:290, r18:413). It is also a PRD non-goal (P docs/orchestrators/product/00-prd.md:93).

This reopens settled decision D13 "macOS + iPhone only" (P docs/orchestrators/product/00-prd.md:403) for hosts. The decision is in Q1.

## 2. Current state: single-host assumptions

Runtime today:
- pocketd (Go) owns the PTYs and a libghostty-vt per Terminal.
- The desktop opens two channels on one unix socket:
  - ops JSON lines, which carry the raw PTY verbs `attach`, `input`, `resize`, `screen` and `close` (P pd/internal/ops/ops.go:280-297);
  - the phone WS protocol, tunnelled over the same socket (P dk/agents/src/agents.rs:616-619).
- The phone uses WS over Tailscale on port 4517.

The WS protocol has no terminal verbs: PTY bytes reach clients only through ops.

| # | Assumption | Where | What breaks with N hosts |
|---|---|---|---|
| S1 | One socket, derived once; the store's home is that socket's directory | P dk/pocket/src/main.rs:42-46 | Needs one socket per host. The store must not be keyed by the remote's directory. |
| S2 | `request()` opens a new unix connection per op on the global socket | P dk/daemon/src/daemon.rs:74, :94-107 | Every one-off op (status, devices, config-set) must take a host (P dk/pocket/src/settings/host.rs:153,180,199,232,271,288). |
| S3 | `Daemon` holds one writer and reconnects with backoff | P dk/daemon/src/daemon.rs:259-304 | Fine per host. Needs N instances whose events are merged. |
| S4 | Spawn ops carry the desktop's login shell, env and HOME | P dk/daemon/src/daemon.rs:116-149, :252-256 | A remote host gets `/opt/homebrew/bin/fish`, the desktop's HOME and local-only vars (`SSH_AUTH_SOCK`, `__CF_USER_TEXT_ENCODING`, :136). Build them from the remote's values instead (M0) or let pocketd resolve them (M1). |
| S5 | One `Agents` per app, holding one host, scopes, caps and version | P dk/agents/src/agents.rs:178-183, :225-242 | Needs one per host. Caps gating is already per connection (P dk/agents/src/agents.rs:302-366). |
| S6 | Git runs `Command::new("git")` locally; Explorer reads with `std::fs` | P dk/git/src/git.rs:48,188,207,212,692; P dk/pocket/src/explorer/preview.rs:62-63 | Remote worktrees have no local checkout. Needs host-side git and fs. |
| S7 | The store keys projects, repos, tracked worktrees and layouts by bare path | P dk/store/src/store.rs:318-337 | `/home/me/app` on two hosts collide. Needs a host dimension. |
| S8 | The registry is `desktop.json` beside the socket, and the desktop is its only writer | P pd/internal/registry/registry.go:1-2, :37 | A remote pocketd has no such file unless its own desktop writes one. Automations and the phone's project list read it. |
| S9 | `serverId` is the hostname | P pd/internal/proto/messages.go:148, :160 | Not unique ("MacBook-Pro"), and it changes on rename. Cannot key hosts. |
| S10 | `HostState` is only `{tailnet, keepingAwake, upgradeFailed}` | P pd/internal/proto/messages.go:316-321 | Has no id, os, arch or providers. |
| S11 | Owner means a unix peer not descended from a Terminal | P pd/internal/peer/classify.go:23-40; P pd/internal/ops/ops.go:161-171; socket is 0600 (P pd/internal/ops/ops.go:90) | Owner can't be reached over TCP. It can be reached over an SSH-forwarded socket (Inference: the peer is sshd's session process). |
| S12 | Peer PID lookup exists only on Darwin | P pd/internal/peer/peer_darwin.go:9 | On Linux it needs `SO_PEERCRED`. |
| S13 | Tailnet detection requires a `utun*` interface | P pd/internal/reach/reach.go:26-30 | Linux names the TUN `tailscale0` [ts-tun]. Userspace networking has no interface at all [ts-userspace]. |
| S14 | macOS-only code | IOKit awake (P pd/internal/awake/iokit.go:4); `proc_darwin.go`, `reap_darwin.go`; launchd label check (P pd/cmd/pocketd/serve.go:73); `scutil` name (:350-356); `internal/launchagent`; ghostty xcframework via xcodebuild (P scripts/build-ghostty.sh:4,7,28) | Linux port. |
| S15 | The phone holds one credential set | P packages/app/src/credentials.ts:7; "one host per phone" (P docs/designs/2026-09-30-reach-lockdown.md:309) | Needs credentials keyed by host. Pairing must add a host, not replace the current one. |
| S16 | Distribution and updates: pocketd ships inside the .app as an SMAppService agent; Sparkle updates the Mac | P docs/adr/0004-release-and-updates.md:3-5 | Linux and other Macs need their own install and update path. |
| S17 | Upgrade rule: a release keeps speaking the previous release's protocol | P docs/adr/0005-pocketd-upgrades-in-place.md:9; `MinVersion = MaxVersion = 3` (P pd/internal/proto/proto.go:11-12) | With a fleet, skew is per host and can span several releases. |

Already in place and reusable:
- The `host.v1` cap (P dk/agents/src/agents.rs:614) and `hello.ok.host` (P pd/internal/proto/messages.go:154).
- The build version in `hello.ok.version` (:155-156).
- Close codes 4401 and 4426 (P pd/internal/wsserver/wsserver.go:42-43).
- Device scopes and pairing (P docs/designs/2026-09-30-reach-lockdown.md §5.3).
- creack/pty, which already supports Linux (Inference from the library's scope).

Prior research:
- **02-13:** multi-host on the phone, with Zeron's presence model of 15s/45s and Dark after 5 min (r02:339).
- **A prior-art host pattern:**
  - loopback-only host
  - `ssh -L`
  - `pair --json` over SSH
  - per-device sha256 tokens
  - launchd and systemd units
  - an environmentId pin
- **18-16:** relay blocked until Noise E2EE (r18:71, r18:224).

## 3. Architecture options

| | A. Direct pocketd per host, tailnet WS | B. SSH bootstrap + socket forward (Zed-style) | C. Embedded tsnet in pocketd | D. Hosted relay / control plane | E. Hub pocketd federates others |
|---|---|---|---|---|---|
| Shape | Desktop and phone dial `host:4517` on each host | Desktop runs `ssh -N -o StreamLocalBindUnlink=yes -L local.sock:~/.coding-pocket/pocketd.sock host`; an argument with `/` is a unix socket [ssh-config]. Unlink lets a reconnect rebind a stale socket. Existing clients connect to `local.sock` | pocketd is its own tailnet node [ts-tsnet] | Hosts dial out to our service; clients dial the same service | One pocketd proxies the others; clients see one host |
| Setup steps | Install pocketd, join the tailnet, then pair desktop and phone per host | Enter `user@host`. The desktop detects os/arch, installs and starts pocketd. Done (§5.2) | Install pocketd and give it an auth key, or open the login URL it prints (`UserLogf`) [ts-tsnet] | Sign in on each host (Omnara: name, token, `curl \| sh`, 3 steps [omnara]) | Add each host to the hub with A or B, then pair clients to the hub once |
| NAT / firewall | Tailscale traverses NAT and falls back to DERP relays [ts-derp] | Whatever SSH reaches, including `ProxyJump` via `~/.ssh/config` (Zed and VS Code inherit it [zed-rd][vsc-ssh]). The phone can't use it | Same as A, with no TUN or root needed (userspace netstack [ts-tsnet]) | Outbound HTTPS only (as Claude Remote Control [claude-rc] and VS Code tunnels [vsc-tun]) | The hub must still reach every host by A or B |
| Trust | Per-host device tokens and scopes as today. Owner over TCP is new and must be minted only over an authenticated channel (§4.6). Tailnet grants can limit :4517 (r18:255-300) | SSH auth plus a 0600 socket give owner, with no new token type. Owner = whoever can SSH in as that user | Like A. `WhoIs` gives node and user per connection [ts-tsnet], but tagged nodes lose user identity [ts-tags]. On a Mac with Tailscale it adds a second device (Inference) | Blocked without Noise E2EE (r18:224). Needs accounts we don't have | The hub stores owner credentials for every host, so one compromise exposes all of them |
| Offline / reconnect | Per-host loop as `Daemon` does now (P dk/daemon/src/daemon.rs:259-304). Host shows Offline with last seen | ssh exits when the network drops, so re-run it. Zed sends heartbeats every 5s, reconnects after 5 misses and tries 3 times [zed-rc]. pocketd outlives the session (Zed's server only lasts 10 min idle [zed-srv]) | Like A. Ephemeral nodes disappear when idle [ts-authkeys] | Presence through the relay (Zeron 15s/45s, r02:339). Claude RC server mode gives up after about 10 min offline [claude-rc] | Hub offline means every host goes dark. A laptop hub sleeps |
| Upgrade / skew | Each host updates itself, or the desktop pushes. Skew is gated by caps per connection | Linux and bare hosts: the desktop installs the exact matching build per connect, as Zed requires [zed-rd] and VS Code enforces [vsc-ver]. Mac hosts keep their own Sparkle-updated bundle (ADR 0004), so skew there, as on the phone, rests on caps and ADR 0005's N-1 rule (§4.7) | Like A. tsnet is pinned inside pocketd, so tracking Tailscale security releases is on us | The relay protocol becomes a third versioned surface | The hub translates for every host version |
| Linux cost | Full port, plus fixing the `utun` check (S13) | Full port. Reach changes are needed only for the phone | Full port. No TUN needed. Binary growth UNVERIFIED (tailscaled + CLI is ~23 MiB [ts-size]) | Full port | Full port |
| Effort | L: `term.v1` over WS, `git.v1`, `fs.v1`, registry on host, owner tokens | M for M0 (desktop multi-host plus ssh supervisor, no pocketd change). L with installer and Linux | M on top of A | XL plus running a service | L–XL |
| Verdict | Phone transport from M2; desktop transport for no-SSH hosts later | **Desktop first** | Opt-in M3 | No | No |

Notes:
- **SSH needs Remote Login on Macs, which is off by default.** UNVERIFIED: the Apple support page was not fetched. Tailscale SSH serves macOS only with the open-source tailscaled [ts-ssh].
  - For "my other Mac", the alternative is installing Anywhere there and pairing by code (§5.3).
- **A Mac host runs pocketd as a LaunchAgent, which runs "only while that user is logged in"** [apple-launchd]. A headless Mac mini needs auto-login.
- **Linux hosts need `loginctl enable-linger` for a systemd user unit to run without a session.** Without linger, the user manager ends with the last session [sd-linger][sd-pam].
  - VS Code prints this tip after `code tunnel service install` over SSH [vsc-svc].
- **Containers:** Zed reaches dev containers via `docker exec` and `docker cp` [zed-docker]. That is another B-style transport for local containers that needs no tailnet. Codespaces idle out after 30 min [gh-cs].
- **Other tools take one of three shapes:**
  - SSH-launched server: Zed, VS Code Remote-SSH, Codex Remote SSH [codex-remote].
  - Outbound relay: Claude RC, VS Code tunnels, Superset [superset], Omnara, Happy [happy].
  - Tailnet: Coder uses WireGuard with DERP [coder-net]; Screens lists Tailscale devices [screens].
  - Nobody in this set federates through a hub.

## 4. Protocol and model changes

### 4.1 Host identity
- **`hostId`:** pocketd generates 128 random bits on first run and persists them in its home.
  - Unlike `serverId` (the hostname, S9), it survives renames and never collides.
  - Later it becomes the fingerprint of the Noise static key that R2 puts in the QR `fp` (r18:290).
- **Pinning:** the client pins `hostId` per Host. A mismatch is "Host identity changed", the same pattern as an environmentId pin.
- **Name:** `computerName` on a Mac (P pd/cmd/pocketd/serve.go:350); `os.Hostname()` on Linux (:136). The user can rename it; the alias is stored client-side.

### 4.2 `hello.ok.host`
Extend `HostState` (P pd/internal/proto/messages.go:316-321) under `host.v2`. Old clients ignore the new fields.

```json
"host": {"id":"h_7Q…","name":"devbox","os":"linux","arch":"arm64","version":"1.4.2",
         "tailnet":true,"keepingAwake":false,"upgradeFailed":"",
         "providers":{"claude":{"found":true,"signedIn":false},"codex":{"found":false}}}
```

- **Version:** the build version already exists at the top level (`hello.ok.version`, :155-156). Keep it there. `host.version` only duplicates it for the ops `status` reply.
- **Providers:** signed-in detection per provider, UNVERIFIED. Claude needs `USER` for its keychain lookup on macOS (P docs/spike-remote-sessions.md:102). On Linux the credential location is UNVERIFIED.

### 4.3 Ids
- **Ids stay host-local on the wire:** Terminal, agent, Local and automation ids do not change.
- **Clients key everything by `(hostId, id)`:**
  - In the store, `projects: Vec<String>` becomes `Vec<{host, path}>`, with a serde default of `host = "local"` so existing files load.
  - `repos`, `tracked` and `layouts` follow (P dk/store/src/store.rs:318-337).
- **No id rewriting** happens anywhere, so no hub is needed (rejects E).

### 4.4 What pocketd must newly serve

| Cap | Verbs (sketch) | Why | Milestone |
|---|---|---|---|
| (none) | `spawn` with `shell:"login"` and no env: pocketd resolves the login shell, env and `~` itself. M0 avoids this by reading them over SSH | S4 | M1 |
| `host.v2` | `hello.ok.host` as in §4.2; `host.changed` carries it | S9, S10 | M1 (M0 keys hosts by SSH destination) |
| `registry.v2` | `project.add`, `project.remove`, `project.list`. pocketd owns the registry; the desktop caches it per host | S8. A remote registry can't be a file the remote desktop alone writes | M1 |
| `git.v1` | `git.status` (count, then pages of N), `git.diff {path}` (streamed, size-capped), `git.log {page}`, `git.watch` → `git.changed {worktree, summary}` | S6. Status for 1000+ files must stream, not arrive as one blob | M1 |
| `fs.v1` | `fs.list {dir, page}`, `fs.read {path, max}` | Explorer and preview | M1 |
| `term.v1` | Over WS: `term.attach` → VT snapshot then binary frames; `term.input`, `term.resize`, `term.close` | Needed only once the desktop or phone reaches a host without SSH (A) | M3 |
| `pair.v2` | Owner-scoped device tokens, minted only via ops (§4.6) | Desktop over tailnet without SSH | M3 |

### 4.5 Aggregation across hosts
- **One `Agents` per host.** Caps, scopes and version stay per connection, as today (P dk/agents/src/agents.rs:302-366).
- **Up next:** merge across hosts by when each session entered Needs you.
  - Order by the time the client received the change, not the host's clock, so clock skew can't reorder the list.
  - Seen stays per agent on its own host, so 02-14's HLC is not needed (r02:340).
- **Badges and counts:** the notch, the sidebar and the git rail count sum over online hosts. An offline host's sessions show their last known status, dimmed, and never count as Needs you (Inference: a stale "Needs you" would pull the user to a dead host).
- **Notifications:** carry the host name when there are two or more hosts.

### 4.6 Trust
- **Owner stays local to the host.**
  - Over SSH the forwarded socket is a local peer, so classification is unchanged (S11).
  - Owner-scoped device tokens (`pair.v2`) are minted only through `pocketd pair --owner --json` run over SSH, or through the one-time join code that the user typed into a shell on the host (§5.4). Never through the phone pairing QR.
- **Tailnet scoping:** a tailnet grant can limit `tcp:4517` to the user's own devices (snippet at r18:255-300; grants [ts-grants]).
  - Optionally, bind a device token to the Tailscale node that first redeemed it by checking `WhoIs` on each connection [ts-tsnet]. This works in tsnet mode; with system tailscaled it needs the LocalAPI (UNVERIFIED for non-tsnet).
- **Join codes:** one-time, 10-minute TTL, at least 128 bits when embedded in a command.
  - For comparison: GitHub runner registration tokens expire after 1 h [gh-runner]; Tailscale one-off keys are spent once used [ts-authkeys].
  - Netdata's claim token lives until regenerated, which is the pattern to avoid [netdata].
- **Revoking:** follows reach-lockdown: 4401 and `devices.revoke` per host. Removing a host on the desktop also revokes that desktop's token there when online.

### 4.7 Upgrades and version skew
- **Mac hosts running Anywhere:** pocketd lives inside the signed, notarized bundle, and Sparkle updates it (P docs/adr/0004-release-and-updates.md:3-5). Pocketd re-execs whenever that binary changes (ADR 0005:3). The desktop must never push a loose binary there: it would break signing and be overwritten at the next Sparkle update. Skew is handled by caps and the N-1 rule, and the host header shows "Needs update". The user updates that Mac, or a later "Update now" asks its Sparkle to install (UNVERIFIED that Sparkle can be triggered remotely).
- **Linux and bare hosts over SSH:**
  - On connect the desktop runs `<pocketd> version` on the host.
  - If the version differs, it installs the desktop's own build, the way Zed does [zed-ssh]: download on the host from the GitHub release for that tag with curl, falling back to wget; if that fails, upload over SSH.
  - It then calls the ops `upgrade` verb, so the ADR 0005 handoff keeps terminals alive (P docs/adr/0005-pocketd-upgrades-in-place.md:3).
  - `syscall.Exec` keeps the pid, so systemd keeps tracking the unit (Inference, same reasoning as launchd in ADR 0005).
- **Host newer than the desktop:** the desktop shows "Update this Mac". It never downgrades a host, because a downgrade breaks the handoff-file rule (ADR 0005:10).
- **Phone:** it connects to hosts at different versions. ADR 0005's N-1 rule holds per host, and a host older than the phone's minimum closes with 4426 `server_too_old`.
- **Hosts without SSH (install command):** they self-update from signed GitHub releases (02-12). The policy is open (Q5).

### 4.8 Latency budgets (CLAUDE.md perf rules)

| Path | Budget | How |
|---|---|---|
| Event fan-in | One `cx.notify()` per frame, whatever N is | Merge all hosts' channels into one queue and drain everything pending before notifying. The 1 s daemon poll per host must not mean N notifies per second |
| Changes list, 1000+ files on a remote worktree | Frame cost unchanged (3 ms baseline) | `git.status` pages land off the UI thread in the same model the local path builds; `uniform_list` as today |
| Git refresh | No polling of remote worktrees | Host-side `git.watch` pushes a summary; the desktop fetches only the visible page. Never the 2 s loop × ~130 ms × worktrees × hosts |
| Click file → diff | Local ~100 ms; remote ≈ RTT + host git time | Skeleton at once. Apply only if the selection still holds (`DiffState::apply`). Cap and stream large diffs |
| Terminal echo | RTT | No local echo prediction. That is mosh's job [mosh] and out of scope |
| Host offline detection | ≤ 15 s | Ping every 5 s and call the host Offline after 3 misses (Zed 5 s [zed-rc]; Zeron 15 s/45 s r02:339) |

## 5. Setup UX

New CONTEXT.md terms, in its style:

> **Host**:
> A machine running pocketd that Anywhere watches and drives: this Mac, another Mac, a Linux box, a cloud VM or a dev container the user added. Each has its own terminals, projects, automations and paired devices; nothing moves between hosts. A host is online while Anywhere can reach its pocketd, and keeps its place on the sidebar while offline.
> _Avoid_: machine, server, remote, box, node
>
> **This Mac**:
> The host the desktop app runs on. It is always first and can't be removed.
> _Avoid_: local host, localhost

CONTEXT.md opens with "running on a Mac" (P CONTEXT.md:3). **Login shell** says "macOS account" (:23-25). Both generalize to "a host".

### 5.1 Entry points
- ⌘K → **Add host…**
- A sidebar footer "+" (shown only once a host exists, so it doesn't clutter the single-Mac case).
- Settings → Hosts.

### 5.2 Flows and step counts

User actions are counted. Waits are not.

| Flow | Who it's for | Steps | Steps in peers |
|---|---|---|---|
| **SSH** | A box you already `ssh` into | 3: ⌘K Add host → pick `devbox` from `~/.ssh/config` → Connect. Plus 1 first time: trust the host key | Zed 4 (open Remote Projects, Connect New Server, type ssh, pick path) [zed-rd]; Codex Remote SSH 4 [codex-remote]; VS Code "Connect to Host…" [vsc-ssh] |
| **On my tailnet** (discovery) | Tailnet peers | 2: ⌘K Add host → click `devbox · Linux`. That pre-fills SSH, or a code if Anywhere is already running there | Screens lists Tailscale devices [screens]; Syncthing "nearby devices" [syncthing-gui] |
| **Pairing code** | Another Mac with Anywhere installed, no SSH | 4 over two machines. There: Settings → Hosts → **Let another Mac drive this one** (shows a code). Here: ⌘K Add host → pick the host from the tailnet list → type the code. The code alone doesn't say which IP to dial; the tailnet list supplies it, or a QR carrying `h=` like the phone's | Syncthing 3 per side, both must add [syncthing]; RFC 8628 code shape [rfc8628] |
| **Install command** | A cloud VM or Linux box with no SSH from this Mac (web console, Codespace terminal) | 4: ⌘K Add host → Copy command → paste and run on the host → Confirm "devbox wants to join" | Omnara 3 [omnara]; Coder `curl … \| CODER_AGENT_TOKEN= sh` [coder-ext]; Tailscale `--auth-key` [ts-authkeys] |
| **Dev container** (M3) | Projects with `.devcontainer` | 3: add the feature to `devcontainer.json` → add the auth key secret → rebuild | Dev container features run `install.sh` as root at build [devc-feat]; `postStartCommand` on each start [devc] |
| **Phone, per host** | Phone already paired to This Mac | 3: host menu → Pair phone → scan → Add | Today's flow (P docs/designs/2026-09-30-reach-lockdown.md:54-72) |
| **Phone, all hosts** (proposal) | A new phone | 2: Settings → Pair phone → scan; the phone lists the hosts, user taps Add all | WhatsApp, Signal and 1Password link via one QR [1pw] |

The SSH flow runs this checklist on its own, each row with a spinner and then a tick (Zed detects with `uname -sm` and installs only if `<bin> version` fails [zed-ssh]):

1. Connect: `ssh -o ControlMaster=yes`. Password prompts go through `SSH_ASKPASS` into our dialog, as Zed does [zed-ssh].
2. Detect: `uname -sm`; `/etc/os-release` or `sw_vers`.
3. Install Anywhere host 1.4.2:
   - Skip if `~/.anywhere/bin/pocketd-1.4.2 version` already works.
   - Else download on the host from the GitHub release.
   - Else upload over SSH.
4. Start service:
   - Linux: write a systemd user unit (`Restart=always`, `WantedBy=default.target`, the VS Code pattern [vsc-svc]) and check linger.
   - Mac: needs the app installed (Q2).
5. Connect to Anywhere: forward the socket; `hello.ok`.
6. Check agents: look for `claude` and `codex` and whether they are signed in.

Codes: the install command embeds a 128-bit code in base32. A typed code follows RFC 8628 §6.1:
- 8 characters from "BCDFGHJKLMNPQRSTVWXZ", shown as `BCDF-GHJK`;
- case-insensitive, punctuation ignored;
- about 34.5 bits, so §5.1 says to rate-limit to about 5 tries [rfc8628]. Reach-lockdown's `pair_locked` already exists (P docs/designs/2026-09-30-reach-lockdown.md:72).

### 5.3 How the install command finds the desktop

The host dials the desktop rather than the desktop searching the tailnet, which avoids discovery entirely:

```
curl -fsSL https://<domain>/install.sh | sh -s -- --join 100.101.7.4:4517/7QFM2XPKD4B6WN3R…
```

1. The script installs pocketd for `uname -sm` and verifies its signature (signing scheme is Q4). It installs the systemd user unit and offers `sudo loginctl enable-linger $USER`.
2. pocketd generates its `hostId` and an owner token for the desktop.
3. pocketd dials the desktop's own pocketd on the tailnet, which already listens on 4517 for phones, and presents the code.
4. The desktop shows Confirm with the host's name, OS and tailnet IP, the way Syncthing asks "wants to connect. Add new device?" [syncthing-gui].
5. On Add, the desktop stores the token and dials back.

Requirements and fallback:
- Needs Tailscale on the host. When it's missing, the script says so and links the one-liner `curl -fsSL https://tailscale.com/install.sh | sh` then `sudo tailscale up` [ts-linux].
- The tsnet fallback (M3) takes an auth key instead [ts-authkeys].

### 5.4 Mockups

Add Host dialog:
```
┌ Add host ─────────────────────────────────────────────┐
│  ○ SSH            devbox▾          (from ~/.ssh/config)│
│  ○ On my tailnet                                       │
│      devbox        Linux · Anywhere not installed      │
│      studio        macOS · Anywhere 1.4.2 running      │
│      pi            Linux · offline 3 d                 │
│  ○ Install command                                     │
│      curl -fsSL https://…/install.sh | sh -s -- --j…  ⧉│
│      Expires in 9:41 · works once                      │
│  ○ Pairing code   [ ____-____ ]                        │
│                                         Cancel  Connect│
└────────────────────────────────────────────────────────┘
```

SSH progress:
```
┌ Adding devbox ────────────────────────────────┐
│  ✓ Connected as mingo@devbox                  │
│  ✓ Linux arm64 (Ubuntu 24.04)                 │
│  ◌ Installing Anywhere host 1.4.2…            │
│  · Start service                              │
│  · Connect                                    │
│  · Check agents                               │
│                                       Cancel  │
└───────────────────────────────────────────────┘
```

Sidebar. Host headers appear only with two or more hosts, and This Mac is first:
```
THIS MAC
  coding-pocket        ●2
    lucky-koala        ● Needs you
  notes
DEVBOX            Linux ●
  api                  ●1
    main               ◐ Working
STUDIO            offline · 14:02
  render               (2 sessions, last known)
PI                Needs update  [Update]
```

Host header states:

| State | Header text | Click |
|---|---|---|
| Online | name + green dot | Collapse |
| Connecting | "Connecting…" | — |
| Offline | "offline · {last seen}" | Retry, Details |
| Updating | "Updating to 1.4.2…" | — |
| Needs update | "Needs update" | Update |
| Newer than this Mac | "Update this Mac" | Check for Updates |
| Revoked | "Access removed" | Pair again, Remove |
| Tailscale off | "Tailscale off" | Open Tailscale |

Phone host picker, and Up next with host subtitles:
```
┌ Hosts ───────────────┐   ┌ Up next ─────────────────┐
│ ● This Mac (MacBook) │   │ ● Needs you              │
│ ● devbox    Linux    │   │   lucky-koala · MacBook  │
│ ○ studio    14:02    │   │ ● Needs you              │
│ + Add host           │   │   api/main · devbox      │
└──────────────────────┘   └──────────────────────────┘
```

### 5.5 Error states and copy

The copy follows the existing reach-lockdown strings (P docs/designs/2026-09-30-reach-lockdown.md:72, :86, :171).

| State | Trigger | Copy | Actions |
|---|---|---|---|
| Unreachable (first add) | SSH or TCP timeout | "Can't reach devbox. Check it's on and on your tailnet." | Retry · Details |
| Offline | Was online, 3 missed pings | "devbox is offline. Last seen 14:02." | Retry |
| Tailscale off here | `reach` reports no tailnet | "Tailscale is off on this Mac. Hosts on your tailnet can't be reached." | Open Tailscale |
| Tailscale off there | Host's `hello.ok.host.tailnet` is false | "Phones can't reach devbox: Tailscale isn't running there." | — |
| Host older | Version below this Mac's, or 4426 `server_too_old` | "devbox runs Anywhere 1.3.0. This Mac runs 1.4.2." | Update devbox |
| Host newer | Version above this Mac's | "devbox runs a newer Anywhere (1.5.0). Update this Mac to manage it." | Check for Updates |
| Updating | Push and `upgrade` in flight | "Updating devbox to 1.4.2… Terminals keep running." | — |
| Update failed | `host.upgradeFailed` | "devbox couldn't update to 1.4.2 and is still on 1.3.0." | Retry · Show log |
| Revoked | 4401 | "This Mac's access to devbox was removed." | Pair again · Remove host |
| Identity changed | `hostId` mismatch | "This isn't the devbox you added: its host ID changed." | Trust new host · Remove |
| SSH key changed | ssh host-key failure | "devbox's SSH key changed. Check with whoever runs it before continuing." | Details |
| No linger | Linux, linger off | "Anywhere on devbox stops when you log out." | Keep running (asks for sudo) |
| Mac logged out | Mac host, no GUI session | "Nobody is logged in on studio, and Anywhere runs only while someone is." | — |
| Provider not signed in | `providers.claude.signedIn` false | "Claude isn't signed in on devbox." | Open terminal on devbox |
| Install failed | Neither curl nor wget, and upload failed | "Couldn't install on devbox: it has neither curl nor wget, and uploading failed." (Zed's case [zed-ssh]) | Show log |
| Join code expired / used | Code TTL passed or code reused | "This command expired. Copy a new one." / "This command was already used." | Copy new |

## 6. Phased plan

| Milestone | Scope | pocketd changes | Size |
|---|---|---|---|
| **M0 Second Mac over SSH** | Store Hosts with `{id, name, ssh}`. Per host: an ssh supervisor (ControlMaster, `-L` socket forward, reconnect), a `Daemon` and an `Agents`. Sidebar host groups and header states. Up next and badges merged. Requires Anywhere installed on the remote Mac and Remote Login on. Hosts keyed by SSH destination until `hostId` lands. Spawn ops built from the remote `$SHELL` and `$HOME` read over SSH, without local-only vars. Projects for that host kept in this desktop's store. Changes and Explorer on remote worktrees show "Not available on remote hosts yet". Skew: caps + N-1; the header says "Needs update". Desktop only | None. Works against the pocketd already shipped. `hostId` and `host.v2` follow right after | M |
| **M1 Linux host + remote git** | Linux pocketd: `/proc`, `SO_PEERCRED`, `tailscale0`, hostname, systemd user unit and linger, awake no-op, libghostty-vt Linux build. Release tarballs per os/arch; desktop push-install over SSH (not to Mac hosts, §4.7); `spawn` resolves shell and env on the host; `hostId`, `host.v2`. Owner-run `registry.v2`, `git.v1`, `fs.v1` | Yes | L |
| **M2 Phone fleet** | Phone credentials keyed by `hostId`; Pair adds a host instead of replacing. Desktop "Pair phone with {host}" via that host's `pair.begin`. Host picker and merged Up next on the phone | Fix `reach` for Linux | M |
| **M3 Hosts without SSH** | Install command with join code; tailnet discovery list; `term.v1` and owner `pair.v2` over WS; tsnet mode with auth key for containers and Codespaces; dev container feature | Yes | L |
| Later | Self-update for hosts without SSH (02-12); Noise E2EE relay (18-16); LAN mDNS | — | — |

Linux port checklist (M1):

| Area | Today | Linux |
|---|---|---|
| Process tree | P pd/internal/proc/proc_darwin.go; build fails `internal/proc/reap.go:14:48: undefined: Proc` (I ran it) | `/proc/<pid>/stat` |
| Peer pid | P pd/internal/peer/peer_darwin.go:9 | `SO_PEERCRED` |
| VT | P pd/internal/vt/vt.go:5 (xcframework); P scripts/build-ghostty.sh:28 (xcodebuild) | Upstream CMake has a `linux-amd64` target [ghostty-cmake]. Support at the pinned `4ae9f1a` is UNVERIFIED. The C API is unstable, so pin it (P docs/spike-remote-sessions.md:111) |
| Keep awake | P pd/internal/awake/iokit.go:4 | No-op on servers. `systemd-inhibit` for laptops is UNVERIFIED (man page fetch failed) |
| Tailnet check | P pd/internal/reach/reach.go:29 (`utun`) | Also `tailscale0` [ts-tun]. Userspace mode has no interface [ts-userspace], so use tsnet (M3) |
| Name | `scutil` (P pd/cmd/pocketd/serve.go:350-356) | `os.Hostname()` |
| Service | `internal/launchagent`; launchd label check (serve.go:73) | systemd user unit plus linger [sd-linger][vsc-svc] |
| Distribution | Inside the .app, via Sparkle (ADR 0004) | `install.sh` plus signed tarballs (Q4) |

### Open questions
1. **D13:** do we reopen "macOS + iPhone only" for Linux hosts only, with no Linux desktop app? M1 depends on it.
2. **SSH for M0:** is SSH an acceptable dependency? It means turning on Remote Login on the other Mac. The alternative is the pairing-code flow, which needs `term.v1` and owner tokens over the network (M3 work moved into M0).
3. **Owner over the network:** do we allow owner-scoped device tokens over tailnet WS at all, or require SSH for owner and give phones only device scopes?
4. **Linux distribution:** which domain serves `install.sh`, which signing scheme (minisign or cosign), and which arches (amd64 and arm64 like Zed [zed-rd]; glibc floor? VS Code requires ≥ 2.28 [vsc-linux])?
5. **Updates:** for SSH hosts the desktop pushes; for hosts without SSH, do they auto-update or wait for the desktop's prompt?
6. **tsnet:** is a second tailnet device per host acceptable? What is the binary growth (UNVERIFIED)? Do we use user-owned nodes or tagged ones, given that tags drop user identity [ts-tags] and the free plan allows 50 tagged resources [ts-pricing]?
7. **Phone pairing:** one QR for all hosts, or one per host?
8. **Provider accounts:** each host signs in to Claude and Codex itself. Do we only detect that and point the user to it, or help with the login?
9. **Remote Mac running its own Anywhere desktop:** two desktops then drive one pocketd. Whose project list wins once `registry.v2` makes pocketd the owner?

## 7. Ideas

| ID | What | Why | Source | Pocket mapping | Size | Depends |
|---|---|---|---|---|---|---|
| 20-1 | Per-host `Daemon` + `Agents`, event fan-in, one notify per frame | N hosts without N× redraws | §4.5, §4.8 | `dk/daemon`, `dk/agents`, `pocket` new `hosts` module | M | — |
| 20-2 | ssh supervisor: ControlMaster, `-L` unix forward, askpass, reconnect | Desktop transport | [zed-ssh] | new `remote` model crate | M | 20-1 |
| 20-3 | `hostId` and `host.v2` | Stable keys, os/arch/providers | §4.1 | `pd/internal/proto` | S | — |
| 20-4 | Remote login shell and env for `spawn`: read over SSH in M0, resolved by pocketd in M1 | Remote terminals | S4 | `dk/daemon`, then `pd/internal/ops` | S | — |
| 20-5 | Version-matched push install over SSH (Linux and bare hosts) | No desktop skew | [zed-ssh][vsc-ver] | `remote` crate + release assets | M | 20-2 |
| 20-6 | Linux pocketd | Linux hosts | §6 | `pd/internal/*`, ghostty build | L | Q1 |
| 20-7 | `git.v1` / `fs.v1`, streamed and paged | Remote Changes and Explorer | §4.4 | `pd/internal/git` (new), `dk/git` client | L | 20-1 |
| 20-8 | `registry.v2` owned by pocketd | Remote projects and automations | S8 | `pd/internal/registry` | M | Q9 |
| 20-9 | Phone multi-host credentials + per-host pair from desktop | Phone fleet | r02:339 | `packages/app` | M | 20-3 |
| 20-10 | Install command with join code, host dials desktop | Hosts without SSH | [omnara][coder-ext] | `install.sh`, `pd` join verb | M | 20-6, Q3 |
| 20-11 | Tailnet discovery list | 2-step add | [screens] | `remote` crate | S | 20-2 |
| 20-12 | tsnet mode | Containers, Codespaces | [ts-tsnet] | `pd/internal/reach` | M | Q6 |
| 20-13 | Dev container feature | Containers | [devc-feat] | new repo folder | S | 20-12 |
| 20-14 | `term.v1` over WS | Desktop or phone terminals without SSH | §4.4 | `pd/internal/wsserver` | M | — |

## 8. What I verified

- **Ran it:**
  - `GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go build ./internal/peer/ ./internal/proc/ ./internal/reach/ ./internal/ops/` → `internal/proc/reap.go:14:48: undefined: Proc`.
  - No non-Darwin `peer.PID` exists (`grep func PID` finds only `peer_darwin.go:9`).
- **Read in code:** every `P` citation above, at a2ee24f.
- **Fetched (primary):**
  - Tailscale docs: tsnet, auth keys, Linux install, tags, key expiry, Machines-page filters, QR setup, pricing, `tailscaled.go`.
  - Zed docs and source at `93d7456`; VS Code docs and source at `c480e9e`.
  - systemd man sources at `c7a8e87`.
  - containers.dev; GitHub Codespaces and runner docs.
  - Claude Remote Control docs; OpenAI Codex remote docs; Omnara, Superset, Happy, Syncthing, Coder, Screens and 1Password docs.
  - RFC 8628; Apple launchd archive; ghostty CMakeLists.
- **UNVERIFIED:**
  - macOS Remote Login is off by default, and its Settings path.
  - Binary growth from tsnet.
  - Whether ghostty `4ae9f1a` builds lib-vt for Linux.
  - `systemd-inhibit` as Linux keep-awake.
  - Where Claude keeps credentials on Linux.
  - WhoIs via LocalAPI with system tailscaled.
  - Whether `enable-linger` for yourself needs sudo on a given distro.
  - Signal and WhatsApp link limits (search snippets only; not used above).
- **Inference, not sourced:**
  - An SSH-forwarded socket classifies as owner.
  - tsnet adds a second device on a Mac running Tailscale.
  - systemd keeps tracking the pid across `exec`.
  - Offline hosts shouldn't count as Needs you.
  - `$SHELL` in an SSH session is the account's login shell (sshd sets it from the passwd entry; man page not fetched).
  - Sparkle on a remote Mac can't be triggered from another Mac today.

## 9. Sources

Tailscale
- [ts-tsnet] https://tailscale.com/kb/1244/tsnet; https://pkg.go.dev/tailscale.com/tsnet
- [ts-authkeys] https://tailscale.com/kb/1085/auth-keys
- [ts-linux] https://tailscale.com/kb/1031/install-linux
- [ts-tags] https://tailscale.com/kb/1068/tags
- [ts-grants] https://tailscale.com/kb/1324/grants
- [ts-ssh] https://tailscale.com/kb/1193/tailscale-ssh
- [ts-userspace] https://tailscale.com/kb/1112/userspace-networking
- [ts-size] https://tailscale.com/kb/1207/small-tailscale
- [ts-tun] https://github.com/tailscale/tailscale/blob/main/cmd/tailscaled/tailscaled.go (`defaultTunName` → `tailscale0`)
- [ts-pricing] https://tailscale.com/pricing
- [ts-derp] https://tailscale.com/kb/1232/derp-servers

OpenSSH
- [ssh-config] https://man.openbsd.org/ssh_config (LocalForward, StreamLocalBindUnlink, ControlMaster)

Zed (commit 93d745668805029476e6d7fbb19652116fed51de)
- [zed-rd] https://zed.dev/docs/remote-development
- [zed-ssh] https://github.com/zed-industries/zed/blob/93d745668805029476e6d7fbb19652116fed51de/crates/remote/src/transport/ssh.rs#L185-L334 and #L1014-L1237
- [zed-rc] https://github.com/zed-industries/zed/blob/93d745668805029476e6d7fbb19652116fed51de/crates/remote/src/remote_client.rs#L161-L167
- [zed-srv] https://github.com/zed-industries/zed/blob/93d745668805029476e6d7fbb19652116fed51de/crates/remote_server/src/server.rs#L520-L555
- [zed-docker] https://github.com/zed-industries/zed/blob/93d745668805029476e6d7fbb19652116fed51de/crates/remote/src/transport/docker.rs#L206-L351

VS Code (commit c480e9e49e2a116c458518ecbdec56d50e4af3d2)
- [vsc-ssh] https://code.visualstudio.com/docs/remote/ssh
- [vsc-linux] https://code.visualstudio.com/docs/remote/linux
- [vsc-tun] https://code.visualstudio.com/docs/remote/tunnels
- [vsc-ver] https://github.com/microsoft/vscode/blob/c480e9e49e2a116c458518ecbdec56d50e4af3d2/src/vs/server/node/remoteExtensionHostAgentServer.ts#L351-L357
- [vsc-svc] https://github.com/microsoft/vscode/blob/c480e9e49e2a116c458518ecbdec56d50e4af3d2/cli/src/tunnels/service_linux.rs#L29-L215

systemd, Apple, containers
- [sd-linger] https://github.com/systemd/systemd/blob/c7a8e871de3427cf654d9a608dc58663874197bb/man/loginctl.xml#L186-L199
- [sd-pam] https://github.com/systemd/systemd/blob/c7a8e871de3427cf654d9a608dc58663874197bb/man/pam_systemd.xml#L52-L74
- [apple-launchd] https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html
- [devc] https://containers.dev/implementors/json_reference/
- [devc-feat] https://containers.dev/implementors/features/
- [gh-cs] https://docs.github.com/en/codespaces/about-codespaces/deep-dive

Peers
- [claude-rc] https://code.claude.com/docs/en/remote-control
- [codex-remote] https://learn.chatgpt.com/docs/remote-connections
- [superset] https://docs.superset.sh/remote-access
- [omnara] https://docs.omnara.com/machines/connect; https://docs.omnara.com/machines/overview
- [happy] https://happy.engineering/docs/how-it-works/
- [coder-net] https://coder.com/docs/admin/networking
- [coder-ext] https://coder.com/docs/admin/templates/managing-templates/external-workspaces
- [screens] https://support.edovia.com/en-GB/screens-5/getting-started/basics
- [mosh] https://mosh.org/

Pairing and tokens
- [syncthing] https://docs.syncthing.net/intro/getting-started.html; https://docs.syncthing.net/dev/device-ids.html
- [syncthing-gui] https://raw.githubusercontent.com/syncthing/syncthing/main/gui/default/index.html
- [rfc8628] https://www.rfc-editor.org/rfc/rfc8628 (§3.3, §5.1, §6.1)
- [gh-runner] https://docs.github.com/en/rest/actions/self-hosted-runners
- [netdata] https://learn.netdata.cloud/docs/netdata-cloud/connect-agent
- [1pw] https://support.1password.com/qr-code-security/

Terminal
- [ghostty-cmake] https://github.com/ghostty-org/ghostty/blob/main/CMakeLists.txt
