# Chat with an agent

In a session shown as a chat, the user types a prompt in the bottom composer and sends it. The prompt appears as a bubble, the agent works and streams a reply, and permission or question asks show inline for the user to answer.

## Sub-features

- `chat-send` sends a follow-up prompt (`agent.prompt`) and shows the bubble plus the streamed reply.
- `chat-permission` shows an agent's permission ask; approving sends `permission.resolve`.
- `chat-stop` stops a reply mid-stream (`agent.interrupt`) and shows it interrupted.
- `chat-slash` opens the `/` command menu from the composer, listing the agent's commands.

## How to get to it (user POV)

- Start a session from New session (it opens as a chat). See `new-session.md`.
- Open an existing session started as a chat from the sidebar or the palette.

## Driving it with drive.sh

Preconditions:

- Scenario `sessions`. The chat comes from the `new-session` journey, because existing fixture sessions open as terminals (see Gotchas).
- `S=.claude/skills/verify-anywhere/scripts; OUT=.ui-review/shots/verify/chat-$(date +%Y%m%d-%H%M%S)`.

- **Follow-up typed (before).** Create the session, then click the chat composer and type. Run `CAPTURE_KEYS="cmd-n;enter" CAPTURE_CLICKS="792,451;528,771" CAPTURE_TYPE="Start|Now add a test" $S/drive.sh sessions $OUT/before typed=key,click,type,key,pause,pause,pause,click,type`. `impl-typed.png` shows the first exchange and "Now add a test" in the bottom composer.
- **Follow-up sent (after).** The same, plus Enter. Run `CAPTURE_KEYS="cmd-n;enter;enter" CAPTURE_CLICKS="792,451;528,771" CAPTURE_TYPE="Start|Now add a test" $S/drive.sh sessions $OUT/after sent=key,click,type,key,pause,pause,pause,click,type,key,pause,pause,pause`. `impl-sent.png` shows a second bubble "Now add a test" followed by a second reply.
- **Side effect.** Run `rg '"agent.prompt"' $OUT/after/pocketd-frames.jsonl`. One frame with `"text":"Now add a test"` and the `agentId` of the new session (`new-<requestId>`).
- **Permission ask.** Send `/fixture permission` as the follow-up, then answer the ask. The fake agent asks to run `pnpm test --filter parser`. Approving must produce a `permission.resolve` frame for request `permission-<agentId>`.
- **Proof.** Cite both screenshots and the `agent.prompt` line.

## Gotchas

- The chat composer coordinate (528,771) is from the screenshot after a new session started, in the default Sidebars layout. Re-measure if the layout or window size changed.
- Existing fixture sessions (for example "Refactor the transcript fold" in scenario `chat`) open as a blank terminal: the fake pocketd draws nothing, and the app shows a chat only for sessions it started as chats. Reaching their chat needs the named capture step `chat`, which is an internal setter. Use it only to set up, never as proof.
- A slash-prefixed prompt isn't answered by the fixture (it only echoes it), except the `/fixture …` and `/compact` commands.
- Replies stream for about 1.2s (40×30ms). Add `pause` steps before the after-screenshot, or set `FIXTURE_CHUNKS=1`.
- Permission and stop haven't been run yet. Measure the Approve and Stop button points from a screenshot first.
