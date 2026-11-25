# 23 — Antigravity (`agy acp`) live checks

Date: 2026-11-25. agy 1.5.16 (darwin-arm64), signed in on the free tier, run as `agy acp` from a throwaway Python JSON-RPC client. Most runs used `gemini-3.8-flash-low`. The client sent the same `initialize` Anywhere sends (`fs` off, `terminal:false`, `session.configOptions.boolean`), and added `elicitation:{form:{}}` where noted. Scratch repo: `/tmp/agy-live/repo`. Every frame both ways was logged as JSONL. The logs are throwaway and not in the repo.

This answers the LIVE checks in `22-antigravity-parity.md` §3, using its numbering (checks 1–5; check 6 is an aside at the end). The evidence below is raw wire JSON, trimmed: `…` marks a cut, ids are shortened, and `~` stands for the home directory. agy's tool updates repeat the call input under both `args` and `rawInput`; only `rawInput` is kept here.

agy's global state was left unchanged. `jetski_state.pbtxt` and `~/.gemini/config` were byte-identical afterwards, a fresh `session/new` still defaults to `gemini-4-hydrogen-high`, and `session/set_model` is per session.

## Summary

| # | Check | Verdict |
|---|---|---|
| 1 | Steering | **Works.** A second `session/prompt` mid-turn makes the first reply `end_turn` at once. The second reply then carries the merged turn and ends `end_turn`. B is applied at the next step boundary and never cuts a step short. A steer while a permission or question is open is **refused** (`-32603`). |
| 2 | Questions | **`elicitation/create` confirmed** when the cap is offered. The schema matches the guess, plus a `toolCallId` param. agy reads answers by `const`; a title answer reads as no answer. Without the cap, or after a refusal, agy writes the question into the chat, ends the turn, and takes the user's next prompt as the answer. Every question also comes as an `ask_question` tool call (`kind:"other"`). |
| 3 | `interaction_*` decision | **Not reproduced** in 9 attempts. Every permission seen used a `call_…` id with exactly `allow_once`/`reject_once`, and every "decision" went through `ask_question` → elicitation (or chat). This does not show the prefix never occurs; it shows it is not the normal question path. |
| 4 | Subagents | **`invoke_subagent` confirmed, with a different shape.** `rawInput` is flat (`{Role, Prompt, TypeName, Model}`), with one call per sub-agent and no `Subagents[]`. The call goes `completed` ~25 ms after launch, with no output. Child tool calls (and child permission requests) **do** arrive as `session/update` on the parent `sessionId`, tagged only by a `"<childConversationId>:"` prefix on `toolCallId`. Child text never streams. |
| 5 | Extra folders | **Both work.** `agentCapabilities.sessionCapabilities.additionalDirectories:{}` is advertised. `session/new` `additionalDirectories` and `agy acp --add-dir` both make the folder part of the workspace: agy reads in it with no permission request, where the baseline asks. A `session/resume` without the field keeps the folder. |

## 1. Steering

Three runs, each sending prompt B while prompt A was still open.

**1a. B sent during a running tool.** A: "Run `sleep 20`, then reply DONE-A". B was sent 2 s after `sleep` started: "Change of plan: … reply BANANA instead".

```
t=4.30  <  tool_call_update {"toolCallId":"call_4438…","name":"run_command","status":"in_progress","kind":"execute","title":"sleep 20",
                            "rawInput":{"CommandLine":"sleep 20","WaitMsBeforeAsync":10000,"toolSummary":"Run sleep command"}}
t=6.295 >  {"id":5,"method":"session/prompt","params":{"sessionId":"e12d…","prompt":[{"type":"text","text":"Change of plan: …BANANA…"}]}}
t=6.306 <  {"id":4,"result":{"stopReason":"end_turn","usage":{"inputTokens":9580,"outputTokens":39,…}}}        ← A, 11 ms after B arrived
t=16.9  <  agent_message_chunk "Understood. I will wait for the command to finish and then reply with `BANANA`."
t=24.36 <  tool_call_update {"toolCallId":"call_4438…","status":"completed",…}
t=26.31 <  agent_message_chunk "\n\nBANANA"
t=26.33 <  {"id":5,"result":{"stopReason":"end_turn","usage":{"inputTokens":4656,"outputTokens":21,"cachedReadTokens":15392,…}}}  ← B
```

