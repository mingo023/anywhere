# 03 — Phone UX spec (iPhone, `packages/app`)

Date: 2026-09-30. Inputs: R05, R07, R08, R13, R14, R16, R17, R18, R19 (`docs/orchestrators/research/`), `CONTEXT.md`, PO decisions D1–D13, Zeron iOS screenshots. Pocket @ 5bc8ea8; main is now `f8f7293` (`app/`, `pd/`, `proto/` unchanged; 05-roadmap §0).

**Overrides.** Where this spec disagrees with 00-prd.md or 05-roadmap.md, the 05-roadmap §7.1 Spec overrides table wins (D42). Plans copy its rows verbatim. The phases in §10 are replaced by the epics in 05-roadmap (E01, E02, E06, E10, E11, E12, E16 PR4, E17).

**Legend**
- `[R08 §F8]` = finding F8 in report 08; `[R08 §S4]` = its UI spec item S4; `[R08 idea 08-12]` = idea 08-12.
- `[P path:L]` = Pocket code. `app/` = `packages/app/src/`, `pd/` = `packages/pocketd/internal/`, `proto/` = `packages/protocol/src/`.
- **new** = a choice made here, not taken from a report. Each one is listed in §12.
- Terms follow CONTEXT.md. UI copy says **Session** where the code says agent (the code keeps its names). Needs you / Working / Done / Idle / Failed / Seen / Attached are used exactly as defined there.
- Scope: iPhone portrait only (D13). iPad is not designed here; content is capped at 768 pt wide so the app doesn't break if it runs on one [R08 §S4].

---

## 1. Principles

1. **Status first.** A glance at the list answers "does anything need me?". Order: Needs you > Failed > Done > Working > Idle [P app/status.ts:9-25; CONTEXT.md §Status].
2. **Stable list, urgent inbox.** The main list never re-sorts on a status change; only the Up next section sorts by urgency (D2).
3. **Never lose a prompt.** Drafts survive navigation. Sends survive blips. A failure is shown on the bubble, never dropped silently. Today sends are a silent no-op offline [P app/client.ts:57-61], and the draft dies on unmount [P app/components/Composer.tsx:16] [R08 idea 08-8, idea 08-9].
4. **Hide fast, show slow.** Blips under 4 s never show. Failures show after 120 s [R08 §F12].
5. **Never stop by accident.** Return never stops. Stop is offered only while Working with an empty field (D12) [R05 idea 05-2].
6. **Answer where you are.** Approvals sit inline, in place of the composer, with every open request kept [R08 idea 08-13].
7. **No dead ends.** No button without a handler. Today Review changes, Open raw terminal, Attach and Dictate do nothing [P app/screens/ChatScreen.tsx:147-159; P app/components/Composer.tsx:28-46] [R14 idea 14-17]. Hide a control until its phase ships.
8. **Safe by construction.** Starting sessions and typing into terminals ship only after R18 pairing, device tokens and scopes (D10). The lock screen shows fixed copy [R18 §F6 R12].
9. **One visual language with the desktop.** Same accent, same status colours (D1), and light + dark from one token set [R07 §F17, idea 07-12].
10. **Accessible by default.** Status is always a word, not only a colour. Dynamic Type, VoiceOver labels and Reduce Motion work in every screen (§10).

---

## 2. Tokens (one set; replaces `design.ts` `d` and `theme.ts`)

Today there are two sets: `d` (chat, composer, timeline) [P app/design.ts:1-26] and `theme` (Connect, Agents, PermissionSheet) [P app/theme.ts:1-14]. [R14 idea 14-18; R07 idea 07-12].

**Implementation**
- `design.ts` exports `palette = { light, dark }` and `useTokens()`, which picks one with RN `useColorScheme()` [R07 §F17].
- `theme.ts` is deleted.
- Light mode needs all of these:
  - `app.json` `"userInterfaceStyle": "automatic"` (today `"dark"`, [P packages/app/app.json:8]);
  - the committed `ios/Anywhere/Info.plist` `UIUserInterfaceStyle` set to `Automatic` (today `Dark`, [P packages/app/ios/Anywhere/Info.plist:93-94]), because `ios/` is committed and nothing regenerates it;
  - `StatusBar style="auto"` (today `"light"`, [P app/App.tsx:47]);
  - `Glass` takes the scheme instead of the hard-coded `colorScheme="dark"` / `tint="dark"` [P app/components/Glass.tsx:19,26].
- There is no in-app appearance override; the app follows iOS (**new**; R07 proposes System/Light/Dark [R07 §UI spec "Appearance setting"]).

### 2.1 Colour

α = alpha applied to the base colour. Sources are R07 §F14 (Zeron iOS palette) and §F17 (phone mapping) unless noted.

| Token | Use | Light | Dark | Source | Replaces |
|---|---|---|---|---|---|
| `bg` | screen | `#F3F3F5` | `#060606` | background | `d.bg`, `theme.bg` |
| `surface` | sheets, pressed row, cards | `#FFFFFF` | `#111113` | elevated | `d.card`, `theme.surface` |
| `chip` | chips, option rows, fields | `#E7E7EB` | `#1C1C20` | chip | `d.chip`, `theme.surfaceAlt` |
| `code` | code blocks, diff body | `#FAFAFB` | `#0B0B0D` | codeBackground | `d.code` |
| `rule` | separators | `#E2E2E6` | `#1E1E22` | hairline | `d.rule`, `theme.border` |
| `stroke` | card and code borders | `#E4E4E8` | `#1F1F23` | codeBorder | `d.cardRule`, `d.stroke` |
| `text` | titles, primary text | `#27272C` | `#E8E8EA` | text | `d.text`, `theme.text` |
| `body` | assistant prose | `#3F3F46` | `#DCDCE0` | inlineCodeText | `d.body` |
| `muted` | meta, secondary text | `#62626A` | `#A9A9AE` | secondary | `d.muted`, `theme.muted` |
| `faint` | Idle, placeholders, timestamps | `#797981` | `#85858A` | textFaint | `d.faint` |
| `tertiary` | line numbers, decoration only | `#97979F` | `#6B6B72` | tertiary | `d.lineNo` |
| `accent` | Working, Send/Queue, links, focus | `#5B43E8` | `#8B7CF6` | accent (= desktop ACCENT [R07 desktop map]) | `theme.accent` |
| `accentSoft` | chosen option, selection | accent α0.12 | accent α0.12 | accentSoft | — |
| `onAccent` | glyph on an accent fill | `#FFFFFF` | `#000000` | on_accent; white on `#8B7CF6` is 3.33 [R07 §F3] | — |
| `controlFill` | disabled button, quiet fills | `#27272C` α0.075 | `#E8E8EA` α0.075 | controlFill | — |
| `warning` | Needs you glyph, fills | `#A16207` | `#FACC15` | warning (= desktop WAITING) | `theme.warn` |
| `warningText` | "Needs you" label text | `#855612` | `#FACC15` | desktop WAITING_TEXT [UXD §8] | — |
| `success` | Done, additions | `#15803D` | `#34D399` | success | `d.green`, `theme.ok` |
| `danger` | Failed, deletions, destructive | `#DC2626` | `#F87171` | danger | `d.red`, `theme.error` |
| `userBubble` | user prompt bubble; its text uses `text` | `#FFFFFF` | `#19191C` | userBubble | `d.bubble`, `d.bubbleText`, `d.greenChip` |
| `addBg` / `delBg` | diff row wash | success α0.055 / danger α0.055 | same | [R07 §UI spec "Diff"] | `d.addBg`, `d.delBg` |
| `addText` / `delText` | diff +/- text | = `success` / `danger` | same | [R07 §F17] | `d.addText`, `d.delText` |
| `addBar` / `delBar` | 3 pt left bar | success α0.55 / danger α0.55 | same | [R07 §UI spec "Diff"] | — |
| `hunk` | hunk header | accent α0.07 | accent α0.08 | [R07 §UI spec "Diff"] | — |
| `synKeyword` / `synType` | code highlight | `#5B43E8` / `#7E22CE` | `#8B7CF6` / `#C084FC` | [R07 §F17] | `d.keyword`, `d.typeName` |
| `path` | file paths | `#0E7490` | `#22D3EE` | teal [R07 §F17] | `d.teal` |
| `claude` / `codex` | provider mark in subtitles | `#D97757` / `#0F9D8A` | same | desktop AGENT_* [R07 desktop map] | `d.teal` on the provider label [P app/screens/ChatScreen.tsx:143] |
| `termFg` / `termBg` | read-only terminal | `#303035` / `#FAFAFA` | `#E8E8EA` / `#090909` | [R07 desktop map, terminal row] | — |
| `scrim` | behind sheets and alerts | black α0.32 | black α0.60 | [R07 §UI spec "Dialog"] | — |

