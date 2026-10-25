# Research: push notifications and Live Activities without a cloud backend

Date: 2026-10-24. Scope: how the new native SwiftUI iOS app gets alert pushes and Dynamic Island / Live Activity (LA) updates when the only server is pocketd on the user's Mac, reached over Tailscale. Apple facts come from developer.apple.com DocC JSON (sources at the end). "Verify" marks claims I couldn't confirm from a primary source.

## 0. Recommendation (TL;DR)

1. **Run one tiny push relay for the app's bundle id (`dev.mingo.anywhere`, packages/app/app.json:12) that holds the `.p8`.** It stays stateless except for rate counters. pocketd never holds Apple credentials. pocketd only ever speaks the relay protocol; self-hosters who build their own app run the same relay binary with their own key and set `push.relay` (option b becomes "run your own relay", one code path).
2. **The relay can only push to tokens that a phone sealed itself.** The phone registers each APNs token (device, push-to-start, per-activity) with the relay and gets an opaque **sealed handle** plus a **per-handle secret**. It gives both to pocketd over the existing authenticated WebSocket. pocketd signs each send with HMAC. The handle fixes the token, the push type and the topic, so a leaked handle can only buzz that one phone and only with that one push type.
3. **Alert pushes are end-to-end encrypted.** The visible `aps.alert` is generic ("A session needs you"). The real content travels in `e`, AES-256-GCM under a key the phone generated and shared only with pocketd. A Notification Service Extension (NSE) decrypts it and rewrites the notification. This is the ntfy pattern, minus the network fetch.
4. **LA payloads can't be E2E-encrypted.** There is no NSE for `liveactivity`, because the widget extension renders `content-state` directly. Keep `content-state` to counts, statuses and timestamps, and have the widget read titles from the App Group cache the app wrote while connected.
5. **Use one Live Activity per paired Mac**, with `relevance-score` by urgency. Compact: status glyph plus a count. Trailing: `Text(timerInterval:)` while working, or "!" when Needs you. Expanded: the top Up next session plus Approve/Deny buttons (`LiveActivityIntent`, which runs in the app process and sends `permission.resolve` to pocketd over the tailnet).
6. **pocketd decides every push** (new `internal/push`), driven from `Registry.OnStatus` (packages/pocketd/cmd/pocketd/serve.go:124). It is Seen-aware, coalesced and debounced. The event kind `push{agent,alert,acted}` is already reserved in packages/pocketd/internal/events/events.go:22 and counted in events/stats.go:132.
7. **Host the relay on a small Go service (Fly.io or any VPS), not a Cloudflare Worker.** APNs requires HTTP/2, and Cloudflare's docs don't say outbound `fetch()` speaks HTTP/2 (spike it if Workers is preferred). The relay reuses the same Go code as pocketd.

## 1. What the repo has today

- **No push code anywhere.** `grep -ri apns|expo-notifications|liveactivit packages/` finds nothing. `packages/app/src/credentials.ts:10` has `pocket.deviceToken`, but that is pocketd's pairing token, not an APNs token. packages/app/package.json has no `expo-notifications`.
- **packages/web is a static landing page** (index.html, app.js with demo data, style.css). There is no backend, so the relay would be new infrastructure.
- **Prior plans assumed Expo.** PRD E11 (docs/orchestrators/product/00-prd.md:219-228; D3 at :392, D29 at :418) planned "Expo Push from pocketd" with `push.register {expoToken, prefs}`, cap `push.v1`, body "Needs you"/"Done"/"Failed" and title = project name. A native SwiftUI app makes the Expo route moot (see §3c). docs/orchestrators/research/08-zeron-mobile.md:472-490 has the competitor (Zeron) design: ES256 JWT cached 50 min, `apns-collapse-id = chatId[:64]`, `thread-id = chatId`, 24h expiry, dead-token pruning. Zeron has no actions, no badge and no seen-suppression. Open questions are at 08:613-616: Mac asleep, lock-screen privacy, Seen window.
- **Devices.** `devices.Device{ID, Name, Platform, Scopes, TokenHash, ...}` lives in packages/pocketd/internal/devices/devices.go:44-56 and is stored in devices.json. Scopes are `observe, drive, approve, spawn` (:27). `Revoke(id)` (:232) must also drop that device's push record. Pairing goes through packages/pocketd/internal/wsserver/pair.go:22-47 (`pair` → `PairOK{deviceId, token}`). A connected phone's principal is `peer.Principal{Kind: Device, Device: d.ID}` (packages/pocketd/internal/peer/scope.go:43-56).
- **Status hook.** `Registry.OnStatus func(id, provider, from, to string)` (packages/pocketd/internal/agent/agent.go:116-118) fires from `update` (agent.go:300-320) under the agent's lock, so it must stay short; enqueue only. Statuses: `working`, `needsYou`, `done` (+ `Failed` flag on the summary), `idle`, `closed` (agent.go:286-298, :349). Today serve.go:124-130 emits a `status` event and kicks the awake keeper.
- **Seen.** `agent.view {agentIds}` → `Registry.SetView(conn, ids)`. `agent.seen` → `MarkSeen` (wsserver.go:485-494, agent.go:211-245). A seen agent never becomes Done (`setSeen` clears `unseenEnd`, agent.go:497-504). Views drop on disconnect (`DropView`, wsserver.go:151). Keepalive is a 20s ping with a 10s timeout (wsserver.go:39-41, :187-196). A phone that suspends without sending `agent.view([])` stays "seen" for up to ~30s and would wrongly suppress a Done push.
- **Permission asks.** `proto.PermissionRequest{RequestID, AgentID, ToolName, Detail, Options []PermissionOption{ID,Label}, Feedback bool}` (packages/pocketd/internal/proto/proto.go:248-262). The answer is `permission.resolve {requestId, decision: allow|deny, option?, message?, answers?}` (proto/messages.go:138-140), which needs scope `Approve` (peer/check.go:28) and is handled at wsserver.go:472-480. That handler emits `answer{agent, principal, decision}`.
- **The desktop already has banner rules to mirror.** In packages/desktop/crates/pocket/src/desktop/alerts.rs:
  - `wants_banner` (:18-26): per-status toggles needs_you/failed/done plus `on_screen`.
  - Body is `"{project} · {worktree}\n{line}"`, with the line cut to 240 chars (:36-43).
  - The line is the ask, the reply's first paragraph, or the status label (:48-56).
  - Actions are `allow`/`deny` (:13-14, `answer_banner` :262-272).

  The desktop's notch island (packages/desktop/crates/pocket/src/notch.rs:14-60) shows `Ask{title, place, verb, subject, shell, feedback, questions}`. Use it as the model for the LA expanded view. Desktop notification settings live in the desktop store, so the phone's prefs must live in pocketd.
