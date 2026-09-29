---
id: 4
title: Knowing which process a PTY runs on macOS
labels: [wayfinder:research]
status: closed
assignee:
blocked_by: []
---

## Question

pocketd holds each shell's PTY master. Can it reliably tell which process is in the PTY foreground (`claude`, `codex`, `npm`, or the prompt), and learn promptly when it starts and ends? Compare:

- `tcgetpgrp` on the master fd, then name and argv via libproc or `sysctl KERN_PROCARGS2` (Go/cgo), with polling or `kqueue` `EVFILT_PROC`;
- shell integration (OSC 133/OSC 7, as ghostty does) for prompt, command and exit code;
- the real process names of `claude` (native binary or node) and `codex` when run via shim, alias, `npx`.

Deliver: latency, cost, required permissions, and blind spots (nested tmux, `exec`, subshells).

## Resolution

Findings: `docs/research/pty-foreground-process.md` on branch `research/pty-foreground-process` (a976e1a).

- **Foreground:** `TIOCGPGRP` on the master fd works in pure Go (x/sys). It costs about 3µs and needs no permissions. A command shows as foreground 4–9 ms after Enter; its exit is seen within 2 ms.
- **Events:** kqueue `EVFILT_PROC` (fork/exec/exit) plus re-checks at about +5/+20/+100 ms, because at fork time the shell still holds the foreground. `NOTE_TRACK` is unsupported.
- **Shell integration:** OSC 133 gives the command line, exit code and "at prompt". fish emits it natively; zsh needs our own `ZDOTDIR` shim (Ghostty's is GPLv3); bash 3.2 was not tested. libghostty-vt has no 133 callback, so pocketd needs a scanner in `pump`.
- **Naming:** native `claude` shows its version (`2.1.283`) as the kernel name, so match argv[0] or the exe path. Standalone `codex` shows as `codex`; npm wrappers put codex inside the group.
- **Blind spots:**
  - nested tmux;
  - subshells;
  - pipelines whose leader exits first;
  - `exec` (same pid; compare the name);
  - background jobs (`cmd &`);
  - sudo.
