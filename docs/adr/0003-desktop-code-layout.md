# Desktop code layout: model crates, feature modules, state per feature

The desktop follows Zed's split. Model crates hold domain logic with no rendering and are unit-tested without GPUI; the `pocket` crate holds views. Inside `pocket`, each feature owns one module and a state struct that `Desktop` composes, instead of `Desktop` carrying every field. We chose this because `Desktop` had grown to ~90 fields touched from every file, so a change to the diff view risked the terminal, and none of the project/worktree rules could be tested. The cost: views still `impl Desktop`, so a feature reads shared state (`projects`, `repos`, `agents`) through `Desktop`; turning a feature into its own `Entity` with `.cached()` rendering is the next step, one feature at a time.

## Rules

- A crate's lib is `src/<crate>.rs`; a module with children is `foo.rs` plus `foo/`. No `mod.rs`.
- Model crates never depend on `ui`, `theme` or `pocket`.
- A feature's state struct lives in its module, is built by its own `new`, and returns its own subscriptions. `Desktop::new` only composes them.
- Logic that only needs a feature's state is a method on that state, or a free function, with tests beside it. `impl Desktop` methods stay thin: gather inputs, call the logic, `cx.notify()`.
- One file per component. Split by component, not by size.

## Map

| Crate | Owns |
|---|---|
| `pocket` | the binary: app bootstrap, window, the `Desktop` shell and feature views |
| `workspace` | the pane tree of a worktree: panes, their tabs, splits |
| `git` | git reads and writes, diffs, commit graph lanes |
| `daemon` | the pocketd socket client |
| `agents` | the phone-socket client, the agent list, worktree display names and automations: the snapshot, the draft's validation and its messages |
| `term` | the ghostty VT |
| `store` | the saved projects, settings (sounds, notifications, appearance, worktree root; `store/prefs` one struct per Settings section) and each worktree's panels |
| `project` | which project and worktree a folder belongs to, the folders git refresh reads |
| `web` | the WKWebView page of a browser tab, address and link parsing |
| `ui`, `theme` | widgets, colours, icons |

| `pocket` module | Owns |
|---|---|
| `desktop` | `Desktop`, navigation, root render; `desktop/project` project and worktree lookups and the git refresh; `desktop/alerts` notifications and the seen set; `desktop/dock` the Dock badge; `desktop/quit` the quit confirm; `desktop/jump` walking and jumping between sessions; `desktop/sounds` sound cues; `desktop/chrome` layout enums and shared bars; `desktop/toast` the error toast |
| `terminals` | terminals mirrored from pocketd, spawn intents, closing; `terminals/bell`, `terminals/link` |
| `panels` | the main area's pane tree: strips, dividers, drag and drop, saved layouts, shortcuts |
| `empty_pane` | the prompt a focused pane with no tabs offers, starting an agent there |
| `terminal_view` | the terminal pane body, keyboard and IME input; `terminal_view/link` the link under the pointer |
| `browser` | browser tabs: pages, address bar, navigation |
| `sidebar` | projects aside, compact rail, `sidebar/project_picker` the rail's project switcher, `sidebar/rename` inline worktree rename, `sidebar/column` the inbox and Automations column and the panel's body, `sidebar/panel` the right-hand panel: its rail and edge drag |
| `removal` | deleting a worktree: its teardown script, its branch, the confirm and the row's "Deleting…" |
| `git_ui` | `changes` panel and commit, `diff` view, `graph` commit graph, `commit` stacked commit diff |
| `add_to_chat` | quoting a selection into an agent's input: the composer, its agent menu, the selection pill |
| `explorer` | file tree; `explorer/preview` code, markdown and image preview |
| `automations` | the Automations screen (⌘⇧A, offered only when pocketd has the capability): `automations/logic` the pure rules, `column` the tabs and lists, `rows`, `runs`, `detail` and `run_detail` the right pane, `editor` with `form` and `trigger` the full-page editor, `glyph`, `parts` |
| `inbox`, `palette` | their screens |
| `settings` | the Settings screen (⌘,): `settings/catalog` the rows every page, search, Advanced and Reset read; `nav`, `search`; `host` pocketd's status, devices and settings; `models` the agents' model and effort lists; `dropdown` the menu their pickers and the project editor share; one page per section (`general`, `appearance`, `notifications` with `notifications/permission`, `keybindings`, `agents`, `provider` (each agent's own page), `automations`, `phone`, `projects`, `sidebar`, `terminal`, `files`, `browser`, `git`, `diff`) |
| `modals` | overlays: new session, add project, confirm, more, pair phone |
