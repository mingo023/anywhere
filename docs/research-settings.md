# Settings screen (⌘,): research report

Paths are relative to `packages/desktop` unless they start with `packages/` or `docs/`. Lines refer to `setting-research` at ba65823. Uncommitted work in `/Users/mingo/.worktrees/coding-pocket/worktree-management` is marked **IN-FLIGHT**. Its line numbers come from the input research, and I did not re-read them.

## 1. TL;DR

- **Where it opens:** a full-window `Screen::Settings` inside the existing window. It replaces the aside, column and main area, as the spec draws it (`docs/orchestrators/product/02-ux-spec-desktop.md:773-806`, ⌘, at `:776`). Not a modal, and not a second window.
- **Shortcut:** ⌘, is bound to `ProjectSettings` today (`crates/pocket/src/actions.rs:39`). Add an `OpenSettings` action and give ⌘, to it. Project settings stay one click away: from the row menu, and from Settings › Worktree management.
- **Storage:** keep everything in `desktop.json` through `store::Store`, as `sounds` does today (`crates/store/src/store.rs:28-41, 81`). That means no new file, no file watcher, no GPUI Global and no new crate in v1. All writes go through `save_soon`.
- **One new pocketd contract:** a global worktrees root. pocketd reads `desktop.json` (`packages/pocketd/internal/registry/registry.go:17-28`), and the `~/.worktrees/<name>` fallback is hardcoded on both sides (`new_session.rs:256`, `create.go:88`).
- **MVP, per section:**
  - General: Desktop notifications, plus Session sounds.
  - Appearance: System / Light / Dark, plus Reduce motion.
  - Keybindings: a read-only table.
  - Worktree management: a projects list linking to the existing modal, plus a global worktrees folder.
- **Rebinding keys is a follow-up, not MVP.** The spec plans a read-only table first (`02-ux-spec-desktop.md:1149`). Rebinding needs a command table, a keystroke recorder and a keymap re-apply, and it has precedence traps.
- **Sequencing:** land after the worktree-management branch merges. It touches `store.rs`, `chrome.rs`, `desktop.rs`, `main.rs`, `sidebar.rs`, `row_menu.rs`, `add_project.rs`, `new_session.rs` and `theme.rs`.

## 2. Current state

