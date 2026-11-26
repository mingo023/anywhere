# Settings revamp: build plan

Builds `docs/designs/2026-11-25-settings-revamp.md` (the spec) to match `.ui-review/prototypes/settings-revamp.html` (the reference). Where the two disagree, the prototype wins. Paths are relative to `packages/desktop/crates/pocket/src` unless they say otherwise.

## Waves

1. **Shell** builds the shared pieces every page uses. It lands first.
2. **Search, Pages, Agents & accounts, Projects & hosts, Keyboard** run in parallel on top of it, each on its own files.

## Data shapes

- **Row state:** a row is *changed* when its stored value differs from the catalog default. Changed is computed from `Setting` plus the store, never stored. The row reset and the footer count both read it.
- **Text drafts:** one `Draft { text, error: Option<String> }` per text field. It commits on Enter or blur, and an invalid draft keeps its text and shows `error`.
- **Toasts:** a `Toasts` stack owned by Settings, holding `Toast { text, undo: Option<Undo> }`. An Undo carries what it restores. A failed save becomes `RowError { setting, message }` shown on its row with Retry.
- **Shortcuts (store):** `keys: BTreeMap<String, String>` keeps its meaning: the key in an action's built-in spot, with `""` for an emptied spot. A new `added: Vec<(String, String)>` holds the extra keys as (binding id, key), in the order they were added. Old files load unchanged. `keymap()` replaces spot keys in place, then appends `added` after every built-in binding of ours and before the terminal's.
- **Projects by host:** `ProjectCard` gains `host: Option<String>`, taken from `accounts::host_of(Some(path))`. `None` means this Mac. There is no protocol change.

## Slices

| Slice | Agent | Owns | Findings |
|---|---|---|---|
| **1 Shell** | claude opus high | `settings.rs`, `settings/nav.rs`, `settings/catalog.rs`, `settings/dropdown.rs`, `settings/general.rs`, the Settings parts of `desktop.rs`, the new `settings/toasts.rs` and `settings/fields.rs`, the bot icon asset in `ui`, and one ⌘[ action in `actions.rs` | N1–N5, C1–C4, C6 (the shared rule), C8 (the shared disabled-with-reason row), F1 |
| **2 Search** | claude sonnet high | `settings/search.rs` | S1–S4 |
| **3 Pages** | claude sonnet high | `settings/notifications.rs`, `settings/notifications/*`, `settings/appearance.rs`, `settings/themes.rs`, `settings/automations.rs`, `settings/terminal.rs`, `settings/files.rs`, `settings/browser.rs`, `settings/diff.rs`, `settings/sidebar.rs`, `settings/phone.rs` | P1, C5 (pages), C7, C8 (Window opacity), P4 |
| **4 Agents & accounts** | claude sonnet high | `settings/agents.rs`, `settings/provider.rs`, `settings/accounts.rs`, `settings/models.rs` | P2, F2, P3 (accounts menu), C6 (provider fields) |
| **5 Projects & hosts** | claude sonnet high | `settings/projects.rs`, `settings/ssh_hosts.rs`, `settings/host.rs`, `settings/side.rs`, `settings/git.rs`, `.ui-review/fixture/*` | F3, P3 (filter, hosts actions), C6 (host fields) |
| **6 Keyboard** | claude opus high | `settings/keybindings.rs`, the rest of `actions.rs`, `crates/keys`, `keys`/`added` in `crates/store` | K1–K6, §3 |

**Shared files after wave 1:** `settings.rs` (`SettingsEvent`, `on_settings_event`) and `settings/side.rs`. Wave-2 slices may only add to them: new enum variants, new match arms, new fields. They never rewrite or reorder what is there. I resolve conflicts when merging.

## Done when

1. **Shell**
   - The nav scrolls at a 600 px window height, with Back and search pinned.
   - The groups are App · Agents · Machines · Workspace · Git.
   - The large header is gone and the toolbar holds the breadcrumb. The breadcrumb's parent is a link, and ⌘[ goes up one level inside Settings.
   - One "Show advanced · N" toggle per page.
   - Every control takes focus with a visible ring: arrows move through the nav, Space and Enter toggle, ←/→ move segments.
   - Menus close on a press outside and on a second press of their trigger. Esc closes the innermost layer first: menu, then recording, then search, then Settings.
   - Toasts show inside Settings, and a failed save shows on its row with Retry.
   - A changed row shows a dot and a per-row reset. The footer shows "N changed in <section> · Reset…" only when N > 0. The Reset sheet lists from → to.
   - Text fields commit on Enter or blur, through `Draft`.
   - A row that can't apply is disabled and gives its reason.
   - Agents uses the bot icon.
2. **Search**
   - Results are live rows.
   - It finds providers, accounts, projects (tagged with their host), themes and shortcuts.
   - Matches highlight whole words only.
   - A section and group chip names where each result lives.
   - Enter opens the top hit, scrolls to it, flashes it and focuses its control.
   - A result hidden behind its parent says "Turn on X first" and offers to turn X on.
3. **Pages**
   - Notifications has two master switches, then one row per event with labelled Banner and Sound toggles, disabled while their master is off. The sound is a dropdown with ▶ preview.
   - Automation intervals use a typed numeric field with a unit.
   - Deleting a theme or sound shows a 6 s Undo toast, and the file goes to the Trash.
   - Window opacity is disabled in light themes and gives its reason.
   - Related links sit at the foot of the groups the spec names (P4).
4. **Agents & accounts**
   - One provider list: Default and Off chips, a status line, a switch, a ⋯ menu (Make default, Disable, Settings…) and a chevron.
   - Providers that aren't installed fold into "N not installed".
   - The separate Status table is gone.
   - Each provider page has a Make default button and the bot icon.
   - The sign-in panel shows an ↩ Enter button inside the code field once it has text; Return sends the code too.
   - Add account reads "Signing in…" while the panel is open.
   - The new account flashes in the list.
5. **Projects & hosts**
   - Projects are grouped by machine, with headers showing state, count, and Add or Reconnect. Groups fold.
   - A filter field matches name, path and host, and opens folded groups.
   - Offline projects are dimmed.
   - With one machine, the list stays flat.
   - Add project… asks which machine.
   - Hosts keep every conditional action, with the most relevant as the button and all of them in ⋯.
   - Host fields commit through `Draft`.
   - The capture fixture has projects on two or more hosts.
6. **Keyboard**
   - Every row is editable, including grouped rows, through a modifier editor.
   - Rows have `+`, a × on each chip, and Enter to re-record. Recording: Esc cancels and ⌫ removes.
   - Unassigned actions are listed. Filters: All · Changed · Unassigned.
   - Conflict notes follow §3 exactly, worked out from the effective keymap order. That covers base text-field keys, the terminal's reserved keys, macOS keys, and Keep both.
   - Reset and Undo run the same conflict check.
   - Find by keys works, with a read-only "Built in elsewhere" group.
   - Fixed keys show a lock.
   - `added` persists and round-trips.
   - A test pins that dispatch order matches what the notes promise.

## Checks for each slice

From `packages/desktop`: `cargo fmt --all`, `cargo clippy -p pocket --all-targets -- -D warnings` (plus `-p` for any other crate touched) and `cargo test -p pocket` (plus the crates touched). I run the workspace-wide checks after merging.
