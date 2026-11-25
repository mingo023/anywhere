-- What a session's last turn did, as agent.list tells it for a session no longer running: a proto.Recap as JSON, or empty.
ALTER TABLE sessions ADD COLUMN recap TEXT NOT NULL DEFAULT '';