| Area | What exists |
|---|---|
| ⌘, | `KeyBinding::new("cmd-,", ProjectSettings, None)` at `crates/pocket/src/actions.rs:39`. The handler is `project_settings` (`modals/add_project.rs:140`), which opens `Overlay::AddRepo` in edit mode. The same handler is called from the sidebar gear `aside-settings` (`sidebar.rs:159-160`), the rail gear `nav-settings` (`sidebar/rail.rs:81-82`) and the row menu "Settings…" (`sidebar/row_menu.rs:97-99`). |
| Screens / overlays | `Screen { Sessions, Inbox }` (`desktop/chrome.rs:11-15`). `Overlay { Palette, NewSession, AddRepo, More, Confirm, PairPhone, PhoneAccess }` (`chrome.rs:44-52`). Opening is gated by `may_open` (`modals.rs:15-22`). Esc is a `capture_key_down` on the Desktop root and closes pickers, menus and overlays only (`desktop.rs:460-471`). |
| Persistence | `Store` is `$POCKET_HOME/desktop.json` (`store.rs:69-87`), and every struct is `#[serde(default)]`. `load` resets the **whole file** on any parse error (`store.rs:92`). Only `layouts` is lenient (`store.rs:134-138`). Writes are atomic, 0600 and mutex-serialised (`store.rs:140-155`). There are two save paths. `save()` runs synchronously on the UI thread. `save_soon` waits 500 ms, encodes on the UI thread and writes in the background (`desktop/geometry.rs:9, 66-72`). Both rewrite the whole file. Capture mode never saves (`geometry.rs:76-78`). |
| Mixed concerns | `desktop.json` holds user prefs (`sounds`, `repos[path]: RepoConfig{name,color,base,worktrees,setup,copy,launch}`) next to app state (`projects`, `collapsed`, `window`, `layout`, `widths`, `layouts`). |
| pocketd | It is a reader only ("the desktop is the file's only writer", `registry.go:1-3`). It re-reads the file on mtime or size change and parses only `projects` and `repos{name,base,worktrees,setup,copy}` (`registry.go:17-28`). `worktree.Dir` falls back to `~/.worktrees/<name>` (`packages/pocketd/internal/worktree/create.go:83-89`). Its own `config.json` holds `phone.maxAccess` and `restore.resumeAgents` (`internal/config/settings.go`). |
| Existing preference UI | Sound toggles exist only in the palette: `cue.flip(&mut self.store.sounds); self.store.save()` (`palette.rs:232-250, 360-361`). Pair phone and Phone access are also palette-only (`palette.rs:246-247`). |
| Theme | `static DARK: AtomicBool` (`crates/theme/src/theme.rs:15-19`). `theme::init` follows `cx.window_appearance()` (`theme.rs:289-292`, called at `main.rs:39`). The observer is `desktop.rs:117`, which calls `Desktop::set_appearance` (`desktop.rs:199-205`). There is no user override. `App::set_window_appearance(Option<_>)` exists (`vendor/gpui-pre/src/app.rs:1508`; macOS sets `NSApp.appearance`, gpui-pre-macos `platform.rs:726-746`), but nothing calls it. |
| Reduce motion | `follow_reduce_motion` mirrors NSWorkspace (`desktop.rs:510-514`), at launch (`main.rs:41`) and on every activation (`desktop.rs:116`). |
| Keymap | Bindings are compiled in and bound once, after `gpui_kit::init` (`main.rs:38, 42-43`). `keys::bindings()` adds Terminal `tab`/`shift-tab` mapped to NoAction (`crates/keys/src/keys.rs:6-8`). A uniqueness test exists (`actions.rs:80-88`). There is no menu bar: `set_menus` is never called, though GPUI has it (`app.rs:2526`). |
| Window | One window, one `Desktop` entity (`main.rs:68-118`). Every feature's data is a field on `Desktop`. |
| Spec | §3.10 Settings surface (`02-ux-spec-desktop.md:773-812`). v1 = Appearance + Notifications + Projects list opening the existing modal (`:1144`). v2 = search + read-only Keybindings + project pages (`:1149`). ⌘, retargets to Settings (`:967`, `:1174`). Nav width equals aside width (`:1192`). |

## 3. Architecture

### Entry point

- **`Screen::Settings`.** I checked the conflict between inputs: the Keybindings research proposed an Overlay variant, but the spec (`:776`) and the other three inputs say page. It is a page, for three reasons:
  - The overlay card (`modals.rs:51-75`) can't hold a nav plus a table.
  - The Esc capture (`desktop.rs:466`) would fight the page's own Back.
  - A separate window would first need `store`, `agents` and the forms moved into a Global, and a second Root/titlebar setup (`main.rs:51-64`).
- **Actions.** Add `OpenSettings`, and rebind `cmd-,` to it (`actions.rs:39`). `ProjectSettings` becomes unbound and keeps its handler.
- **Gears.** Repoint the sidebar gear (`sidebar.rs:160`) and the rail gear (`rail.rs:82`) to `OpenSettings`. The row menu (`row_menu.rs:97-99`) keeps opening the project sheet. Rename its label to "Project settings…" (open question).
- **Back and Esc.** `SettingsState.back: Screen` remembers where to return. Extend the Esc branch in `desktop.rs:460-471`: if no picker, menu or overlay is open and the screen is `Settings`, call `close_settings`. Overlays opened from Settings (AddRepo, PairPhone) still close first.
- **Layout.** When `screen == Settings`, render `settings_view` instead of `lead`, `column` and `page` (`desktop.rs:~490-495`). `visible_panes` already returns empty off `Screen::Sessions` (`desktop/alerts.rs:184-186`). This means banners and sounds fire for the session the user just left, which is the same as Inbox today (open question).
- **Follow-ups.** A palette entry "Settings ⌘,", and `cx.set_menus` with an app menu (About, Settings… ⌘,, Quit).