**1b. B sent while text was streaming.** A: "Write a 600-word story…". B was sent 0.5 s after the first chunk: "Stop the story now. Reply with exactly: STEERED".
- A's reply came 6 ms after B arrived: `{"stopReason":"end_turn"}`, with **no usage**.
- The story kept streaming to its end (3.6 kB, ~11 s), then `"\n\nSTEERED"`, then B's reply: `{"stopReason":"end_turn","usage":{…"outputTokens":807…}}`.
- So B does not interrupt a generation in flight. It is read at the next step boundary.

**1c. B sent while a permission request was open.** B was refused at once, and A was unaffected:

```
t=4.392 <  {"method":"session/request_permission","id":"1","params":{"sessionId":"ac71…","toolCall":{…"toolCallId":"call_1454…","title":"echo hello-a",…}, "options":[…]}}
t=4.393 >  {"id":5,"method":"session/prompt",…"Also reply with the word BANANA at the end."}
t=4.394 <  {"id":5,"error":{"code":-32603,"message":"Can't add this prompt to session ac71…: the running prompt is waiting for a permission or question answer; answer it or send session/cancel, then send this prompt again"}}
t=8.394 >  {"id":"1","result":{"outcome":{"outcome":"selected","optionId":"allow"}}}
t=10.52 <  {"id":4,"result":{"stopReason":"end_turn",…}}          ← A finishes normally, B never applied
```

**1d. Steer, then `session/cancel`.**
- A: `sleep 20`. B was sent at 4.8 s; A replied `end_turn` at once.
- `session/cancel` at 7.85 s brought `{"id":5,"result":{"stopReason":"cancelled"}}` (B) and `tool_call_update … "status":"failed"` for the sleep.
- A new prompt C then ran normally (`end_turn`). Only C was echoed back as a `user_message_chunk`; steered prompts in 1a and 1b never were.

Other facts:
- Every update carries the same `sessionId`, with no marker of which prompt it belongs to.
- `initialize` returns `"agentInfo":{"name":"Antigravity ACP Server","version":"1.5.16"}`.

**Impact on PR 4 (steering).**
- The mechanism is as guessed. The reply order is the reverse of the risky case: **the first prompt ends `end_turn` immediately** (not `cancelled`), and the steer's reply ends the merged turn. `finish()` must therefore not end the turn on A's `end_turn` while a steer is outstanding. Keep the counter from §2.3 and end on the last outstanding reply. A's early reply can lack `usage`.
- The false-"interrupted" worry from §2.3 does not arise. B ends `cancelled` only after a real `session/cancel`, and that should show interrupted.
- **Don't steer while a card is open.** While a permission or question from this session is awaiting the user, keep queuing (today's behaviour). Also treat a `-32603` reply to a steer as "requeue", not as a failed turn.
- A steer takes effect at the next step, so a long generation still runs to its end. The UI should not promise an instant redirect.
- The gate can use `agentInfo.name == "Antigravity ACP Server"` with `version` ≥ 1.5.0. Anywhere's flavor already identifies the provider, so the version is the only check needed.
- pocketd must record B's user message itself, because agy doesn't echo it.

## 2. Questions (`elicitation/create`)

Prompt: "Before doing anything else, use your ask-the-user tool to ask me which fruit I prefer: Apple or Pear. … reply YOU PICKED <fruit> (or NO ANSWER)". Default mode, with `"elicitation":{"form":{}}` offered.

**Request** (identical shape in every run):

```json
<  {"method":"session/update","params":{"sessionId":"3233…","update":{"sessionUpdate":"tool_call","toolCallId":"call_9289…","name":"ask_question",
     "status":"in_progress","kind":"other","title":"Run ask_question",
     "rawInput":{"questions":[{"is_multi_select":false,"options":["(Recommended) Apple","Pear"],"question":"Which fruit do you prefer?"}]}}}}
<  … same call, "status":"pending"
<  {"jsonrpc":"2.0","id":"1","method":"elicitation/create","params":{"sessionId":"3233…","toolCallId":"call_9289…","mode":"form",
     "message":"The agent has a question.",
     "requestedSchema":{"type":"object","properties":{
       "q1":{"type":"string","title":"Which fruit do you prefer?","oneOf":[{"const":"1","title":"(Recommended) Apple"},{"const":"2","title":"Pear"}]},
       "q1_other":{"type":"string","title":"Other","description":"Optional free-form answer to: Which fruit do you prefer?"}}}}}
```

