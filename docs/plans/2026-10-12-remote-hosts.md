# Remote hosts (MonoCode "machines" for Anywhere)

Goal: projects can live on another Mac, a Linux box or a Windows PC reached over SSH. Their terminals and agents run in that host's pocketd; Explorer, Changes and git work on the host's checkout. Desktop only; the phone is out of scope.

Reference: MonoCode `docs/remote-access.md` (clone at `/tmp/monocode-research/monocode`), research `docs/orchestrators/research/20-fleet-hosts.md`.

## Decisions

- Hosts: macOS, Linux, Windows. The desktop stays macOS-only.
- A session on a host is a terminal in that host's pocketd, as locally.
- UI copy says **Host** (Settings › Hosts, "Open folder on a host…"). Layout follows MonoCode: a host's projects sit in the normal project list with a globe and the host's link dot.
- Mac and Linux: the desktop forwards the host's `pocketd.sock` over `ssh -L` (existing `hosts` crate). A forwarded unix peer is owner; no token.
- Windows: pocketd also listens on `127.0.0.1:<port>`; the desktop forwards that TCP port and authenticates with an owner token minted by `pocketd pair --json` over SSH.
- The desktop installs pocketd over SSH when the host has none: download the release archive for the host's os/arch on the host, verify its `.sha256`, unpack to `~/<home>/runtime/<version>/`, install a user service (LaunchAgent, systemd user unit + linger, Task Scheduler). A pocketd already running is reused, never replaced, except by **Update host**.
- Remote git and files go through pocketd ops `exec` and `fs.*`; the Rust `git` crate takes a `Runner`, so parsing stays in one place.
- Upgrades: Mac/Linux keep the in-place handoff. Windows restarts, and the Update dialog says terminals will close.
- Windows is built and unit-tested by cross-compilation; there is no Windows machine to run it on here.

## Wire contracts

### Ops (JSON lines on the socket, owner only)

`status` reply gains:

```json
{"hostId":"8f0c…","os":"linux","arch":"arm64","home":"/home/me","shell":"/bin/bash","name":"devbox","caps":["host.v2","fs.v1","exec.v1"]}
```

`hostId` is a random 128-bit hex id kept in `<home>/state/host.json`, made on first serve.

`spawn` with `"login":true` lets pocketd pick the program and env: it runs the host's login shell with its captured login env, in `cwd` (`~` or empty means the host's home). With `"argv":["claude",…]` it runs that inside the login shell and drops back to the shell when it exits, like the desktop's `agent_op`.

`exec` (cap `exec.v1`):

```json
{"op":"exec","id":"1","cwd":"/abs","argv":["git","status","--porcelain"],"stdin":"<base64>","timeoutMs":30000}
→ {"ev":"exec","id":"1","code":0,"stdout":"<base64>","stderr":"<base64>"}
```

Runs with the login env (so `git`, `gh`, `claude` resolve as in a terminal). `code` is -1 when killed by the timeout. stdout and stderr are each capped at 64 MiB.

`fs.*` (cap `fs.v1`), all paths absolute on the host:

| op | request | reply |
|---|---|---|
| `fs.list` | `path` | `entries:[{name,dir,link,size,mtime,ignored}]` (ignored = gitignored) |
| `fs.stat` | `path` | `exists,dir,size,mtime` |
| `fs.read` | `path`, `max` (bytes, default 16 MiB) | `data` (base64), `size`, `truncated` |
| `fs.write` | `path`, `data`, `create` (fail if exists) | `{}` |
| `fs.mkdir` | `path`, `parents` | `{}` |
| `fs.rename` | `from`, `to` | `{}` |
| `fs.remove` | `path`, `recursive` | `{}` |
| `fs.trash` | `path` | `trashed` (where it went) |

Errors reply `{"ev":"error","error":"…"}` as other ops do.

### Desktop keys

A path on a host is written `//<id>/<path>`, where `<id>` is the desktop's `hosts::Host.id` (a short slug), everywhere the desktop keys by path (store, project list, layouts, terminal cwds). Windows paths use forward slashes: `//<id>/C:/Users/me/app`. `hosts::place(id, path)` builds one, `hosts::split(key)` returns `(Some(id), path)` or `(None, key)` for this Mac. Terminal and agent ids from a host are prefixed `<id>/` in the desktop and stripped when talking to that host.