### Storage

- **Location.** `desktop.json` in `$POCKET_HOME`, which already isolates tests, capture and e2e.
- **Shape.** New top-level `#[serde(default)]` structs, one per section that needs them. `sounds` and `repos` stay where they are, so nothing migrates:
  ```rust
  pub notifications: Notifications,   // { banners: bool = true }
  pub appearance: Appearance,          // { mode: Mode (system|light|dark), reduce_motion: Option<bool> }
  pub worktree: WorktreeDefaults,      // { root: String } "" = ~/.worktrees; absolute, never "~"
  // Sounds gains `all: bool = true`
  ```
  I picked the key `worktree.root` so it can't be confused with `repos[].worktrees` (open question). pocketd mirrors only `worktree.root`.
- **Lenient enums (required).** An unknown `Mode` variant from a newer build would hit `store.rs:92` and wipe every project and layout. Give each enum field a fallback deserializer, following the `readable_layouts` pattern.
- **Writes.** Every Settings control mutates `self.store.*`, then calls `save_soon(cx)` and `cx.notify()`. I checked the conflict here: the Worktree research wanted an immediate background write so pocketd never sees a stale root. 500 ms of lag is irrelevant for a folder picker. Mixing a synchronous `save()` with a pending `save_soon` can lose the newer write, because the two background `write`s can take the mutex in either order (`geometry.rs:69-70` vs `store.rs:141-145`). So use one path.
- **Phase 2 only** (if hand-editable or dotfile settings, or a keymap file, are wanted): a sparse `settings.json` / `keymap.json` in `$POCKET_HOME`, patched at a key path so unknown keys survive. Keys pocketd needs stay in `desktop.json`.
- **No repo-committed project settings.** A repo-supplied `setup` command would be a trust problem, and the in-flight design rules it out.

### Crate layout (ADR 0003)

- **`store`:** the new structs and enums, the lenient deserializer, and `worktrees_dir(&self, repo, home) -> String`, a pure function (repo override → global root → `~/.worktrees`, then `/<display name>`). Update the ADR map row for `store`.
- **`theme`:** `init` takes the already-resolved appearance. No dependency on `store`.
- **`pocket`:**
  - `settings.rs` holds `SettingsState { section: Section, back: Screen }`. `new` returns `(state, subs)`, and the struct is a `settings` field on `Desktop`.
  - Children are one file each: `settings/nav.rs`, `settings/general.rs`, `settings/appearance.rs`, `settings/keybindings.rs`, `settings/worktrees.rs`.
  - Pure logic is tested beside it:
    - `Section` order and labels;
    - `forced(Mode) -> Option<WindowAppearance>`;
    - `reduce_motion(pref, system) -> bool`;
    - `shortcut_rows(&[KeyBinding]) -> Vec<Row>`.
  - Add a row to the ADR `pocket` table.
- **`ui`:** add `ui::switch(on)`. Reuse `ui::segmented` (`ui.rs:151`) and `kbd`. Don't adopt gpui-component's `setting::*`, because it is themed through its own ActiveTheme rather than our tokens.
- **Thin `impl Desktop`:** `open_settings`, `close_settings` and one setter per control (write store → apply → `save_soon` → notify).

### Live apply

| Setting | Apply |
|---|---|
| Appearance mode | `cx.set_window_appearance(forced(mode))`. The existing observer (`desktop.rs:117`) then runs `set_appearance`, which updates tokens, glass/opaque and recolours diff and commit. At launch, call it before `theme::init` and make `init` take `forced.unwrap_or(cx.window_appearance())`, so `window_background()` (`main.rs:60`) is right on the first frame. Skip it when capturing (capture forces light at `capture.rs:182`). This replaces the infra idea of gating the observer: forcing NSApp also restyles native chrome and sheets. |
| Reduce motion | `follow_reduce_motion(cx, Option<bool>)`. An override calls `set_reduce_motion` with the fixed value; `None` reads NSWorkspace as today. Callers: `main.rs:41` and `desktop.rs:116`. |
| Banners | `sync_alerts` passes `self.capturing \|\| !self.store.notifications.banners` as the existing `quiet` argument (`alerts.rs:159`). |
| Sounds | Already live: `Cue::on` is read per event (`desktop/sounds.rs:34-40`). It also needs to check `all`. |
| Worktree root | Read when a form opens (`new_session.rs:254-257`, `add_project.rs:362`). pocketd picks it up on its next mtime check. |
| Keybindings (later) | `clear_key_bindings`, then the gpui-kit snapshot taken after `gpui_kit::init`, then `actions::bindings(overrides)`, then `keys::bindings()`, which must stay last. Run once per edit, never in render. |

