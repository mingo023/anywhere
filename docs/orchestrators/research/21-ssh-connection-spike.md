# 21 — SSH connection spike: tunnel, failures, auth and the supervisor

Date: 2026-10-12. Worktree `lucky-koala` @ a2ee24f, plus the untracked `packages/desktop/crates/hosts/` in the working tree.

Question: how should the desktop hold an SSH tunnel to a remote pocketd (r20 option B, M0), and how does it tell each failure apart, retry it, and explain it to the user? I ran five experiments against throwaway sshd containers, and three independent reruns of the most surprising results. This note turns them into a supervisor spec that can be built in the GPUI-free `hosts` crate.

Citation legend:
- `[N1]`..`[N5]` are the five experiments (§2). `[N2-5]` means experiment N2, case 5.
- `[V0]`..`[V2]` are independent reruns of three surprises, with new scripts and new containers (§3.6).
- `P path:L` is this repo. `dk/` = `packages/desktop/crates/`. `dk/hosts/` is untracked working-tree code, not in a2ee24f.
- `rNN:L` is an earlier research note in this folder.
- "Inference" is my reasoning, not something I measured.
- **UNVERIFIED** means I did not test it.

## 1. TL;DR

- **The tunnel works, and it's ready the moment the local socket file exists.** In every timed run the first connect after the file appeared succeeded on the first try [N1]. The best readiness signal is a remote command instead of `-N`: `echo READY; exec cat` with stdin held open. READY arrives 1-3 ms after the socket file, and the app keeps the pid [N1].
- **A Daemon `up` does not mean pocketd is reachable.** With ssh up and pocketd down, ssh accepts each local connect and then closes it, and `Daemon::spawn` flaps up/down every ~200 ms forever, because each connect resets its backoff (P dk/daemon/src/daemon.rs:276-281). Four of the five experiments hit this [N1][N2-5][N3][N5]. The supervisor must count a host as Online only after a `status` reply, and the Daemon needs a separate fix.
- **Detecting a silent outage is entirely up to ssh's ServerAlive.** Without it, a 62 s outage went unnoticed and `Daemon::send`/`input` blocked for 59.9 s [N2-1a]. With 5×3 detection took 19.8 s, with 2×2 it took 5.0 s [N2-1b, 1c]. TCPKeepAlive made no difference [N2-6].
- **Retrying a failed login makes it worse.** sshd ≥ 9.8 PerSourcePenalties blocks the whole source IP for ~15-20 s after 3-4 failed key probes, or after one connection with 3 wrong passwords. During the block even a good key sees only `Connection closed by <host> port <p>` [N3][N4]. Auth failures must stop and wait for the user; a bare `Connection closed by` must wait at least 20 s.
- **Every GUI auth path works without a TTY:** BatchMode key probe; password once through `SSH_ASKPASS` with the password in an env var (an inherited fd does not reach askpass); idempotent key install over that one session; and host-key trust via `ssh-keyscan`, pinned into a per-host known_hosts, then `StrictHostKeyChecking=yes` [N4].

## 2. Setup of the spike

| | N1 Tunnel up | N2 Failure detection | N3 Error classification | N4 Auth paths | N5 Retry and side commands |
|---|---|---|---|---|---|
| Container | `sshspike-1`, 127.0.0.1:2210 | `sshspike-2`, :2220 | `sshspike-3`, :2230 | `sshspike-4` :2240 and `sshspike-4b` :2241 on network `sshspike-4net` | `sshspike-5`, :2250 |
| Docker | Docker Desktop | Docker Desktop | Docker Desktop | Docker Desktop | OrbStack |
| Client | macOS OpenSSH_10.3p1 | same | same, plus OpenSSH_9.6p1 (musl, Alpine 3.19) | same (LibreSSL 3.3.6) | same |
| sshd | OpenSSH_10.3p1, image `sshspike:1` (Alpine) | same | same | same | same |
| Daemon | real `daemon::Daemon::spawn` via probe crate `probe1` | real `Daemon::spawn` and `daemon::request` via `probe2bin` | real `Daemon::spawn` via `probe3` | none (raw Python socket) | real `Daemon::spawn` via `probe5` |
| Scripts | `/tmp/sshspike/1/` | `/tmp/sshspike/2/` (logs in `runs/`) | `/tmp/sshspike/3/` | `/tmp/sshspike/4/` | `/tmp/sshspike/5/` |

All runs used key auth with `BatchMode=yes` and their own `-F` file (or `-F /dev/null`) and `UserKnownHostsFile`, so none touched `~/.ssh`. Every container and every ssh process started for the spike was removed or stopped. The `/tmp/sshspike/` scripts are scratch and will not survive a reboot. The reproduction lines in §3 name them so they can be rebuilt.

Stand-ins:
- **pocketd:** a Python `fakepocketd` that answers `{"op":"status"}` and emits an output event every ~1 s [N3]. I did not run real pocketd on either side: it doesn't build for Linux yet (r20 §1).
- **Network:** `docker pause` stood in for a blackhole and `docker network disconnect` for a lost route. In both, the TCP peer is Docker's localhost port proxy, not a real WAN host [N2].
- **Host restart:** `docker restart`. sshd is PID 1 in the container, so restarting sshd alone could not be tested [N2].

Not tested at all:
- **Sleep and wake** on the Mac (§4.6 reasons about it from the pause test).
- **Wi-Fi switch or other network change** on the Mac.
- **Real pocketd**, on Linux or macOS.
- Real WAN latency and packet loss (the container had no `tc` or `NET_ADMIN`) [N5].
- The user's own `~/.ssh/config` (ProxyJump only through an app-written config [N4]), agent keys from a Finder-launched app, passphrase keys and 2FA [N3][N4].

## 3. Measurements

### 3.1 N1: getting the tunnel up