- **Mac awake.** `awake.Keeper` holds the Mac awake while `agent.Busy` (working/needsYou) and lingers after (packages/pocketd/internal/awake/awake.go:18-23; serve.go:131-133). Pushes therefore keep flowing while agents work. A sleeping Mac sends nothing, so the LA must go stale on its own (`stale-date`, §4.3).
- **Protocol.** `PROTOCOL_VERSION = 3` with caps such as `registry.v1` (packages/protocol/src/constants.ts:1-9). New messages go behind a cap `push.v1`.

## 2. Hard constraints from Apple

- **Token-based APNs.**
  - Hosts: `api.push.apple.com:443` and `api.sandbox.push.apple.com:443` (port 2197 also works). Requires HTTP/2 and TLS 1.2+.
  - JWT ES256 with header `{alg:"ES256", kid:<10-char key id>}` and claims `{iss:<team id>, iat}`. Refresh no more than once per 20 min and at least once per 60 min. A token older than 1h gets `403 ExpiredProviderToken`; refreshing too often gets `429 TooManyProviderTokenUpdates`.
  - Apple now has topic-specific and environment-specific keys, and a connection binds to the first key and topic it uses. Create a topic-scoped key per environment so a leak only exposes this app.
- **Payload limits.** Max 4096 bytes and the body can't be compressed. LA static plus dynamic data has a combined 4 KB cap.
- **Store-and-forward.** APNs stores only one notification per bundle id while the device is offline. A nonzero `apns-expiration` means "store and retry until then"; `0` means try once. Priority 5 and 1 may be delivered in bursts.
- **LA lifetime.** Active for at most 8 h, then it stays on the Lock Screen for up to 4 h more (12 h max). After `end` it lingers up to 4 h by default; `dismissal-date` overrides that.
- **LA start rules.** An app can start an LA only in the foreground, unless it uses a `LiveActivityIntent` or push-to-start (iOS 17.2+). Starting needs Activities permission, and the count of concurrent activities is device-dependent.
- **Background execution.**
  - `applicationDidEnterBackground` gets ~5 s; `beginBackgroundTask` extends that (`backgroundTimeRemaining`, typically ~30 s, verify).
  - After that the app is suspended and its WebSocket dies.
  - Background (`content-available`) pushes are throttled; Apple says not to send more than 2-3 per hour.
- **Extensions.**
  - The NSE gets ~30 s and only runs for alert pushes with `mutable-content: 1`.
  - A notification action launches the app in the background and calls `userNotificationCenter(_:didReceive:withCompletionHandler:)`.
  - LA and widget buttons are inactive on a locked device until the user authenticates.

## 3. Delivery designs compared

