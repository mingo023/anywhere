# pocketd keeps its state in SQLite

## Why

pocketd keeps its state in ten JSON files. Each store rewrites its own file whole on every change, and no write can span two files.

- Removing a project leaves its Tickets, Automations and Locals behind. They point at a folder that is no longer a project.
- Tickets and Automations point at a project by `folder`, a path. Projects have had an id since `2026-11-24-pocketd-owns-shared-state.md`, and nothing but the registry uses it.
- Anywhere Cloud needs to read a machine's projects and devices by id.

pocketd already uses SQLite for `state/orchestrator.db` (`internal/orch/store.go`) and `state/skills.db`. This plan moves the rest of pocketd's durable state into one more database.

## Decisions

All of this ships as one PR.

1. pocketd keeps its durable state in `state/pocketd.db`. The database is separate from `state/orchestrator.db`, which is large, written all the time, and safe to delete. `pocketd.db` is small and holds what a user can't get back.
2. These files move into the database:

	| File | Store |
	|---|---|
	| `state/registry.json` | `internal/registry` |
	| `state/locals.json` | `internal/locals` |
	| `state/automations.json` | `internal/automation` |
	| `state/board.json` | `internal/board` |
	| `state/hosts.json` | `internal/hosts` |
	| `state/names.json` | `internal/names` |
	| `state/host.json` | `internal/state` (`HostID`) |
	| `devices.json` | `internal/devices` |
	| `push.json` | `internal/push` |
	| `config.json` | `internal/config` |

3. These files stay as they are:
	- `desktop.json` belongs to the desktop. pocketd reads it once, as it does today, and never writes it.
	- `state/terminals.json` is restore state that `state.Writer` saves every second. `state/mcp`, `events.jsonl`, `holders` and the `*-claimed` markers are runtime data too.
4. Each kind of thing gets its own table with typed columns. A project's `repo` settings stay one JSON column, because pocketd round-trips fields it doesn't read (`launch`, `icon`) and the desktop adds fields without telling pocketd.
5. A Ticket, an Automation and a Local point at their project by `project_id`, a foreign key with `ON DELETE CASCADE`. Removing a project deletes them, and deleting an Automation deletes its Runs.
6. Only the pocketd daemon writes the database. A CLI process that needs state (`pocketd worktree`, `pocketd push`, `pocketd owner`) opens the database read-only. A CLI process that changes state sends a message over the socket, as `pocketd config set` does today.
7. Each store keeps its Go API and its copy in memory. `registry.Load()` and its callers don't change. Only the code that reads and writes the file changes.
8. The wire doesn't change. A Ticket and an Automation still carry `folder`, which pocketd builds from the project's path. The desktop and the iOS app need no change.
9. pocketd imports the JSON files once, in one transaction. After the import it never reads or writes them, and it doesn't delete or rename them. A pocketd from before this change still starts from them, with the state they held at the import.

### Behaviour that changes

This PR changes behaviour, so it isn't a refactor under `CLAUDE.md`. Each change below gets a test.

- `project.remove` deletes the project's Tickets, Automations, Runs and Locals. Today they stay in their files.
- pocketd refuses to save a Ticket, an Automation or a Local whose folder is in no project. The WS handlers for automations and the Board already refuse (`errUnknownProject` in `internal/wsserver/automation.go` and `board.go`). The MCP tool (`internal/mcp/tools_b.go`) and Locals follow the same rule.
- An edit to `config.json` by hand does nothing after the import. `pocketd config set` is the way to change a setting.

## Schema

The first migration is `internal/statedb/migrations/001_init.sql`. Times are unix ms, as in the JSON files.