**Contrast rules**
- `text`, `body` and `muted` must reach ≥ 4.5:1 on `bg` and `surface`. Light `faint` is 4.32:1 on white [R07 §F3], so `faint` never carries information by itself: it is for Idle, placeholders, timestamps and anything already said elsewhere.
- Add a `test/tokens.test.mts` that computes WCAG ratios: ≥ 4.5 for text roles (incl. `warningText`), ≥ 3.0 for `accent` and `faint` [R07 idea 07-3].
- The same test composites every §2.2 glyph colour at its α on `bg` and `surface` and requires ≥ 3.0 (non-text). Raise α where a value fails.

### 2.2 Status (D1)

`look()` changes only the tones: Done moves `accent` → `success`, Working moves `ok` → `accent` [P app/status.ts:9-21].

| Status | Glyph (12 pt slot) | Colour | Glyph α | Label |
|---|---|---|---|---|
| Needs you | 7 pt dot | `warning` | 0.6 | "Needs you" |
| Failed (Done + failed) | 7 pt dot | `danger` | 0.65 | "Failed" |
| Done | check, stroke 1.6 | `success` | 0.9 | "Done" |
| Working | cell spinner: 2×3 cells of 3.5 pt, gap 1.5, 750 ms cycle | `accent` | 0.55 | "Working" |
| Idle | none | `faint` | 0.14 | relative time ("2m") |
| Not attached | none | `faint` | — | "Not attached" (row at 0.5 opacity, [P app/screens/AgentsScreen.tsx:74]) |

- Glyph sizes and α values: [R08 §S2; R07 §UI spec "Status"].
- The label is in the full tone colour, Medium 13; Needs you uses `warningText`.

### 2.3 Type (Geist / Geist Mono [P app/design.ts:28-35])

| Role | Size / line | Weight | Source |
|---|---|---|---|
| Large title (Sessions) | 30 | SemiBold | [R08 §S1] |
| Pair title | 34 | SemiBold | [R19 §UI spec "Phone pair screen"] |
| Panel question | 17 | SemiBold | [R08 §S5] |
| Row title | 16.5 | Medium; SemiBold when Needs you or Done | [R08 §S1] |
| Composer input | 16.5 | Regular | [R08 §F8] |
| Header title / subtitle | 16 / 12 | SemiBold / Regular | [R08 §S3] |
| Prose | 15 / 23 | Regular | [P app/components/TimelineView.tsx:258] |
| Row meta | 13.5 | Regular | [R08 §S1] |
| Status label, pill, chip | 13 | Medium | [R08 §S1, §S3] |
| Panel header | 12.5 | Medium | [R08 §S5] |
| Code detail | Mono 13.5 | Regular | [R08 §S5] |
| Diff | Mono 12, line 21 | Regular | [R07 §UI spec "Diff"] |
| Terminal grid | Mono, fit to width, floor 7 | Regular | [R16 §F6] |

### 2.4 Shape and spacing

| Element | Value | Source |
|---|---|---|
| Screen margin (list) | 20 | [R08 §S1] |
| Header / composer inset | 12 | [P app/screens/ChatScreen.tsx:188; R08 §S4] |
| Timeline content | px 16 | [P app/components/TimelineView.tsx:257] |
| Timeline gaps | turn 30 / reply 18 / block 12 | [R08 §S6] |
| Glass circle button | 44, r 22 | [P app/screens/ChatScreen.tsx:190-197] |
| Row pressed highlight | r 16, inset 1/8 | [R08 §S1] |
| Composer | resting r 25, h 50; card r 26 | [R08 §S4] |
| Inline panel | r 26, insets 16/16/16/12 | [R08 §S5] |
| Option row | r 14, insets 11/12, spacing 6 | [R08 §S5] |
| Code block, field | r 12 | [R08 §S5] |
| Bubble | r 20, padding 15×10, max width `max(0.86w, w−56)` | [R08 §S6] |
| Tool / plan card | r 14 | [P app/components/ToolGroup.tsx:92-98] |
| Status pill | h 30, capsule | [R08 §S3] |
| Hit target | ≥ 44 | [P app/screens/ChatScreen.tsx:190] |

---

## 3. Navigation map

There is no navigation library today: `App.tsx` swaps screens on state [P app/App.tsx:15-31]. Use `@react-navigation/native-stack` (install with `npx expo install`) for native push, swipe-back and deep links [R14 idea 14-18].

```
Launch ── SecureStore has credentials? ─ no ─▶ PairStack
   │                                          Pair ──▶ EnterCode ──▶ ConfirmPair
   │                                                     ▲ anywhere://pair?… (deep link)
   yes
   ▼
MainStack (root: Sessions)
   Sessions ──push──▶ Session ──push──▶ Changes
      │                  │    └─push──▶ Terminal
      │                  └─ inline: Composer ⇄ Approval panel ⇄ Needs-you notice
      ├─ row "Not attached" ──push──▶ Terminal            (Phase 3)
      ├─ (+) ──modal──▶ NewSession ──replace──▶ Session   (Phase 4)
      └─ (⋯) ──modal──▶ Settings
Any screen ── close 4401 / "Not paired" ──▶ Revoked (replaces root) ──▶ PairStack
Notification tap ──▶ Session(agentId) (held until online) [R08 idea 08-3]
```

- **Linking:** prefix `anywhere://`; `pair` → ConfirmPair. The scheme is already registered [P packages/app/app.json:7; R19 §F2].
- **Seen:** Session, Changes and Terminal all keep `agent.view([agentId])` while the app is active. Sessions and Settings send `agent.view([])`. This extends today's Session-only rule [P app/screens/ChatScreen.tsx:101-105] (**new**; CONTEXT.md §Seen).
- **Removed:** the global PermissionSheet modal [P app/App.tsx:29] gives way to the inline panel (§4.5).
- **Back badge (new):** the Session back button shows the count of *other* Sessions that are Needs you, e.g. `‹ 2`, so a request elsewhere is visible without the global sheet (PRD FR 12-3, 17-3).

---

## 4. Screens

### 4.1 Pair (Phase 2; replaces ConnectScreen)

Layout: Zeron sign-in metrics [R19 §UI spec "Phone pair screen"].
- symbol 44 light; title 34; body 17 `muted`; stack spacing 12; centre Y −80;
- capsule button (bg `text`, fg `bg`, insets 15/20, SemiBold 17), 24 from the safe edges;
- status line 14 `danger`.

```
┌──────────────────────────────────────────┐
│                                          │
│                   ▣                      │
│           Pair with your Mac             │
│   On your Mac, press ⌘K → Pair phone,    │
│   then scan the code with the Camera     │
│   app.                                   │
│                                          │
│   Can't reach mac-mini.tail1234.ts.net.  │ ← status line, danger
│   Is Tailscale on?                       │
│                                          │
│  ╭────────────────────────────────────╮  │
│  │        Enter code instead          │  │ ← capsule, bg = text
│  ╰────────────────────────────────────╯  │
└──────────────────────────────────────────┘
```

- **Scanning:** the system Camera opens `anywhere://pair?v=1&h=<host:port>&c=<code>&n=<mac name>`, so the app needs no camera permission and no scanner [R19 §F2, idea 19-11; R18 §F6 R2]. An in-app scanner (expo-camera) is deferred (**new**; R18 offers either [R18 idea 18-4]).
- **Copy:** body text points at the desktop Pair phone dialog (PRD FR 03-7; the Devices sheet is S5), with R19's Camera wording [R18 §UI spec "Phone"; R19 §F2].
- **The QR never carries a token.** It holds a 22-char one-time code: 5 min TTL, single use, lock for 60 s after 5 failures [R18 §F6 R2] (D10; supersedes [R08 idea 08-18]).

**EnterCode** (push)
```
┌──────────────────────────────────────────┐
│ ‹                                        │
│  Mac address                             │ ← label 12
│  ┌────────────────────────────────────┐  │
│  │ mac-mini.tail1234.ts.net:4517      │  │ ← placeholder; input p12, 15
│  └────────────────────────────────────┘  │
│  Code                                    │
│  ┌────────────────────────────────────┐  │
│  │ 22 characters from your Mac        │  │
│  └────────────────────────────────────┘  │
│  ╭────────────────────────────────────╮  │
│  │              Continue              │  │
│  ╰────────────────────────────────────╯  │
└──────────────────────────────────────────┘
```
- Field metrics are today's Connect styles (padding 24, label 12, input p12 / 15) [R18 §UI spec "Phone"; P app/screens/ConnectScreen.tsx:64-86].
- The author's IP placeholder goes away [P app/screens/ConnectScreen.tsx:36].
- `ws://` is allowed only for loopback and Tailscale ranges, otherwise `wss://` [R18 §F6 R4].