| | (a) Developer-run relay (recommended) | (b) User-supplied `.p8` in pocketd | (c) Third-party push SaaS |
|---|---|---|---|
| Works with App Store build | Yes | **No**: tokens are minted for the publisher's topic and only the publisher's team key can push them | Only if the SaaS holds the publisher's `.p8` |
| Secret on user's Mac | Per-handle HMAC secret only (scoped to one phone token) | The user's own `.p8` | SaaS API key (a team-wide secret, can't ship) |
| Live Activities | Yes (relay sets `liveactivity` headers) | Yes | OneSignal/Airship yes; Expo push: not documented |
| Content privacy | E2E for alerts; LA state minimal | Direct to Apple | SaaS sees everything unless E2E |
| Ops | One small service and a key | None for us | Vendor account, pricing, SDK |
| Precedents | ntfy upstream poll relay, Bitwarden push relay (installation id/key), Home Assistant relay (500/day/device, 429 + `resetsAt`) | Matrix Sygnal-style self-host | Expo, FCM, OneSignal |

- **(b)** is only viable for people who build and sign their own app with their own bundle id and team, which is a tiny audience. Serve them by making the relay URL configurable and shipping the relay binary (`pocketd-relay`), not with a second APNs code path in pocketd.
- **(c) Expo.** The default Expo push API needs no access token: anyone holding an `ExponentPushToken` can send. "Enhanced push security" adds an access token, which would have to ship in pocketd. The limit is 600 notifications/s per project. Expo needs the Expo SDK to mint tokens and documents no `liveactivity` support, so it doesn't fit a native SwiftUI app.
- **(c) FCM.** Needs the Firebase SDK, and sending needs a service-account credential, the same can't-ship secret.
- **(c) OneSignal and Airship.** These support LA, but their REST keys are team-wide.

**Verdict: (a).** Its abuse surface is small because a handle is unforgeable and bound to one token, push type and topic.

### 3.1 Relay protocol (exact formats)

Relay keys, held only by the relay:
- `K_seal`: 32 bytes, AES-256-GCM, for sealing handles.
- `K_mac`: 32 bytes, for deriving per-handle secrets.
- The APNs `.p8`: one per environment, topic-scoped.

The relay stores no token database. Only rate counters live in memory or KV.

**Register** (phone → relay, over the public internet; the phone has normal connectivity):

```
POST https://push.<domain>/v1/register
Content-Type: application/json

{"token":"<hex APNs token>","kind":"alert"|"activity"|"start","env":"production"|"development"}
```

`kind` selects the headers the relay will later use:
- `alert` → topic `dev.mingo.anywhere`, `apns-push-type: alert`.
- `activity` → topic `dev.mingo.anywhere.push-type.liveactivity`, `apns-push-type: liveactivity`; the payload must have `aps.event` set to `update` or `end`.
- `start` → the same topic and type; the payload must have `aps.event == "start"` and an `aps.alert`.

Response `200`:

```json
{"handle":"<b64url( nonce12 || AES-GCM(K_seal, {"v":1,"t":token,"k":kind,"e":env,"x":expUnix}) )>",
 "secret":"<b64url(HMAC-SHA256(K_mac, handle))>",
 "expiresAt":1764000000}
```

Notes:
- **Expiry.** Handles expire after 30 days (`x`). The phone re-registers on each `hello.ok` (cheap, idempotent) and re-sends the result to pocketd, which also handles APNs token rotation.
- **App Attest is optional hardening, not required.** A valid APNs token for our topic can only come from our app on a real device, and a handle only reaches that device. To stop handle minting by non-app clients, add `DCAppAttestService` assertions later.

**Send** (pocketd → relay):

```
POST https://push.<domain>/v1/send
Content-Type: application/json
X-Anywhere-Timestamp: 1761300000
X-Anywhere-Signature: b64url( HMAC-SHA256(secret, timestamp + "." + body) )

{"handle":"...","priority":10,"expiration":1761301800,"collapseId":"a3f9...","payload":{"aps":{...},"e":"..."}}
```

Relay checks, in order (fail fast):
1. `|now - ts| <= 300`.
2. `secret' = HMAC(K_mac, handle)`, then a constant-time compare of the signature.
3. Unseal the handle and check `x > now`.
4. The marshalled payload is ≤ 4096 bytes; otherwise `413`.
5. The payload shape matches `kind` (`aps.event` rules above). Reject `content-available` on `alert` handles, and reject `critical` sounds.
6. Rate limits. Per handle: alert 120/h (burst 10), activity 900/h, start 12/h. Plus a global per-source-IP limit. Over the limit returns `429 {"reason":"RateLimited","resetsAt":"<ISO8601 UTC>"}` (Home Assistant convention).
7. POST to APNs. `priority` must be 5 or 10 (alert/activity) and `collapseId` ≤ 64 bytes.

Responses:
- `200 {"apnsId":"<uuid>"}`.
- APNs `410 Unregistered/ExpiredToken`, `400 BadDeviceToken` or `DeviceTokenNotForTopic` are passed through as `410 {"reason":...}`. pocketd deletes that handle and never retries.
- APNs 429/5xx return `503 {"retryAfter":s}`. pocketd retries with backoff (Apple: retry 5xx after 15 min with backoff; 429 `TooManyRequests` with delay).

The relay logs only `apnsId`, kind, status and size, never payloads. Encrypted `e` and minimal LA state keep it blind to content.

**The relay → APNs request** (what the relay emits):

```
POST /3/device/<token>   (host api.push.apple.com, or api.sandbox.push.apple.com when env=development)
authorization: bearer <JWT, cached ~40 min>
apns-topic: dev.mingo.anywhere            | dev.mingo.anywhere.push-type.liveactivity
apns-push-type: alert                     | liveactivity
apns-priority: 10|5
apns-expiration: <unix>
apns-collapse-id: <≤64 bytes>             (alert only)
apns-id: <uuid v4 lowercase>
<payload JSON>
```

### 3.2 End-to-end encryption of alert payloads

- **Key.** At `push.register`, the phone generates `K_e2e`: 32 random bytes, one per paired Mac. It stores the key in the App Group keychain with `kSecAttrAccessibleAfterFirstUnlock` so the NSE can read it on a locked phone, then sends it to pocketd. pocketd stores it in `push.json` (0600, next to devices.json).
- **Encryption.** pocketd seals `Notice` JSON with AES-256-GCM: stdlib `crypto/aes` + `crypto/cipher`, no new dependency. Go's `gcm.Seal(nonce, nonce, plain, aad)` emits `nonce(12)||ciphertext||tag(16)`, which is exactly CryptoKit's `AES.GCM.SealedBox(combined:)` layout. Use `aad = macID`.
- **Visible payload.** It must say nothing: `aps.alert = {title:"Anywhere", body:"A session needs you"}`, `mutable-content:1`, `category`, `thread-id = b64url(HMAC(K_e2e,"thread:"+agentId))[:16]` (opaque), plus `e`.
- **Decryption.** The NSE replaces title, subtitle and body, keeps `threadIdentifier`, and sets `userInfo` (mac, agent, request), the badge and `targetContentIdentifier`.
- **Failure.** If decryption fails or times out, the generic text shows (the ntfy "New message" fallback).
- **Alternative (rejected as primary).** The NSE fetches details from pocketd over the tailnet (the ntfy poll model). That needs the Tailscale VPN to be up and adds latency, while E2E needs no network. Keep fetch as an enrichment option, for example longer reply text.

## 4. Live Activities

### 4.1 Tokens and lifecycle

- **Update token.** `Activity.request(attributes:content:pushType: .token)` yields one; observe it with `activity.pushTokenUpdates`. It is asynchronous, can change, and each new token must replace the old one on the server. When it rotates, the app gets background time.
- **Push-to-start token.** `Activity<MacActivity>.pushToStartTokenUpdates` (iOS 17.2+) works even if no activity was ever started. It is per app and attributes type, so every paired Mac gets the same one.
- **Starting by push.**
  - `aps.event:"start"` with `attributes-type`, `attributes`, `content-state` and a **required** `alert`.
  - On iOS 18+, add `"input-push-token": 1` to get an update token for the new activity. The system wakes the app with background time, the app gets `Activity.activityUpdates` / `pushTokenUpdates`, and it registers the token with the relay and pocketd.
  - Broadcast channels can't start activities and aren't needed here, since each LA is one person's.
- **Starting from the app.** The app can start the activity locally while in the foreground, with no alert. Inside a `LiveActivityIntent.perform()` it can start one from the background.
- **End.** `aps.event:"end"` with the final `content-state` and `"dismissal-date"` (past = remove now; within 4 h = custom).
- **Policy.**
  - (1) The app starts or keeps an LA for a Mac whenever it is foreground and that Mac has a Working or Needs you agent.
  - (2) While backgrounded, pocketd push-to-starts only on **entry to Needs you**, when no live activity exists for that Mac. That is the moment an alert is wanted anyway. Send the start with `alert` and no `sound`, and let the E2E alert push carry the sound and actions.
  - (3) It ends 10 min after nothing is Working or Needs you, with `dismissal-date = now+15m` and a final "All done · 3" state.
  - (4) At the 8 h cap the system ends it. pocketd sees a 410 or the phone reports `push.activity {ended}`, and the next Needs you push-to-starts again.
- **Multiple Macs.** Use one activity per Mac (`attributes.mac`). Separate daemons would otherwise clobber one shared `content-state`. Set `relevance-score`: Needs you 100, Failed 75, Working 50, done/idle 10. The most urgent Mac wins the Dynamic Island.

### 4.2 Payloads (exact)

Decoding gotcha: ActivityKit decodes `content-state` with the default `JSONDecoder` and ignores custom strategies. `Date` then decodes as seconds since 2001-01-01, not Unix time. Send timestamps as `Double` Unix seconds (`sinceUnix`) and build `Date(timeIntervalSince1970:)` in the view. This avoids an off-by-31-years bug (verify on device).

Update (priority 5 for Working-count churn, 10 for entering or leaving Needs you):

```json
{"aps":{"timestamp":1761300000,"event":"update","relevance-score":100,
  "stale-date":1761300300,
  "content-state":{"working":2,"needsYou":1,"unseen":1,
    "top":{"slot":"k7","status":"needsYou","sinceUnix":1761299950,"verb":"Run","subject":"npm test","answerable":true,"request":"r_8f2"}}}}
```

Start (iOS 18+):

```json
{"aps":{"timestamp":1761300000,"event":"start","input-push-token":1,
  "attributes-type":"MacActivity","attributes":{"mac":"m_3c1","macName":"Studio"},
  "content-state":{...as above...},
  "alert":{"title":"Anywhere","body":"A session needs you"},
  "relevance-score":100,"stale-date":1761300300}}
```

End:

```json
{"aps":{"timestamp":1761303000,"event":"end","dismissal-date":1761303900,"content-state":{"working":0,"needsYou":0,"unseen":3,"top":null}}}
```

Notes:
- `top.slot` is an opaque per-agent key that the widget maps to a title or place via App Group storage the app keeps fresh while connected. The fallback text is "A session".
- `verb` and `subject` are the only cleartext ask detail, and they should be opt-in (the 08:615 privacy question). Default: `verb` only ("Run", "Edit"), with `subject` omitted.
- Keep total payloads under 4 KB with margin: cap `subject` at 60 chars.

### 4.3 Budget and staleness

- **Budget.** Apple publishes no number, only "a certain budget per hour". Priority 10 counts against it and priority 5 doesn't. Mix them.
- **Frequent updates.** Set `NSSupportsLiveActivitiesFrequentUpdates = YES`. The user can turn it off, so read `ActivityAuthorizationInfo().frequentPushesEnabled` and `frequentPushEnablementUpdates` and report it to pocketd, which then slows down.
- **Coalescing rule in pocketd.**
  - At most one priority-5 update per 15 s per activity.
  - Priority 10 only on transitions into or out of Needs you, and on Failed.
  - Never push per streamed token. Elapsed time ticks locally via `Text(timerInterval:)` at no push cost.
- **Liveness.**
  - Each update sets `stale-date = now + 5 min`.
  - While anything is Working, pocketd sends a priority-5 heartbeat every 4 min to advance it.
  - If the Mac sleeps or loses network, the LA shows `context.isStale` → "Studio is offline".
  - APNs timestamps let the device drop out-of-order updates.
  - LA sends use `apns-expiration: now+300`.

### 4.4 Dynamic Island layout (what to show)

- **compactLeading:** a status glyph tinted by urgency (Needs you = accent, Working = animated dots, Failed = red, Done = check). With `isDynamicIslandLimitedInWidth`, fit it in a 28 pt circle.
- **compactTrailing:**
  - Needs you: "!" plus the count (`needsYou`).
  - Else, working > 0: `Text(timerInterval: since...Date.distantFuture, countsDown: false)` with a monospaced digit width (elapsed time of the longest working turn), or "×2".
  - Else: unseen count.
  - Return an empty view in limited width; the system hides it.
- **minimal:** a single glyph with a number badge (`needsYou` if > 0, else `working`).
- **expanded:**
  - `.leading`: provider icon plus the Mac name.
  - `.trailing`: timer or status.
  - `.center`: session title (from the cache) and "project · worktree".
  - `.bottom`: the ask ("Run · npm test") with **Allow / Deny** buttons when `answerable`; otherwise **Open**. Use `Link(destination: anywhere://mac/<mac>/agent/<slot>)` or `.widgetURL` for the whole activity.
  - Answerable means plain allow/deny with no `options` or `questions`. Asks with options or questions open the app.
- **Lock Screen:** the same content as the expanded view, plus "+N more".

### 4.5 Interactive Approve/Deny from the Island

- **Where it runs.** `struct AnswerIntent: LiveActivityIntent` runs in the **app process**: the system launches the app in the background. Buttons are `Button(intent: AnswerIntent(mac:, request:, decision:))` in the widget extension. The intent type must be in the app target, and in the widget target so the button compiles there.
- **perform()** must finish quickly:
  1. Load the Mac's host and device token from the keychain.
  2. Open the WebSocket over the tailnet and send `hello`.
  3. Send `permission.resolve {requestId, decision}`, then await `ack` or the `error` "Permission request is no longer open" (wsserver.go:475).
  4. Optimistically `Activity.update` locally to "Allowed ✓" before the network round trip, and roll back on error.
- **If the Tailscale VPN is down**, the connection fails. Update the LA to "Couldn't reach Studio", and let `openAppWhenRun`/`supportedModes: .foreground(.dynamic)` fall back to opening the app.
- **Faster path (optional).** A one-shot HTTP endpoint on pocketd, `POST /v1/resolve` with `Authorization: Bearer <device token>`, avoids the WS handshake. Measure first.
- **Locked phone.** Buttons are inactive until the user authenticates, which matches `approve`'s security posture.
- **Recording.** pocketd records `answer{principal:"device"}` and can set `push{acted:true}`.

## 5. Notifications

- **Categories**, registered at launch with `setNotificationCategories`:
  - `PERM`: Allow (`.authenticationRequired`), Deny (`[.destructive, .authenticationRequired]`).
  - `PERM_NOTE` (when `Feedback`): Allow, Deny, and "Deny with note…" (`UNTextInputNotificationAction`, which sends `message`).
  - `QUESTION`: Open only, because options need UI.
  - `DONE` and `FAILED`: "Reply…" (`UNTextInputNotificationAction` → `agent.prompt`), plus Open.
  - `AUTOMATION`: Open.
  - Categories are static, so dynamic `PermissionOption` labels can't be buttons; asks that have them use `QUESTION`.
- **Action handling.** `didReceive` runs with the app launched in the background. Wrap the work in `beginBackgroundTask`, connect to pocketd, send, then call `completionHandler`. Use the same `PocketLink.oneShot` as the intent.
- **The NSE** (`mutable-content:1`) decrypts `e` (§3.2) and sets:
  - `title` = session title, `subtitle` = "project · worktree", `body` = the ask, last reply line or "Failed".
  - `threadIdentifier` = opaque per-session id, so one session's alerts stack.
  - `summaryArgument` = Mac name.
  - `interruptionLevel`: comes from the payload, so the NSE needn't change it.
  - `relevanceScore`.
  - `targetContentIdentifier` = `agent:<id>` for iPad scenes.
  - `badge`: the sum of per-Mac counts the NSE keeps in the App Group. Each pocketd only knows its own Mac, so it sends `badgeForMac` inside `e` and the NSE sums the stored values.
  - On failure it returns `bestAttemptContent`.
- **Interruption levels.** Needs you → `time-sensitive` (entitlement `com.apple.developer.usernotifications.time-sensitive`; users who granted permission before the entitlement may need a reinstall or Settings toggle). Failed → `active`. Done → `active` with `relevance-score` 0.5. Automation runs → `passive` unless failed.
- **Collapse and replace.** `apns-collapse-id` = opaque agent key. A newer status for a session replaces its older banner instead of stacking. APNs offline storage keeps only the latest per bundle anyway.
- **Clearing stale banners.** When the app foregrounds and syncs, call `removeDeliveredNotifications(withIdentifiers:)` for agents no longer Needs you/Done. There is no reliable remote delete: background pushes are capped at 2-3/h.
- **Expiration.** Needs you: `now+1h`. Done/Failed: `now+6h`. A stale Needs you could arrive after it was answered elsewhere; the NSE can't know without network, which is acceptable because the tap opens the live state.
- **Deep links.** A tap with no action → `userInfo{mac, agent}` → the router opens `anywhere://mac/<mac>/agent/<agent>`. Hold the route until the WebSocket reconnects (PRD FR 11-5).
- **Foreground.** In `willPresent`, return `[]` if the open screen is that session, else `[.banner, .sound]` (or an in-app toast).
- **Why not local notifications from the live socket.**
  - Backgrounding gives about 5 s plus a `beginBackgroundTask` window (~30 s); then the socket dies.
  - BGAppRefresh is opportunistic.
  - Silent pushes are throttled to 2-3/h, so they can't keep a socket alive.
  - Every out-of-app alert must therefore be a remote push. Local notifications are only worth it for the few seconds after backgrounding, which isn't worth the duplication logic.
- **Seen on background.** On `scenePhase != .active`, send `agent.view([])` immediately inside a `beginBackgroundTask`. That clears Seen at once rather than after the 30 s keepalive and fixes the open question at 08:616.

## 6. What pocketd must track (new `internal/push`)

`~/<home>/push.json` (0600, atomicfile like devices.json):

```go
type Record struct {
	Device     string `json:"device"`          // devices.Device.ID
	Key        []byte `json:"key"`             // K_e2e
	Alert      Handle `json:"alert"`
	Start      Handle `json:"start,omitempty"`  // push-to-start
	Activity   *Live  `json:"activity,omitempty"`
	Prefs      Prefs  `json:"prefs"`           // needsYou, failed, done, automations, details(bool: show verb/subject)
	Frequent   bool   `json:"frequent"`        // frequentPushesEnabled
	UpdatedAt  int64  `json:"updatedAt"`
}
type Handle struct{ Handle, Secret string; ExpiresAt int64 }
type Live struct {
	ID string; Handle Handle; StartedAt int64
	LastSent int64; LastState []byte // for coalescing / no-op skip
}
```

**Wire (cap `push.v1`, scope Observe in peer/check.go):**
- `push.register {key, alert:{handle,secret,expiresAt}, start?, prefs, frequent}` → ack. Re-sent on every `hello.ok`.
- `push.activity {activityId, handle?, ended?}`.
- `push.prefs {prefs}`.
- `push.unregister`.
- `devices.Revoke` deletes the record.

**Triggers**, from `OnStatus` via a buffered channel (agent.go:116 forbids work under the lock):

| from→to | Push | Category | Level | Condition |
|---|---|---|---|---|
| *→needsYou | alert + LA (p10, may push-to-start) | PERM / PERM_NOTE / QUESTION | time-sensitive | not seen; pref on; debounce 1.5 s (auto-allow and desktop answers often resolve within it) |
| working→done, Failed=false | alert + LA p5 | DONE | active | not seen at send time; transition observed live by this process (not baseline/restore); debounce 3 s |
| working→done, Failed=true | alert + LA p10 | FAILED | active | same |
| needsYou→working | LA p10 (clear ask); cancel pending needsYou alert | — | — | — |
| idle→working / count change | LA p5, coalesced 15 s | — | — | only if an activity exists |
| →closed | LA p5 | — | — | — |
| automation run ends (automation/scheduler.go:118 `observe`) | alert | AUTOMATION | passive (failed: active) | pref `automations` |

**Suppression and dedupe.**
- **Seen.** Check `a.Summary()` at send time, after the debounce, and don't push if the agent is seen.
- **Foreground phone.** Skip the alert if the target device currently has a foreground connection (track `device → conns` in wsserver). The app shows it in-app instead.
- **Dedupe key.** `(agent, status, updatedAt)`, so the same transition never pushes twice. Restarting with 3 Done must give 0 pushes (PRD 11-1).
- **Collapse and ordering.** One `apns-collapse-id` per agent. Each device has its own send goroutine, so ordering holds per device.
- **Errors.** `410` → drop the handle (the alert, or the activity, then send `push.activity`-less). `503` → retry with backoff, up to 3 times.
- **Events.** `events.Event{Kind:"push", Agent, Alert:"needsYou"|"done"|"failed"|"automation"}`, plus a later `Acted` (already in the schema).
- **Ops.** `pocketd push status` lists devices, handle expiry and the last 30 sends (PRD 11-4).

## 7. Code sketches

### 7.1 Go: relay client and notifier in pocketd

```go
package push

import (
	"bytes"; "context"; "crypto/aes"; "crypto/cipher"; "crypto/hmac"; "crypto/rand"; "crypto/sha256"
	"encoding/base64"; "encoding/json"; "errors"; "net/http"; "strconv"; "time"
)

var ErrGone = errors.New("push: handle gone")

type Client struct {
	URL  string       // default https://push.<domain>; config push.relay overrides
	HTTP *http.Client // net/http negotiates HTTP/2 via ALPN; no x/net needed
}

type Send struct {
	Handle     string          `json:"handle"`
	Priority   int             `json:"priority"`
	Expiration int64           `json:"expiration,omitempty"`
	CollapseID string          `json:"collapseId,omitempty"`
	Payload    json.RawMessage `json:"payload"`
}

func (c *Client) Send(ctx context.Context, h Handle, s Send) error {
	s.Handle = h.Handle
	body, err := json.Marshal(s)
	if err != nil {
		return err
	}
	ts := strconv.FormatInt(time.Now().Unix(), 10)
	secret, _ := base64.RawURLEncoding.DecodeString(h.Secret)
	mac := hmac.New(sha256.New, secret)
	mac.Write([]byte(ts + "."))
	mac.Write(body)
	req, _ := http.NewRequestWithContext(ctx, http.MethodPost, c.URL+"/v1/send", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Anywhere-Timestamp", ts)
	req.Header.Set("X-Anywhere-Signature", base64.RawURLEncoding.EncodeToString(mac.Sum(nil)))
	res, err := c.HTTP.Do(req)
	if err != nil {
		return err
	}
	defer res.Body.Close()
	switch res.StatusCode {
	case http.StatusOK:
		return nil
	case http.StatusGone:
		return ErrGone
	default:
		return errors.New("push: relay " + res.Status)
	}
}

// Seal emits nonce||ciphertext||tag, CryptoKit's AES.GCM.SealedBox.combined layout.
func Seal(key []byte, aad string, v any) (string, error) {
	block, err := aes.NewCipher(key)
	if err != nil {
		return "", err
	}
	gcm, _ := cipher.NewGCM(block)
	nonce := make([]byte, gcm.NonceSize())
	rand.Read(nonce)
	plain, _ := json.Marshal(v)
	return base64.StdEncoding.EncodeToString(gcm.Seal(nonce, nonce, plain, []byte(aad))), nil
}

type Notice struct {
	Mac, Agent, Kind, Title, Place, Line, Request string
	Badge                                         int
}

var generic = map[string]string{"needsYou": "A session needs you", "done": "A session is done", "failed": "A session failed", "automation": "An automation finished"}

func AlertPayload(key []byte, n Notice, category, level, thread string) (json.RawMessage, error) {
	e, err := Seal(key, n.Mac, n)
	if err != nil {
		return nil, err
	}
	return json.Marshal(map[string]any{
		"aps": map[string]any{
			"alert":              map[string]string{"title": "Anywhere", "body": generic[n.Kind]},
			"mutable-content":    1,
			"category":           category,
			"thread-id":          thread,
			"interruption-level": level,
			"sound":              "default",
		},
		"e": e,
	})
}
```

Wiring: in serve.go, after line 124, `d.Agents.OnStatus` also does `notifier.Kick(id, from, to)`, a non-blocking send on a buffered channel. The `Notifier` goroutine debounces per agent with `time.AfterFunc`, re-reads `Agents.Get(id).Summary()` at fire time, checks `Seen` and prefs, builds the payloads and calls `Client.Send` per device record.

### 7.2 Go: relay APNs side (JWT gotcha)

ES256 JWT signatures must be raw `r||s` (64 bytes), not ASN.1 DER.

```go
func jwt(key *ecdsa.PrivateKey, keyID, teamID string, now time.Time) string {
	enc := base64.RawURLEncoding
	h := enc.EncodeToString([]byte(`{"alg":"ES256","kid":"` + keyID + `"}`))
	c := enc.EncodeToString([]byte(`{"iss":"` + teamID + `","iat":` + strconv.FormatInt(now.Unix(), 10) + `}`))
	sum := sha256.Sum256([]byte(h + "." + c))
	r, s, _ := ecdsa.Sign(rand.Reader, key, sum[:])
	sig := make([]byte, 64)
	r.FillBytes(sig[:32]); s.FillBytes(sig[32:])
	return h + "." + c + "." + enc.EncodeToString(sig)
}
// key: x509.ParsePKCS8PrivateKey(pem.Decode(p8).Bytes); cache token 40 min (Apple: refresh 20–60 min).
// One long-lived http.Client per APNs host; Transport{ForceAttemptHTTP2: true}; reuse connections for hours.
```

### 7.3 Swift: attributes, widget, intent, NSE

```swift
import ActivityKit

struct MacActivity: ActivityAttributes {
    let mac: String
    let macName: String
    struct ContentState: Codable, Hashable {
        var working: Int
        var needsYou: Int
        var unseen: Int
        var top: Top?
        struct Top: Codable, Hashable {
            var slot: String
            var status: String          // needsYou | working | failed | done
            var sinceUnix: Double       // Unix seconds; never a Date (default decoder = since 2001)
            var verb: String?
            var subject: String?
            var answerable: Bool
            var request: String?
            var since: Date { Date(timeIntervalSince1970: sinceUnix) }
        }
    }
}
```

```swift
import WidgetKit, SwiftUI, AppIntents

struct MacActivityWidget: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: MacActivity.self) { ctx in
            LockView(ctx: ctx)
                .widgetURL(URL(string: "anywhere://mac/\(ctx.attributes.mac)"))
        } dynamicIsland: { ctx in
            let s = ctx.state
            return DynamicIsland {
                DynamicIslandExpandedRegion(.leading) { Label(ctx.attributes.macName, systemImage: "desktopcomputer") }
                DynamicIslandExpandedRegion(.trailing) { Elapsed(top: s.top) }
                DynamicIslandExpandedRegion(.center) { Text(Titles.title(mac: ctx.attributes.mac, slot: s.top?.slot) ?? "A session").lineLimit(1) }
                DynamicIslandExpandedRegion(.bottom) {
                    if let t = s.top, t.answerable, let r = t.request {
                        Text([t.verb, t.subject].compactMap { $0 }.joined(separator: " · ")).lineLimit(1)
                        HStack {
                            Button(intent: AnswerIntent(mac: ctx.attributes.mac, request: r, decision: "deny")) { Text("Deny") }
                            Button(intent: AnswerIntent(mac: ctx.attributes.mac, request: r, decision: "allow")) { Text("Allow") }
                        }
                    } else if ctx.isStale {
                        Text("\(ctx.attributes.macName) is offline")
                    }
                }
            } compactLeading: {
                StatusGlyph(state: s)
            } compactTrailing: {
                if s.needsYou > 0 { Text("!\(s.needsYou)") }
                else if let t = s.top, t.status == "working" {
                    Text(timerInterval: t.since...Date.distantFuture, countsDown: false).monospacedDigit().frame(width: 44)
                } else { EmptyView() }
            } minimal: {
                StatusGlyph(state: s)
            }
        }
    }
}

struct AnswerIntent: LiveActivityIntent {           // runs in the app process
    static var title: LocalizedStringResource = "Answer"
    @Parameter(title: "Mac") var mac: String
    @Parameter(title: "Request") var request: String
    @Parameter(title: "Decision") var decision: String
    init() {}
    init(mac: String, request: String, decision: String) { self.mac = mac; self.request = request; self.decision = decision }
    func perform() async throws -> some IntentResult {
        await LiveActivities.markAnswering(mac: mac, decision: decision)   // optimistic Activity.update
        do { try await PocketLink.oneShot(mac: mac, .permissionResolve(requestId: request, decision: decision)) }
        catch { await LiveActivities.markFailed(mac: mac) }
        return .result()
    }
}
```

```swift
import UserNotifications, CryptoKit

final class NotificationService: UNNotificationServiceExtension {
    var handler: ((UNNotificationContent) -> Void)?
    var best: UNMutableNotificationContent?

    override func didReceive(_ request: UNNotificationRequest, withContentHandler h: @escaping (UNNotificationContent) -> Void) {
        handler = h
        best = request.content.mutableCopy() as? UNMutableNotificationContent
        guard let best, let e = best.userInfo["e"] as? String, let data = Data(base64Encoded: e),
              let n = try? Self.open(data) else { return h(request.content) }
        best.title = n.title
        best.subtitle = n.place
        best.body = n.line
        best.userInfo = ["mac": n.mac, "agent": n.agent, "request": n.request]
        best.targetContentIdentifier = "agent:\(n.agent)"
        best.badge = NSNumber(value: Badges.store(mac: n.mac, count: n.badge))   // sum across Macs, App Group
        h(best)
    }

    override func serviceExtensionTimeWillExpire() { if let best { handler?(best) } }

    static func open(_ data: Data) throws -> Notice {
        let box = try AES.GCM.SealedBox(combined: data)
        // aad = mac id: try each paired Mac's key (usually one); keys live in the App Group keychain.
        for (mac, key) in Keys.all() {
            if let plain = try? AES.GCM.open(box, using: key, authenticating: Data(mac.utf8)) {
                return try JSONDecoder().decode(Notice.self, from: plain)
            }
        }
        throw CocoaError(.coderInvalidValue)
    }
}
```

App side:
- At launch, register categories (§5).
- `UIApplication.registerForRemoteNotifications()`, then hex-encode `deviceToken` and `POST /v1/register kind=alert`.
- `Task { for await t in Activity<MacActivity>.pushToStartTokenUpdates { register(kind: "start") } }`.
- `Task { for await a in Activity<MacActivity>.activityUpdates { for await t in a.pushTokenUpdates { register(kind: "activity", activity: a.id) } } }`.
- Forward every resulting handle to each paired pocketd with `push.register` / `push.activity`.
- `env` is `development` for Xcode builds and `production` for TestFlight/App Store, read from the `aps-environment` entitlement.

## 8. Open questions and risks

1. **Paid Apple Developer Program is mandatory** for APNs and LA push (PRD Q2, 00-prd.md:437). The relay also needs a domain and hosting owned by the publisher.
2. **Cloudflare Workers and HTTP/2 to APNs are unverified.** Cloudflare docs don't state outbound h2. A Workers-port APNs lib exists (`@fivesheepco/cloudflare-apns2`) but is barely used. Recommend Go on Fly.io; otherwise spike Workers against the sandbox.
3. **LA privacy.** `content-state` is cleartext at the relay and Apple. The proposal keeps titles out, with verb/subject opt-in. Unverified alternative: decrypt a sealed string inside the widget view using the App Group keychain. The widget extension renders on push arrival, so this may work, but needs a device spike.
4. **Mac asleep or offline means no pushes.** The keeper holds the Mac awake only while busy. `stale-date` makes the LA say so. Zeron's cloud-side diffing isn't available to us.
5. **Push-to-start requires an alert,** so every remote start buzzes. Starting only on Needs you limits the noise. On iOS 17.2-17.x there's no `input-push-token`; the update token arrives through the app wake, which is the same flow.
6. **Intent and action handlers need the tailnet up.** Tailscale's iOS VPN must be connected (on-demand). Measure WS hello + resolve latency in the background; consider a one-shot HTTP resolve endpoint.
7. **Minimum iOS version.** Assumed iOS 18+ (`input-push-token`, `ActivityStyle`). Confirm the deployment target with the owner.
8. **Badges across multiple Macs** rely on NSE-side summation. A Mac whose count drops never pushes again, so badges reconcile on the next foreground sync. Accept or clear on sync.

## Sources

- Apple, Starting and updating Live Activities with ActivityKit push notifications: https://developer.apple.com/documentation/activitykit/starting-and-updating-live-activities-with-activitykit-push-notifications
- Apple, Displaying live data with Live Activities (8 h/12 h, 4 KB, compact/minimal/expanded, 28 pt, buttons, background start): https://developer.apple.com/documentation/activitykit/displaying-live-data-with-live-activities
- Apple, Activity.request(attributes:content:pushType:): https://developer.apple.com/documentation/activitykit/activity/request(attributes:content:pushtype:)
- Apple, pushToStartTokenUpdates: https://developer.apple.com/documentation/activitykit/activity/pushtostarttokenupdates
- Apple, NSSupportsLiveActivitiesFrequentUpdates: https://developer.apple.com/documentation/bundleresources/information-property-list/nssupportsliveactivitiesfrequentupdates
- Apple, LiveActivityIntent: https://developer.apple.com/documentation/appintents/liveactivityintent
- Apple, Adding interactivity to widgets and Live Activities (LiveActivityIntent runs in app process; locked device): https://developer.apple.com/documentation/widgetkit/adding-interactivity-to-widgets-and-live-activities
- Apple, AppIntent.supportedModes: https://developer.apple.com/documentation/appintents/appintent/supportedmodes
- Apple, Sending notification requests to APNs (hosts, headers, 4 KB, push types, best practices): https://developer.apple.com/documentation/usernotifications/sending-notification-requests-to-apns
- Apple, Establishing a token-based connection to APNs (ES256, 20–60 min, topic-specific keys): https://developer.apple.com/documentation/usernotifications/establishing-a-token-based-connection-to-apns
- Apple, Handling notification responses from APNs (410, retry rules): https://developer.apple.com/documentation/usernotifications/handling-notification-responses-from-apns
- Apple, Generating a remote notification (aps keys: thread-id, category, mutable-content, interruption-level, relevance-score, stale-date, dismissal-date): https://developer.apple.com/documentation/usernotifications/generating-a-remote-notification
- Apple, Modifying content in newly delivered notifications (~30 s): https://developer.apple.com/documentation/usernotifications/modifying-content-in-newly-delivered-notifications
- Apple, UNNotificationServiceExtension: https://developer.apple.com/documentation/usernotifications/unnotificationserviceextension
- Apple, Declaring your actionable notification types: https://developer.apple.com/documentation/usernotifications/declaring-your-actionable-notification-types
- Apple, UNNotificationActionOptions.authenticationRequired: https://developer.apple.com/documentation/usernotifications/unnotificationactionoptions/authenticationrequired
- Apple, UNNotificationInterruptionLevel.timeSensitive: https://developer.apple.com/documentation/usernotifications/unnotificationinterruptionlevel/timesensitive
- Apple, Pushing background updates to your app (2–3/h): https://developer.apple.com/documentation/usernotifications/pushing-background-updates-to-your-app
- Apple, Extending your app's background execution time (5 s, beginBackgroundTask): https://developer.apple.com/documentation/uikit/extending-your-app-s-background-execution-time
- Time-sensitive entitlement notes: https://help.pushwoosh.com/hc/en-us/articles/27979836066717-How-do-I-configure-and-use-Time-Sensitive-notifications-for-my-iOS-app , https://developer.apple.com/forums/thread/691183
- LA budget is undisclosed/dynamic: https://documentation.onesignal.com/docs/en/live-activities , https://developer.apple.com/forums/thread/731715
- ntfy iOS upstream poll relay: https://docs.ntfy.sh/config/ , https://docs.ntfy.sh/known-issues/
- Bitwarden push relay: https://bitwarden.com/help/configure-push-relay/
- Home Assistant relay rate limits: https://developers.home-assistant.io/docs/api/native-app-integration/notifications , https://companion.home-assistant.io/docs/notifications/notification-details
- Expo push (no token by default, 600/s): https://docs.expo.dev/push-notifications/sending-notifications/
- Cloudflare Workers protocols (no outbound HTTP/2 statement): https://developers.cloudflare.com/workers/reference/protocols/ ; Workers APNs lib: https://jsr.io/@fivesheepco/cloudflare-apns2
