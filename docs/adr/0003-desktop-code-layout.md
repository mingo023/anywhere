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
| `workspace` | tabs and splits of a worktree |
| `git` | git reads and writes, diffs |
| `daemon` | the pocketd socket client |
| `agents` | the phone-socket client and the agent list |
| `term` | the ghostty VT |
| `store` | the saved projects |
| `project` | which project and worktree a folder belongs to, the folders git refresh reads |
| `ui`, `theme` | widgets, colours, icons |

| `pocket` module | Owns |
|---|---|
| `desktop` | `Desktop`, navigation, root render; `desktop/project` project and worktree lookups and the git refresh; `desktop/alerts` notifications and the seen set; `desktop/chrome` layout enums and shared bars |
| `terminals` | terminals mirrored from pocketd, spawn intents, closing |
| `terminal_view` | the session page: tabs, panes, keyboard and IME input |
| `sidebar` | projects aside, compact rail, sessions column |
| `git_ui` | `changes` panel and commit, `diff` view and comments |
| `explorer` | file tree; `explorer/preview` code, markdown and image preview |
| `inbox`, `palette` | their screens |
| `modals` | overlays: new session, add project, confirm, more |