### Testing

- **`store`:**
  - `a_desktop_json_from_before_settings_loads_with_defaults`
  - `settings_round_trip_through_desktop_json`
  - `an_unknown_appearance_mode_falls_back_to_system_and_keeps_projects`
  - `a_sounds_block_without_all_reads_as_on`
  - `worktrees_dir_prefers_repo_then_root_then_home`
- **`pocket/settings`:**
  - `system_mode_forces_nothing`
  - `light_and_dark_force_the_window`
  - `a_reduce_motion_override_ignores_macos`
  - `sections_follow_the_spec_order`
  - `every_binding_appears_in_the_shortcut_table`
  - `closing_settings_returns_to_the_screen_it_came_from`
- **`actions.rs`:** `cmd_comma_opens_settings`, using the `bound()` helper (`actions.rs:91-94`). The uniqueness test must stay green.
- **pocketd:** a `registry_test.go` field test, and a `create_test.go` test that `Dir` uses the root, falls back to home, and that the repo override wins.
- **Visual:** `.ui-review/fixture/capture.sh` for each page, light and dark. Capture must not persist anything.
- **Before committing:** `cargo build/clippy/test --workspace`.

## 4. Sections

Shared layout, from spec §3.10:

- **Nav:** the left nav has the same width as the aside. Entries: General, Appearance, Keybindings, Worktree management. Back is pinned at the bottom.
- **Page:** "Settings / {Section}" crumb, then a 20/26 Display title, then groups.
- **Rows:** each group is an r12 `FILL_1` card with a `HAIRLINE` ring. Rows are px16 py14, with the label and a 12 `TEXT_2` hint on the left and the control on the right (at most 60% of the row).
- **Behaviour:** controls apply on click. There is no Save button, and render does no IO.

### 4.1 General

| Key | Control | Default | Scope | Priority | Effort | Consumer |
|---|---|---|---|---|---|---|
| `notifications.banners` | switch | true | global | **MVP** | S | `alerts.rs:159` `quiet` arg |
| `sounds.all` + `needs_you/done/failed` | master switch + 3 indented switches (disabled while the master is off) | true | global | **MVP** | S | `sounds.rs:34-40`; `store.rs:28-41` |
| `phone.max_access` | segmented Ask / Edits / Auto; disabled until `agents.phone_max` is set; shown only for the owner | ask (pocketd) | global, **pocketd-owned** | next | S | `agents.rs:369-372` → WS `config.set`; never cache in `desktop.json` |
| `phone.pair` | button "Pair phone…" → `Overlay::PairPhone` | n/a | global | next | S | `modals.rs:18` gate |
| `notifications.dock_badge` | switch; turning it off clears the badge | true | global | later | S | `desktop/dock.rs:15-21` |
| `notifications.only_in_background` | child switch | false | global | later | S | `alerts.rs:157-159` |
| About / background service | read-only block, fetched once on open | n/a | global | later | S | pocketd `ops status` (`ops.go:206`) |
| `agents.default_provider` | segmented Claude / Codex | claude | global | later | S | `new_session.rs:91, 153-157`; rarely applies, because every launch writes a per-project pick (`:347-351`) |
| Devices, start at login, commit-message model, link target, editor, shell | various | n/a | global | maybe | S–L | Shell goes against ADR 0001 and needs pocketd parity. Model/effort defaults reverse fd2e1ab. |

