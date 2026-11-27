# Anywhere Cloud spike

Step 2 of `2026-11-25-anywhere-cloud.md`. The plan's goals for the design are aims until code tests them, and steps 3 to 5 wait on numbers. This spike builds the member rules, the encrypted channel and the relay as code to throw away, and tests each goal against a cloud that lies.

The code is `packages/cloudspike`. Nothing in pocketd, the desktop or iOS uses it. Steps 3 to 5 rewrite what they keep.

## What it built

- **Members** (`members/`). The member list is a chain of ops: `genesis`, `add`, `remove`. Each op holds the hash of the one before it and is signed with an Ed25519 key. Genesis is signed by the first member. Later ops are signed by a member already in the list. An `add` may instead be signed by the recovery key. Every device keeps a `View`, its own copy. A View only takes a longer log that starts with its own ops.
- **Channel** (`channel/`). TLS 1.3. Each end's certificate is its member key, self-signed, and each end checks the other's key against its View instead of a CA. A member also checks that the machine is the one it asked for. Once the handshake is done, each end sends the other its log. Then the stream carries pocketd's protocol unchanged.
- **Relay** (`relay/`). A Go WebSocket server. A machine holds one line open on `/host`. A member dials `/dial`. The relay names a session on the machine's line, the machine dials `/accept`, and the relay splices the two sockets. It counts the bytes per account (decision 22) and holds no key.
- **`spike` command**: `relay`, `demo` (a local port tunnelled through the relay to a TCP service on the machine) and `bench`.

## The goals

Each test plays the cloud as an attacker. `scripts/mutate.sh` turns off 19 checks one at a time, and a test fails for each. It doesn't cover the genesis checks in `Replay`.

| Goal | Holds? | Tests |
|---|---|---|
| Someone who controls the cloud can't reach a machine | Yes for a machine already in the list. **No for a new machine under decision 7 as written** (Findings 1), and not against a phone under decision 6 (Findings 2) | `TheCloudCantApproveADeviceWithItsOwnKey`, `…SwapTheKeyInAnApproval`, `…DropAnOpFromTheMiddle`, `AMachineRejectsAnAccountTheCloudMadeUp`, `ANewMachineRejectsAnApproval…`, `TheCodeIsUnknownUntilTheNewDeviceRevealsItsNonce`, `TheApproverRejectsANonce…`, `APhoneCanAddAPhoneButNotADesktopOrMachine`, `AMemberWontTalkToAMachineItDidNotAskFor`, relay `…WhichSeesOnlyCiphertext` |
| The cloud never learns the recovery code | Yes, if the code has enough entropy (Findings 4) | `TheRecoveryCodeHas130BitsAndNeverEntersTheLog`, `TheRecoveryCodeAddsADeviceWithNoApproval`, `TheRecoveryCodeRejectsAGenesisTheCloudMade…`, `TheRecoveryKeyCantRemoveAMember` |
| A reset gives its author no access to a machine in the old list | Yes | `AResetListIsAnotherAccountToAnOldMachine`, channel `AResetAuthorCantReach…` |
| A removed member can't reach a machine that heard of the removal | Yes | `AMachineDropsAMember…`, `AStaleLogFromTheCloudDoesNotUndoARemoval`, channel `ARemovedMemberCantReach…`, `TheNextMemberCarriesAHiddenRemoval…`, relay `ARemovedMemberCantReach…ThroughTheRelay` |

The plan asks how long a machine trusts a member list it hasn't refreshed. Findings 3 answers it.

## Findings