```sql
CREATE TABLE meta (
	key   TEXT PRIMARY KEY,
	value TEXT NOT NULL
);

CREATE TABLE projects (
	id       TEXT PRIMARY KEY,
	path     TEXT NOT NULL UNIQUE,
	position INTEGER NOT NULL,
	repo     TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE worktrees (
	path TEXT NOT NULL,
	kind TEXT NOT NULL CHECK (kind IN ('tracked', 'agent')),
	PRIMARY KEY (path, kind)
);

CREATE TABLE shared_settings (
	id                   INTEGER PRIMARY KEY CHECK (id = 1),
	git_remote           TEXT NOT NULL DEFAULT '',
	git_commit_all       INTEGER,
	git_force_with_lease INTEGER NOT NULL DEFAULT 0,
	access               TEXT NOT NULL DEFAULT '',
	worktree_root        TEXT NOT NULL DEFAULT ''
);

CREATE TABLE config (
	key   TEXT PRIMARY KEY,
	value TEXT NOT NULL
);

CREATE TABLE locals (
	id         TEXT PRIMARY KEY,
	project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
	name       TEXT NOT NULL,
	auto       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE names (
	path  TEXT PRIMARY KEY,
	title TEXT NOT NULL,
	auto  INTEGER NOT NULL DEFAULT 0
);
```

The other tables follow the same pattern. Each has one column per field of the struct it stores, with the exceptions listed.

| Table | Struct | Exceptions |
|---|---|---|
| `tickets` | `proto.Ticket` | `folder` becomes `project_id` and `tree`. |
| `automations` | `proto.Automation` | `folder` becomes `project_id` and `tree`. `schedule` is JSON. `webhook_secret` is a column. |
| `runs` | `proto.Run` | `automation_id` references `automations(id) ON DELETE CASCADE`. `turn_after`, `turn_prompt` and `turn_entry` are columns. |
| `hosts` | `hosts.Host` | `target` is JSON. |
| `devices` | `devices.Device` | `scopes` is JSON. |
| `push_records` | `push.Record` | `device` is the primary key. `alert`, `start`, `activity` and `prefs` are JSON. `key` is a BLOB. |
| `push_sends` | `push.Sent` | None. |

Rules for the columns:

- `tree` is the folder when the folder is a worktree of the project, and NULL when the folder is the project's own path. The wire's `folder` is `tree` when it is set, else the project's `path`.
- `projects.position` keeps the order that `registry.list` sends.
- `worktrees` holds the registry's `tracked` and `agent_worktrees` lists. A path can be in both, so the key is the pair. The table has no `project_id`: the lists are paths today, and nothing here needs the link.
- `config` holds every key of `config.json` under the dotted name that `pocketd config set` takes, such as `port`, `listen`, `ownerListen` and `phone.maxAccess`. The value is the JSON value. Keys pocketd doesn't know (`profiles`) are imported and kept.
- `meta` holds the single values: `imported_at`, `host_id`, `board.limit`, `board.last_number` and `devices.grace_ended`.

## Import

pocketd imports when `meta` has no `imported_at`. The import and the `imported_at` row commit together, so an import that fails leaves nothing and runs again at the next start.

1. Read the projects from `state/registry.json`. If the file is missing or broken, read `desktop.json` as `importDesktop` does today. Keep each project's id, or mint one for a project from `desktop.json`.
2. Read the hosts from `state/hosts.json`. If the file is missing, read `desktop.json` as `hosts.ImportDesktop` does today.
3. Read every other file in decision 2. A missing file imports as empty. A file pocketd can't parse imports as empty, and pocketd logs its path.
4. For each Ticket and Automation, find the project for its `folder`:
	- If `folder` is a project's path, set `project_id` and leave `tree` NULL.
	- If `folder` is in a project's worktree, set `project_id` and set `tree` to `folder`. Use the lookup that places a session (`internal/worktree`).
	- Otherwise drop the row and log its id and folder.
5. For each Local, set `project_id` from its `project` path. Drop a Local whose path is no project, and log it.
6. Drop a Run whose Automation was dropped.

The import runs where the stores open today in `cmd/pocketd/serve.go`, after pocketd holds `pocketd.lock`. During an upgrade the new pocketd adopts the lock from the old one, so check in `upgrader.run` that the old pocketd has stopped writing its JSON files before the new one imports them.