Variants:
- **Multi-select:** `"q1":{"type":"array","title":"Which fruits do you like?","items":{"anyOf":[{"const":"1","title":"Apple"},{"const":"2","title":"Pear"},{"const":"3","title":"Plum"}]}}`, plus `q1_other` as above.
- **Two questions:** `"message":"The agent has 2 questions."`, with `q1`, `q1_other`, `q2`, `q2_other`. Each question's consts restart at `"1"`. No `required` array was ever sent.

**Answers and what agy did with them.** agy's `completed` update carries no output, so how agy read each answer is inferred from the model's next text.

| Reply sent | agy's text after | Read as |
|---|---|---|
| `{"action":"accept","content":{"q1":"2"}}` | `YOU PICKED Pear.` | const → option ✓ |
| `{"action":"accept","content":{"q1":"Pear"}}` (title) | `NO ANSWER` | **not matched: titles don't work** |
| `{"action":"accept","content":{"q1_other":"Mango"}}` | `YOU PICKED Mango` | free text ✓ |
| multi: `{"action":"accept","content":{"q1":["1","3"]}}` | `YOU PICKED Apple, Plum.` | array of consts ✓ |
| multi: `{"action":"accept","content":{"q1":"3"}}` | `YOU PICKED Plum.` | a bare const is also accepted |
| two questions: `{"action":"accept","content":{"q1":"2"}}` | `YOU PICKED Pear AND User Skipped.` | a missing `qN` reads as skipped |
| `{"action":"decline"}` | `NO ANSWER` | turn goes on, `end_turn` |
| `{"action":"cancel"}` | `YOU PICKED NO ANSWER` | **same as decline; the turn is not stopped** |
| error `-32602` | question written into the chat (below), `end_turn` | chat fallback ✓ |
| error `-32601` | same chat fallback | ✓ |

After each accept, decline or cancel, the `ask_question` call goes `in_progress` → `completed` with no `content` or `rawOutput`.

**Without the cap, or after a refusal:**

```
t=3.951 <  tool_call {"toolCallId":"call_8208…","name":"ask_question","status":"in_progress","kind":"other","title":"Run ask_question",…}
t=3.952 <  tool_call_update {"toolCallId":"call_8208…","status":"pending",…}
t=3.952 <  agent_message_chunk "\nWhich fruit do you prefer: Apple or Pear?\n- [1] Apple\n- [2] Pear\n"
t=3.952 <  {"id":4,"result":{"stopReason":"end_turn",…}}             ← ask_question still "pending"
t=3.952 >  {"id":5,"method":"session/prompt",…"text":"2"}          (a separate run sent "Pear": same result)
t=3.954 <  tool_call_update {"toolCallId":"call_8208…","status":"in_progress"} ; {… "status":"completed"}
t=6.094 <  agent_message_chunk "YOU PICK" "ED Pear"
t=6.13  <  {"id":5,"result":{"stopReason":"end_turn",…}}
```

The next prompt is the answer, by number or by name, and it closes the pending `ask_question` call. `finish()` closes open calls only on a stop, so reading the code (not tested through pocketd), that row would show as running until the user replies.

