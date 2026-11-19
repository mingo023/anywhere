# Agents outlive pocketd: a holder per terminal owns its pty

On macOS and Linux, each Terminal runs under its own `pocketd hold` process instead of under pocketd. The holder calls `setsid`, owns the pty master, is the parent of the Terminal's process, and serves one unix socket in `<home>/holders/`. pocketd is a client of its holders: it keeps the VT, the OSC 7501 scanner, query replies, presence and everything else it had, and only the bytes, the size, signals, the foreground group and the exit code cross the socket. When pocketd starts it dials every holder it finds and reattaches its Terminal under the same id; a saved Terminal with no holder left, as after a reboot, is restored with `--resume` as before. This supersedes ADR 0005 for those platforms.

We chose this because any death of pocketd killed every agent. Under ADR 0005 pocketd held every pty master, so a crash, a "Restart service" (`launchctl kickstart -k`), or any failure after the upgrade's `exec` closed them all, and the next pocketd reaped what was left. One user lost 30 agents when the exec'd image exited 75 within 130ms because it couldn't take back `pocketd.lock`. `--resume` brings the conversation back but not what ran inside the agent: sub-agents, background shells, dev servers. ADR 0005 also kept growing a list of startup steps that must never fail after an exec. Run as a throwaway LaunchAgent, a Terminal under a holder survived `kickstart -k`, `launchctl kill SIGKILL` and a binary swap; under ADR 0005 it died at the first.

We rejected two other shapes. A hardened handoff in one process (hand down the lock, roll back by exec'ing the old binary) fixes upgrades only: launchd has no fd store, so nothing survives a crash or an external kill. One shared pty host for all Terminals moves the problem down a level: when it crashes every agent dies, and it needs its own in-place upgrade. A holder per Terminal loses one session when it crashes, and is never upgraded: it runs the code it started with until its Terminal ends.

## How it works

- **The holder is dumb on purpose.** It relays bytes and keeps a ring of the last 4 MiB of output, so pocketd can be away without the agent blocking. pocketd sends it a VT snapshot every 256 KiB of output with the offset it stands for. A reattach replays the snapshot and the bytes after it into a new VT, with query replies and OSC 7501 report callbacks muted, since the agent asked those questions of a pocketd that is gone.
- **A holder whose process ended waits for pocketd.** pocketd acknowledges the exit when it is back, and restores that Terminal as after a reboot. A holder whose exit nobody acknowledges leaves after a day.
- **SIGTERM restarts pocketd, nothing else.** It no longer closes Terminals. Before it exits, and before an upgrade's `exec`, pocketd flushes its store and writes `handoff.json` with each Terminal's saved entry and its Agent's phase, timeline epoch and seq, and transcript read offset, so the next pocketd continues each timeline exactly. After a crash there is no file, and a reattached agent continues from what the store had: lines its transcript gained while pocketd was down are not shown, and an agent started in the last second before the crash, which pocketd hadn't saved yet, comes back under a new id.
- **An upgrade is a plain `exec` of the new binary.** Only the MCP listener and the lock's fd are handed down. If the new image fails to start, launchd starts it again and the holders wait.
- **The holder rewrites `POCKETD_PARENT` to its own pid**, so `reap` spares an agent's children while its holder lives.
- **Ending sessions is explicit.** Uninstalling the service (`pocketd daemon uninstall --force`, `pocketd service uninstall`) and closing a Terminal end holders; nothing else does. On Linux the systemd unit sets `KillMode=process`, or stopping the unit would kill every holder in its cgroup.

## Rules

- A release must reattach the holders of every earlier release that has them. The holder protocol has a version in its hello, and pocketd speaks every version from `holder.MinVersion` up. Changing what a holder does reaches new Terminals only.
- A release reads the `handoff.json` the release before it wrote. A file from a release before holders carries pty fds; pocketd moves each into a new holder, which can't learn that process's exit code, so it reports -1.
- Going back to a release without holders is not supported while holders run. Its binary swapped in is refused, since it reads no `handoff.json` past format 1 and this release writes format 2. Started cold, it would restore every Terminal a second time beside the one still running.

## Not covered

Windows keeps ADR 0005's exception: Terminals stay inside pocketd, and Update host restarts it and closes them. A Windows holder would own the ConPTY, but must run from a copy of `pocketd.exe` that an update can't lock, and break away from the logon task's job.