**Notes:**
- General is mostly Notifications plus Phone. The spec has a separate Notifications page (`:1144`), but the requested list does not (open question).
- The keyboard path to the sound toggles is the palette (roadmap S1), and it stays.
- A failed phone-access change is reported only through `self.error` (`alerts.rs:118-120`). The page must render that error line, or the failure is silent.
- Banners never post under unbundled `cargo run`. Test the banners toggle through `Alerts::sync` on the `quiet` flag.

### 4.2 Appearance

| Key | Control | Default | Scope | Priority | Effort | Consumer |
|---|---|---|---|---|---|---|
| `appearance.mode` | segmented System / Light / Dark | system | global | **MVP** | S | `set_window_appearance` (`app.rs:1508`) → observer `desktop.rs:117`; launch path `main.rs:39, 60` |
| `appearance.reduce_motion` | segmented System / On / Off; hint "Stops spinners, cursor blink and tab slides." | system (`null`) | global | **MVP** | S | `desktop.rs:510-514`, `main.rs:41`, `desktop.rs:116` |
| `terminal.font_size` | stepper 9–24 | 13 | global | first follow-up | S | `terminal_view/surface.rs:12` `MAIN`; `pane.rs:52,55`, `inbox/detail.rs:41` |
| `appearance.code_font_size` | stepper 10–20 + sample | 13 | global | later | M | `diff.rs:22, 495-496`; `commit.rs:198-199`; `preview/code.rs:92`; `markdown.rs:26`; re-applied in `theme.rs:314`; `ListState::remeasure` |
| `terminal.cursor_style` / `cursor_blink` | segmented / switch | block / true | global | later | S | `crates/term/src/shim.c:48-75` |
| `appearance.translucent_window` | switch (dark only) | true | global | later | M | `theme.rs:321-323`, `:65` |
| code font family, file icons, accent, UI font size | n/a | n/a | global | maybe | M–L | UI size needs a px→rem migration (172 `text_size(px(..))`); out of v1 |

**Notes:**
- Palette actions "Appearance: Match system / Light / Dark" (`spec :1106`) are cheap and follow the pattern of the Sound entries.
- Layout (Sidebars/Compact/Focus) is chrome state toggled by ⌘B, ⌘\ and ⌘. It is not an Appearance setting.
- Font-size work must re-apply the gpui-kit mono size inside `theme::set_appearance`, which resets it to 13 on every scheme switch (`theme.rs:302, 314`).

### 4.3 Keybindings

| Key | Control | Default | Scope | Priority | Effort | Consumer |
|---|---|---|---|---|---|---|
| read-only shortcut table | grouped list: Command / Shortcut (`ui::kbd`) / Where (Terminal, Browser); about 35 rows, so no virtualisation | n/a | global | **MVP** | S | `shortcut_rows(actions::bindings() + keys::bindings())`. Labels come from a static table beside `bindings()`, because `no_json` actions (`actions.rs:7-19`) have no name lookup. |
| label lookup | none (prep) | n/a | n/a | next | M | Route the hard-coded labels (`palette.rs:241-245`, `sidebar.rs:116`, `explorer.rs:116`, `desktop.rs:428`, `tab_menu.rs:64,71`, `selection.rs:86`, `inbox/detail.rs:55`) through one lookup, cached and never computed in render |
| `keymap` overrides | recorder per row + Reset; search; Reset all | `{}` | global | later | L | `BTreeMap<String, Option<String>>` in Store; parsed with `KeyBinding::load` (never `new`, which panics) |
| `keybindings.option_as_meta` | switch | true | global | later | S | `keys.rs:14-15, 50, 53` |
| keymap file, chords | n/a | n/a | global | maybe | M | phase-2 file only |

**MVP layout:** a subtitle "Shortcuts work everywhere unless marked Terminal or Browser". Groups: Sessions, Navigation, Panels & tabs, Browser, App. Below them, a collapsed "Fixed keys" group: ⌘1–9, Esc, palette and Inbox navigation, ⌘↵ submit.