## Work

### pocketd (`packages/pocketd`)

- `internal/statedb`: open `state/pocketd.db` (0600) with the pragmas `orch.OpenPath` uses (WAL, `busy_timeout`, `foreign_keys`, one connection) and run the migrations. Move the migration runner out of `internal/orch/store.go` into a package both stores use, so there is one copy. Add `OpenReadOnly` for CLI processes.
- `internal/statedb/import.go`: the import above. It takes the home folder and a transaction, and it has no caller but `Open`.
- `internal/registry`, `internal/locals`, `internal/automation`, `internal/board`, `internal/hosts`, `internal/names`, `internal/devices`, `internal/push`: `Open` takes the database and loads from it. Each save becomes a transaction that writes only the rows that changed. Drop `atomicfile` from each store, and drop the `.bad` and `.broken` renames.
- `internal/registry`: `RemoveProject` deletes the project row. After the commit, `locals`, `board` and `automation` drop the cascaded rows from memory and publish their lists, so clients see the Tickets and Locals go.
- `internal/automation`, `internal/board`, `internal/locals`: resolve a folder to `project_id` and `tree` on save, and refuse a folder in no project.
- `internal/config`: `Load`, `OwnerListen` and `Settings` read and write the `config` table. `Settings` reads the file on every call today. Keep one copy in memory, which is correct because only the daemon writes.
- `internal/state`: `HostID` reads and writes `meta`.
- `cmd/pocketd/serve.go`: open the database first and pass it to every store. `cmd/pocketd/worktree.go`, `push.go` and `owner.go` open it read-only.
- Fix the package comments that name a JSON file.

### Desktop (`packages/desktop`)

No code change. Two tests write `config.json` into a new home before pocketd starts (`crates/daemon/tests/restore.rs` and `crates/hosts/tests/owner_token.rs`). They keep working, because a new home has no database and the import reads the file. Run both to confirm.

Check each sender of `Change::Remove` (`crates/pocket/src/registry.rs`, `fleet.rs`, `sidebar/scratch.rs`, `sidebar/row_menu.rs`, `modals/confirm.rs`). A removal the desktop sends on its own, without the user asking, must not fire for a project that has Tickets, Automations or Locals.

### Docs

- `CONTEXT.md`, **Project**: removing a project also deletes its Tickets, Automations and Locals.
- `CLAUDE.md`, "Where state lives": name `state/pocketd.db`.
- `docs/plans/2026-11-24-pocketd-owns-shared-state.md` stays as written. It records `state/registry.json` as the decision of its day.

## Done when

- A home with the ten JSON files and no database starts with the same projects, ids, settings, Locals, Tickets, Automations, Runs, hosts, names, devices, push records, host id and config that the files held. `registry.list`, `project.list`, the Board and the automation list send the same frames as before the change. The golden fixtures in `internal/proto` don't change.
- A home with only `desktop.json` imports its projects and hosts as it does today.
- A Ticket whose folder is a worktree of a project keeps that folder on the wire. A Ticket whose folder is in no project is dropped, and the log names it.
- An import that fails halfway leaves no `imported_at`, and the next start imports again.
- The JSON files are byte-for-byte the same after the import and after any later change.
- Restarting pocketd keeps every row. Deleting `state/orchestrator.db` keeps every project and device.
- `project.remove` deletes the project's Tickets, Automations, Runs and Locals, and every subscribed client receives the shorter lists.
- Saving an Automation or a Ticket with a folder in no project answers the error that the WS handler answers today.
- `pocketd config set port 4600` changes the port after a restart, and `config.json` still holds the old port.
- `pocketd worktree`, `pocketd push` and `pocketd owner` work while the daemon runs and while it is stopped.
- From `packages/pocketd`: `gofmt`, `env -u POCKETD_SOCK go vet ./...`, staticcheck and `env -u POCKETD_SOCK go test -race -count=1 ./...` pass. From `packages/desktop`: `cargo test --workspace` passes.