**Impact on PR #384.** The elicitation half matches reality. These are the mismatches and the fixes they need:
1. **An extra "other" row beside the card.** Every question also comes as a `tool_call` with `name:"ask_question"`, `kind:"other"` and `title:"Run ask_question"`, before the `elicitation/create`. PR #384 would show it as a generic row next to the question card. Fix: hide `ask_question` calls when the flavor elicits, matching by `name` or by the elicitation's `params.toolCallId`. Without the cap that row stays `pending` until the next prompt.
2. **`cancel` doesn't stop the turn.** agy reads it as "no answer" and carries on. If Anywhere's Stop/dismiss is meant to stop the agent, it must also send `session/cancel`. Otherwise the reply is fine as it is.
3. **Option titles may start with `(Recommended) `.** That is cosmetic: the card shows the prefix. Strip it into the description or a flag if wanted. The const mapping is unaffected.
4. **The fake agent's fixture differs from the wire.**
   - Its `message` is the question; the real one is `"The agent has a question."` / `"The agent has N questions."`.
   - Its `q1_other` title is `"Something else"`; the real one is `"Other"` with a `description`.
   - It sends no `toolCallId` param and no `ask_question` tool call.

   Update `fakeacp askForm` to send the `ask_question` tool_call first, then the real `message`, `toolCallId` and `q1_other`.
5. Confirmed as built:
   - the reply uses the `const`, not the title;
   - `qN_other` takes typed text;
   - multi-select answers are an array of `const`s;
   - `decline` works;
   - a `-32602` refusal falls back to chat with no failed step;
   - field order `q1…qN` holds;
   - unanswered questions are left out.

## 3. Decision question → `interaction_*`

**Not reproduced.** Each attempt below, and what arrived instead:

| Attempt | Mode / cap | What arrived |
|---|---|---|
| "Present a single-choice decision (Proceed with A / B) through your approval mechanism" | default, cap | `ask_question` → `elicitation/create` |
| same | accept-edits, no cap | `ask_question` → chat text |
| "Add hello.txt" | plan | plan artifact written (`write_file` with `ArtifactMetadata.RequestFeedback:true`), then chat "Please review the plan…", `end_turn`. No request. |
| "Write an implementation plan, request my review, wait for approval" | default | same as plan: no request |
| same | auto | same: no request |
| read a file outside the workspace | default | `session/request_permission`, `toolCallId:"call_3678…"`, options below |
| fetch a URL | default | no request |
| `git push --force` | auto | no request (ran, failed: no remote) |
| subagent child runs a command (check 4) | default | `session/request_permission`, `toolCallId:"<childId>:call_…"`, same options |

Every permission request seen had exactly these options:

```json
"options":[{"optionId":"allow","name":"Allow","kind":"allow_once"},{"optionId":"deny","name":"Deny","kind":"reject_once"}]
```

The binary has the protobuf variants `RequestedInteraction_{AskQuestion,Elicitation,Permission,FilePermission,Mcp,Deploy,BrowserAction,OpenBrowserSetup,…}` and `sendToolAndInteractionUpdates`. The untested paths (browser setup, MCP auth, deploy) are the likely remaining sources of an `interaction_` id.

**Impact on PR #384.**
- The headline bug ("auto/full silently picks the first choice") is **unconfirmed**. No `interaction_` request occurred, and agy's choice questions go through `ask_question`.
- The `decision()` branch is harmless, because it only fires on the prefix with no `reject*` option. Keep it as a guard, or drop it until a capture exists.
- Correct the PR description: decisions don't come "as `session/request_permission` with an `interaction_` id". In practice they come as `elicitation/create`.
- The `fakeacp decide` fixture models traffic that was never observed. Label it a guess.

## 4. Subagents (`invoke_subagent`)

**One sub-agent.** "Use your invoke_subagent tool to launch one subagent that runs `echo child-step-ok`…"

```
t=2.962 < tool_call {"toolCallId":"call_8494…","name":"invoke_subagent","status":"in_progress","kind":"other","title":"Run invoke_subagent",
                     "rawInput":{"Model":"inherit","Prompt":"Run the shell command `echo child-step-ok` and report its output.","Role":"Command Runner","TypeName":"self"}}
t=2.986 < tool_call_update {"toolCallId":"call_8494…","name":"invoke_subagent","status":"completed",…}     ← 24 ms later; no content, no rawOutput
t=4.995 < agent_message_chunk "The subagent has been launched and I am waiting for its response."
t=6.087 < tool_call {"toolCallId":"49b4f6e1-…-1ffc46e0c461:call_3133…","name":"run_command","status":"pending","kind":"execute","title":"echo child-step-ok",…}
t=6.088 < {"id":"1","method":"session/request_permission","params":{"sessionId":"e6d0…"  ← parent id
              ,"toolCall":{"toolCallId":"49b4f6e1-…:call_3133…","name":"run_command",…},"options":[allow_once, reject_once as above]}}
t=6.127 < tool_call_update {"toolCallId":"49b4f6e1-…:call_3133…","status":"completed","rawOutput":{"output":"…child-step-ok\r\n","exit_code":0},"content":[…]}
t=7.19  < tool_call manage_subagents {"Action":"list"} … ; t=9.1 tool_call schedule {"DurationSeconds":2,"Prompt":"Check subagent status",…}
t=10.97 < agent_message_chunk "\n\nThe subagent finished running the command and reported: … child-step-ok …"
t=11.17 < {"id":4,"result":{"stopReason":"end_turn",…}}
```