**ConfirmPair** (push, or opened by the deep link)
```
┌──────────────────────────────────────────┐
│      Pair with Mingo's MacBook Pro?      │ ← 22 SemiBold
│      mac-mini.tail1234.ts.net:4517       │ ← mono 13, muted
│                                          │
│  Name on your Mac                        │
│  ┌────────────────────────────────────┐  │
│  │ Mingo's iPhone                     │  │ ← prefilled device name
│  └────────────────────────────────────┘  │
│  A paired phone can run commands on      │ ← 13, muted
│  this Mac as you.                        │
│  ╭────────────────────────────────────╮  │
│  │               Pair                 │  │ ← spinner in place of the label while pairing
│  ╰────────────────────────────────────╯  │
│                 Cancel                   │
└──────────────────────────────────────────┘
```
- Sends pre-auth `pair {code, name, platform, protocolVersion}` and gets back `pair.ok {deviceId, token}` (PRD FR 02-4) [R18 §F6 R2]. Trust copy: [R18 idea 18-18].
- Token storage: SecureStore `WHEN_UNLOCKED_THIS_DEVICE_ONLY` [R18 §F6 R13].
- On success: success haptic, then replace the root with Sessions. On relaunch, connect automatically [R19 idea 19-11].

**Revoked** (root)
```
┌──────────────────────────────────────────┐
│                   ▣                      │
│         This phone was removed.          │
│  ╭────────────────────────────────────╮  │
│  │            Pair again              │  │
│  ╰────────────────────────────────────╯  │
└──────────────────────────────────────────┘
```
- Trigger: close code 4401 or `error "Not paired"`. SecureStore is already wiped; no retry loop [R18 §F6 R1, R5; §UI spec "Phone"] (PRD FR 02-7).

**Interim, before R18 (Phase 1):** ConnectScreen keeps host + token, plus the fixes below [R19 §F2]:
- auto-connect on launch from SecureStore `pocket.host` / `pocket.token` [P app/screens/ConnectScreen.tsx:7-8];
- the host placeholder becomes `mac-mini.tail1234.ts.net:4517`;
- after `error "Rejected"`, show "Rejected — token or version mismatch" and stop retrying (today it retries forever under "Disconnected — retrying" [P app/screens/ConnectScreen.tsx:59; R19 §F2]);
- the offline copy becomes "Can't reach {host}. Is your Mac awake (lid open) and on Tailscale?".

### 4.2 Sessions (root)

```
┌──────────────────────────────────────────┐
│ Sessions                        (+) (⋯)  │ ← 30 SemiBold; glass 44 buttons
│ 1 needs you · 1 done · 1 working         │ ← summary 13 Medium muted
│                ( Reconnecting in 8s )    │ ← pill, only after 4 s (§5.1)
│ ┌ Get notified ──────────────────────┐   │ ← pre-prompt card, once (§4.9)
│ │ When a session is done, needs you  │   │
│ │ or fails        [Not now] [Turn on]│   │
│ └────────────────────────────────────┘   │
│ UP NEXT                                  │ ← 13 Medium muted, only if ≥ 1
│ Fix login redirect         ● Needs you   │ ← row 62: title 16.5
│ Claude · pocket                          │   meta 13.5 muted
│ Add retry to uploader           ✓ Done   │
│ Codex · api                              │
│ ──────────────────────────────────────── │
│ ALL SESSIONS                             │
│ Add retry to uploader           ✓ Done   │ ← stable: newest created first
│ Codex · api                              │
│ Refactor store               ⠿ Working   │
│ Claude · pocket                          │
│ Fix login redirect         ● Needs you   │
│ Claude · pocket                          │
│ Tidy README                         2m   │ ← Idle: relative time, faint
│ Claude · docs                            │
│ zsh                                      │ ← 0.5 opacity
│ Not attached · open on your Mac          │
└──────────────────────────────────────────┘
```

**Ordering (D2)**
- **All sessions:** `createdAt` descending. New sessions are prepended, as `applyAgentUpdate` already does [P app/agents.ts:3-10]. Never re-sort on a status change. Today the whole list is sorted by urgency [P app/screens/AgentsScreen.tsx:9]; that goes away.
- **Up next:** Needs you, Failed and Done (not Seen) sessions, ranked Needs you > Failed > Done, then oldest transition first (D39). Today `byUrgency` breaks ties by most recently updated [P app/status.ts:23-25]; that changes. Rows appear in both sections so the main list stays stable (**new**).
- Put ordering in a pure `src/order.ts` (`upNext(agents)`, `allSessions(agents)`) with tests next to `test/status.test.mts`.

**Row** [R08 §S1, idea 08-10]
- Height 62, margins 20, no card border (today: bordered r12 cards [P app/screens/AgentsScreen.tsx:64-73]).
- Title: `agent.title`.
- Meta: `{Provider} · {project}`, where project is `basename(cwd)` for now. Once AgentSummary carries worktree and branch [R14 idea 14-21], use `{Provider} · {project} / {worktree}` when the worktree isn't the main one.
- Corner: status glyph + label (§2.2); Idle shows relative time.
- Not attached: 0.5 opacity; meta "Not attached · open on your Mac" [P app/screens/AgentsScreen.tsx:39]. Disabled until Phase 3, then it opens Terminal and the meta becomes "Not attached · terminal only" (**new**).

**Summary line:** parts in urgency order, zero parts omitted, e.g. "1 needs you · 1 failed · 2 done · 3 working" (plural "2 need you"). When all are zero, "N sessions" [R08 idea 08-11]. The same grammar is unit-tested (PRD FR 17-3).

**Header**
- Glass `(+)` = New session (hidden until Phase 4).
- `(⋯)` = Settings. It replaces the "Disconnect" link [P app/screens/AgentsScreen.tsx:13-18].

**Pull to refresh:** redial if offline, then `agent.list` [R08 idea 08-7].

**Empty states**
- Online, no sessions: "No sessions yet" / "Start one on your Mac." In Phase 4 add the button "New session". Replaces "Start one on your Mac: pocketd run claude" [P app/screens/AgentsScreen.tsx:24].
- First connect not done yet: §5.1.

### 4.3 Session (timeline)

```
┌──────────────────────────────────────────┐
│ (‹2) ( Fix login redirect     )(±)(>_)   │ ← glass 44 · title pill · diff · terminal
│      ( Claude · pocket · opus )          │ ← subtitle 12; provider in its mark colour
│           ( Offline — sends are saved )  │ ← connectivity pill (§5.1)
│                 ┌──────────────────────┐ │
│                 │ Fix the redirect so  │ │ ← userBubble r20
│                 │ `next` survives login│ │
│                 └──────────────────────┘ │
│ Thought for 4s ›                         │
│ Ran 2 commands · edited 1 file ›         │ ← tool group summary, 40
│ The redirect drops `next` because …      │ ← prose 15/23 body
│ ┌ Plan ─────────────────────────────┐    │
│ │ ○ Keep next in session storage    │    │
│ └───────────────────────────────────┘    │
│ ✳ Working 1m 4s · opus                   │ ← working row [TimelineView:67-98]
│                                    (⌄)   │ ← jump, glass 40
│ ╭──────────────────────────────────────╮ │
│ │ Message Claude…                  (■) │ │ ← composer (§4.4)
│ ╰──────────────────────────────────────╯ │
└──────────────────────────────────────────┘
```

**Header** [R08 §S3; P app/screens/ChatScreen.tsx:131-159]
- Back (glass 44) with the §3 badge.
- Title pill: title SemiBold 16; subtitle 12 `{Provider} · {project} · {model}` [P app/screens/ChatScreen.tsx:85], max width 240.
- `(±)` diff pill `+N −N`: opens Changes. Hidden when there are no edits.
- `(>_)`: opens Terminal. Hidden until Phase 3.

**Connectivity pill:** a single 30 pt glass capsule under the header for connectivity only (§5.1). Working and Needs you stay in the timeline tail, which is what Zeron iOS ships [R08 idea 08-16 note] (**new**: no status pill).

**Timeline**
- Tool groups get the summary grammar ("Ran 2 commands · edited 1 file · 1 failed") in place of "N actions · Ns" [P app/components/ToolGroup.tsx:75-89] [R05 idea 05-1].
- Groups are open while streaming and fold when settled [R05 idea 05-10].
- User bubble: `userBubble` in place of the green bubble [P app/components/TimelineView.tsx:163-168]. Past 8 lines it folds to 6 with "Show more" / "Show less" [R08 §S6; R05 idea 05-12]. A pending bubble is at 0.65 opacity (§5.2) [R05 idea 05-9].
- Working row keeps today's copy: "Working Ns · model" / "Compacting context Ns" [P app/components/TimelineView.tsx:67-98]. "Waiting for approval" becomes "Needs you" (CONTEXT.md avoid list).
  - The elapsed time comes from `turnStartedAt` once AgentSummary carries it [R05 idea 05-8]. Until then it is the local timer, which resets on remount [P app/screens/ChatScreen.tsx:107-109].
- Result row "Done in X.Xs." and failure "Run failed." stay [P app/components/TimelineView.tsx:124-139].

**Scroll** [R08 §S6, idea 08-14]
- Dragging releases follow. Follow re-latches within 70 pt of the tail at velocity ≤ 50.
- The jump button (glass 40, chevron 15, 16 from the trailing edge, 12 above the composer) shows past 140 pt from the tail. It replaces the 46 pt button [P app/components/TimelineView.tsx:231-256].
- The send runway [R08 idea 08-15] is later.

