# pushrelay

The push relay for Anywhere ([ADR 0007](../../docs/adr/0007-native-ios-app.md), [protocol](../../docs/mobile/protocol.md)). It holds the app's APNs key so pocketd never does, and pushes only to handles a phone sealed itself. It stores no tokens: a handle carries its token sealed under `SEAL_KEY`, and only rate counters live in memory.

## Routes

- `POST /v1/register {"token","kind":"alert"|"activity"|"start","env"}` → `{"handle","secret","expiresAt"}`. Handles open for 30 days; the phone registers again on every hello.
- `POST /v1/send` with `X-Anywhere-Timestamp` and `X-Anywhere-Signature: b64url(HMAC-SHA256(secret, ts + "." + body))`, body `{"handle","priority","expiration"?,"collapseId"?,"payload"}`.
  Checks, in order: timestamp within 300 s (401), signature (401, constant time), the handle opens and hasn't expired (410), payload ≤ 4096 bytes (413), payload fits the kind (400: `aps.event` update/end only on activity handles, start with an alert only on start handles, no `content-available` or critical sound), priority 5 or 10 and collapse id ≤ 64 bytes (400), then rate limits (429 `{"reason":"RateLimited","resetsAt"}`).
  APNs answers map to `200 {"apnsId"}`, `410 {"reason"}` when the token is gone (410, `BadDeviceToken`, `DeviceTokenNotForTopic`), `503 {"retryAfter"}` when APNs is busy (429, 5xx, a provider token it refused) and `502 {"reason"}` for anything else.
- `GET /healthz`.

Rate limits per handle: alert 120/h (burst 10), activity 900/h (burst 30), start 12/h (burst 3). Per address: 3600 sends/h (burst 120), 120 registers/h (burst 20).

The topic is `BUNDLE_ID` for alert handles and `BUNDLE_ID.push-type.liveactivity` for activity and start handles. Only alert pushes carry `apns-collapse-id`. The JWT is ES256 (raw r‖s), cached for 40 minutes; Apple wants a new one every 20 to 60.

Logs hold the kind, status, size and `apns-id` of each send, never a payload.

## Config

| Env | |
|---|---|
| `APNS_KEY_P8` | path to the `.p8` Apple issued (a topic-scoped key per environment is safest) |
| `APNS_KEY_ID`, `APNS_TEAM_ID` | the key's id and the team's |
| `BUNDLE_ID` | default `dev.mingo.anywhere` |
| `SEAL_KEY`, `MAC_KEY` | base64 of 32 random bytes each (`openssl rand -base64 32`). Changing either invalidates every handle |
| `LISTEN` | default `:8080` |
| `CLIENT_IP` | the header a proxy puts the caller's address in, `Fly-Client-IP` on Fly.io |

## Run

```sh
go test ./...
APNS_KEY_P8=AuthKey.p8 APNS_KEY_ID=… APNS_TEAM_ID=… SEAL_KEY=… MAC_KEY=… go run ./cmd/pushrelay
```

`fly.toml` is an example for Fly.io: put the keys in secrets, then `fly deploy`. Point a pocketd at another relay with `pocketd config set push.relay https://<host>`.