1. **A new machine must check the account, not only the approver.** Decision 7 has the user compare the new machine's code on the approving member. That stops the cloud swapping the new key. It doesn't stop the cloud making an account of its own, approving the machine there, and so becoming a member of it. The machine can't tell that approval from a real one. The fix is an 8-digit code that binds the new key to the account's root. The approver shows it, and the user confirms it **on the new machine**: typed into `pocketd login`, or shown on a screen and accepted there. Eight digits over the root and key alone aren't enough. The cloud knows both, so it can compute the real code and try fake roots until one gives the same code. That takes about 10^8 tries, roughly 17 minutes on a laptop. So the code also takes two nonces, as Bluetooth's numeric comparison does: `SAS(root, key, nd, na)`. The new device sends `Commit(nd)` with its key. The approver's `na` comes back with the log. Only then does the device reveal `nd`, and the approver checks it against the commitment. The cloud has to fix its fake root before it can compute any code, so each try matches one time in 10^8, and each try puts a code in front of the user. `nd` is used once. A failed or abandoned join starts again with a fresh `nd` and commitment: if a revealed `nd` were reused, the cloud could compute the real code and grind a root offline. The recovery code needs no SAS, because only the real account holds its public key.
2. **A phone could raise itself to an owner.** Decision 6 lets any member approve any new member, and decision 9 gives a phone fewer scopes than a desktop. A phone, or a cloud that has stolen one, could add a desktop member and then run `exec` through it. The spike's rule: a phone can add only a phone. A desktop, a machine or the recovery code can add anything.
3. **A hidden removal lasts until the next honest member connects.** Every channel starts with the two logs swapped inside TLS. The cloud can't drop that exchange without dropping the whole link. So the first honest member to reach a machine brings the removal, and the machine closes the removed member's open channels at once. A machine that only the removed member reaches never learns. That is the limit the plan names. The machine can't detect it, because a stale log looks the same as a current one. Recommendation: accept the limit, and on the phone show "last confirmed by <device>, <time>" for each machine. A timeout would lock a user out of their own machine whenever their other devices sleep.
4. **The recovery code needs 128 bits or more.** The cloud stores the public key the code derives. A short code falls to an offline search. The spike's code is 26 base32 characters (130 bits), shown in five groups of five. A shorter, friendlier code would need a slow KDF (Argon2id) in front. The recovery key also signs the genesis when the account starts. Without that signature the cloud could copy the public key into a genesis of its own, and a device that joins with the code would trust the fake account.
5. **Two ops on the same head fork the list.** The cloud orders writes, so this only happens if the cloud splits members on purpose. A View refuses a fork and keeps its own branch. Step 4 needs a rule to bring a forked device back. The simplest is re-approval.
6. **The relay doesn't check who opens a host line.** Anyone could register as machine H and receive H's dials. The channel catches this, because the member checks H's key, so the cost is a denial of service. Step 5 gates `/host` and `/dial` on the account's sign-in token and `plan`.
7. **The channel adds about one round trip.** The log exchange follows the TLS handshake. Logs are small: each op is about 300 bytes in JSON.

## Numbers

`scripts/bench.sh` ran a relay as its own process on an M-series Mac, sampled with `ps`. Each busy link carries the measured host link: 16.6 KB/s from machine to member and 4.6 KB/s back, in ten chunks a second. An idle machine holds only its relay line, with a ping every 30 s.

| Load | Relay RSS | Relay CPU (one core) |
|---|---|---|
| 1000 idle machines | 8.6 MB | 0.2% |
| 100 busy links | 26.9 MB | 7.1% |
| 500 busy links | 92.9 MB | 29.8% |
| 1000 busy links + 2000 idle machines | 148.8 MB | 44.1% |

That is about 150 KB of memory and 0.06% of a core per busy link. The relay served plain `ws://` on loopback, with the bench on the same Mac. So these figures leave out TLS termination for `wss://`, in the relay or in a proxy in front of it, and they leave out a real network. A guess for an Ampere A1 core, which is slower, is 0.1–0.15% per link, or several hundred busy links per OCPU. Measure that on the host before relying on it. Egress runs out first: 1000 busy links forward about 21 MB/s, which is about 55 TB a month. Oracle's free 10 TB covers about 180 links busy all day, and Railway at 0.05 USD per GB charges about 2.7 USD per link-month. Either way, cutting the link's own traffic (the plan's `2026-11-25-host-link-traffic.md`, not yet written) is worth more than tuning the relay.

End to end: `scripts/prove.sh` ran a real pocketd on a scratch `POCKET_HOME` and tunnelled its owner port through the relay with `spike demo`. A WebSocket `hello` with an owner token came back `hello.ok`. A bad token came back pocketd's own `not_paired`. pocketd's protocol passes unchanged.

## Not measured

- The phone's traffic, and how many hours a day a paying user spends on the relay. Both need real users.
- TLS termination for `wss://`, and the relay on Oracle or Railway hardware, and its behaviour across a deploy. A deploy drops every link, and each machine reconnects with backoff from 1 s to 1 min.
- Direct connections (decisions 14 and 15). The channel runs on any `net.Conn`, so a direct TCP path needs no change to it. Choosing between paths isn't built.

## Changes to the plan

- Decision 7: a committed two-nonce code, confirmed on the new member as well as on the approver (Findings 1).
- Decisions 6 and 9: a phone approves only phones (Findings 2).
- Decision 10: a recovery code of at least 128 bits (Findings 4).
- "One limit is known already": answer it with Findings 3.
- Step 4: a rule for a forked device (Findings 5). Step 5: gate the relay's endpoints on sign-in (Findings 6).
- Decisions 23, 25 and 26 and the cost section still assume Railway. Hosting (Railway, Oracle or Cloudflare) is still open, and the numbers above apply to each.