**Context chip (D7)**, in the composer toolbar (§4.4)
- "NN% context": `muted` below 75 %, `warning` from 75 %, `danger` from 90 %. Hidden when AgentSummary has no usage or window [R14 idea 14-21] (today it has neither [P proto/timeline.ts:83-99]).
- Tap opens a sheet: "Context window" / "184,000 / 200,000 tokens" / "16,000 tokens remaining", and the button "Compact now" → `agent.compact` [R05 §UI spec "Context ring"; R13 §UI spec].

**Session ended:** the agent turns `closed` and drops out of the list [P app/agents.ts:4]. Keep the timeline, replace the composer with "Session ended." in `muted`, and keep Back (**new**).

### 4.4 Composer (D12 fix)

**Primary button mode (Phase 0)**, as a pure function in `src/composer.ts`, tested in `test/composer.test.mts`:

```ts
export type ComposerAction = "send" | "queue" | "stop";
export function composerAction(working: boolean, text: string): ComposerAction {
  if (!working) return "send";
  return text.trim() ? "queue" : "stop";
}
```

- `working = agent.status === "working"` [P app/screens/ChatScreen.tsx:90]. Sources: [R05 idea 05-2, §UI spec "Composer button"; R08 §F8].
- It replaces `busy ? onInterrupt : submit` [P app/components/Composer.tsx:49-55], where the only button is Stop while busy.
- Return (`onSubmitEditing`) calls submit only: Send or Queue with text, nothing with an empty field. It never stops [R05 §UI spec "Composer button"].
- Send is disabled when not Working and the text is empty.
- Queue sends `agent.prompt` now. pocketd types the text and Enter into the TUI [P pd/terminal/terminal.go:278-287], and the TUI decides whether it queues or steers. The E01 step-0 check verifies it per provider; where the text doesn't queue, the "queue" action is labelled "Send" (D37).

| State | Look | a11y label / hint |
|---|---|---|
| Send, enabled | 34 pt circle, `accent` bg, `onAccent` arrow.up bold 15 | "Send" |
| Send, disabled | `controlFill` bg, `tertiary` glyph | "Send", dimmed |
| Queue | as Send | "Queue" / "Sends while {Provider} works" |
| Stop | `text` bg, stop.fill 11 in `bg` colour | "Stop" |

- Colours and glyphs: [R08 §F8]. The morph between states is a spring 0.32 / 0.8 (§7).

**Shapes (Phase 1)** [R08 §S4, §F8]
```
Resting                                     Focused / has text
╭──────────────────────────────────────╮    ╭──────────────────────────────────────╮
│ Message Claude…                  (↑) │    │ Refactor the store so drafts         │
╰──────────────────────────────────────╯    │ survive a remount                    │
 r25 · h50 · 1–2 lines                      │                                      │
                                            │ [82% context] [Runs in your shell](↑)│
                                            ╰──────────────────────────────────────╯
                                             r26 · up to 8 lines · toolbar 50
```
- Multiline input 16.5; max 2 lines at rest, 8 in the card [R08 §F8]. In multiline, Return inserts a newline and the button sends; hardware ⌘↩ sends [R08 §F8].
- Phase 0 keeps today's single-line field [P app/components/Composer.tsx:35-43].
- Placeholder: "Message {Provider}…" [P app/screens/ChatScreen.tsx:174].
- Toolbar chips (capsule, 13.5 Medium, tint α0.1):
  - context (§4.3);
  - "Runs in your shell" when the text starts with `!` [R18 §F6 R8, idea 18-9].
- Attach "+" and Dictate are removed until they are wired. Attach needs pocketd `agent.attach` [R05 idea 05-13]; iOS already puts a mic on the keyboard [P app/components/Composer.tsx:28-32,44-46] [R14 idea 14-17].
- `/compact` still routes to `agent.compact` [P app/screens/ChatScreen.tsx:51-53].
- Placement: max width 768, 12 insets, 8 above the keyboard [R08 §S4]. Keep today's keyboard ride [P app/screens/ChatScreen.tsx:13-37].
- **Drafts:** a per-session `Map<agentId, string>` in `SessionProvider`. The composer becomes controlled, so text survives leaving and re-opening a session [R08 idea 08-9; R05 idea 05-11]. It lives in memory only; persisting across app kills is deferred (**new**).
- **Hidden** while this session has an open permission request (the panel takes its place, §4.5) or is Needs you without one (the notice takes its place). Typing into the TUI's open dialog would answer it; pocketd waits for the same reason [P pd/daemon/daemon.go:161-163].

**Later (needs D4):** long-press menu "While the agent works" [R08 §F8, idea 08-12]:
- "Queue for next turn";
- "Steer now" / "Deliver into the running turn" (Codex via `turn/steer`, D4);
- "Stop and send" (destructive).

### 4.5 Approval panel and question panel

**Approval panel** (Phase 1) replaces PermissionSheet [P app/components/PermissionSheet.tsx] [R08 §S5, idea 08-13].

```
╭──────────────────────────────────────────╮
│ Bash · 1 of 2                            │ ← 12.5 Medium muted
│ ┌──────────────────────────────────────┐ │
│ │ pnpm --filter @pocket/app test       │ │ ← Mono 13.5, code r12 px14,
│ └──────────────────────────────────────┘ │   ≤ 8 lines + fade + "Show more"
│ Do you want to proceed?                  │ ← 17 SemiBold, 14 below
│ ╭──────────────────────────────────────╮ │
│ │ ①  Yes                               │ │ ← Medium 16, r14, chip α0.55
│ ╰──────────────────────────────────────╯ │
│ ╭──────────────────────────────────────╮ │
│ │ ②  Yes, and don't ask again for …    │ │ ← options[] from pocketd
│ ╰──────────────────────────────────────╯ │
│ ╭──────────────────────────────────────╮ │
│ │ ③  Yes, and switch to auto mode      │ │
│ ╰──────────────────────────────────────╯ │
│ ╭──────────────────────────────────────╮ │
│ │ ④  No                                │ │
│ ╰──────────────────────────────────────╯ │
│ ┌──────────────────────────────────────┐ │
│ │ Tell Claude what to do differently   │ │ ← 44, chip α0.6, r12
│ └──────────────────────────────────────┘ │
╰──────────────────────────────────────────╯
```