**Two sub-agents** ("in a single invoke_subagent call, launch two…"):
- agy made **two** `invoke_subagent` calls, 30 ms apart. Each had a flat `rawInput` (`"Role":"Alpha Runner"…`, `"Role":"Beta Runner"…`) and each was `completed` within 30 ms.
- The children's `run_command` calls came interleaved on the parent session, as `5a433588-…:call_…` and `8b9d983b-…:call_…`. Two permission requests (`"id":"1"`, `"id":"2"`) were open at once.
- Each child also wrote a poem. **No child text was sent**; the poems reached the user only through the parent's summary. No `agent_thought_chunk` arrived in any run, from parent or child, so these runs say nothing about thoughts.
- The parent's last frame before the reply was a `usage_update` at 24.2 s, after its summary text (23.4–24.1 s). Its `session/prompt` reply came at **78.96 s**, after 55 s with no frames at all. That is consistent with the 1.5.15 "stay open across background wake-ups" change; no frame shows the cause.
- **Nothing arrived out of turn.** The client stayed connected 40 s (two sub-agents) and 45 s (one) after each reply, and no frame came in either window.
- Nothing on the wire maps a child id to its launch call. On disk, `~/.gemini/antigravity-cli/brain/<parentSessionId>/.system_generated/subagents/<childId>.json` holds it:
  ```json
  {"conversationId":"5a433588-…","subagentDescriptor":{"typeName":"self","role":"Alpha Runner"},"state":"SUBAGENT_STATE_ALIVE","spawnStepIndex":3,"workspaceUris":["file:///tmp/agy-live/repo","file:///private/tmp/agy-live/repo"]}
  ```
  The children were still `ALIVE` after their work was done.

**Impact on PR #383 (launch card).** Its guessed shape is wrong in three places:
1. **There is no `Subagents[]`.** `rawInput` is the single sub-agent's own `{Role, Prompt, TypeName, Model}`. With the PR as written, `launch()` finds no prompts and falls back to the title. The title is `"Run invoke_subagent"`, not `"invoke_subagent"`, so the card's description reads **"Run invoke_subagent"**.
   - Fix: read the top-level `Prompt`, and use `Role` (else `TypeName`) as the label.
   - Several sub-agents are several calls, so the join-with-`role:` branch is unnecessary. Keep `Subagents[]` only if wanted as a fallback for older versions.
2. **The title is `"Run invoke_subagent"`.** The "title only repeats the tool's name" check (`t.Title == t.Name`) never matches. Compare against `"Run " + t.Name` instead, or drop the check, since the prompt is always present.
3. **`completed` arrives at launch, with no output.** The card flips to done while the child is still working. This was one of the PR's listed unknowns, and it is now settled. The "never completes / background forever" risk is moot.

The PR author may also want to:
- leave the call un-done until the turn ends, or until child steps for it stop;
- at minimum, not present "done" as "finished".

Fix the `fakeacp launch` fixture too:
- title `"Run invoke_subagent"`;
- flat `rawInput`;
- `in_progress`, then `completed` at once with no content;
- the child's `run_command` as `"<childId>:call_…"` on the parent session.

