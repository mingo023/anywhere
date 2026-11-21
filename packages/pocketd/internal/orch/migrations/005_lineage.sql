-- The line of each closed session retention would drop while the worktree it worked in is still there, so deleting
-- its parent's worktree still finds it.
CREATE TABLE lineage (
    id TEXT PRIMARY KEY,
    parent_id TEXT NOT NULL DEFAULT '',
    relationship TEXT NOT NULL DEFAULT '',
    cwd TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL DEFAULT 0,
    isolated INTEGER NOT NULL DEFAULT 0
);