- **State:** `permission?: PermissionRequest` becomes `permissions: Record<requestId, PermissionRequest>`. Today a second request overwrites the first [P app/session.tsx:43,61-66; R08 §F9]. The panel pages over this session's requests in arrival order.
- **Rows:**
  - "Yes"; then `options[]` (the "always" suggestion label; "Yes, and switch to auto mode" when the mode isn't auto [P pd/daemon/permission.go:18-27]); then "No" [P app/components/PermissionSheet.tsx:32-38].
  - Glyph `N.circle`. The chosen row turns `accent` on `accentSoft`.
- **Detail text:** unchanged. shell → command, read/edit/write → path, search → query, task → description, other → JSON ≤ 300 chars [P app/components/PermissionSheet.tsx:12-28].
- **Tap:**
  - highlight + selection haptic, then submit once after 220 ms, with a medium haptic [R08 §S5];
  - then the next page crossfades in over 0.2 s; the panel closes after the last page.
- **Feedback field:** submit = `deny` + `message`. pocketd denies with "Denied from phone", interrupts, and types the message as the next prompt after the turn ends [P pd/daemon/daemon.go:147-153,161-173].
- **"No" stops the turn** (deny + interrupt [P pd/daemon/daemon.go:148]). The row label stays "No" to match Claude's own dialog (**new**; §13).
- **Stale:** `error "Permission request is no longer open"` [P pd/wsserver/wsserver.go:156-158] or `permission.resolved` drops the page without a message [R08 §S5].
- **Clearing:** do not clear locally before the server confirms. Today `resolvePermission` clears first [P app/session.tsx:116-119]; clear on `ack {id}` or `permission.resolved` instead (**new**).
- **Motion:** the panel replaces the composer with a spring 0.38 / 0.86 [R08 §F9].

**Needs-you notice** (Phase 1): status is `needsYou` with no open request, e.g. a TUI question or dialog.
```
╭──────────────────────────────────────────╮
│ Claude needs you in the terminal.        │ ← 15 Medium
│ Answer on your Mac.                      │ ← 13 muted; no button until S7 (PRD FR 12-2)
╰──────────────────────────────────────────╯
```

**Plan variant** (E12; `question.request` kind plan, from `ExitPlanMode`). Same card: header "Plan"; the rendered plan markdown (≤ 60 % of the screen, scroll); an optional feedback field; rows "Approve" / "Keep planning". Feedback is sent with Keep planning (PRD FR 12-3).

**Question panel** (E12; needs pocketd `question.request` [R05 idea 05-4]). It uses the same card as the approval panel:
- header "{header} · 1 of 3";
- options with auto-advance after 220 ms for single-select; "Select one or more options." for multi-select;
- free text "Type your own answer, or pick an option above";
- "Back" / "Next" → "Submit", at 0.4 opacity while blocked [R05 §UI spec "Question wizard"; R08 §F9].

### 4.6 Changes (Phase 1)

Source: the edit and write tool calls in this session's timeline, which carry a `FileDiff` [P proto/timeline.ts:25-33]. No protocol change is needed.
- File list and totals use `diffTotals` (latest edit per path) [P app/screens/ChatScreen.tsx:62-77].
- Expanding a file shows each of its edits, oldest first, via `DiffView` [P app/components/DiffView.tsx].
- A git-truth worktree diff needs a `files`-scope RPC [R18 §F6 R6, R10] and comes later.

```
┌──────────────────────────────────────────┐
│ (‹)          Changes             +42 −7  │
│ Edits in this session · 3 files          │ ← 13 muted
│ ──────────────────────────────────────── │
│ ›  app/session.tsx            +30  −4    │ ← row 44; path in `path` colour
│ ⌄  app/components/Composer.tsx +12 −3    │
│   Edit 2 of 2                            │ ← 12.5 muted
│  ┃49 - accessibilityLabel={busy ? …      │ ← del wash + 3pt bar
│  ┃49 + accessibilityLabel={label[mode]}  │ ← add wash + 3pt bar
│    50   onPress={…}                      │
│ ›  README.md                   +0  −0    │
└──────────────────────────────────────────┘
```
- Diff metrics: mono 12 / line 21; marker column 28; row wash α0.055; bar 3 pt α0.55; hunk `hunk` [R07 §UI spec "Diff"]. Today: number column 26, 12/20 [P app/components/DiffView.tsx].
- Keep horizontal scroll, "Empty file" and `contentOnly` writes [P app/components/DiffView.tsx:26; P proto/timeline.ts:20].
- Collapse 180 ms; chevron 200 ms [R07 §UI spec "Diff"].

### 4.7 Terminal, read-only (Phase 3, D9)

Source: pocketd renders frames; there is no VT on the phone [R16 §F6, idea 16-13].
- `terminal.watch {terminalId}` → `terminal.frame` (≤ 4 Hz, only when dirty); `terminal.unwatch`.
- `terminal.history {before, lines}` for Log mode.
- `terminalId` is already in AgentSummary [P proto/timeline.ts:83-99].
- Needs the `observe` scope, so it ships after R18 device tokens (D10) [R18 §F6 R6].

```
┌──────────────────────────────────────────┐
│ (‹)  ( zsh · pocket          ) [Log|Grid]│ ← segmented, 13 Medium
│      Read-only · Showing 120-column      │ ← 12 muted
│      terminal                            │
│ ┌──────────────────────────────────────┐ │
│ │$ pnpm --filter @pocket/app test      │ │ ← termFg on termBg
│ │✔ agents.test.mts (4 ms)              │ │   Log: reflowed, selectable
│ │✔ status.test.mts (3 ms)              │ │   Grid: PTY cols, fit ≥ 7 pt,
│ │                                      │ │   else pan; pinch zoom
│ │                                      │ │
│ └──────────────────────────────────────┘ │
│                     ( Jump to bottom ↓ ) │ ← 28, r14
└──────────────────────────────────────────┘
```
- **Default mode:** Log for shells; Grid when the program uses the alt screen or DECSTBM [R16 §F6]. Needs a flag on the frame. With VoiceOver on, always start in Log (**new**; §10).
- **Grid:** rows are `<Text>` runs `{t, fg, bg, b, i, u, inv}` plus the cursor [R16 §F6]. Measure the Geist Mono advance once, then `fontSize = max(7, width / (cols × advance))`.
- **Never resize the PTY from the phone** [R16 §F6].
- **States** [R16 §UI spec "States"]:
  - "Connecting…";
  - "This session is not running.";
  - "Process exited with code {c}";
  - Jump pill once scrolled back.
- **Key bar and input** (Esc, Tab, sticky Ctrl, ←↑↓→, Enter, ^C + text field → `terminal.input`) come later and need trust checks [R16 idea 16-18] (D10).

### 4.8 New session (Phase 4, D5)

Sends one `agent.create` with a LaunchSpec `{project, checkout: {worktree} | {new: {name}}, provider, model?, effort?, access, plan, prompt}` (D41; the phone never sends `base`). pocketd builds argv [R17 idea 17-10, §F13] (D5).

**Phone limits (D10)** [R18 §F6 R7, idea 18-10]:
- claude | codex only;
- registered projects only;
- no cmd, env or cwd;
- base branch not sent: pocketd uses the repo's configured `RepoConfig` base and runs its setup [R17 §S6] (D41).

Layout: a modal, composer-first [R08 §F10; R17 §S1].

```
┌──────────────────────────────────────────┐
│ Cancel          New session              │
│                                          │
│   What should we work on in pocket?      │ ← 22 SemiBold [R17 §S1 alt headline]
│ ╭──────────────────────────────────────╮ │
│ │ Describe what the agent should do…   │ │ ← 16.5, grows to 8 lines
│ │                                      │ │
│ │ [▣ pocket ⌄] [◐ Claude · opus ⌄]     │ │ ← chips
│ │ [🔒 Ask ⌄] [Plan first]          (↑) │ │
│ ╰──────────────────────────────────────╯ │
│  [⎇ New worktree ⌄]  branch calm-otter   │ ← 12, inline editable name
│                                          │
│  Trust this folder in Claude on your Mac │ ← error row, danger (states)
│  first.                                  │
└──────────────────────────────────────────┘
```

| Chip | Menu | Default / memory | Source |
|---|---|---|---|
| Project | registered projects by name | last used | [R18 §F6 R7] |
| Checkout | worktrees by name (main first, meta "on {branch}"), then "New worktree" | New worktree; not remembered | D5; [R17 §S6] |
| Worktree name | inline field "branch {name}"; slug from the first 4 words of the prompt, else adjective-noun | — | [R17 §S1] |
| Agent | "Claude Code" / "Codex" sections; model rows; "Effort" row | last pick per provider. Before the model catalog, the last-seen label or the provider name, never "Default model"; omit `--model` unless picked | [R17 §S2, §F13, §S6] |
| Access | Ask ("Ask before commands and file changes.") · Auto-accept edits · Auto, each up to `phone.maxAccess`; never Full access (D18) | Ask; remembered | [R17 §F14, §S3] |
| Plan first | toggle "Review a plan before building." Disabled for Codex: "Codex can't plan first in a terminal session" (D31) | off | [R17 §S3] |

- **Access from the phone:**
  - Only Ask + Plan first are enabled by default [R18 §F6 R7].
  - Auto-accept edits and Auto are enabled up to `phone.maxAccess` (default Ask), which the owner sets on the Mac: `pocketd config set phone.maxAccess`, or ⌘K → Phone access level (D18, PRD FR 06-7). Locked chips carry the hint "On your Mac: ⌘K → Phone access level".
  - Full access is never offered from the phone this horizon (D18).
- **Send:** disabled while the prompt is empty (**new**; R17 leaves empty prompts open). On the reply with the new `agentId`, replace this modal with Session.

**States** [R17 §S5; R18 §F6 R7]

| State | UI | Copy |
|---|---|---|
| pocketd without `agent.create` | Send disabled | "Update Pocket on your Mac." (today it answers "Malformed message" [P pd/wsserver/wsserver_test.go:157-160]) |
| No registered project | modal body replaced | "Add a project on your Mac first." (**new**, adapted from "Add a project to begin.") |
| No CLI | agent chip text, Send disabled | "No agents available" / "Install Claude Code or Codex, then reopen." |
| Name taken or invalid | inline under the name, `danger` | "A worktree or branch with this name already exists" / "Use letters, digits, - _ or ." |
| Folder not trusted (`folder_not_trusted`, PRD FR 06-6) | error row, draft kept | "Trust this folder in Claude on your Mac first" |
| Worktree or spawn failure | error row, draft kept | git or pocketd error verbatim |
| Offline | connectivity pill, Send disabled, draft kept | §5.1 |

### 4.9 Settings (modal)

```
┌──────────────────────────────────────────┐
│ Settings                           Done  │
│ THIS MAC                                 │
│ Mingo's MacBook Pro                      │ ← 16.5
│ via mac-mini.tail1234.ts.net · Paired    │ ← 13 muted
│ Sep 30                                   │
│ NOTIFICATIONS                            │
│ 🔔 Notifications                   [on]  │
│    When a session is done, needs you or  │ ← or "Turned off in iOS Settings"
│    fails                                 │
│ ● Needs you                        [on]  │ ← per-cue, default on (D11)
│ ✓ Done                             [on]  │
│ ● Failed                           [on]  │
│ Show details on lock screen       [off]  │
│ SECURITY                                 │
│ Require Face ID                   [off]  │ ← later (R13)
│ Unpair this phone                        │ ← danger
│ Anywhere 0.0.0 · protocol 3         │ ← 12 faint
└──────────────────────────────────────────┘
```
- Section and rows: [R18 §UI spec "Phone settings"; R08 §S8].
- **Master toggle:** off → `push.unregister`. On while iOS permission is `notDetermined` → request it; if denied → alert (below) [R08 §S8; R19 §F2 P4].
- **Per-cue toggles:** sent as `push.register {token, prefs {needsYou, done, failed}}` on every `hello.ok`; pocketd filters [R08 idea 08-2] (D3, D11).
- **"Show details on lock screen":** off by default. When on, the tool name and command (≤ 80 chars) are added [R18 §F6 R12].
- **Unpair:** alert "Unpair? You'll need your Mac to pair again." → `devices.revokeSelf`, wipe SecureStore, go to Pair [R18 §F6 R1, R13].
- **Before R18:** the SECURITY section is one row, "Disconnect", which keeps today's behaviour [P app/screens/AgentsScreen.tsx:13-18].

**Pre-prompt card** (Sessions, §4.2)
- Shown once, the first time the list has ≥ 1 session after pairing (the phone can't start sessions before Phase 4) [R19 §F2 P4, idea 19-13].
- "Turn on" → `requestPermissionsAsync` only while `notDetermined`. Granted → `getExpoPushTokenAsync({projectId})` → `push.register` [R19 §F6].
- Denied alert: "Notifications are off" / "Allow notifications for Anywhere in iOS Settings." with "Not Now" / "Open Settings" [R19 §F2 P4].

**Push behaviour** (D3, D11) [R18 §F6 R12; R08 §S7, idea 08-3]
- pocketd sends through Expo Push on Seen-aware transitions only, with collapse id and thread id = agentId.
- Title = project name. Body = fixed copy (§9).
- Payload: `{agentId, requestId, category}`.
- **Foreground:** `setNotificationHandler` suppresses the banner when `agentId` is the open Session (Seen). It shows banner and sound otherwise (D11; narrows [R08 idea 08-3]'s blanket mute).
- **Tap:** navigate to Session(agentId), held until online.
- **Native setup** [R19 §F6]: needs `expo-notifications`, `aps-environment` in the committed entitlements (today an empty dict [P packages/app/ios/Anywhere/Anywhere.entitlements:4]) and `extra.eas.projectId`.
- **Lock-screen Allow / Deny** (both `isAuthenticationRequired`, Deny `isDestructive`, no "Always allow" or auto mode) come later [R08 idea 08-19; R18 §F6 R12].

---

## 5. States

### 5.1 Connection (graced) [R08 §F12, idea 08-6, idea 08-7]

Today any close sends the user back to ConnectScreen, which says "Disconnected — retrying" [P app/App.tsx:22-26; P app/screens/ConnectScreen.tsx:59]. Backoff is 1 s doubling to a 30 s cap [P app/client.ts:7].

| State | Detection | UI |
|---|---|---|
| First connect | no `hello.ok` since launch, credentials present | Sessions shows a centred spinner + "Connecting to {host}…" (**new**) |
| First connect failing | 10 s without `hello.ok` (**new**) | "Can't reach {host}. Is your Mac awake (lid open) and on Tailscale?" + "Try again" [R19 §F2]; backoff continues |
| Blip | socket closed < 4 s ago | nothing; screen stays; sends are held (§5.2) |
| Offline | NetInfo reports no network for ≥ 4 s | pill "Offline — sends are saved" |
| Reconnecting | network up, socket down ≥ 4 s | pill "Reconnecting in {n}s"; tap = redial now (**new**) |
| Rejected / not paired | `error "Rejected"` (pre-R18) or 4401 / "Not paired" | Connect with error, or Revoked (§4.1); no retry |
| Version mismatch | protocol range has no overlap [R19 idea 19-10] | full screen "Update Pocket on your Mac." or "Update Pocket on this phone." |

- **Client** [R08 §F12]:
  - backoff 250 ms → 16 s cap, reset after 30 s stable;
  - `ConnectionState` gains `retryAt`;
  - redial immediately on AppState `active` and NetInfo online (add `@react-native-community/netinfo`).
- **Resync on `hello.ok`:** `agent.list`, then for each loaded timeline `agent.timeline {sinceSeq: lastSeq}` and merge; refetch the whole timeline when `epoch` changed [P proto/messages.ts; R08 idea 08-7]. Today the app refetches the whole page on mount [P app/session.tsx:83-86].
- **Pill precedence**, highest first: "Not delivered · Tap to retry" > "Offline — sends are saved" > "Reconnecting in {n}s" > hidden [R08 §S3]. Put it in a pure `src/connectivity.ts` with tests.

### 5.2 Delivery [R08 idea 08-8; R05 idea 05-9]

Ships in the E10 phone PR (PRD FR 10-5), on `ack {requestId, result, code}`. The minimal connection layer of §5.1 (no eject, redial, resync) ships earlier in E17 PR2.

pocketd already answers `agent.prompt` with `ack {id}` or `error {id, message}` [P pd/wsserver/wsserver.go:191-208]. The client doesn't correlate them yet [P app/session.tsx:67-69].

| Send state | When | Bubble |
|---|---|---|
| Sending | written, waiting for `ack {id}` | 0.65 opacity; working row "Sending…" [R05 §UI spec "Working trailer"] |
| Held | socket down at send time | 0.65 opacity, "Sends when your Mac is back" (**new**, 12 muted) |
| Delivered | `ack {id}` | full opacity |
| Not delivered | `error {id}`; or held > 120 s; or socket dropped before the ack | `danger` line "Not delivered · Tap to retry" + the reason in 12 muted; warning haptic |

```
                 ┌──────────────────────────┐
                 │ Also fix the logout link │   ← 0.65 while sending/held
                 └──────────────────────────┘
                   Not delivered · Tap to retry   ← danger, 12.5 Medium
                   Agent is no longer in the foreground   ← reason, 12 muted
```

- Held sends flush in order after `hello.ok` (**new**).
- A send that was written but not acked before a drop is **not** resent automatically: it turns Not delivered, because pocketd doesn't deduplicate yet. Automatic resend needs pocketd dedupe by client id [R08 idea 08-8] (**new**).
- Message ids must be unique across reconnects, e.g. `p-{Date.now() base36}-{random}`. Today they are `c${++seq}` [P app/client.ts:57-61], and the clientId is a constant [P app/session.tsx:76].

### 5.3 Errors (server string → phone)

| Server message | Where | Phone behaviour / copy |
|---|---|---|
| "Permission request is no longer open" [P pd/wsserver/wsserver.go:158] | panel | drop the page silently [R08 §S5] |
| "Agent is no longer in the foreground" [P pd/daemon/presence.go:119] | send | Not delivered + reason verbatim |
| "Not authenticated" / "Rejected" [P pd/wsserver/wsserver.go:146; R19 §F2] | connect | "Rejected — token or version mismatch" (pre-R18) |
| "Not paired", close 4401 [R18 §F6 R5] | any | Revoked screen |
| Protocol mismatch [R18 §F6 R5; R19 idea 19-10] | connect | "Update Pocket on your Mac." / "Update Pocket on this phone." |
| "Malformed message" on `agent.create` | new session | "Update Pocket on your Mac." |
| Pairing: expired / used / unreachable [R18 §UI spec "Phone"] | pair | "Code expired. Make a new one on your Mac." / "This code was already used." / "Can't reach {host}. Is Tailscale on?" |
| anything else with an id | origin | bubble or row error, verbatim |
| anything else without an id | Session | today's banner, tap to dismiss [P app/screens/ChatScreen.tsx:162-166]; `delBg`/`delText` → `danger` α0.08 / `danger` |

### 5.4 Session states (phone view)

| Status | List | Session screen |
|---|---|---|
| Working | spinner + "Working" | Composer: Queue (or Send, D37) / Stop; working row |
| Needs you (request open) | Up next, dot | Approval panel replaces the composer |
| Needs you (no request) | Up next, dot | Needs-you notice replaces the composer |
| Done | Up next, check, bold title | Opening it marks it Seen, so it turns Idle (pocketd) |
| Failed | Up next, red dot | "Run failed." row; composer: Send |
| Idle | relative time | Composer: Send |
| Not attached | 0.5 opacity | no timeline; Terminal only (Phase 3) |
| Closed | removed [P app/agents.ts:4] | "Session ended." |
| Restored after a pocketd restart (PRD FR 09-6) | row notice "Resumed" · "Interrupted by restart" · "Full access resumed as Ask" · "Couldn't resume" | the same notice as a timeline row |

---

## 6. Motion

RN springs use `Animated.spring({stiffness, damping, mass: 1})`. To convert an iOS spring (response r, damping fraction ζ): stiffness = (2π/r)², damping = 4πζ/r.

| Motion | Spec | RN value | Source |
|---|---|---|---|
| Composer resting ⇄ card | spring 0.42 / 0.86 | stiffness 224, damping 26 | [R08 §S4] |
| Send / Queue / Stop morph | spring 0.32 / 0.8 | stiffness 386, damping 31 | [R08 §S4] |
| Panel ⇄ composer | spring 0.38 / 0.86 | stiffness 273, damping 28 | [R08 §S5] |
| Jump button in/out | spring 0.35 / 0.8 | stiffness 322, damping 29 | [R08 §S6] |
| Option → submit | 220 ms delay | — | [R08 §S5] |
| Panel page change | crossfade 200 ms | — | [R08 §S5] |
| Glass appear / disappear | 350 / 250 ms | — | [R07 §F10] |
| Tool group fold | 140 ms, bezier (0, 0, 0.58, 1) | `Easing.bezier(0,0,0.58,1)` | [R05 §UI spec "Tool group"] |
| Tool row reveal | 360 ms expo (0.16, 1, 0.3, 1), stagger 65, first +90 | — | [R05 §UI spec "Tool group"] |
| Collapse / chevron (diff, bubble fold) | 180 / 200 ms | — | [R05 §UI spec "Motion tokens"; R07 §UI spec "Diff"] |
| Streaming shimmer | 3400 ms sweep | keep [P app/components/Shimmer.tsx] | [R05 §UI spec "Tool group"] |
| Working asterisk pulse | 800 ms, 0.35 ↔ 1 | keep | [P app/components/TimelineView.tsx:67-98] |
| Status spinner | 750 ms cycle | — | [R08 §S2] |
| Menus (chips) | in 140 ms, out 100 ms | — | [R05 §UI spec "Motion tokens"] |
| Sheets / modals | native stack defaults | — | (**new**) |

- **Reduce Motion** (`AccessibilityInfo.isReduceMotionEnabled`): springs and fades snap to the end state; the spinner and pulse show a static frame; the shimmer is off [R07 §UI spec "Motion settings"; R13 idea 13-10].
- No motion on status changes arriving in the list: rows don't move (D2).

## 7. Haptics (`expo-haptics`, new dependency)

| Event | Haptic | Source |
|---|---|---|
| Option picked (approval, question, New session menus) | `selectionAsync` | [R08 §F9] |
| Approval / question submitted (fires once) | `impactAsync(Medium)` | [R08 §F9] |
| Send / Queue tapped | `impactAsync(Light)` | **new** |
| Stop tapped | `impactAsync(Medium)` | **new** |
| Send becomes Not delivered | `notificationAsync(Warning)` | **new** |
| Paired | `notificationAsync(Success)` | **new** |
| Log / Grid toggle | `selectionAsync` | **new** |

- None for remote status changes while the app is open, scrolling or refreshing (**new**).

---

## 8. Accessibility

### 8.1 VoiceOver labels

| Element | Label | Hint / trait |
|---|---|---|
| Session row | "{title}, {status label}, {Provider}, {project}" (Idle: "idle, {n} minutes ago") | "Opens the session"; button |
| Not attached row | "{title}, not attached" | Phase 3: "Opens the terminal" |
| Summary line | text as shown | header |
| Up next / All sessions | section title | header |
| (+) / (⋯) | "New session" / "Settings" | button |
| Back | "Back to sessions" (+ ", {n} need you" with a badge) | replaces "Back to agents" [P app/screens/ChatScreen.tsx:131] |
| Diff pill | "Review changes, {n} added, {m} removed" | [P app/screens/ChatScreen.tsx:147] |
| Terminal button | "Open terminal" | replaces "Open raw terminal" [P app/screens/ChatScreen.tsx:155] |
| Connectivity pill | text as shown | Reconnecting: "Double-tap to retry now" |
| Composer field | "Message {Provider}" | — |
| Primary button | "Send" / "Queue" / "Stop" | Queue: "Sends while {Provider} works" [R08 §F8] |
| Context chip | "{n} percent of context used" | "Shows context details" |
| Jump button | "Scroll to latest" | keep [P app/components/TimelineView.tsx:234] |
| Tool group header | summary text + "collapsed" / "expanded" | button |
| Approval panel | on appear, announce "{Provider} needs you: {toolName}" | header on "{toolName} · 1 of N" |
| Option row | "{n}. {label}" | button; selected trait when chosen |
| Feedback field | "Tell {Provider} what to do differently" | — |
| Pending bubble | "{text}, sending" / "not delivered" | Not delivered: "Double-tap to retry" |
| Terminal Log text | selectable text | — |
| Terminal Grid | one element: "Terminal, {cols} columns. Switch to Log to read." | Grid cells are hidden from VoiceOver |
| Status spinner / pulse | hidden (`accessibilityElementsHidden`) | the word carries the status |

### 8.2 Dynamic Type

- Every `Text` scales (RN default `allowFontScaling`) [R08 §F8].
- Caps via `maxFontSizeMultiplier` (**new**):
  - prose, bubbles, row titles, panel text: 2.0;
  - chips, pills, status labels, header subtitle: 1.4;
  - code, diff, terminal Log: 1.5, with horizontal scroll;
  - terminal Grid: 1.0 (fit-to-width owns the size, [R16 §F6]).
- Layouts grow instead of truncating:
  - row height is a 62 minimum, and the title wraps to 2 lines above the accessibility sizes;
  - the composer grows (up to 8 lines, then scrolls);
  - option rows wrap.
- The header title pill truncates the title to 1 line and the subtitle to 1 line; VoiceOver reads the full text.

### 8.3 Other

- Status is always a word, never colour alone (§2.2).
- Reduce Transparency (`isReduceTransparencyEnabled`): `Glass` renders a solid `surface` with a `stroke` border (**new**).
- Hit targets ≥ 44 pt (the 34 pt composer buttons get `hitSlop` 5, **new**).
- Contrast rules: §2.1.

---

## 9. Copy

Tone: short, plain, sentence case, no exclamation marks. Say "Session" (CONTEXT.md).

| Where | Text | Source |
|---|---|---|
| Sessions title | "Sessions" | CONTEXT.md; replaces "Agents" [P app/screens/AgentsScreen.tsx:13-18] |
| Summary | "{n} needs you" / "{n} need you" · "{n} failed" · "{n} done" · "{n} working"; fallback "{n} sessions" | [R08 idea 08-11] |
| Sections | "Up next", "All sessions" | **new** |
| Empty list | "No sessions yet" / "Start one on your Mac." (Phase 4 adds "New session") | **new**, replaces [P app/screens/AgentsScreen.tsx:24] |
| Not attached meta | "Not attached · open on your Mac" (Phase 3: "Not attached · terminal only") | [P app/screens/AgentsScreen.tsx:39] |
| Composer placeholder | "Message {Provider}…" | [P app/screens/ChatScreen.tsx:174] |
| Shell chip | "Runs in your shell" | [R18 §F6 R8] |
| Working row | "Working {elapsed} · {model}" · "Compacting context {elapsed}" · "Sending…" · "Needs you" | [P app/components/TimelineView.tsx:67-98; R05 §UI spec "Working trailer"] |
| Elapsed | `Ns`, `Nm Ns`, `Nh Nm`, `Nd Nh` | [R05 §UI spec "Working trailer"] |
| Bubble fold | "Show more" / "Show less" | [R05 idea 05-12] |
| Delivery | "Not delivered · Tap to retry" · "Sends when your Mac is back" | [R08 §S3]; **new** |
| Connectivity | "Offline — sends are saved" · "Reconnecting in {n}s" · "Connecting to {host}…" · "Can't reach {host}. Is your Mac awake (lid open) and on Tailscale?" | [R08 §S3; R19 §F2]; **new** |
| Approval | "{toolName} · {i} of {n}" · "Do you want to proceed?" · "Yes" · "No" · field "Tell {Provider} what to do differently" | [R08 §S5; P app/components/PermissionSheet.tsx:61,73-83] |
| Needs-you notice | "{Provider} needs you in the terminal." / "Answer on your Mac." ("Open terminal" only with S7) | **new** |
| Plan | "Plan" · "Approve" · "Keep planning" · "Feedback (optional)" | PRD FR 12-3 |
| Restore | "Resumed" · "Interrupted by restart" · "Full access resumed as Ask" · "Couldn't resume" | PRD FR 09-6 |
| Context | "{n}% context" · "Context window" · "{used} / {window} tokens" · "{left} tokens remaining" · "Compact now" | [R05 §UI spec "Context ring"; R13 §UI spec] |
| Session ended | "Session ended." | **new** |
| Changes | "Changes" · "Edits in this session · {n} files" · "Edit {i} of {n}" · "Empty file" | **new**; [P app/components/DiffView.tsx:26] |
| Terminal | "Read-only" · "Log" / "Grid" · "Showing {cols}-column terminal" · "Jump to bottom" · "Connecting…" · "This session is not running." · "Process exited with code {c}" | [R16 §UI spec] |
| New session | "New session" · "What should we work on in {project}?" · "Describe what the agent should do…" · chips per §4.8 · "On your Mac: ⌘K → Phone access level" · "Codex can't plan first in a terminal session" · "Trust this folder in Claude on your Mac first" | [R17 §S1, §S3, §S7]; **new** |
| Pair | "Pair with your Mac" · "On your Mac, press ⌘K → Pair phone, then scan the code with the Camera app." · "Enter code instead" · "Mac address" · "Code" · "Continue" · "Pair with {name}?" · "Name on your Mac" · "A paired phone can run commands on this Mac as you." · "Pair" · "Cancel" | [R18 §UI spec "Phone"; R18 idea 18-18; R19 §F2] |
| Pair errors | "Code expired. Make a new one on your Mac." · "This code was already used." · "Can't reach {host}. Is Tailscale on?" · "Update Pocket on your Mac." · "Update Pocket on this phone." | [R18 §UI spec "Phone"]; **new** (last) |
| Revoked | "This phone was removed." · "Pair again" | [R18 §UI spec "Phone"] |
| Settings | "This Mac" · "via {host}" · "Paired {Mon D}" · "Notifications" · "When a session is done, needs you or fails" · "Turned off in iOS Settings" · "Needs you" · "Done" · "Failed" · "Show details on lock screen" · "Require Face ID" · "Unpair this phone" · "Unpair? You'll need your Mac to pair again." · "Disconnect" (pre-R18) | [R18 §UI spec "Phone"; R08 §S8]; cue names per CONTEXT.md |
| Pre-prompt | "Get notified" · "When a session is done, needs you or fails" · "Turn on" · "Not now" | [R19 §F2 P4] |
| Denied alert | "Notifications are off" · "Allow notifications for Anywhere in iOS Settings." · "Not Now" · "Open Settings" | [R19 §F2 P4] |
| Push | title = project name; body "Needs you" · "Done" · "Failed" | [R18 §F6 R12], wording per CONTEXT.md (§12) |

---

## 10. Phased adoption (6–8 weeks)

Each phase must pass `pnpm --filter @pocket/app typecheck && pnpm --filter @pocket/app test` [R14 §F7]. Pure logic lives in `src/*.ts` with `test/*.test.mts`, as in `status.test.mts` does today.

| Phase | Week | Scope (phone) | pocketd / protocol dependency | Tests |
|---|---|---|---|---|
| **0 Fix now (D12)** | 1 (1 day) | §4.4 `composerAction` morph; Return never stops. Manual check: Queue on claude and codex (§13) | none | `composer.test.mts`: not working → send; working + text → queue; working + "  " → stop |
| **1a Tokens + nav** | 1–2 | §2 one token set, light mode (app.json, Info.plist, StatusBar, Glass); D1 tones in `status.ts`; native stack (§3); "Sessions" copy; remove dead Attach / Dictate / terminal buttons | none | `tokens.test.mts` contrast; update `status.test.mts` tones |
| **1b List + chat** | 2–3 | §4.2 Up next + stable order + summary; §4.3 tool summary, fold rules, bubble fold, scroll rules; §4.5 approval panel with `Record<requestId>` and the Needs-you notice; §4.6 Changes; composer shapes + drafts | none | `order.test.mts` (stable order, Up next ranking); `summary` grammar; permissions map add / resolve / stale |
| **1c Connectivity** | 3 | §5.1 graced states, backoff 250 ms → 16 s, redial on AppState / NetInfo, `sinceSeq` resync; §5.2 held / sending / not delivered; interim Connect fixes (§4.1) | none (pocketd already acks prompts) | `connectivity.test.mts` pill precedence and the 4 s / 120 s timers; outbox state machine |
| **2 Pair + push** | 4–5 | §4.1 Pair / EnterCode / ConfirmPair / Revoked; §4.9 Settings (This Mac, Unpair, notifications); pre-prompt card; foreground handler + tap-to-open | R18 R1–R6 (bind, device tokens, `pair`, scopes) (D10); 19-10 protocol range; `push.register/unregister {token, prefs}`; Expo sender with Seen-aware triggers (D3); `aps-environment` + EAS projectId | pair URL parser; push prefs payload; handler suppresses only the open session |
| **3 Terminal (D9)** | 6 | §4.7 read-only terminal, Log / Grid; not-attached rows open it; "Open terminal" in the notice | `vt.Frame`, `terminal.watch/unwatch/frame/history` (16-13); `observe` scope | grid font-fit function; frame → spans mapping |
| **4 New session (D5)** | 7–8 | §4.8 modal and states; (+) in Sessions; empty-state button | `agent.create` LaunchSpec + argv builder (17-10); project + worktree list for the phone; allowed access modes and CLI availability in `hello.ok`; created `agentId` in the reply; `spawn` scope; folder-trust check (R18 R7) | LaunchSpec builder from chip state; access gating; slug from prompt |
| **When 14-21 lands** | any | §4.3 context chip (D7); worktree in row meta; `turnStartedAt` timer | AgentSummary usage, window, worktree, branch, `turnStartedAt` [R14 idea 14-21; R05 idea 05-8] | chip thresholds 74 / 75 / 89 / 90 / unknown |
| **Later** | — | key bar + `terminal.input` (16-18, D10); lock-screen Allow / Deny (08-19); question panel (05-4); long-press Queue / Steer / Stop and send (08-12, D4); attachments (05-13); send runway (08-15); Face ID (18-14); pocketd dedupe + auto-resend (08-8); in-app QR scanner (18-4); iPad (08-20, out per D13) | per idea | — |

Order constraints:
- 0 before everything.
- 1a before 1b, because every new component uses the tokens.
- 1c before 2, because pairing reuses the connection states.
- 2 before 3, 4 and all Later items that touch the Mac (D10).

---

## 11. Code touch map (for /implement)

| File | Change | Phase |
|---|---|---|
| `app/components/Composer.tsx` | `composerAction`; controlled text; multiline; drop Attach / Dictate; chips | 0, 1b |
| `app/design.ts`, `app/theme.ts` | `palette {light, dark}`, `useTokens`; delete `theme.ts` | 1a |
| `app/status.ts` | D1 tones | 1a |
| `app/App.tsx` | native stack; auto-connect; `StatusBar auto`; drop global PermissionSheet | 1a, 1c |
| `app/components/Glass.tsx` | scheme prop; Reduce Transparency fallback | 1a |
| `app.json`, `ios/Anywhere/Info.plist` | `automatic` interface style | 1a |
| `app/screens/AgentsScreen.tsx` → `SessionsScreen.tsx` | §4.2 | 1b |
| `app/screens/ChatScreen.tsx` → `SessionScreen.tsx` | header, pill, panel / notice slot, Changes / Terminal navigation | 1b |
| `app/components/PermissionSheet.tsx` → `ApprovalPanel.tsx` | §4.5 | 1b |
| `app/components/ToolGroup.tsx`, `TimelineView.tsx`, `DiffView.tsx` | summary grammar, fold, bubble, scroll, diff metrics | 1b |
| `app/session.tsx` | `permissions` map; drafts; outbox; ack / error correlation; resync | 1b, 1c |
| `app/client.ts` | backoff, `retryAt`, redial triggers, unique ids | 1c |
| new `app/screens/{Pair,EnterCode,ConfirmPair,Revoked,Settings,Changes,Terminal,NewSession}.tsx` | §4 | 1b–4 |
| new `app/{order,composer,connectivity,summary}.ts` + tests | pure logic | 0–1c |

---

## 12. Decisions made here (not settled elsewhere)

- Up next rows also appear in All sessions, so the main list never reorders (D2).
- No status pill: Working and Needs you live in the timeline tail; the pill is for connectivity only [R08 idea 08-16 note].
- Composer hidden while Needs you, so typed text can't land in a TUI dialog.
- The approval panel clears a request only on `ack` or `permission.resolved`.
- Drafts and the outbox are in memory for now. Written-but-unacked sends become Not delivered instead of being resent, until pocketd dedupes.
- There is no in-app scanner in v1; the system Camera opens the deep link [R19 §F2].
- No appearance override; the app follows iOS.
- New session: no base-branch chip (the phone never sends `base`, D41); the prompt is required; no Full access; Auto modes up to `phone.maxAccess` (D18) [R18 §F6 R7].
- Push body and settings copy use CONTEXT.md terms ("Needs you" / "Done" / "Failed") instead of R08 / R18's "Waiting on your input" / "Run finished" / "Run failed". The fixed-copy privacy rule [R18 §F6 R12] still holds.
- Changes and Terminal keep the session Seen.
- The back button carries a badge for other sessions that need you, replacing the global sheet.
- VoiceOver starts the terminal in Log mode; Grid is a single element.

## 13. Open questions

- **Queue on the PTY:** answered per provider by the E01 step-0 check (D37): "Queue" where the text queues, else "Send", until E10 wires `turn/steer` / `turn/start` [R05 §Open questions].
- **"No" interrupts the turn** [P pd/daemon/daemon.go:148]. Should the label say "No, stop"? Claude's own dialog says "No".
- **Project registry** for New session: pocketd must own or mirror it [R18 §Open questions].
- **Folder-trust dialog** in phone-started Claude sessions [R18 §Open questions]: answered by PRD FR 06-6 (pre-seed for Worktrees of trusted Projects, else `folder_not_trusted`).
- **Lock-screen actions** need a background reconnect within ~30 s, or an HTTP endpoint [R08 idea 08-19; R18 §Open questions].
- **`supportsTablet: true`** [P packages/app/app.json:11] while D13 is iPhone only: keep it (the app runs at iPhone layout, capped at 768) or turn it off?
