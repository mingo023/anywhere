# Antigravity quota on the built-in account

> **For Claude:** run with `/brainless docs/plans/2026-11-25-antigravity-quota.md`. Each numbered section is one commit-sized unit; land them in order.

**Goal:** Settings › Accounts shows Antigravity's built-in login with its Gemini 5-hour and weekly usage. pocketd reads that usage from `agy`'s own `/usage` report in print mode. This is PR 3 of `docs/orchestrators/research/22-antigravity-parity.md` (§2.5, §4 item 3).

**Live check and later changes.** Run against a real agy 1.5.16 (research 23, check 6, and the follow-up PR). Where they disagree with the rest of this plan, they win:

- The free tier's Gemini group has **only a `weekly` bucket**, no `5h`, so `Session` is often empty. An unused bucket's `reset_time` is the Unix epoch, read as no reset. `accounts/testdata/agy-usage.json` is now that live capture.
- `agy --version` prints `1.5.16`. The flags `--print-timeout`, `--settings`, `--disable-slash-commands` and `--log-file` exist on 1.5.16.
- Signed out, `--print /usage` prints `Authentication required. Please visit the URL…` on stderr, **runs `open` with the Google sign-in URL** (a browser tab), waits up to 60 s for a pasted code, then prints `{"status":"ERROR","error":"authentication failed or timed out","num_turns":0,…}` and exits 0. The probe stops it at the sign-in line, and puts no-op `open` and `xdg-open` first on its PATH so no tab opens. agy ignores `BROWSER` and has no no-browser flag.
- A signed-in `/usage` took 3.9–4.4 s.
- **Antigravity is listed only while agy is found** (`Store.Agy`). With no agy there is no row, rather than a signed-out one.
- `account.list` still waits at most 5 s (`usageWait`) on the refresh, but the refresh now runs on past that wait, so a slow probe's usage is there for the next list instead of being killed.

**Decisions already made:**
- Antigravity has **one account, the built-in login**. agy keeps its credential in the OS keyring, so pocketd can't keep other accounts or switch between them. There is no "Add account", and no ⋯ menu on its row on the desktop (Sign out would do nothing: `specCLI.logout` returns nil, `pocketd/internal/launch/agents.go:158`).
- **No protocol change.** Gemini's `5h` bucket fills `Usage.Session` and its `weekly` bucket fills `Usage.Week`, so it reaches the desktop as `usage.session/week` and iOS as `fiveHour/week`. `proto.Account` and the goldens stay as they are.
- **Only the Gemini pools.** The third-party-model pools are dropped (research §4 item 7, deferred).
- **The guards run in this order, and each stops the probe:** agy is found, then its version is ≥ 1.1.11, then `/usage` is run. Its stdout must be the CLI's own report. A sign-in line on stderr marks the row signed out. An old CLI sends `/usage` to the model as a prompt, so the version gate always runs first.
- **The refresh floor and single-flight live in `accounts`**, not in `workspace.UsageTTL`. `Store.Refresh` has three callers:
  - `account.list`, at `workspace/accounts.go:71`, with a TTL of 1 min;
  - the desktop's `accounts.usage` op, at `launch/accounts.go:27`, with no TTL (the desktop polls every 300 s and on reload);
  - a limit move, at `daemon/switch.go:67`.
- **iOS needs no change.** iOS has no "Add account" control and no accounts page: `grep -rn "Add account" packages/ios` finds nothing. Its only touchpoints are:
  - the new-session account picker (`FeatureSessions/NewSessionView.swift:233`), which already lists a lone built-in Claude login the same way;
  - `AccountChip`, which shows only when a provider has more than one account.

  Its name and meters come from pocketd, so no `swift test` is needed.

