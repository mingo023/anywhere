# A sent prompt reaches the agent, or says why it didn't

## Why

Issues #380 and #381 (the same report): on 0.1.60, every chat prompt showed "Not delivered", and no `message.sent` reached `orchestrator.db` from 01:30 to 02:49 UTC. The reporter had about 80 terminals. 0.1.61 fixed the slow store writes behind the first half of the outage (#364). Every step that turns a busy pocketd into a lost prompt is still in 0.1.61:

- pocketd handles a connection's frames on the goroutine that reads them (`c.handle(raw)` in `wsserver.go`). `coder/websocket` reads a pong only while a read runs, so a handler that runs past the 10-second ping timeout gets the connection closed.
- The desktop resends `agent.create` after a reconnect but drops every prompt in flight (`refuse_all` in `agents/src/link.rs`).
- pocketd waits up to 30 seconds for the agent to take a prompt (`promptWait`). Past that it reports a failure, but the `turn.start` effect still runs, so a retry delivers the prompt twice.
- The desktop marks a prompt "Not delivered" after 120 seconds without an echo (`GRACE_MS` in `chat/src/outgoing.rs`), whether or not pocketd ever got it.
- `agent.list` calls `Sink.Flush`, which waits up to 5 seconds ("orch: flush timed out") on every connect.
- On every connect, the desktop asks for 500 items of every session, then pages back through each chat's whole conversation (#368).
- Neither side logs a refused prompt or a closed connection, and `PromptFailed` carries no reason, so the second half of the outage (02:13 to 02:45) can't be explained from what was left.

## Decisions

This ships as its own PR, before `2026-11-25-agent-worktrees.md`.

### The rule

1. On this Mac, no timer closes a connection or fails a prompt. Only a real error does: a closed socket, or a failure pocketd reports. The user's machine may be short of CPU and memory, so a slow step must stay slow, not turn into a failure.

### pocketd

2. Each connection gets a goroutine that only reads frames and a goroutine that handles them in order, joined by a queue of 1024 frames. When the queue is full, the reader stops reading until the handler catches up. It doesn't close the connection.
3. A connection over the unix socket gets no ping. A peer that dies closes the socket, and the read fails at once. Phones over the network keep the ping.
4. `agent.prompt` replies with an ack once `Dispatch` commits the command. pocketd no longer waits for the agent (`promptWait` goes for prompts). It reports the result of the `turn.start` effect afterwards: started, or failed with its reason.
5. pocketd uses `prompt:<clientId>:<id>` as the command id, so `receipts` turn a resent prompt into a replay. The agent gets it once.
6. `agent.list` stops calling `Flush`. `Sink` keeps the newest summary of each session that it has queued but not yet written. `openSessions` reads the store and applies those summaries on top: a session whose close is queued drops out, and a queued status wins. `session()` and `subagentTrees()` keep `Flush`, because only rare actions the user takes call them.
7. Each session summary in `agent.list` and `agent.update` gains `recap`: `asked`, `read`, `edited`, and `said`, the four fields `activity/recap.rs` computes today. pocketd updates it when a turn ends.
8. `pocketd.log` records each refused prompt (code, reason, the first 8 characters of the session id), each closed connection (client, reason, how long it lived), and each handler that runs longer than 2 seconds (verb and duration). It never records prompt text.

### Desktop

9. A prompt id is unique across launches: a random prefix made at launch, then the counter. Today `prompt-1` repeats on every launch and would replay an older receipt.
10. After a reconnect, the desktop resends the prompts that have no ack, `queue: true` ones included, with the same id. It doesn't resend `agent.interrupt` or the `queue.*` verbs, because the state they acted on may have changed. Those fail at once when the link drops.
11. A prompt shows one of these states:

	| State | Text |
	|---|---|
	| The link is down, and the prompt will go again | `Sending again…`, muted |
	| The link stayed down for 2 minutes | `Not delivered — can't reach Anywhere's service · click to retry`. The prompt leaves the resend list, so it can never arrive late. |
	| pocketd refused it, or reported that `turn.start` failed | `Not delivered — <pocketd's reason> · click to retry` |
	| pocketd acked it, and the agent hasn't started | `Delivered, waiting for agent…` |

	The 120-second grace goes.
12. The 5-second `STALL` applies only to a host's link over SSH, not to this Mac's socket.
13. The desktop loads timelines on demand. On connect it takes only the session list. It loads a chat's newest page when the chat opens or is on screen, an older page when the user scrolls up, and after a reconnect only what is newer (`sinceSeq`). Activity, Inbox, and system alerts read `recap` from the summary. Hand off loads the timeline of the session it hands off when the user presses it, and shows a spinner meanwhile.
14. `~/.anywhere/logs/desktop.log` (new, rotated like `pocketd.log`) records each link drop and reconnect, each resent prompt, and each prompt that becomes "Not delivered", with the reason.

### GitHub

15. Ask the reporter on #380 now: whether "Not delivered" came back on 0.1.61 with about 80 terminals, how long after sending the red line appeared, and whether they sent a prompt into chat `1983f72b`.
16. When this PR lands, comment on #380 with what changed, and close #381 as its duplicate.

## Out of scope

- Agent worktrees on the sidebar (#382): `2026-11-25-agent-worktrees.md`.
- The updater not offering 0.1.61. CI shows the 0.1.61 build notarized and stapled.
