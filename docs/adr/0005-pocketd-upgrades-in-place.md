# pocketd upgrades itself in place, and an update never interrupts a terminal or an agent

When its own binary changes on disk, because Sparkle swapped the bundle or `make pocketd` rebuilt it, or when it is sent `pocketd upgrade`, pocketd hands its live state to the new binary within the same process. It pauses its PTY readers, so unread output waits in the kernel and nothing is lost, and writes a versioned handoff file. It then dry-runs `<new> handoff --check <file>`, clears close-on-exec on the PTY masters, and calls `syscall.Exec`. The new image adopts those fds, rebuilds each VT from its snapshot and every agent under its old id with its timeline seq, and resumes reading. The new image binds its listeners and takes the lock again, so clients see a reconnect: the desktop reattaches and gets the snapshot, as on any reconnect today. We chose exec over Superset's spawn-a-successor because the pid stays the same: launchd still owns the job, and shells stay our children so their exit codes can be reaped. The cost is that there is no going back once exec runs, so a bug in the adopt path kills the terminals; the dry run and e2e tests guard it. The other option was restarting only when idle, which can wait days while a terminal stays open.

Permission prompts in flight carry over. The Claude hook redials and resends its request when the socket drops. Codex's app-server keeps pending approvals across a disconnect and replays them on `thread/resume`. Only a launch in progress delays the upgrade; an open pairing code is dropped.

## Rules

- A release keeps speaking the protocol of the previous release's pocketd (`proto.MinVersion`). The phone app already depends on this.
- A release reads the handoff file the previous release wrote. The file carries a format version.
- The upgrade path never reaches `CloseAll`. Only a real shutdown closes terminals.
- The old binary's code runs the handoff and its in-flight hooks, so the hook retry and the old side of the handoff ship in the first release.
- If the dry run fails, the old pocketd keeps running and the app shows a toast.