**Not in scope:** extra quota pools; reading agy's account email (no known source, see Unverified); hiding the rows of providers that aren't installed (Codex's row already shows when Codex is absent). Antigravity's row was later hidden without agy; see above.

**Hard rule:** never name the third-party product that the research doc calls "a competitor", in any file, comment, test, branch or commit message.

**Toolset:**
- pocketd (`packages/pocketd`): `gofmt -l .`, `go vet ./...`, `staticcheck ./...`, `go test ./internal/accounts/ ./internal/launch/ ./internal/workspace/ ./internal/daemon/`, then `go test ./...`.
- desktop (`packages/desktop`): `cargo fmt --all`, `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p agents providers`, `cargo test -p pocket accounts`, then `cargo test --workspace`.
- Screens: from the repo root, `.ui-review/fixture/capture.sh <dir> <name>=settings,settings-accounts`.

**Read first:**
- `CLAUDE.md`, `docs/adr/0003-desktop-code-layout.md`, and research doc §2.5.
- `pocketd/internal/accounts/accounts.go` (the `login` interface, `noLogin`, `Providers`, `List`, `Add`, `Finish`), `usage.go` (`Refresh`), `claude.go` and `codex.go` (two `login`s), `pick.go`, `accounts_test.go:300-329`.
- `pocketd/internal/launch/accounts.go` (`ConfigDir`, `signIn`), `launch.go:619` (`find`), `cmd/pocketd/serve.go:206-222` (where the `Store` and `Launcher` are built).
- `desktop/crates/pocket/src/settings/accounts.rs` (`machine_rows`, `account_row`), `crates/pocket/src/accounts.rs` (`name`, `subline`), `crates/agents/src/providers.rs` (`Builtin`, `pastes_code`).

---

## 1. Antigravity gets a built-in-only account row

`Providers` now means "has an account row". A new predicate means "keeps other accounts in their own folders". Assign every call site of `Providers` to one meaning:

- `accounts/accounts.go`:
  - add `func Managed(provider string) bool { return envKey(provider) != "" }`, with a doc line saying pocketd keeps that provider's accounts in their own folders beside its built-in login;
  - `logins` gains `providers.Antigravity.ID: antigravity{}`;
  - `Add` refuses `!Managed(provider)` with `ErrUnknown`, in place of `!slices.Contains(Providers, provider)`;
  - update the package comment: it also lists Antigravity's built-in login.
- `accounts/antigravity.go` (new): `type antigravity struct{ noLogin }`. For now `status` returns `(false, false)`; unit 2 fills `status` and `usage`. `envKey`, `unset`, `identity`, `shared`, `history` and `shareMore` come from `noLogin`. An empty `envKey` already makes `Store.Env` and `Store.Of` no-ops.
- `launch/accounts.go`:
  - `ConfigDir` (`:63`): `!accounts.Managed(provider)` returns `""`. Without this the skills scan would get `~/.antigravity`.
  - `signIn` (`:92`): `id == "" && !accounts.Managed(provider)` maps to `accounts.BuiltIn`. Without this, a sign-in would call `Add`.
- `Unset` keeps iterating `Providers`. Antigravity's `unset()` is nil, so it's unchanged.

Tests (`accounts/accounts_test.go`):
- `TestOnlyClaudeAndCodexHaveAccounts` → `TestClaudeCodexAndAntigravityAreListedButOnlyClaudeAndCodexKeepMoreAccounts`. It checks `Providers == [claude codex antigravity]`, and that `Managed` holds for the first two only.
- `TestAProviderWithoutAccountsRunsOnItsOwnLoginOnly` keeps skipping `Providers`, so it now skips Antigravity.
- New: `TestAntigravityRunsOnItsBuiltInLoginOnly`. `Env` is nil, `Of` is `""`, `Resolve("antigravity", "")` is `builtin`, `Add` errors, and `List` holds exactly one Antigravity row, the built-in one.
- `launch`: a test beside `ConfigDir` that Antigravity's config dir is `""`.
- Fix any existing assertion on a whole `List()` that now gains the Antigravity row. Check `accounts_test.go`, `pick_test.go`, `daemon/switch_test.go` and `wsserver/workspace_test.go`.

## 2. The `/usage` probe

All of it lives in `accounts/antigravity.go`, with tests in `accounts/antigravity_test.go`.

### Injection

There are two seams. Both are `Store` fields; the first is wired in `serve.go` like `Secret`.

```go
// Agy finds Antigravity's CLI as a launch would: its path and the login env. false when it isn't installed or is a shell function.
Agy func() (exe string, env []string, ok bool)
// runAgy runs it; nil runs the real process. Tests set it, so they never need agy.
runAgy func(ctx context.Context, exe string, env []string, dir string, args ...string) (agyRun, error)
```

`type agyRun struct{ out []byte; code int; signIn bool }`. `out` is stdout. `signIn` is true when stderr carried the sign-in line. stderr itself is never returned.

`cmd/pocketd/serve.go`, after `l := launch.New(...)`:

```go
d.Accounts.Agy = func() (string, []string, bool) {
	env := d.LoginEnv()
	exe, ok := l.Find(providers.Antigravity.ID, env)
	return exe, env, ok && filepath.IsAbs(exe)
}
```

`launch.Launcher.Find` is a new exported one-liner over `find` (`launch.go:619`). It goes through `find` so that it honours Settings' command for agy, the login PATH, and the 30 s probe cache. A nil `Agy` (every existing test) counts as not installed and spawns nothing.

### The real runner, `runAgyProcess`

- `exec.CommandContext(ctx, exe, args...)` with:
  - `Dir = dir`, an empty `os.MkdirTemp("", "agy-usage-")`, removed afterwards with the log file in it;
  - `Env = append(env, "AGY_CLI_DISABLE_AUTO_UPDATE=true")`;
  - `WaitDelay = 2 * time.Second`;
  - stdin nil.
- stdout goes to a buffer capped at 1 MiB.
- stderr goes to `signInWatch`, an `io.Writer` that scans whole lines, including ones split across writes. Its buffer is capped at 64 KiB. On a line containing `Authentication required` it sets `signIn` and calls the `cancel` of a child ctx, which kills agy before it can wait on a browser login. Nothing else from stderr is kept.
- `code` is `ProcessState.ExitCode()`.
- A run that never started returns the `exec` error. A run killed by the deadline returns `ctx.Err()`.

### Commands (exact argv)

1. Version gate: `agy --version`.
   - Parse the first `(\d+)\.(\d+)\.(\d+)` in stdout and compare it with `1.1.11`. Unparseable counts as too old.
   - Cache the result keyed by `exe` + its mtime (`os.Stat`; a failed stat keys on the path alone). A passing binary is never checked again. A failing one isn't run again until its path or mtime changes.
2. Usage:

   ```
   agy --print /usage --output-format json --print-timeout 10s --settings {"disableSlashCommands":false} --disable-slash-commands=false --log-file <dir>/agy.log
   ```

   `--settings` is one argv element holding exactly `{"disableSlashCommands":false}`; no shell is involved.

### Timeouts and the floor

```go
const (
	agyTimeout = 12 * time.Second // both runs together; agy's own --print-timeout is 10s
	agyFloor   = 5 * time.Minute  // from the start of the last finished probe
	agyMinimum = "1.1.11"
)
```

- The deadline is the caller's ctx, capped at `agyTimeout`.
  - A probe that the **caller's** ctx cancelled leaves the outcome and usage unchanged, and returns the cached result as below. It still starts the floor, so a hanging agy costs a caller at most one wait per floor. `limitHit`'s 10 s `LimitRefresh` can cut it.
  - `Refresh` waits on every account, so `account.list`, the desktop's poll and a limit move can now wait on the probe: up to 12 s, once per floor.
  - A probe that hit our own 12 s cap is a failure: it starts the floor, and `Refresh` marks the last usage stale.
- Single-flight:
  - `agyProbe` holds `run sync.Mutex`, held across a probe, and `mu sync.Mutex`, which guards its fields.
  - A caller that waited on `run` and then finds the floor still fresh returns the cached result. That is the last `Usage` with a nil error, or the last error, so `Refresh` never marks a fresh read stale.
  - `status` reads only under `mu`, because `List` calls `status` while holding `s.mu`, and a 12 s probe must not block `List`.
- `Finish(provider, id, …)` with `provider == antigravity` calls `s.agy.forget()`. That clears the floor and the signed-out flag. It runs when Settings' "Sign in…" terminal ends. That terminal is bare `agy` (`specCLI.login`, `launch/agents.go:150`), which probably stays in its TUI after signing in, so the row recovers when the user quits it, or else at the floor.
- `Refresh` (`usage.go:34`) still probes Antigravity's row when it is signed out: skip `!a.SignedIn` only when `a.Provider != providers.Antigravity.ID`. Add a one-line why: its sign-in is known only from its probe. Without this, a signed-out row would never be probed again. Signed out, it costs one short spawn per floor, killed at the sign-in line. Not installed, it costs nothing.

### Outcomes

`usage()` first checks `Agy`. If agy isn't found, nothing runs and the outcome is `missing`. Otherwise it runs the version gate, then `/usage`, and maps the result:

| Outcome | Condition | `status` signedIn | Usage | `Account.Error` (desktop note) | Probed again |
|---|---|---|---|---|---|
| `unknown` | never probed | false | none | — | next refresh |
| `missing` | `Agy` nil or `ok == false` | false | none | — | next refresh (no spawn) |
| `old` | version < 1.1.11 or unparseable | true | none | `Update agy to 1.1.11 or later to see its usage` | when the binary changes |
| `notReport` | exit 0, stdout fails the report guard | true | none | the same note | when the binary changes (latched: a retry could be a model turn) |
| `signedOut` | `signIn` set (checked before anything else) | false | none | — | after the floor, or at once after a sign-in |
| `apiKey` | exit code 2 and stdout isn't a report | true | none | — | after the floor |
| `failed` | timeout, other exit code, non-JSON with a non-zero exit | as before | last one, marked stale by `Refresh` | — | after the floor |
| `ok` | a report | true | the Gemini windows | — | after the floor |

- What `usage()` returns, given that `Refresh` (`usage.go:45-52`) overwrites on a nil error and marks the old usage stale on any error:
  - every outcome whose Usage column says "none" returns `Usage{At: now}` with a nil error, so an earlier good read never lingers as "As of …";
  - `failed` returns an error;
  - a caller-cancelled probe returns the last result: its `Usage` with a nil error when there is one, else its error.
- Errors never wrap stdout or stderr: use fixed sentinel errors such as `errAgyNotReport` and `errAgySignedOut`. Nothing from either stream is logged.
- `List` sets `Error` on Antigravity's row from `s.agy.note()`, the only non-pending row that carries one. Update `Account.Error`'s comment in `accounts.go` and `daemon::Account.error`'s comment in `desktop/crates/daemon/src/daemon.rs` to match: "why a sign-in failed, or why a signed-in login shows no usage".

### Report guard and mapping

Accept stdout only when all three hold: `status == "SUCCESS"`, `num_turns` is present and `0`, and `command.name == "usage"`. Then:

- For each group whose `name` contains `gemini` (case-insensitive), take the first bucket with `window == "5h"` as `Session` and the first with `window == "weekly"` as `Week`.
  - Bucket ids are never used; they are [CT] only.
  - Skip a bucket with `disabled: true`.
  - Skip a bucket with no `remaining_fraction`: a missing meter is better than a false 100 %.
- `Percent = clamp((1 - remaining_fraction) * 100, 0, 100)`.
- `ResetsAt` is `reset_time` parsed as RFC 3339, in unix ms. It is `0` when missing or unparseable.
- `At = now`. A report with no Gemini group is `ok` with both windows nil, shown as no meters.

Expected JSON as first guessed (research §2.5 [C]/[CT]); the live report differs, see "Live check" above:

```json
{"status":"SUCCESS","num_turns":0,"command":{"name":"usage","data":{"groups":[
  {"name":"Gemini Models","buckets":[
    {"id":"gemini-5h","window":"5h","remaining_fraction":0,"reset_time":"2026-11-23T18:00:00Z"},
    {"id":"gemini-weekly","window":"weekly","remaining_fraction":0.75}]},
  {"name":"Claude and GPT models","buckets":[
    {"id":"3p-weekly","window":"weekly","remaining_fraction":1},
    {"id":"3p-5h","window":"5h","disabled":true}]}]}}}
```

Keep it as `accounts/testdata/agy-usage.json`. The test that loads it carries a comment saying it is derived from research doc §2.5 and not captured. The doc's sample elides a field with `…`, which this version drops so it stays valid JSON. It maps to `Session{Percent: 100, ResetsAt: 1795456800000}` and `Week{Percent: 25, ResetsAt: 0}`.

### Tests (`accounts/antigravity_test.go`)

Use a fake `runAgy` keyed on `args[0]`, which records calls, and a fake `Agy`.
- `TestTheUsageReportFillsTheSessionAndWeekFromGeminisPools`: the fixture maps as above, and the third-party pools are ignored.
- `TestADisabledOrUnreadBucketShowsNoMeter`
- `TestOutputThatIsNotTheCLIsOwnReportIsRejected`: table cases:
  - `status:"ERROR"`;
  - `num_turns:1` with a `result` string (a model answer);
  - no `num_turns`;
  - `command.name:"quota"`;
  - plain text;
  - empty.

  Each leaves no usage and the note.
- `TestAnAgyOlderThanTheGateIsNeverAskedForItsUsage`: `1.1.10` gives only the `--version` call, the note, and signed in. A second refresh past the floor makes no call. A changed mtime (or path) checks again.
- `TestAVersionAgyCantSayCountsAsTooOld`
- `TestTheSignInLineMarksAntigravitySignedOutAndASignInEndingClearsIt`
- `TestAntigravityIsProbedAtMostOncePerFloor`:
  - two `Refresh` calls in a row make one run;
  - two concurrent ones make one run;
  - moving `s.agy.at` back by `agyFloor` makes a second run;
  - the cached hit leaves `Stale` false.
- `TestAProbeTheCallerCancelledKeepsTheLastReadFresh`: through `Refresh`, after a good read, the last usage stays and isn't stale, and the next probe waits for the floor.
- `TestALaterOldOrNoDataOutcomeDropsTheEarlierMeters`
- `TestWithoutAgyTheRowIsSignedOutAndNothingRuns`: nil `Agy`, and `ok == false`.
- `TestAGeminiAPIKeyExitLeavesTheRowSignedInWithoutUsage`: code 2.
- `TestNothingAgyPrintedReachesAnError`: stdout and stderr hold `https://accounts.example/login?code=SECRET`, and neither the returned error nor `List` contains `SECRET`.
- `TestTheSignInWatchStopsAgyOnTheSignInLine`: write the line in two chunks, and check that `cancel` was called and nothing is kept.
- `TestTheRealRunnerKillsAgyAtTheSignInLine`: skipped on Windows. `runAgyProcess` with `/bin/sh -c 'echo "Authentication required. Please visit the URL to log in" >&2; sleep 30'` returns `signIn` within 3 s. Precedent for `/bin/sh` fakes: `launch/agents_test.go`.

## 3. A signed-in login with no email isn't "Not signed in"

agy's identity isn't readable, so the row has no label and no email.

- `pocketd/internal/workspace/accounts.go` `name`: after label and email, return `Signed in` when `a.SignedIn`, else `Not signed in`. iOS shows this name.
- Desktop `crates/pocket/src/accounts.rs` `name` (`:255`): the same fallback. Its doc says it mirrors pocketd's.
- Tests:
  - `workspace`: `TestASignedInAccountWithNoEmailIsNamedSignedIn`;
  - desktop `accounts.rs`: `a_signed_in_login_with_no_email_is_named_signed_in`.

## 4. Desktop: no "Add account" and no ⋯ menu for Antigravity; the note shows

- `crates/agents/src/providers.rs`:
  - add `pub adds_accounts: bool` to `Builtin`, documented "pocketd keeps more accounts of it beside its own login, so Settings › Accounts offers Add account";
  - `true` for `claude` and `codex`, `false` in `CHAT_ONLY_DEFAULTS`;
  - test `only_claude_and_codex_take_more_accounts`.
- `crates/pocket/src/settings/accounts.rs`:
  - `machine_rows` (`:135`): build `add` only when `agents::providers::get(provider).is_some_and(|b| b.adds_accounts)`;
  - `account_row` (`:194-199`): same condition for the ⋯ trigger and its menu;
  - the default radio stays as it is. It is a lone selected account, as for a lone Claude login.
- `crates/pocket/src/accounts.rs` `subline`: when `!a.pending && !a.error.is_empty()`, append `a.error`, last. Test `a_signed_in_login_says_why_it_shows_no_usage`.
- Before/after captures:
  - `.ui-review/fixture/fixture.ts`: add `FIXTURE_ANTIGRAVITY=usage|out|old` to `ACCOUNTS`, which adds one `{id:"builtin", provider:"antigravity", …}` row:
    - `usage`: signed in, `usage(100, 25)`, with the session reset 2 h ahead;
    - `out`: `signedIn:false`, no email;
    - `old`: signed in, no usage, with the note in `error`.

    Document it in the header comment beside `FIXTURE_ONE_ACCOUNT`.
  - Capture `settings,settings-accounts` for each, plus one without the variable as the "before". Each screen should show a single Antigravity row with no "Add account", no ⋯, and the right meters, sign-in action or note.

## UI states (what each surface shows)

| State | Desktop Settings › Accounts | iOS |
|---|---|---|
| Not installed | no Antigravity row | no Antigravity row |
| Not probed yet | "Not signed in", subline "Antigravity isn't signed in on <machine>", "Sign in…" | not in the picker (`filter(\.signedIn)`) |
| Signed out | the same, with "Sign in…" running bare `agy` in a terminal | not in the picker |
| Old CLI, or not its report | "Signed in", no meters, subline note "Update agy to 1.1.11 or later to see its usage" | "Signed in", no percent (indistinguishable from no data) |
| No data (API key, no Gemini group, every bucket skipped) | "Signed in", no meters | "Signed in", no percent |
| Read | "Signed in", 5h and Week meters; "Limited · back 18:00" at 100 % | "Signed in · 100%" in the picker |
| Read failed after a good one | meters greyed, "As of 14:05" | last percent |

Antigravity's usage is at most 5 minutes old, or 10 when the desktop's 300 s poll lands just inside the floor.

## Unverified without a live, signed-in `agy`

The live check above settled the JSON shape, the version format, the flags, the sign-in output and whether `--print` opens a browser, and timed `/usage`. The rest stays open.

These rest on research evidence only. Run research §3 check 6 before or right after merging:

- The JSON shape:
  - the group name containing "Gemini", and the `5h` and `weekly` windows;
  - whether a spent bucket sends `remaining_fraction: 0` or omits it (omitted shows no meter);
  - whether `num_turns: 0` is present (if agy omits zeros, every report is rejected);
  - `reset_time`'s format, and whether `weekly` ever carries one.
- `agy --version`'s output format.
- The flags `--settings`, `--disable-slash-commands=false`, `--print-timeout` and `--log-file` were seen only in the 1.5.16 binary, not on 1.1.11.
- The exact sign-in line, and that it goes to stderr. Also whether `--print` opens a browser before we kill it, and how long it takes to print.
- What ends Settings' sign-in terminal: whether bare `agy` exits after signing in or stays in its TUI.
- That a `GEMINI_API_KEY` run exits with code 2, from the changelog only.
- `/usage`'s latency against the 12 s cap and the desktop's 15 s op timeout.
- Whether agy's account email is readable anywhere. If it is, fill `identity` later and the row shows the email instead of "Signed in".

## Done when

- Every command in the Toolset passes.
- With no `agy` on PATH, pocketd lists no Antigravity row and spawns nothing (changed after the live check; this plan first listed a signed-out row).
- The fake-runner tests cover each row of the Outcomes table.
- The captures show the three Antigravity states with no "Add account" and no ⋯.
- `git grep -i` for the competitor's name over the diff finds nothing.