| Case | Result |
|---|---|
| Remote path `~/.coding-pocket/pocketd.sock` in `-L` | Not expanded. Local file is created, then the first request reads EOF and stderr says `channel 1: open failed: connect failed: open failed` |
| Relative remote path `.coding-pocket/pocketd.sock` | Same failure: not resolved against `$HOME` |
| Absolute `/home/dev/.coding-pocket/pocketd.sock` | Works |
| Plain `ssh -N -o ExitOnForwardFailure=yes -L …`, spawn → file / → status (5 reps, busy machine) | file 160/187/214/87/235 ms; status 166/188/216/90/240 ms; 1 connect attempt each |
| Same, quiet machine (4 reps) | file 40/52/38/40 ms; status 43/53/39/41 ms; 1 attempt each |
| `-f -N`: parent exit as the signal (5 reps) | exit at 165/158/124/98/181 ms, rc 0; file present every time; status 2-7 ms after exit |
| `-v` line at readiness | `debug1: Local forwarding listening on path …` at 240.4 ms, file at 240.6 ms. Default LogLevel prints nothing |
| Remote command `echo READY; exec cat`, stdin held (6 reps) | file 37.6-43.5 ms; READY 38.7-46.7 ms (1-3 ms after file); status 0.1 ms after READY. Closing stdin ends ssh with rc 0 ~1010 ms later |
| `Daemon::spawn` started together with ssh (9 reps) | immediate `down`, first `up` on a backoff step: 212-232 ms or 619 ms |
| `Daemon::spawn` started 600 ms after ssh (3 reps) | `up` 1-2 ms after start; status 1 ms after `up` |
| ssh leaves its local socket file | always: after SIGTERM, `-O exit` and ControlPersist expiry. Connecting gives `ConnectionRefusedError(61)` |
| Restart over a stale file, no `StreamLocalBindUnlink` | rc 255 at 251.6 ms: `unix_listener: cannot bind to path …: Address already in use` / `Could not request local forwarding.` |
| Same with `ExitOnForwardFailure=no` | ssh stays alive with no forward, silently |
| Same with `StreamLocalBindUnlink=yes` | status at 198 ms, new inode, empty stderr |
| pocketd down, `-f -N -o ExitOnForwardFailure=yes` | ssh succeeds (rc 0 at 225 ms); every connect reads EOF |
| `Daemon::spawn`, ssh up, pocketd down ~4 s | 20 up + 20 down in 7 s, one pair every ~203 ms. First flapping `send` returned true. Steady `up` 99 ms after fakepocketd started |
| `Daemon::spawn` on a stale or missing file, no ssh | exactly one `down`, then silence |
| SIGTERM to ssh under a connected Daemon | Daemon `down` ~46 ms later (inferred: the kill timestamp was lost); ssh reported **rc 0** |
| Wrong key / closed port / unknown host key | rc 255 in 156 / 43 / 62 ms |

