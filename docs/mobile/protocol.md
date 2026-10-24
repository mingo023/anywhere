# Phone protocol additions (protocol 3)

The contract between pocketd (`packages/pocketd/internal/proto`, `internal/wsserver`) and the iOS app (`packages/ios/Packages/AnywhereKit/Sources/Wire`). Every addition is behind a cap negotiated in `hello`; a verb sent without its cap is `Malformed message`, as `locals.v1` does today. Conventions are the existing ones: every request has `id`; the reply echoes it as `ack`, a typed reply, or `error{id, message, code?}`; pushes have no `id`. Field names are camelCase. Optional fields are omitted, never `null`, unless stated.

Every verb is registered in `peer.Needs` with the scope given here, and appears in `peer/check_test.go`'s matrix, in golden files on both sides (`internal/proto/testdata/golden/{client,server}`) and in `packages/protocol`'s schema.

A **tree** is an absolute path that `worktree.Containing` (or the Locals store) places inside a registered project. Anything else is `error{code:"not_a_tree"}`. Relative `path`/`dir` fields are relative to the tree and may not escape it (`..`, symlinks out of the tree): `error{code:"outside_tree"}`.

## Fixes and timeline

- `ws:agent.btw` joins `peer.Needs` with scope `drive`.
- `agent.timeline {id, agentId, beforeSeq?, sinceSeq?, limit?}`. With `beforeSeq`, the reply holds the newest `limit` items with `seq < beforeSeq`, and `hasOlder` is exact: true only when an item older than the oldest returned exists. `beforeSeq` and `sinceSeq` together are malformed. No cap: an old pocketd ignores the field and the phone sees `hasOlder` as before.

## terminal.v1

| Verb | Scope |
|---|---|
| `terminal.list`, `terminal.attach`, `terminal.detach` | observe |
| `terminal.input`, `terminal.resize`, `terminal.create`, `terminal.close` | drive, and refused for every peer but the owner with `error{code:"terminals_off"}` when the owner setting `phone.terminals` is `off` (default `on`; set with `config.set {key:"phone.terminals", value:"on"|"off"}` from an owner, or `pocketd config set`) |

```
→ {type:"terminal.list", id}
← {type:"terminal.list", id?, terminals:[Terminal]}       // also pushed whole, without id, on spawn, exit and title change
Terminal = {id, cwd, cols, rows, title?, localId?, agentId?, exited?:bool}
   // title = the foreground process text; agentId = the agent running in it now, if any

→ {type:"terminal.attach", id, terminalId}
← {type:"terminal.attached", id, terminalId, cols, rows}
   then one binary SNAPSHOT frame, then binary OUTPUT frames, in PTY order
← {type:"terminal.resized", terminalId, cols, rows}         // push to attached conns
← {type:"terminal.exited", terminalId, code}                // push to attached conns; the attach ends
→ {type:"terminal.detach", id, terminalId}  ← ack           // a closed socket detaches everything

→ {type:"terminal.input", id, terminalId, data}             // data = base64 bytes; no reply unless refused
→ {type:"terminal.resize", id, terminalId, cols, rows}  ← ack   // cols and rows 1..1000
→ {type:"terminal.create", id, requestId, cwd?, localId?, cols?, rows?}
← {type:"terminal.created", id, requestId, terminalId}      // a login shell, like the desktop's; cwd must be a tree
→ {type:"terminal.close", id, terminalId}  ← ack
```

Binary frames (WebSocket binary opcode, server → phone only):

```
byte 0      kind: 1 = SNAPSHOT (replace the screen: reset, then feed), 2 = OUTPUT
byte 1      n = length of the terminal id
bytes 2..   terminal id, printable ASCII, n bytes (1..255)
rest        payload bytes, fed to the terminal emulator as-is
```

Each attach has its own bounded queue (512 KiB) drained by a writer goroutine that coalesces output (≤16 ms or 32 KiB per frame). The PTY pump never blocks on a phone. On overflow the queue is dropped and a fresh SNAPSHOT is sent. A phone renders at the terminal's own size by default; `terminal.resize` is an explicit "fit to phone".

## workspace.v1

| Verb | Scope |
|---|---|
| `branch.list` | observe |
| `worktree.create`, `worktree.delete` | spawn |
| `local.create/rename/delete` | owner, as before |
| `account.list` | observe |
| `agent.account` | drive |