**Facts that shape rebinding.** Verified in `vendor/gpui-pre/src/keymap.rs:186-188, 249-253`:
- Global bindings get depth = `contexts.len()`. They tie with the deepest context and win as later-added, so a user global beats `Input` keys in text fields.
- Bindings dispatch before the terminal's key listener (`window.rs:5929-5961`), so any global takes the key from the PTY.
- Handlers stop propagation by default.

**Fixed bug.** The global `cmd-enter` tied with `Input`'s ⌘↵ and, bound later, swallowed submit in the commit box, the new-session prompt and the composer. `OpenSession` is now scoped to the `Inbox` context (`actions.rs`).

**Recorder (later):**
- Use `cx.intercept_keystrokes` only while recording. `capture_key_down` is too late, because ⌘K would open the palette first.
- The Esc capture at `desktop.rs:460-471` must skip while recording.
- Keep `keys::bindings()` bound last.

### 4.4 Worktree management

| Key | Control | Default | Scope | Priority | Effort | Consumer |
|---|---|---|---|---|---|---|
| Projects list | a row per `store.projects` entry: name, swatch, effective folder (marked "custom"), "Edit…" → `select_project` + `project_settings` (as `row_menu.rs:97-99` does) | as today | project | **MVP** | S | existing `RepoConfig` + AddRepo modal |
| `worktree.root` | path row: tilde-shown path, "Change…" (`pick_path`), "Use default"; preview "New worktrees go in ~/.worktrees/<project>/<name>. Existing ones stay put." | `""` = `~/.worktrees` | global | **MVP** | M | Rust: `store` `worktrees_dir`, replacing `new_session.rs:254-257` and the `add_project.rs:362` placeholder. Go: `registry.File` + `worktree.Dir` (`create.go:83-89`). |
| inline project form (base, folder, setup, teardown, copy) | shared `RepoForm`, replacing the modal | as today | project | later | M | after IN-FLIGHT merges (`add_project.rs`) |
| `pr.status` | switch + gh hint | on | global | later | S | IN-FLIGHT `git_ui/pull_requests.rs:23` |
| `pr.draft` | switch | off | global | later | S | IN-FLIGHT `git/src/github.rs:38-39` (`--draft`) |
| `worktree.branch_prefix` | text + live preview | `""` | global | later | L | pocketd `Prepare`/`Validate`/`Taken` (`create.go:42-71, 156`) + desktop `name_problem`; needs a capability gate |
| delete-branch default, on-merge action, copy default, teardown timeout, auto-name | n/a | n/a | global | maybe | S–M | IN-FLIGHT `removal.rs:8, 29, 75`; `copy.go:33-39` |

**What the in-flight branch delivers** (uncommitted on 4074f80; plan at `docs/plans/2026-10-05-worktree-gaps.md` in that worktree):
- `RepoConfig.teardown`, with a "When a worktree is deleted" input in the modal.
- `removal.rs`: `TEARDOWN_LIMIT` 120 s; teardown → remove → delete branch; `branch_deletable` refuses main, master and the base.
- A "Delete worktree and branch…" row-menu item.
- Opening an existing branch or PR (`open.go`, caps `open.v1`).
- PR status polling via `gh` (`pull_requests.rs`) and `github.rs`.
- A new `Confirm::TeardownFailed`.
- Busy row marks in `sidebar.rs`.

**What it does not do:** no global settings, no Settings screen, no ⌘, change.

**Compatibility rules for Settings:**
1. Never edit `RepoConfig` from Settings in MVP. Only link to the modal. That way the `teardown` field and the form rework land untouched.
2. Add the new `worktree` block at the `Store` level, not to `RepoConfig`, so its diff (`store.rs:15`, plus a test at the file end) doesn't conflict.
3. Keep the path rule identical in Go and Rust. Test both against the same cases. The in-flight `Draft::folder` guess must call the new `store` function.
4. Any later PR or delete-branch settings consume its constants and functions (`pull_requests.rs:23`, `github.rs:38`, `removal.rs:29`) rather than duplicating them.
5. Rebase after it merges, and don't touch that worktree.