The readiness spread across experiments has known causes: 38-52 ms on a quiet machine [N1], 87-240 ms under load [N1], 144-199 ms in N3, 405 ms in N2 (that is Daemon's backoff, not ssh), 528 ms through a ProxyJump [N4-6].

### 3.2 N2: failure detection

All with a 1 s restart loop, `ExitOnForwardFailure=yes`, `StreamLocalBindUnlink=yes`, `ConnectTimeout=5`.

| Fault | ServerAlive | Detected (Daemon `down`) | ssh stderr | Back up |
|---|---|---|---|---|
| `docker pause` 62 s | unset | never; output gap of 62006 ms on unpause | — | resumed same session |
| `docker pause` | 5×3 | 19820 ms | `Timeout, server 127.0.0.1 not responding.` | 1735 ms after unpause |
| `docker pause` | 2×2 | 5126 ms | same | 1033 ms after unpause |
| `network disconnect` | 2×2 | 5012 ms; TCP was not reset | same; retries `Connection refused` in ~7 ms | 1896 ms after reconnect |
| `network disconnect` 40 s | unset | never; same ESTABLISHED TCP resumed | — | gap 41166 ms |
| `kill -9` ssh client | n/a | 3 ms (rc 137) | — | 1434 ms |
| `kill -9` sshd-session `[priv]` | n/a | no effect | — | — |
| `kill -9` unprivileged sshd-session | n/a | 57 ms | `Connection to 127.0.0.1 closed by remote host.` | 1469 ms |
| `docker restart` | n/a | 40 ms | same | tunnel 1448 ms, then flapped 3.9 s until fakepocketd started |
| kill fakepocketd only | n/a | 65 ms, then ~45 false `up`s in 9.1 s | one `channel 1: open failed` per attempt | real `up` 260 ms after restart |
| `TCPKeepAlive=no`, pause | 2×2 | 5005 ms | same as 2×2 above | 958 ms |

During a 62 s pause with no ServerAlive [N2-1a]:
- `Daemon::send(status)` returned true in 0 ms. The write sat in local buffers: a false success.
- `daemon::request(status)` took 5001 and 5002 ms and returned `Err("Resource temporarily unavailable (os error 35)")`. The local connect succeeds, then the 5 s read timeout fires (P dk/daemon/src/daemon.rs:76).
- 16 KB `Daemon::input` writes #0-95 (~2.1 MB, about one SSH channel window) went through at once. Write #96 blocked 59893 ms, and a `send` issued 8 s later blocked 51896 ms on the writer Mutex. With 2×2 the blocked write was released with `ok=false` when ssh exited [N2-1c].
- With pocketd dead and ssh up, `request(status)` fails in 1 ms with `EOF while parsing a value at line 1 column 0` [N2-5].

macOS TCP keepalive is `keepidle=7200000` ms, so it could not detect anything before about 2 h 10 min [N2-6].

### 3.3 N3: error classification

| Case | rc | Time | stderr (key line) |
|---|---|---|---|
| Unknown host key, `StrictHostKeyChecking=yes` | 255 | 81-116 ms | `No ED25519 host key is known for …` / `Host key verification failed.` |
| Unknown host key, `accept-new` | 0 | 86-120 ms | `Warning: Permanently added …` |
| Changed host key, `yes` or `accept-new` | 255 | 77-81 ms | `@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @` … `Host key verification failed.`; known_hosts unchanged |
| Changed host key, `no`, plain command | **0** | 159 ms | banner, then password and kbd-int disabled |
| Changed host key, `no`, with `-L` and `ExitOnForwardFailure` | 255 | 119 ms | `Error: forwarding disabled due to host key check failure` |
| Wrong key / no key / passphrase key without agent / wrong user | 255 | 80-127 ms | `<user>@<host>: Permission denied (publickey,password,keyboard-interactive).` (indistinguishable) |
| Key file mode 0644 | 255 | — | `WARNING: UNPROTECTED PRIVATE KEY FILE!`, `bad permissions`, then Permission denied |
| PerSourcePenalties, good key after 3-4 failures | 255 | 55 ms | `Connection closed by 127.0.0.1 port 2230`; good key failed at +5/+10/+15 s, worked at +20 s (penalty 19.517 s) |
| Connection refused | 255 | 34-44 ms | `ssh: connect to host … port 2230: Connection refused` |
| Timeout (10.255.255.1, `ConnectTimeout=5`) | 255 | 5043-5048 ms | `ssh: connect to host … port 22: Operation timed out` |
| DNS failure | 255 | 36-79 ms | `ssh: Could not resolve hostname nosuch.invalid: nodename nor servname provided, or not known` |
| Remote socket missing, dir missing, `~` or relative path, `AllowStreamLocalForwarding no` | none, ssh stays up | EOF 1 ms after each connect | `channel 1: open failed: connect failed: open failed` (identical for all of them) |
| Same, `LogLevel=ERROR` | none | — | empty |
| Remote socket path 103 / 104+ chars | 0 / 255 | ~150 ms | `Bad local forwarding specification '…'`, client-side, before connecting |
| Local socket path 111 chars | 255 | — | same |
| Stale local socket file | 255 | ~40 ms | `Address already in use` / `Could not request local forwarding.` |
| Preflight `printf "%s\n" "$HOME"; test -S …/pocketd.sock && echo sock-present \|\| echo sock-missing` | 0 | 105 ms | stdout `/home/dev`, `sock-present` |
| Real `Daemon::spawn` on a broken forward, 3 s | — | — | up=16, down=15; backoff stuck at 200 ms |
| Mid-session pause, ServerAlive 2×2 | 255 | 5141-5284 ms | `Timeout, server 127.0.0.1 not responding.` (hidden by `LogLevel=ERROR`) |
| Mid-session sshd kill / container restart | 255 | 140-166 / 493-764 ms | `Connection to 127.0.0.1 closed by remote host.` |
| OpenSSH 9.6p1 (musl) client | — | — | Same text for all but DNS: `Name does not resolve` |

`LogLevel=ERROR` hides `channel N: open failed`, the penalty's `Connection closed by` and the keepalive `Timeout, server … not responding.` It does not hide the changed-key banner or `closed by remote host` [N3]. ssh keeps the first value it reads for `-o` and for `-p`, so a later `-p 2299` or `-o StrictHostKeyChecking=no` is silently ignored [N3].

### 3.4 N4: auth paths

| Case | Result |
|---|---|
| BatchMode, unauthorized key | rc 255 in 214/265/224 ms, Permission denied |
| Password via `SSH_ASKPASS` + `SSH_ASKPASS_REQUIRE=force`, new session, no tty, password in env `POCKET_SSH_PW`, pubkey on stdin, install script as the command | rc 0; ~680 ms net first time, ~240 ms on re-run. 1 askpass call, argv[1] = `dev@127.0.0.1's password: `. Output `added`, then `already`; authorized_keys 1 line; modes 700/600. No `DISPLAY` needed |
| Password in any argv | No: not in ssh's or askpass's. `ps -E` / `ps eww` showed no env vars for same-uid processes on this macOS |
| Password through an inherited fd (`pass_fds`) | rc 255. The fd was gone before askpass ran (lsof: fds 0, 1, 2, 5) |
| Reconnect with BatchMode and only the installed key | rc 0 in 254-315 ms |
| Wrong password, `NumberOfPasswordPrompts=1` | rc 255 in 211 ms, 1 askpass call |
| Wrong password, `NumberOfPasswordPrompts=3` | rc 255 in 222 ms, 3 calls; this single connection set an 18.211 s penalty |
| askpass exits 1 (cancel) | rc 255 in 1014 ms, same stderr as a wrong password |
| `sudo -S -p ''` over a key session, password on stdin | rc 0; not in argv on either side. Wrong password rc 1, `sudo: 1 incorrect password attempt`. `sudo -S -v` then `sudo -n` fails: no timestamp reuse without a tty |
| `-i` key at 0644 | rc 255, `bad permissions`: the key is ignored |
| ProxyJump, `-N -L` unix socket through it | Works: socket at 528 ms, status in 7 ms, events stream |
| `-o` versus config file | `-o` wins for the final hop. The jump hop's child gets only `-F` (and `-v`), not `-o`. Without a `Host *` block, the jump hop uses `~/.ssh/known_hosts` and `StrictHostKeyChecking ask` (via `-G`) |
| `ssh-keyscan -T 3 -p 2240 -t ed25519` | rc 0 in 80 ms, one parseable line |
| `StrictHostKeyChecking=yes` probe, empty known_hosts | rc 255 in 86 ms, no credentials sent; fingerprint only in `-v` (`debug1: Server host key: ssh-ed25519 SHA256:…`) |
| Pinned keyscan line, then strict connect | rc 0 in 101 ms |
| Pinned key mismatch with `accept-new` | rc 255 in 160 ms, changed-key banner |
| Agent holds the key; `IdentitiesOnly=yes` + missing `IdentityFile` | Agent not used, rc 255 |
| Same with `IdentitiesOnly=no` | Agent key used, rc 0 |
| `IdentitiesOnly=yes -i agentkey.pub` | rc 0: the agent signs for the named public key |
| `IdentitiesOnly=yes`, `IdentityAgent=none`, `-i agentkey.pub` | rc 255 |
| 20 BatchMode wrong-key probes | 1-4 Permission denied; 5-20 `Connection closed by 127.0.0.1 port 2240`; penalty 19.325 s |
| 4 wrong-password connections, then good key | penalty 19.167 s; good key failed at +0/+5/+10/+15 s, worked at +20 s |

### 3.5 N5: retry policy, side commands and path limits

| Case | Result |
|---|---|
| Side command, fresh connection (10 runs) | median 61 ms (54-174) |
| Side command through a mux master (`auto`/`ControlPersist=yes`) | median 28 ms (22-33) |
| Side command through a foreground master (`ControlMaster=yes ControlPersist=no -N -L`), client `ControlMaster=no` | median 26 ms |
| Stale ControlPath, client `ControlMaster=no` | rc 0 in 100 ms, falls back to direct; stderr `Control socket connect(…): Connection refused` |
| Stale ControlPath, client `ControlMaster=auto` | rc 0 in 67 ms; becomes a new persistent master with **no forward** |
| New master with `ControlMaster=yes` on a stale ControlPath | tunnel comes up, but `ControlSocket … already exists, disabling multiplexing`; no error exit |
| Blackhole with a master holding `-L` | `-O check` says `Master running` and the socket accepts but stays silent until ServerAlive fires at +5.5 s (2×2) |
| Side command 0.4 s into the blackhole | rc 255 after 10072 ms: dead master, then fresh connect, then ConnectTimeout |
| Host stop | ssh exits in 0.1-0.17 s, `closed by remote host` |
| Connect while host is down | rc 255 in 10-40 ms, `Connection refused` |
| Flap 5 s up / 3 s down, policy A (base 1 s, ×2, cap 30 s, reset if ssh lived ≥ 10 s) | 8 spawns in 100 s; recovered 2.3 s and 1.5 s, then slept through 3 up windows; last recovery 26.4 s |
| Same, policy B (cap 15 s, reset if lived ≥ 2 s) | 19 spawns; all 6 cycles recovered in 0.43-2.39 s |
| Host down 120 s, policy A | 8 failed attempts; recovered 12.2 s after the host returned |
| Extra lag from `Daemon`'s own backoff against the stale socket | `up` 0.4-1.7 s after the tunnel answered |
| ssh up, pocketd down, real `Daemon::spawn` 5 s | 50 events, 25 up/down cycles |
| `-L` local path length | 103 bytes works; 104, 105 fail with `Bad local forwarding specification` |
| ControlPath length | 86 bytes works; 87+ fails and aborts the session: `unix_listener: path "<path>.XXXXXXXXXXXXXXXX" too long for Unix domain socket` (ssh binds `<path>.<16 random>` first) |
| SIGTERM to a master | ControlPath unlinked, `-L` file left |
| Killing the supervisor process | its ssh child kept running and holding the forward |

### 3.6 Reruns of three surprises

| Claim | Rerun | Change to the claim |
|---|---|---|
| `ControlMaster=auto` + `ControlPersist` makes `ssh -N -L` return rc 0 at once, with the backgrounded master owning the forward [N1] | [V0], 5 runs | Confirmed, and faster: 69.0-77.5 ms, not ~130 ms. Needs both settings: `auto` alone or `ControlPersist` alone kept running past 2 s. A second mux client exited in 10.0 ms and its forward stayed in the master |
| `ControlPersist=3` kills the master after the last forwarded connection closes and leaves a stale socket [N1] | [V1], 2 runs each | Confirmed, and stronger: an **unused** forward dies too, 2902 ms after the foreground ssh returned. An open forwarded connection keeps it alive; after close the master died at 3238-3864 ms |
| `StreamLocalBindUnlink=yes` lets a second ssh silently take the path from a live first one [N1] | [V2] | Confirmed. "Both bound" in lsof is by name only: the first ssh listens on an unlinked inode nobody can reach. Killing the second leaves a dead path while the first ssh still runs |

The other surprises (the Daemon flap, the send/input stall, PerSourcePenalties, `-o` not reaching the jump hop, the 86-byte ControlPath) each come from one experiment. The flap was seen independently in four [N1][N2][N3][N5] and the penalty in two [N3][N4].

## 4. Supervisor spec

Scope: one `Supervisor` per host in `dk/hosts/`, with no GPUI. It owns one ssh child, runs side commands, and reports a `HostState`. The `pocket` side spawns a `Daemon` and `Agents` per host only when the state becomes Online, and drops them when it leaves Online. Its decisions are free functions over plain data (classify stderr, next delay, next state), so they can be tested without a process.

### 4.1 Files and path limits

Everything for a host lives in `<home>/hosts/<id>/`, where `<home>` is `~/.coding-pocket` (release) or `~/.coding-pocket-dev` (dev, P dk/channel/src/channel.rs:18). The dir is 0700.

| File | Purpose | Limit |
|---|---|---|
| `d.sock` | local end of `-L` | ≤ 103 bytes [N3][N5] |
| `c` | ControlPath | ≤ 86 bytes [N5] |
| `key`, `key.pub` | app key, ed25519, `key` 0600 (0644 is ignored [N3][N4]) | — |
| `known_hosts` | pinned host key | — |
| `ssh.pid` | pid of the tunnel ssh, to clean up after an app crash | — |

Path arithmetic with `<u>` = the macOS username length and `<id>` = the host id length:
- `/Users/<u>/.coding-pocket/hosts/<id>/d.sock` is 36 + u + id bytes, so u + id ≤ 67.
- `/Users/<u>/.coding-pocket/hosts/<id>/c` is 31 + u + id, so u + id ≤ 55.
- The dev channel adds 4 bytes: u + id ≤ 63 and ≤ 51.

| id | Longest username, release (`d.sock` / `c`) | Longest username, dev |
|---|---|---|
| 10 hex chars | 57 / 45 | 53 / 41 |
| 24-char slug (today's `ID_LEN`, P dk/hosts/src/hosts.rs:89) | 43 / 31 | 39 / 27 |

Decision: id = the first 10 hex chars of sha256 of `user@host:port` (or the alias), with the display name kept in the store, as N5 suggests. When a host is added, compute both paths. If `c` is too long, use `/tmp/cp-<uid>/<id>` (0700) for the ControlPath [N5]. If `d.sock` is too long, refuse with a clear message. The remote path `<remote $HOME>/.coding-pocket/pocketd.sock` must also be ≤ 103 bytes because the Mac client checks it [N3]; on `/home/<u>` that allows a remote username up to 69 chars. The other files have no limit.

### 4.2 ssh argv

App options come first, because ssh keeps the first value it reads [N3][N4].

**Tunnel** (one long-lived child per host; stdin piped and held open, stdout and stderr piped):

```
ssh
  -F <home>/hosts/<id>/config            # see "Alias targets" below
  -o BatchMode=yes
  -o ConnectTimeout=10
  -o ServerAliveInterval=5
  -o ServerAliveCountMax=3
  -o StrictHostKeyChecking=yes
  -o UserKnownHostsFile=<home>/hosts/<id>/known_hosts
  -o IdentitiesOnly=yes
  -o IdentityFile=<home>/hosts/<id>/key
  -o LogLevel=INFO
  -o ExitOnForwardFailure=yes
  -o StreamLocalBindUnlink=yes
  -o ControlMaster=yes
  -o ControlPersist=no
  -o ControlPath=<home>/hosts/<id>/c
  -L <home>/hosts/<id>/d.sock:<remote $HOME>/.coding-pocket/pocketd.sock
  [-p <port>] [-l <user>]
  <host>
  'echo READY; exec cat'
```

Why each choice:
- **`echo READY; exec cat` instead of `-N`.** Forwards are set up before the session starts, so READY on stdout comes 1-3 ms after the socket exists, and the app keeps the pid. Closing stdin ends ssh with rc 0 [N1]. This matches `TUNNEL_COMMAND` (P dk/hosts/src/ssh.rs:17). `-f` also signals ready but loses the pid [N1].
- **Absolute remote path.** `~` and relative paths fail per connection with the same text as pocketd being down [N1][N3]. The preflight (§4.5) reads `$HOME`.
- **`ServerAliveInterval=5`, `CountMax=3`.** Detection measured at 19.8 s [N2-1b]. 2×2 detects in 5.0 s [N2-1c] but kills the tunnel on any blip over 4-6 s, while 5×3 rides out short ones. Wake and network-change hooks (§4.6) cover the cases where speed matters. TCPKeepAlive is left at its default: it had no effect [N2-6].
- **`ConnectTimeout=10`.** 5 was measured: it bounds a banner hang during an outage at 5016 ms [N2-1c]. I chose 10 because a ProxyJump adds a hop; 10 itself is **UNVERIFIED** [N5].
- **`StreamLocalBindUnlink=yes`.** ssh never removes its socket file, so every restart fails without it [N1][N2][N3][N4][N5]. It also steals a live ssh's path silently [N1][V2], so the supervisor must guarantee one tunnel per host (pid file plus kill before spawn).
- **`ExitOnForwardFailure=yes`.** It catches local bind errors only, not a missing remote socket [N1][N3].
- **`StrictHostKeyChecking=yes` with a pinned key**, not `accept-new`. `accept-new` on a password connect would send the password to a host nobody checked [N4]. Never `no`: it disables forwarding on a changed key and still runs commands [N3].
- **`LogLevel=INFO` explicitly.** A user config with `QUIET` or `ERROR` would hide `channel N: open failed`, the penalty line and the keepalive timeout [N3]. Drop lines starting `Warning: Permanently added` or `Control socket connect(` before classifying: the second is a side command's harmless fallback when the ControlPath is stale [N5], and it contains `Connection refused`, which would hide a `Connection closed by` penalty behind it.
- **Foreground `ControlMaster=yes`, `ControlPersist=no`.** Side commands then cost 26 ms instead of 61 ms on localhost, and fall back to a direct connect on their own if the master is gone [N5]. Never `auto` plus `ControlPersist`: `ssh -N -L` then returns rc 0 in 69-78 ms and hands the forward to a background master the app doesn't own [N1][V0], and `ControlPersist=<N>` kills even an unused forward after N s [V1]. **UNVERIFIED:** a master that runs a remote command instead of `-N`. N5 always ran its master with `-N`. If that combination misbehaves, drop the three Control options: side commands still work, at 61 ms each [N5].
- **No `-f`.** The supervisor owns the foreground pid [N5].

**Side commands** (preflight, `uname`, install, version checks):

```
ssh <the same options up to LogLevel=INFO>
  -o ControlMaster=no
  -o ControlPath=<home>/hosts/<id>/c
  [-p <port>] [-l <user>] <host> '<cmd>'
```

Each side command gets an app-level timeout of 10 s, because one hung 10.07 s on a dead master [N5]. Kill it on timeout and report TimedOut.

**Alias targets and ProxyJump.** The jump hop's child ssh gets only `-F`, never the app's `-o` [N4]. So the trust options go into `<home>/hosts/<id>/config` as a `Host *` block (`UserKnownHostsFile`, `StrictHostKeyChecking`), and also as `-o` for the final hop. `BatchMode`, `IdentitiesOnly` and `IdentityFile` stay `-o` only, so the password connect (§4.6) can use the same config. The jump hop then authenticates with whatever the user's config gives it (Inference). For a `Target::Alias` from the user's `~/.ssh/config`, that file must also `Include ~/.ssh/config` after the `Host *` block so the user's `HostName`, `User`, `Port` and `ProxyJump` still apply. **UNVERIFIED:** the `Include` arrangement [N4]. Until it's tested, the fallback for aliases is no `-F` (the user's config as is) and `UserKnownHostsFile="<app file> ~/.ssh/known_hosts"`, as `args` does today (P dk/hosts/src/ssh.rs:48). In that case the jump hop uses the user's own known_hosts and `ask` (which fails under BatchMode). That is Q2.

**Lifetime.** `std::process::Child` has no kill-on-drop (that is tokio). The supervisor's `Drop` must kill and reap the child, and the child must be in its own process group so nothing else signals it. Write `ssh.pid` after spawn. On app start, for each host: if `ssh.pid` names a live ssh, kill it; then run `ssh -O exit` on the ControlPath, and unlink a leftover `c`. A stale `c` with `ControlMaster=yes` silently disables multiplexing [N5]; ssh unlinks it on SIGTERM but not after SIGKILL or a crash [N5]. Killing the parent alone left ssh holding the forward [N5].

**After a deliberate stop, ignore the exit.** A SIGTERMed ssh reported rc 0 [N1], `kill -9` reported 137 [N2-3]. The supervisor sets a `stopping` flag before it kills and doesn't classify that exit.

### 4.3 State machine

```
pub enum HostState {
    Idle,                                        // not started, or removed
    Connecting { attempt: u32 },                 // preflight, tunnel spawn, READY, status
    Online,                                      // status answered; Daemon running
    NotRunning { retry_at: Instant },            // ssh is fine, pocketd isn't answering
    Reconnecting { attempt: u32, retry_at: Instant, last: Class },
    Failed { class: Class },                     // waits for the user
}
```

Events: `Start`, `PreflightDone { home, sock_present }`, `Ready` (READY on stdout), `StatusOk`, `StatusEof` (EOF in ~1 ms), `StatusTimeout` (5 s `os error 35`), `SshExit { stderr }`, `DaemonDown`, `Timer`, `Wake`, `NetworkChanged`, `UserRetry`, `UserTrusted`, `UserRemove`.

| From | Event | To | Action |
|---|---|---|---|
| Idle | Start | Connecting{0} | clean up (§4.2 Lifetime), run preflight |
| Connecting | PreflightDone | Connecting | build `-L` from `home`, check path lengths, spawn tunnel |
| Connecting | Ready | Connecting | `daemon::request_at(d.sock, status)` |
| Connecting | StatusOk | Online | reset attempt; `pocket` spawns `Daemon` now (`up` in 1-2 ms instead of 216-619 ms [N1]) |
| Connecting | StatusEof | NotRunning | keep ssh; copy chosen by `sock_present` (§4.4 rows 10-11) |
| Connecting, Online, NotRunning | SshExit | Reconnecting or Failed | `classify(stderr)` (§4.4) |
| Online | DaemonDown, ssh alive | Online | `request(status)`; StatusEof → NotRunning; StatusTimeout → stay, ServerAlive will end ssh within ~20 s [N2] |
| NotRunning | Timer | NotRunning or Online | `request(status)` with backoff 2 s → 15 s; Ok → Online |
| Reconnecting | Timer | Connecting{attempt} | respawn |
| Reconnecting, NotRunning | Wake, NetworkChanged, UserRetry | Connecting{0} | kill ssh, `-O exit`, reset backoff, respawn now |
| Online | Wake, NetworkChanged | Connecting{0} | same: the link is probably dead and ssh won't notice for up to 20 s (Inference from [N2][N5]) |
| Failed | UserRetry, UserTrusted | Connecting{0} | — |
| any | UserRemove | Idle | kill ssh, `-O exit`, delete `<home>/hosts/<id>/` |

**Telling "Anywhere isn't running there" from "ssh down".**
- The Daemon alone can't do it. With pocketd down it flaps up/down every ~200 ms; with ssh gone it sends one `down` and goes quiet [N1]. It also can't tell "ssh never started" from "ssh died" [N1].
- The supervisor decides from three things it owns: whether its ssh child is alive, its stderr on exit, and a `status` request over `d.sock`.
  - ssh exited: it's an ssh problem, so classify stderr.
  - ssh alive and status EOFs in ~1 ms: pocketd isn't answering [N2-5].
  - ssh alive and status times out after 5 s: the link is gone [N2-1a]. Wait for ServerAlive.
- The preflight separates "pocketd isn't running" (`sock-missing`) from "socket is there but forwarded connects close" (`sock-present`). The second means `AllowStreamLocalForwarding` is off, or pocketd is wedged or left a stale socket. The client stderr is identical for all of these [N3].
- While NotRunning the `pocket` side holds no `Daemon`, so nothing flaps. Dropping the receiver ends its thread at the next `up` (P dk/daemon/src/daemon.rs:278-280).

### 4.4 Retry table

Backoff for transient classes [N5 policy B]: delay = min(1 s × 2^attempt, 15 s) × uniform(0.5, 1.0), so the steps are 0.5-1, 1-2, 2-4, 4-8, 7.5-15 s. Reset `attempt` on StatusOk, not on ssh lifetime: a fixed "lived 10 s" reset slept through 3 up windows and took 26.4 s to recover [N5]. A down host refuses in 10-40 ms [N5], so cap 15 s costs about 8 spawns per 2 minutes.

Classify on ssh exit (rc 255, or any rc that wasn't a deliberate stop), first match wins, in this order [N3]:

| # | Class | stderr contains | Auto-retry | Backoff | User copy |
|---|---|---|---|---|---|
| 1 | KeyChanged | `REMOTE HOST IDENTIFICATION HAS CHANGED`, or `forwarding disabled due to host key` | no | — | "{host}'s SSH key changed since you last connected. The server may have been reinstalled, or someone may be intercepting the connection. Check with whoever runs it before trusting the new key." |
| 2 | UnknownKey | `Host key verification failed` | no | — | "This is the first time this Mac connects to {host}. Check the fingerprint SHA256:… and choose Trust." |
| 3 | Auth | `Permission denied (` | no: retrying sets a ~20 s penalty for the whole IP [N3][N4] | — | "{host} didn't accept the SSH key for {user}." Actions: Sign in with password · Choose key |
| 4 | PathTooLong | `Bad local forwarding specification`, or `too long for Unix domain socket` | no: our bug, prevent it before spawning | — | "Internal: socket path too long for {host}." |
| 5 | StaleSocket | `Address already in use` | once, after unlinking `d.sock` | immediate | none (shouldn't happen with `StreamLocalBindUnlink=yes`) |
| 6 | NoSuchHost | `Could not resolve hostname` | no when adding the host; yes if it was ever Online (could be offline) [N3] | standard | "Can't find a host called {host}. Check the name, or your connection." |
| 7 | Refused | `ssh: connect to host` … `Connection refused` | yes | standard | "{host} refused the connection. Is it on, with SSH turned on?" |
| 8 | TimedOut | `timed out` | yes | standard | "{host} didn't answer. Check your network or VPN." |
| 9 | Dropped | `not responding`, or `closed by remote host` | yes | standard | "Lost the connection to {host}. Reconnecting…" |
| 10 | Throttled | `Connection closed by` (not `closed by remote host`, matched above) | yes | ≥ 20 s, never the standard steps [N3][N4] | "{host} is refusing connections for now, usually after failed sign-ins. Trying again in 20 seconds." |
| 11 | NotRunning (ssh alive) | status EOF; preflight `sock-missing` | yes, status poll only, ssh kept | 2 s → 15 s | "Anywhere isn't running on {host}." |
| 12 | ForwardBlocked (ssh alive) | status EOF; preflight `sock-present` | slow status poll, as row 11 | 2 s → 15 s | "Can't reach Anywhere on {host}. Its SSH server may block socket forwarding (AllowStreamLocalForwarding), or Anywhere there is stuck." |
| 13 | Other | none of the above | 3 attempts, then Failed | standard | "SSH failed: {last stderr line}" |

Notes:
- The timeout text comes from strerror: `Operation timed out` on macOS and musl, `Connection timed out` on glibc (from source, not tested). The DNS suffix comes from the OS resolver. So match only the prefixes above [N3].
- Penalty retries during the block did not extend it in one observed cycle [N3]. Repeat offences can grow toward the 600 s max (**UNVERIFIED**) [N4].
- Row 10 also matched once while a host was stopping [N5]. Waiting 20 s there costs little.
- Only a stale ControlPath was measured for side commands [N5]. A ControlPath that doesn't exist yet (the preflight runs before any master) is **UNVERIFIED**; I expect the same direct fallback (Inference).

### 4.5 Health check

- **Preflight, once per Connecting:** a side command `printf '%s\n' "$HOME"; test -S "$HOME/.coding-pocket/pocketd.sock" && echo sock-present || echo sock-missing` (105 ms [N3]). It gives the absolute `$HOME` for `-L` and tells rows 11 and 12 apart. `probe_command` already does this (P dk/hosts/src/ssh.rs:12).
- **Link:** ServerAlive 5 s × 3 inside ssh, so a dead link ends ssh in ~15-20 s [N2-1b]. No app-level link ping: `daemon::request` can't see a dead link faster than its 5 s read timeout [N2-1a].
- **pocketd:** `daemon::request_at(d.sock, {"op":"status"})` once after READY, once on every Daemon `down` while ssh is alive, and on the NotRunning backoff. No periodic poll while Online: the Daemon's reader sees pocketd die within 65 ms [N2-5].
- **Wake and network change:** don't trust the old link. Kill ssh, reset the backoff and reconnect now (§4.3). This is Inference: sleep and wake were not tested [N5].
- **Writes during an outage:** `send`/`input` can block for the life of ssh once ~2 MB is queued [N2-1a]. Never call them on the UI thread, and treat `true` as "queued", not "delivered".

### 4.6 Auth flows from a GUI with no TTY

All run as side commands without the master, in a new session (`setsid` in `pre_exec`, as N4's `start_new_session`), with stdin from a pipe or `/dev/null`.

1. **Trust the host key (first connect).**
   - `ssh-keyscan -T 5 -p <port> -t ed25519 <host>` returns one line in 80 ms [N4]. Pipe it to `ssh-keygen -lf -` and show the SHA256 fingerprint.
   - On Trust, write that exact line to `hosts/<id>/known_hosts`. Every later connect uses `StrictHostKeyChecking=yes`, so a key that changes between scan and connect fails closed [N4].
   - keyscan can't follow a ProxyJump (Inference). For jumped aliases, run a `StrictHostKeyChecking=yes -v` probe and parse `debug1: Server host key: ssh-ed25519 SHA256:…` (86 ms, no credentials sent [N4]).
   - Each scan or probe uses up some of sshd's no-auth penalty budget [N4], so run it once per add.
2. **Key probe.** The preflight under BatchMode is the probe: it succeeds, or it fails as Auth, UnknownKey or something else. Run it once. 3-4 failed probes lock out the IP for ~20 s, including the user's own terminal ssh [N4].
3. **Password once, then install the app key.**
   - Generate `hosts/<id>/key` with `ssh-keygen -t ed25519 -N '' -f …` (dir 0700, key 0600).
   - Ask for the password in our own dialog first, so a cancel never reaches ssh: an askpass cancel looks the same as a wrong password [N4].
   - Run it with its **own** option list, not the common one: the first value wins [N3][N4], so a `BatchMode=yes` or `IdentityFile` ahead of these would turn the password path into a silent Auth failure. `ssh -F <config> -o BatchMode=no -o PubkeyAuthentication=no -o PreferredAuthentications=password,keyboard-interactive -o NumberOfPasswordPrompts=1 -o StrictHostKeyChecking=yes -o UserKnownHostsFile=<home>/hosts/<id>/known_hosts -o ConnectTimeout=10 -o LogLevel=INFO [-p <port>] [-l <user>] <host> '<install script>'`. That is why the `Host *` block in `-F` sets only the trust options (§4.2), with env `SSH_ASKPASS=<helper>`, `SSH_ASKPASS_REQUIRE=force`, `POCKET_SSH_PW=<password>`, and `key.pub` on stdin. The helper prints `$POCKET_SSH_PW`. No `DISPLAY` is needed [N4]. Never pass the password by fd: it doesn't reach askpass [N4]. A 0600 FIFO path in env is an **UNVERIFIED** alternative.
   - The install script: `umask 077; mkdir -p ~/.ssh; chmod 700 ~/.ssh; touch ~/.ssh/authorized_keys; chmod 600 ~/.ssh/authorized_keys;` then append the key if it's missing. N4 matched the whole line with `grep -qxF`, which re-adds the key if its comment changes. Match on fields 1-2 (type and base64) instead (**UNVERIFIED**).
   - Then reconnect with BatchMode and only the app key: 254-315 ms [N4].
   - A wrong password costs penalty budget: 4 wrong connections set a 19.2 s penalty [N4]. Allow 2 tries, then ask the user to wait.
   - The pin from step 1 must come first: never `accept-new` on a password connect [N4].
4. **sudo, if an install needs root.** Run `sudo -S -p '' sh -c '<all steps>'` in one session, with the password on ssh's stdin [N4]. The sudo timestamp doesn't carry over without a tty, so batch every step into one call. A wrong password gives rc 1 with `sudo: 1 incorrect password attempt`. The first use prints the lecture to stderr, so don't treat stderr as failure.
5. **Use my existing key (later).** `-o IdentitiesOnly=yes -i <user's key>.pub` lets an agent-held key sign without offering others [N4]. `SSH_AUTH_SOCK` in a Finder-launched app is **UNVERIFIED**.

### 4.7 Separate changes this needs

These are bugs found by the spike, each to be pinned by a test and fixed on its own (CLAUDE.md "Refactors change no behaviour"):
- **Daemon flap.** `Daemon::spawn` resets `wait` to 200 ms and emits `up` on every connect (P dk/daemon/src/daemon.rs:276-281). Emit `up` only after the first line arrives, and don't reset the backoff on a connect that closes with no data [N1][N3].
- **Daemon reconnect lag.** After a tunnel comes back, the Daemon's backoff, sitting at its 2 s cap, adds 0.4-1.7 s [N5]. With the supervisor spawning the Daemon only after StatusOk, this goes away for hosts.
- **Writer stall.** `send` and `input` hold the writer Mutex across a blocking write with no timeout (P dk/daemon/src/daemon.rs:312-316) [N2-1a]. Add a write timeout on the stream.

## 5. Risks and open questions

Risks:
1. **Master plus remote command is untested** (§4.2). The fallback is to drop multiplexing.
2. **Sleep and wake, Wi-Fi switches and real WANs were not tested.** The 2 h macOS TCP keepalive [N2-6] means only ServerAlive or our wake hook will notice a dead link.
3. **Real pocketd was never on the far end.** Real pocketd's quiet-time output and its socket cleanup may differ from fakepocketd (r20 §1).
4. **Docker's proxy was the TCP peer** [N2]. On a real WAN, RSTs and retransmission timeouts may end ssh sooner without ServerAlive (**UNVERIFIED**).
5. **The user's `~/.ssh/config`:** ProxyJump via `Include`, `ControlMaster auto` set by the user, `LogLevel QUIET`, and `Match` blocks were not run [N3][N4]. Our `-o` options and first-value-wins cover the final hop only.
6. **Older clients or servers:** wording before ~9.x (for example `kex_exchange_identification:` for a pre-banner drop), glibc text, and sshd < 9.8 without penalties were not tested [N3].
7. **Penalty escalation** toward 600 s after repeat offences was not measured [N4].
8. **One SIGTERM timing was inferred** (~46 ms), because the timestamp line was lost [N1].

Gap to the current working-tree code (`dk/hosts/`, untracked; not edited here):

| Today | Spec |
|---|---|
| `StrictHostKeyChecking=accept-new` (P dk/hosts/src/ssh.rs:40) | `yes` after a pinned keyscan (§4.6) |
| `ConnectTimeout=5` (:37) | 10 (5 is the measured value) |
| Backoff 1, 2, 4 … cap 30 s, no jitter (:112) | cap 15 s, jitter ×U(0.5, 1.0), reset on StatusOk |
| Socket at `hosts/<id>.sock`, 24-char slug id (P dk/hosts/src/hosts.rs:44, :89) | `hosts/<10hex>/d.sock` and `hosts/<10hex>/c` |
| No `Address already in use` or `too long for Unix domain socket` patterns (:143) | rows 4-5 in §4.4 |
| No ControlMaster, pid file or orphan cleanup | §4.2 Lifetime |

Open questions:
1. **ServerAlive 5×3 or 2×2?** 19.8 s to notice versus dropping on 4-6 s blips [N2]. I picked 5×3 plus wake and network hooks.
2. **Alias targets:** generate `Host *` + `Include ~/.ssh/config` (needs a test), or use the user's config as is and accept their known_hosts and the jump hop's `ask`?
3. **Honour `~/.ssh/known_hosts`** for a host the user already trusts, or always pin into our own file?
4. **App key or the user's key?** Installing our key means editing `authorized_keys` on their host. Using their key means relying on their agent or key files.
5. **ControlMaster at all?** It saves ~35 ms per side command on localhost [N5], at the cost of the stale-ctl cleanup and the untested master-plus-command combination.
6. **Remote channel:** does a dev desktop forward to `.coding-pocket-dev/pocketd.sock` on the host, or always to release?