```
→ {type:"worktree.create", id, requestId, project, name, base?|branch?|pr?:number, setup?:bool}
   // at most one of base, branch, pr; name may be "" only with branch or pr
← {type:"worktree.progress", id, requestId, step, note?}*   // steps as agent.progress: prepare, verify, fetch, worktree, copy, setup
← {type:"worktree.created", id, requestId, path, branch}
→ {type:"worktree.delete", id, path, deleteBranch?:bool}  ← ack   // closes its terminals; the main worktree is refused (code "main_worktree")

→ {type:"branch.list", id, tree}
← {type:"branch.list", id, current?, local:[{name, at}], remote:[{name, at}]}   // at = unix seconds; newest first; ≤200 each

← {type:"project.list", projects}       // pushed without id to workspace.v1 conns whenever the list changes

→ {type:"account.list", id, provider?}
← {type:"account.list", id, accounts:[Account]}
Account = {id, provider, name, plan?, email?, signedIn, isDefault?, fiveHour?:Usage, week?:Usage}
Usage = {pct, resetsAt?}                 // pct 0..100, resetsAt unix seconds
→ {type:"agent.account", id, agentId, account}  ← ack        // switch a chat-only agent's account
```

## git.v1

| Verb | Scope |
|---|---|
| `git.status`, `git.watch`, `git.diff`, `git.log`, `git.commit` | observe |
| `git.stage`, `git.discard`, `git.commit.create`, `git.push`, `git.pull` | drive |

```
→ {type:"git.status", id, tree, cursor?:int, limit?:int}     // limit default 50, max 5000, max 2000; cursor = file offset
← {type:"git.status", id, tree, rev, branch?, detached?:bool, upstream?, base?, ahead, behind, unpushed?,
   totals:{files, added, removed, staged},
   files:[{path, oldPath?, status, staged, unstaged, added?, removed?, binary?}],
   commits:[{sha, subject}],                                  // ≤20 since base
   nextCursor?:int}
   // status ∈ "M" "A" "D" "R" "U" (conflict) "?" (untracked). Files sorted by path.
   // rev = opaque stamp of the porcelain output. added/removed omitted for untracked files.

→ {type:"git.watch", id, trees:[tree]}  ← ack                  // replaces this conn's watch set; [] stops
← {type:"git.changed", tree, rev}                              // push when a watched tree's rev moves (polled ~2 s)
← {type:"gh.changed", tree}                                    // push when a watched tree's PR result changes (polled ~30 s)

→ {type:"git.diff", id, tree, path, at, context?:int, ignoreWhitespace?:bool, maxLines?:int}
   at = {kind:"working"} | {kind:"staged"} | {kind:"commit", rev} | {kind:"base", rev?}
← {type:"git.diff", id, tree, path, oldPath?, language, added, removed,
   binary?:bool, tooLarge?:bool, image?:bool,
   lines:[{k, o?, n?, t, w?:[[start,end]], cut?:bool}],
   truncated?:{shown, total}}
   // k ∈ "h" hunk header, "c" context, "a" added, "d" deleted. o/n = old/new line numbers.
   // w = word-level changed ranges as UTF-8 byte offsets into t, only on equal del/add runs of ≤5 lines.
   // t is capped at 1024 chars (cut:true). maxLines default 4000.
   // language = a lowercase id: rust, typescript, tsx, javascript, python, go, swift, kotlin, java, json,
   //   markdown, toml, yaml, bash, css, html, text.

→ {type:"git.log", id, tree, all?:bool, skip?:int, limit?:int}  // limit default 50, max 200
← {type:"git.log", id, tips:{head?, branch?, upstream?, base?}, commits:[{sha, parents, author, at, subject}], more}   // at = unix seconds

→ {type:"git.commit", id, tree, sha}
← {type:"git.commit", id, sha, subject, body?, author, at, files:[{path, oldPath?, status, added?, removed?}]}

→ {type:"git.stage", id, tree, paths:[…], staged:bool}  ← ack     // paths non-empty, each non-empty
→ {type:"git.discard", id, tree, paths:[…]}  ← ack
→ {type:"git.commit.create", id, tree, message, amend?:bool, all?:bool}
← {type:"git.committed", id, sha}
→ {type:"git.push", id, tree, lease?:bool}  ← ack
→ {type:"git.pull", id, tree}  ← ack                           // pull --ff-only
   // failures: error{code:"git_failed", message:<first stderr line>, detail?:<stderr>}
```

## gh.v1

| Verb | Scope |
|---|---|
| `gh.status`, `gh.pr`, `gh.prs` | observe |
| `gh.pr.create`, `gh.pr.merge`, `gh.pr.ready`, `gh.thread.reply`, `gh.thread.resolve` | drive |