**Notes:**
- Store absolute paths, because Go doesn't expand `~`. Display them with `tilde()`.
- An old pocketd ignores the root. The desktop's guessed path is then corrected by `Creates::started` (`creating.rs:211`), which shows as a flicker, not data loss.
- A project rename already moves where new worktrees go (`create.go:88`, `new_session.rs:256`). A visible root makes that more noticeable.

## 5. Phased plan

**MVP.** Each step is one commit and is testable on its own. All steps come after worktree-management merges.

1. **Store prefs.** `Notifications`, `Appearance{mode, reduce_motion}`, `Sounds.all`, `WorktreeDefaults{root}`, lenient enum deserializer, `worktrees_dir`. Tests: old file loads, round trip, unknown variant keeps projects, path order. No UI.
2. **Shell.**
   - `OpenSettings` on ⌘,, and `Screen::Settings`.
   - `SettingsState` (section, back), and `settings.rs` + `nav.rs`.
   - Gears repointed, Esc/Back wired.
   - Test `cmd_comma_opens_settings`, plus back-target tests.
   - Capture before and after.
3. **Appearance page.** Mode and reduce motion. Launch path in `main.rs` (skipped in capture), `theme::init(resolved)`. Tests for `forced` and reduce-motion resolution. Manual check that forcing flips glass/opaque on macOS 15/26.
4. **General page.** `ui::switch`; banners via `quiet`; sounds master and children. Tests on the `Alerts::sync` quiet flag.
5. **Keybindings page (read-only).** Static label table, `shortcut_rows`, test `every_binding_appears_in_the_shortcut_table`.
6. **Worktree management page.**
   - 6a. Projects list with Edit… (desktop-only).
   - 6b. Global root: `store` function used by `new_session.rs` and the `add_project.rs` placeholder; `registry.File.Worktree.Root` + `worktree.Dir` in pocketd, with Go tests. Release the desktop and pocketd together.

**Follow-ups, in order:**
1. Palette entries (Settings; Appearance ×3) and `cx.set_menus` with "Settings…".
2. Phone rows on General (access level, pair), with owner gating and the error line.
3. Terminal font size, then Text group (code size), then cursor style/blink.
4. Fix ⌘↵ propagation (separate bug commit, test first). Then the label-lookup prep, then rebinding (recorder, conflicts, `keymap` overrides, re-apply).
5. Inline project form replacing the modal (spec phase 7); then PR status and draft toggles.
6. Settings search (reuse `palette::matches`), per-row Reset.
7. Phase-2 files (`settings.json` / `keymap.json`), only if asked for.

## 6. Risks

- **Whole-file reset.** `Store::load` drops everything on any parse error (`store.rs:92`). A new enum without a lenient deserializer could wipe projects after a downgrade or upgrade. Never suggest hand-editing `desktop.json`.
- **Merge conflicts** with the in-flight branch (31 files, including `store.rs`, `chrome.rs`, `desktop.rs`, `main.rs`, `sidebar.rs`, `row_menu.rs`, `theme.rs`, `ui.rs`). Mitigate by sequencing after it merges.
- **⌘, muscle memory.** Project settings move one click deeper. Mitigate with the row menu, a gear, and Edit… on the Worktree management page.
- **Forced appearance is app-wide.** It also restyles WKWebView tabs, sheets and alerts, and fights capture's forced light (`capture.rs:182`) unless skipped while capturing. A forced Light with libghostty's default ANSI colours may read badly.
- **Save races.** Mixing a synchronous `save()` with `save_soon` can lose a toggle. Settings uses `save_soon` only.
- **Redraw cost.** Every `cx.notify()` (1 s poll, 2 s git refresh) re-renders the page. No IO, stat, `ops status` or keymap scans in render.
- **Alerts on open.** Opening Settings empties the viewed set (`alerts.rs:184-186`), so the session the user just left may banner or chime.
- **Path rule in two languages.** Rust and Go can drift. Mitigate with shared test cases. An old pocketd ignores the root, giving a flicker but no data loss.
- **Rebinding (later).** Globals take keys from text fields and the PTY. `clear_key_bindings` also wipes gpui-kit's bindings. A leaked interceptor swallows every key. Non-US layouts and IME misfire on bare or ⌥ keys.
- **Scope creep.** UI font size (px→rem) and accent colour (contrast tests) would dominate the work. Keep them out of v1.

