CREATE TABLE events (
	seq INTEGER PRIMARY KEY AUTOINCREMENT,
	session_id TEXT NOT NULL,
	kind TEXT NOT NULL,
	payload TEXT NOT NULL,
	at INTEGER NOT NULL
);
CREATE INDEX events_session ON events (session_id, seq);

CREATE TABLE sessions (
	id TEXT PRIMARY KEY,
	project TEXT NOT NULL DEFAULT '',
	worktree TEXT NOT NULL DEFAULT '',
	cwd TEXT NOT NULL DEFAULT '',
	branch TEXT NOT NULL DEFAULT '',
	agent TEXT NOT NULL DEFAULT '',
	account TEXT NOT NULL DEFAULT '',
	access TEXT NOT NULL DEFAULT '',
	model TEXT NOT NULL DEFAULT '',
	effort TEXT NOT NULL DEFAULT '',
	conversation TEXT NOT NULL DEFAULT '',
	terminal TEXT NOT NULL DEFAULT '',
	chat INTEGER NOT NULL DEFAULT 0,
	status TEXT NOT NULL DEFAULT 'idle',
	failed INTEGER NOT NULL DEFAULT 0,
	error TEXT NOT NULL DEFAULT '',
	pinned INTEGER NOT NULL DEFAULT 0,
	title TEXT NOT NULL DEFAULT '',
	origin TEXT NOT NULL DEFAULT '',
	parent_id TEXT NOT NULL DEFAULT '',
	relationship TEXT NOT NULL DEFAULT '',
	depth INTEGER NOT NULL DEFAULT 0,
	created_by TEXT NOT NULL DEFAULT 'user',
	resets INTEGER NOT NULL DEFAULT 0,
	created_at INTEGER NOT NULL DEFAULT 0,
	updated_at INTEGER NOT NULL DEFAULT 0,
	closed_at INTEGER NOT NULL DEFAULT 0,
	archived INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX sessions_parent ON sessions (parent_id);

CREATE TABLE turns (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	session_id TEXT NOT NULL,
	state TEXT NOT NULL,
	started_at INTEGER NOT NULL,
	ended_at INTEGER NOT NULL DEFAULT 0,
	error TEXT NOT NULL DEFAULT '',
	results_before INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX turns_session ON turns (session_id, id);

CREATE TABLE items (
	session_id TEXT NOT NULL,
	item_id TEXT NOT NULL,
	seq INTEGER NOT NULL,
	epoch INTEGER NOT NULL,
	kind TEXT NOT NULL,
	parent_tool TEXT NOT NULL DEFAULT '',
	text TEXT NOT NULL DEFAULT '',
	body BLOB NOT NULL,
	ts INTEGER NOT NULL,
	PRIMARY KEY (session_id, item_id)
);
CREATE INDEX items_seq ON items (session_id, seq);

CREATE VIRTUAL TABLE items_fts USING fts5 (session_id UNINDEXED, item_id UNINDEXED, text);

CREATE TABLE delegations (
	task_id TEXT PRIMARY KEY,
	parent_id TEXT NOT NULL,
	parent_turn INTEGER NOT NULL DEFAULT 0,
	child_id TEXT NOT NULL DEFAULT '',
	title TEXT NOT NULL DEFAULT '',
	role TEXT NOT NULL DEFAULT '',
	agent TEXT NOT NULL DEFAULT '',
	brief TEXT NOT NULL DEFAULT '',
	isolation TEXT NOT NULL DEFAULT 'worktree',
	mode TEXT NOT NULL DEFAULT 'async',
	status TEXT NOT NULL DEFAULT 'queued',
	work_state TEXT NOT NULL DEFAULT '',
	summary TEXT NOT NULL DEFAULT '',
	error TEXT NOT NULL DEFAULT '',
	access TEXT NOT NULL DEFAULT '',
	stepped_down TEXT NOT NULL DEFAULT '',
	completion_wake TEXT NOT NULL DEFAULT 'settled_only',
	delivery TEXT NOT NULL DEFAULT 'none',
	pending_question TEXT NOT NULL DEFAULT '',
	client_request_id TEXT NOT NULL DEFAULT '',
	created_at INTEGER NOT NULL,
	settled_at INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX delegations_parent ON delegations (parent_id);
CREATE INDEX delegations_child ON delegations (child_id);

CREATE TABLE queue (
	id TEXT PRIMARY KEY,
	session_id TEXT NOT NULL,
	text TEXT NOT NULL,
	origin TEXT NOT NULL DEFAULT 'user',
	ord REAL NOT NULL,
	state TEXT NOT NULL DEFAULT 'pending',
	refs TEXT NOT NULL DEFAULT '',
	created_at INTEGER NOT NULL
);
CREATE INDEX queue_session ON queue (session_id, state, ord);

CREATE TABLE requests (
	id TEXT PRIMARY KEY,
	session_id TEXT NOT NULL,
	kind TEXT NOT NULL,
	payload TEXT NOT NULL,
	state TEXT NOT NULL DEFAULT 'open',
	created_at INTEGER NOT NULL
);
CREATE INDEX requests_session ON requests (session_id, state);

CREATE TABLE transfers (
	id TEXT PRIMARY KEY,
	source_id TEXT NOT NULL,
	target_id TEXT NOT NULL,
	kind TEXT NOT NULL,
	source_point INTEGER NOT NULL DEFAULT 0,
	state TEXT NOT NULL,
	created_at INTEGER NOT NULL
);

CREATE TABLE receipts (
	command_id TEXT PRIMARY KEY,
	spec_hash TEXT NOT NULL,
	result TEXT NOT NULL,
	at INTEGER NOT NULL
);

CREATE TABLE outbox (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	session_id TEXT NOT NULL,
	kind TEXT NOT NULL,
	payload TEXT NOT NULL,
	state TEXT NOT NULL DEFAULT 'pending',
	attempts INTEGER NOT NULL DEFAULT 0,
	last_error TEXT NOT NULL DEFAULT '',
	command_id TEXT NOT NULL DEFAULT '',
	then_cmd TEXT NOT NULL DEFAULT '',
	else_cmd TEXT NOT NULL DEFAULT '',
	result TEXT NOT NULL DEFAULT '',
	created_at INTEGER NOT NULL
);
CREATE INDEX outbox_claim ON outbox (state, session_id, id);

CREATE TABLE tokens (
	hash TEXT PRIMARY KEY,
	session_id TEXT NOT NULL,
	agent TEXT NOT NULL,
	issued_at INTEGER NOT NULL,
	last_alive INTEGER NOT NULL,
	expired INTEGER NOT NULL DEFAULT 0,
	revoked INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX tokens_session ON tokens (session_id);

CREATE TABLE held (
	session_id TEXT NOT NULL,
	task_id TEXT NOT NULL,
	until_at INTEGER NOT NULL,
	PRIMARY KEY (session_id, task_id)
);