```
→ {type:"gh.status", id, tree?}
← {type:"gh.status", id, state:"missing"|"signedOut"|"ready", login?}

→ {type:"gh.pr", id, tree}
← {type:"gh.pr", id, tree, problem?:"missing"|"signedOut", pr?:PR}      // no pr and no problem = no PR for the branch
PR = {number, title, url, state:"OPEN"|"CLOSED"|"MERGED", draft, base, head,
      checks:{passed, failed, pending}, runs:[{name, outcome:"passed"|"failed"|"pending"|"skipped", url?}],
      review?:"APPROVED"|"CHANGES_REQUESTED"|"REVIEW_REQUIRED", reviews:[{login, state}], requested:[login],
      mergeable?:"MERGEABLE"|"CONFLICTING"|"UNKNOWN", gate,
      threads:[{id, path, line?, left?:bool, outdated, resolved, comments:[{author, body, at, hunk?}]}]}   // at = unix seconds
   // gate ∈ done draft conflicts checksFailed checksRunning changesRequested reviewRequired checking blocked ready
   //   (Pr::gate in packages/desktop/crates/git/src/github.rs)

→ {type:"gh.prs", id, project}
← {type:"gh.prs", id, items:[{tree, number, title, state, draft, gate, checks:{passed, failed, pending}}]}

→ {type:"gh.pr.create", id, tree, title, body, draft?:bool, base?}  ← {type:"gh.pr.created", id, number, url}
→ {type:"gh.pr.merge", id, tree, number, method:"squash"|"merge"|"rebase"}  ← ack
→ {type:"gh.pr.ready", id, tree, number}  ← ack
→ {type:"gh.thread.reply", id, tree, thread, body}  ← ack
→ {type:"gh.thread.resolve", id, tree, thread, resolved:bool}  ← ack
   // failures: error{code:"gh_failed"|"gh_missing"|"gh_signed_out", message, detail?}
```

- `head` is the PR's head commit oid; `reviews[].state` is GitHub's own (APPROVED, CHANGES_REQUESTED, COMMENTED, DISMISSED, PENDING); `hunk` is the last 4 lines of the comment's diff hunk without its `@@` header, all the desktop shows (`hunk_tail`).
- `gh.pr.create` pushes the branch first, as the desktop does (`git -c push.autoSetupRemote=true push`); a failed push is `error{code:"git_failed", message, detail}` and opens nothing. Without `base`, gh opens it into the repo's default branch.
- `gh.prs` lists the cached PRs of the project's worktrees; a project not registered is `error{code:"unknown_project"}`.
- pocketd polls while any gh.v1 conn is connected: one gh at a time, killed after 60 s; the trees conns name in `git.watch` every 30 s, every other non-main, non-detached worktree every 5 min; a missing or signed-out gh is asked again after 5 min. `gh.pr` answers from that cache while it is under 30 s old, else asks gh then.

## fs.v1

| Verb | Scope |
|---|---|
| `fs.list`, `fs.read`, `fs.search` | observe |
| `attach.put` | drive |

```
→ {type:"fs.list", id, tree, dir}                               // dir "" = the tree's root
← {type:"fs.list", id, tree, dir, entries:[{name, dir:bool, link?:bool, size, mtime, ignored?:bool, git?}]}
   // folders first, then by name; git = the status letter of a changed file, or the shared letter of a folder's changes
   // mtime = unix ms here and in fs.read, as hostop.List

→ {type:"fs.read", id, tree, path, max?:int}                    // max default 524288 for text
← {type:"fs.read", id, tree, path, kind:"text"|"image"|"binary"|"tooLarge"|"unreadable", size, mtime,
   text?, truncated?:bool, language?, mime?, data?}             // data = base64 image bytes (≤20 MB)

→ {type:"fs.search", id, tree, query, limit?:int}               // limit default 50
← {type:"fs.search", id, paths:[path]}                          // fuzzy over git ls-files --cached --others --exclude-standard

→ {type:"attach.put", id, uploadId, name, index, data, last:bool}   // data = base64, ≤512 KiB decoded per chunk, ≤20 MB total
   // uploadId is 1..64 of A-Z a-z 0-9 _ -, as a localId
← ack per chunk; on last: {type:"attach.done", id, uploadId, path}  // path under <home>/attachments, as the desktop's pasted images
   // png jpg jpeg gif webp heic only; code "attach_invalid" otherwise
```