## 7. Open questions for the user

1. ⌘, opens app Settings, and project settings move to the row menu plus Worktree management. Does `ProjectSettings` get no shortcut, or ⌘⇧,? Should the row menu read "Project settings…"?
2. Sections: should Notifications stay inside General (as proposed), or become a fifth section as the spec has it?
3. Should the Phone rows (access level, pair) be on General in the next step, or stay palette-only until a Devices page exists?
4. Keybindings v1: read-only table (proposed), or rebinding now (effort L)?
5. Is a hand-editable `settings.json` / `keymap.json` ever wanted? If not, phase 2 never happens.
6. Should the global worktrees root ship in MVP, given it changes pocketd and needs a joint release? Or should MVP be the projects list only?
7. JSON key names: `notifications`, `appearance`, `worktree.root` at top level. OK, or group them under one `settings` key?
8. Should Settings count as "viewing nothing" for alerts (like Inbox), or keep the last Sessions view?
9. Should the per-project form move into Settings after worktree-management merges (spec phase 7), or stay a modal linked from it?
10. Should the palette get "Appearance: Match system / Light / Dark" with the MVP?

## 8. Prior art

- **Zed:** a graphical Settings Editor on ⌘, in its own window, with JSON as the escape hatch. Defaults are embedded, a sparse user file sits on top, and keymap.json is separate. Edits change only the touched key. A window works for Zed because its settings live in a Global; Pocket's live on `Desktop`, so an in-window page is cheaper. https://zed.dev/blog/settings-ui , https://zed.dev/docs/key-bindings , https://zed.dev/docs/appearance
- **VS Code:** the Settings UI and settings.json are two views of the same data, with modified markers and per-setting reset (possible because only overrides are stored). Its Keyboard Shortcuts editor (Command / Keybinding / When / Source) is the model for our table. Workspace settings needed Workspace Trust. https://code.visualstudio.com/docs/configure/keybindings
- **Ghostty:** our terminal engine. `window-theme`, `cursor-style` (block), and blink left unset so the program wins. `performable:` keybinds correspond to `cx.propagate()`. https://ghostty.org/docs/config/reference , https://ghostty.org/docs/config/keybind
- **Warp:** a shortcuts editor whose conflict warnings don't name the other binding, a frequent complaint. Name the conflicting command, and offer Replace. https://github.com/warpdotdev/Warp/issues/4851
- **Conductor:** per-repo `scripts.setup` / `archive`, `delete_branch_on_archive`, `archive_on_merge`, branch prefix. Model defaults are user-level only. https://www.conductor.build/docs/reference/settings/reference
- **Superset:** lifecycle scripts in repo files with user overrides; a CLI over the same settings store. https://docs.superset.sh/setup-teardown-scripts , https://docs.superset.sh/cli/cli-reference
- **Cursor:** a forced `cursor/` branch prefix that users can't turn off, and a prefix setting that was ignored in one create path. If we add a prefix: empty by default, applied in one place (pocketd). https://forum.cursor.com/t/allow-disabling-branch-prefix-cursor-for-cloud-agent/161901
- **gpui-component 0.6.6 `setting::*`:** pages, groups, search and reset. Use it as a behaviour reference only (`~/.cargo/registry/src/*/gpui-component-0.6.6/src/setting/`).
- **macOS HIG:** the app menu's "Settings…" item on ⌘, opens one surface for the whole app. https://support.apple.com/en-us/102650
- **Internal:** `docs/orchestrators/product/02-ux-spec-desktop.md` §3.10, `05-roadmap.md:491` (S1), `docs/orchestrators/research/04-zeron-desktop-shell-ux.md`, `12-monocode-shell-design.md`.