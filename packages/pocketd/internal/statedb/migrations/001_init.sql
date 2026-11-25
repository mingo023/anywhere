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
	path     TEXT NOT NULL,
	kind     TEXT NOT NULL CHECK (kind IN ('tracked', 'agent')),
	position INTEGER NOT NULL,
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
	auto       INTEGER NOT NULL DEFAULT 0,
	position   INTEGER NOT NULL
);

CREATE TABLE names (
	path  TEXT PRIMARY KEY,
	title TEXT NOT NULL,
	auto  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE tickets (
	id          TEXT PRIMARY KEY,
	project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
	tree        TEXT,
	title       TEXT NOT NULL,
	prompt      TEXT NOT NULL,
	provider    TEXT NOT NULL,
	base        TEXT NOT NULL DEFAULT '',
	state       TEXT NOT NULL,
	sort_order  INTEGER NOT NULL DEFAULT 0,
	status      TEXT NOT NULL DEFAULT '',
	why         TEXT NOT NULL DEFAULT '',
	agent_id    TEXT NOT NULL DEFAULT '',
	terminal_id TEXT NOT NULL DEFAULT '',
	worktree    TEXT NOT NULL DEFAULT '',
	created_at  INTEGER NOT NULL DEFAULT 0,
	changed_at  INTEGER NOT NULL DEFAULT 0,
	closed_at   INTEGER NOT NULL DEFAULT 0,
	closed_why  TEXT NOT NULL DEFAULT '',
	relaunch    INTEGER NOT NULL DEFAULT 0,
	number      INTEGER NOT NULL DEFAULT 0,
	priority    INTEGER NOT NULL DEFAULT 0,
	labels      TEXT NOT NULL DEFAULT '[]',
	position    INTEGER NOT NULL
);

CREATE INDEX tickets_project ON tickets (project_id);

CREATE TABLE automations (
	id             TEXT PRIMARY KEY,
	project_id     TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
	tree           TEXT,
	name           TEXT NOT NULL,
	prompt         TEXT NOT NULL,
	provider       TEXT NOT NULL,
	schedule       TEXT NOT NULL,
	enabled        INTEGER NOT NULL DEFAULT 0,
	next_run_at    INTEGER NOT NULL DEFAULT 0,
	access         TEXT NOT NULL DEFAULT '',
	new_worktree   INTEGER NOT NULL DEFAULT 0,
	base           TEXT NOT NULL DEFAULT '',
	model          TEXT NOT NULL DEFAULT '',
	effort         TEXT NOT NULL DEFAULT '',
	created_by     TEXT NOT NULL DEFAULT '',
	bound          TEXT NOT NULL DEFAULT '',
	webhook        INTEGER NOT NULL DEFAULT 0,
	webhook_secret TEXT NOT NULL DEFAULT '',
	position       INTEGER NOT NULL
);

CREATE INDEX automations_project ON automations (project_id);

CREATE TABLE runs (
	id            TEXT PRIMARY KEY,
	automation_id TEXT NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
	status        TEXT NOT NULL,
	trigger       TEXT NOT NULL,
	why           TEXT NOT NULL DEFAULT '',
	summary       TEXT NOT NULL DEFAULT '',
	started_at    INTEGER NOT NULL DEFAULT 0,
	finished_at   INTEGER NOT NULL DEFAULT 0,
	agent_id      TEXT NOT NULL DEFAULT '',
	terminal_id   TEXT NOT NULL DEFAULT '',
	access        TEXT NOT NULL DEFAULT '',
	worktree      TEXT NOT NULL DEFAULT '',
	turn_after    INTEGER NOT NULL DEFAULT 0,
	turn_prompt   TEXT NOT NULL DEFAULT '',
	turn_entry    TEXT NOT NULL DEFAULT '',
	position      INTEGER NOT NULL
);

CREATE INDEX runs_automation ON runs (automation_id);

CREATE TABLE hosts (
	id       TEXT PRIMARY KEY,
	name     TEXT NOT NULL,
	target   TEXT NOT NULL,
	windows  INTEGER NOT NULL DEFAULT 0,
	position INTEGER NOT NULL
);

CREATE TABLE devices (
	id            TEXT PRIMARY KEY,
	name          TEXT NOT NULL,
	platform      TEXT NOT NULL DEFAULT '',
	scopes        TEXT NOT NULL DEFAULT '[]',
	token_hash    TEXT NOT NULL DEFAULT '',
	created_at    INTEGER NOT NULL DEFAULT 0,
	last_seen_at  INTEGER NOT NULL DEFAULT 0,
	last_addr     TEXT NOT NULL DEFAULT '',
	legacy        INTEGER NOT NULL DEFAULT 0,
	grace_ends_at INTEGER NOT NULL DEFAULT 0,
	position      INTEGER NOT NULL
);

CREATE TABLE push_records (
	device     TEXT PRIMARY KEY,
	key        BLOB,
	env        TEXT NOT NULL DEFAULT '',
	alert      TEXT,
	start      TEXT,
	activity   TEXT,
	prefs      TEXT NOT NULL DEFAULT '{}',
	frequent   INTEGER NOT NULL DEFAULT 0,
	updated_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE push_sends (
	position INTEGER PRIMARY KEY,
	at       INTEGER NOT NULL,
	device   TEXT NOT NULL,
	kind     TEXT NOT NULL,
	agent    TEXT NOT NULL DEFAULT '',
	priority INTEGER NOT NULL DEFAULT 0,
	outcome  TEXT NOT NULL DEFAULT ''
);
