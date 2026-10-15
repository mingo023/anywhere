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
| `agents` | the phone-socket client, the agent list, worktree display names, Locals and automations: the snapshot, the draft's validation and its messages |
| `term` | the ghostty VT |
| `store` | the saved projects, `store/sections` their pins and groups, settings (sounds, notifications, appearance, worktree root; `store/prefs` one struct per Settings section) and each worktree's panels |
| `project` | which project and worktree a folder belongs to, the folders git refresh reads |
| `hosts` | SSH hosts: what one is and how it's saved, `hosts/ssh` the ssh commands and what their failures mean, `hosts/link` the tunnel to a host's pocketd that reconnects by itself, `hosts/sign_in` the one password sign-in that gives a host the app's own key, `hosts/ssh_config` the hosts `~/.ssh/config` names, `hosts/remote` exec and file ops through a host's pocketd, `hosts/session` terminals and agents on hosts and which pocketd holds them, `hosts/install` putting pocketd on a host as a user service, `hosts/checklist` an install's progress, `hosts/owner` a Windows host's owner token |
| `web` | the WKWebView page of a browser tab, address and link parsing |
| `ui`, `theme` | widgets, colours, icons |

| `pocket` module | Owns |
|---|---|
| `desktop` | `Desktop`, navigation, root render; `desktop/project` project and worktree lookups and the git refresh; `desktop/alerts` notifications and the seen set; `desktop/dock` the Dock badge; `desktop/quit` the quit confirm; `desktop/jump` walking and jumping between sessions; `desktop/sounds` sound cues; `desktop/chrome` layout enums and shared bars; `desktop/toast` the error toast; `desktop/locals` adding, naming and deleting Locals |
| `terminals` | terminals mirrored from pocketd, spawn intents, closing; `terminals/bell`, `terminals/link` |
| `panels` | the main area's pane tree: strips, dividers, drag and drop, saved layouts, shortcuts |
| `composer` | the prompt box that starts an agent, shared by the new session page and the empty pane: its agent, account, model and access chips and pasted images; `composer/launch` the chips' rows and labels |
| `empty_pane` | the prompt a focused pane with no tabs offers, starting an agent there |
| `terminal_view` | the terminal pane body, keyboard and IME input; `terminal_view/link` the link under the pointer; `terminal_view/menu` its right-click menu |
| `browser` | browser tabs: pages, address bar, navigation |
| `sidebar` | projects aside, compact rail, `sidebar/sections` the aside's Pinned, groups and Other with the Projects header and its organize menu, `sidebar/filter` the header's filter, `sidebar/fold` the folds' spring open and shut, `sidebar/project_picker` the rail's project switcher, `sidebar/rename` inline worktree rename, `sidebar/column` the inbox and Automations column and the panel's body, `sidebar/panel` the right-hand panel: its rail and edge drag |
| `removal` | deleting a worktree: its teardown script, its branch, the confirm and the row's "Deleting…" |
| `git_ui` | `changes` panel and commit, `diff` view, `graph` commit graph, `commit` stacked commit diff |
| `accounts` | each machine's Claude Code and Codex sign-ins as its pocketd lists them, their usage, the sign-in terminals, and the account a session starts on |
| `add_to_chat` | quoting a selection into an agent's input: the composer, its agent menu, the selection pill |
| `explorer` | file tree; `explorer/preview` code, markdown and image preview; `explorer/menu` its right-click menu, `explorer/edit` naming new and renamed items, `explorer/files` their disk writes, `explorer/toast` copy, trash and Mentioned notices, `explorer/frontmatter` a markdown file's front matter table, `explorer/mermaid` its mermaid diagrams |
| `automations` | the Automations screen (⌘⇧A, offered only when pocketd has the capability): `automations/logic` the pure rules, `column` the tabs and lists, `rows`, `runs`, `detail` and `run_detail` the right pane, `editor` with `form` and `trigger` the full-page editor, `glyph`, `parts` |
| `inbox`, `palette` | their screens |
| `fleet` | each SSH host's link, adding, installing, updating and removing hosts, signing in to one with a password |
| `host_agents` | each host's pocketd agent channel, opened once the host serves projects |
| `fs` | where a tree's files are read and changed: this Mac or a host |
| `notch` | the island that grows out of the notch to answer permission asks: which asks it offers, and the `anywhere-notch` helper that draws it (a SwiftPM package in `packages/desktop/notch`), spoken to in JSON lines |
| `settings` | the Settings screen (⌘,): `settings/catalog` the rows every page, search, Advanced and Reset read; `nav`, `search`; `host` pocketd's status, devices and settings; `models` the agents' model and effort lists; `dropdown` the menu their pickers share; one page per section (`general`, `appearance`, `notifications` with `notifications/permission` and `notifications/sounds` (the sound list), `keybindings`, `agents`, `provider` (each agent's own page), `accounts`, `automations`, `phone`, `ssh_hosts`, `projects`, `sidebar`, `terminal`, `files`, `browser`, `git`, `diff`) |
| `modals` | overlays: new session, project settings, `modals/clone_project` Clone from URL, confirm, more, pair phone |