**Impact on PR 6 (child progress).**
- **Child steps do stream over ACP**, so no `~/.gemini` polling is needed for tool-level progress.
  - They arrive on the **parent** `sessionId`, with no `_meta`, and only while the parent's prompt is open.
  - The only tag is the `toolCallId` prefix `"<childConversationId>:"`; splitting on the first `:` gives the child id.
  - Child permission requests carry the same prefix, on the parent session.
- Today pocketd would show child tool rows inline in the parent's transcript as ordinary rows, and approvals for them already work.
- **What's missing:**
  - Child text is not sent.
  - Nothing on the wire links a child id to its `invoke_subagent` call. With one launch at a time, attribute child ids to the most recent launch. With parallel launches, the only mapping is the on-disk descriptor's `role` matched against the call's `Role`, or the order the child ids first appear (unreliable).
- Recommendation: route `"<childId>:"`-prefixed calls into `kids`, keyed by child id, and attach each child to a launch card by `Role` via the descriptor. Without the descriptor, attach by first-appearance order. Flip `SubAgents` only once that grouping is in.

## 5. Extra folders

The `initialize` result advertises `"sessionCapabilities":{"resume":{},"close":{},"load":{},"list":{},"additionalDirectories":{}}`. The same object is repeated as `session_capabilities`.

Each run used `cwd:"/private/tmp/agy-live/repo"` and the extra folder `/private/tmp/agy-live/extra`, which holds only `EXTRA_MARKER_7731.txt`. The client answered every permission request **Deny**. Prompt: "Use your read_file tool (not the shell) to read /private/tmp/agy-live/extra/EXTRA_MARKER_7731.txt…".

| Run | Permission requests | Result |
|---|---|---|
| baseline | 1: `read_file`, `title:"File ../extra/EXTRA_MARKER_7731.txt"` → denied, call `failed` | not read |
| `session/new` `{"cwd":…,"mcpServers":[],"additionalDirectories":["/private/tmp/agy-live/extra"]}` | **0** | `read_file` `completed`; text `only-in-extra` |
| `agy acp --add-dir /private/tmp/agy-live/extra` | **0** | `read_file` `completed`; text `only-in-extra` |
| `session/resume` of the `additionalDirectories` session in a new process, **without** the field | **0** | `only-in-extra` |
| same resume, reading `FRESH_9902.txt`, created in the extra folder after the session was made and never read before | **0** | `read_file` `completed`; text `fresh-after-resume` |

With either option, the model also names the folder as a workspace folder when asked. Its own search used `find /private/tmp/agy-live/extra /private/tmp/agy-live/repo …`, while the baseline searched only the repo.

**Impact on PR 5 (extra folders).**
- Prefer the ACP-standard `additionalDirectories` on `session/new`, gated on `sessionCapabilities.additionalDirectories`. It is a per-session parameter and needs no change to `acpArgv`. That one process can then serve sessions with different folders is an inference; it was not tested.
- Keep `--add-dir` as the fallback for agents that lack the capability. It is process-scoped.
- agy remembers the folders across `session/resume`: a file new to the session was read with no request.
- Not tested:
  - whether `session/load` or `session/resume` with a *different* list adds or removes folders;
  - relative paths (pass absolute ones).

  Test the first before relying on it for editing a project's folders.
- The product prerequisite (§2.4: folders on `Project`) is unchanged.

## Aside: check 6 (`/usage`)

`agy --print /usage --output-format json` on this free-tier account returned `status:"SUCCESS"`, `num_turns:0` and `command.name:"usage"`. Its groups were:

```json
{"groups":[
 {"name":"Gemini Models","buckets":[{"id":"gemini-weekly","window":"weekly","remaining_fraction":0.8398,"reset_time":"2026-12-02T05:33:02Z",…}]},
 {"name":"Claude and GPT models","buckets":[{"id":"3p-weekly","window":"weekly","remaining_fraction":1,"reset_time":"1970-01-01T00:00:00Z"},
                                            {"id":"3p-5h","window":"5h","remaining_fraction":1,"reset_time":"1970-01-01T00:00:00Z"}]}]}
```

These runs spent about 16% of the weekly Gemini pool. Two facts matter for the quota PR:
- This tier has **no `gemini-5h` bucket**, so `Session` must be optional.
- An unused pool reports `reset_time` as the Unix epoch, which must be treated as "no reset".