## push.v1

All `observe`. Records live in `<home>/push.json` (0600) keyed by device id; revoking a device drops its record.

```
→ {type:"push.register", id, key, env:"development"|"production", alert:Handle, start?:Handle, prefs:Prefs, frequent?:bool}  ← ack
   key = base64 32-byte AES-256-GCM key for sealing alert content
Handle = {handle, secret, expiresAt}       // from the relay's /v1/register
Prefs = {needsYou, done, failed, automations, details}   // booleans; details = show the ask's verb/subject on the Live Activity
→ {type:"push.activity", id, activityId, handle?:Handle, ended?:bool}  ← ack   // one Live Activity per Mac
→ {type:"push.prefs", id, prefs}  ← ack
→ {type:"push.unregister", id}  ← ack
→ {type:"push.test", id}  ← ack            // sends a test alert to this device
```

pocketd config `push.relay` (default `https://push.getanywhere.dev`) is the relay base URL: an https URL, or http to loopback for a relay under test. The owner sets it with `pocketd config set push.relay <url>` or `config.set {key:"push.relay", value}`; anything else is `error{code:"invalid_config"}`.

### Relay (`packages/pushrelay`)

```
POST /v1/register  {"token":"<hex>","kind":"alert"|"activity"|"start","env":"development"|"production"}
  → 200 {"handle","secret","expiresAt"}
POST /v1/send      headers X-Anywhere-Timestamp: <unix>, X-Anywhere-Signature: b64url(HMAC-SHA256(secret, ts + "." + body))
  {"handle","priority":5|10,"expiration"?,"collapseId"?,"payload":{…}}
  → 200 {"apnsId"} | 410 {"reason"} (drop the handle) | 413 | 429 {"reason","resetsAt"} | 503 {"retryAfter"}
```

`handle` = b64url(nonce12 ‖ AES-GCM(K_seal, {"v":1,"t":token,"k":kind,"e":env,"x":expiresAt})); `secret` = b64url(HMAC-SHA256(K_mac, handle)). The relay checks the timestamp (±300 s), the signature, the handle's expiry, the 4096-byte payload cap, that the payload fits the kind (`aps.event` start/update/end only for activity/start kinds), and per-handle rate limits; then posts to APNs with a cached ES256 JWT. Topic `<bundle>` for alert, `<bundle>.push-type.liveactivity` for activity and start. Bundle id `dev.mingo.anywhere`.

### Alert payload (pocketd → relay)

```json
{"aps":{"alert":{"title":"Anywhere","body":"A session needs you"},"mutable-content":1,"category":"PERM",
        "thread-id":"<opaque>","interruption-level":"time-sensitive","sound":"default"},
 "e":"<base64 nonce‖ciphertext‖tag, aad = serverId>"}
```

`e` opens to `Notice = {mac, agent, kind:"needsYou"|"done"|"failed"|"automation"|"test", title, place, line, request?, badge}` (mac = the `hello.ok` serverId). Categories: `PERM` (Allow, Deny), `PERM_NOTE` (Allow, Deny, Deny with note), `QUESTION` (Open), `DONE` and `FAILED` (Reply), `AUTOMATION`. Levels: needsYou time-sensitive, others active, automation passive unless failed. `apns-collapse-id` = opaque per agent.

Triggers: →needsYou (debounce 1.5 s), working→done (3 s, Failed picks FAILED), automation run end. Nothing is pushed for an agent that is Seen at send time, to a device with a foreground connection, or twice for one `(agent, status, updatedAt)`.

### Live Activity

`ActivityAttributes` type name `MacActivity`: `{mac, macName}`; `ContentState = {working, needsYou, unseen, top?:{slot, status, sinceUnix, verb?, subject?, answerable, request?}}`. `slot` = agent id; `sinceUnix` = Unix seconds as a Double (ActivityKit decodes `Date` from 2001). pocketd sends `update` priority 5 coalesced to ≤1 per 15 s, priority 10 on entering or leaving Needs you or on Failed, a heartbeat every 4 min while anything works, `stale-date = now+300`, and `end` with `dismissal-date = now+900` ten minutes after nothing works or needs you. With a `start` handle and no live activity, entering Needs you sends `event:"start"` with `input-push-token:1`.

## Deep links

`anywhere://pair?v=1&h=…&c=…&n=…` (pairing, unchanged), `anywhere://mac/<serverId>`, `anywhere://mac/<serverId>/agent/<agentId>`.
