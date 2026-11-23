-- A search row takes its item's rowid: fts5 finds a row by an UNINDEXED column only by reading every row.
CREATE VIRTUAL TABLE items_fts_keyed USING fts5 (session_id UNINDEXED, item_id UNINDEXED, text);
INSERT INTO items_fts_keyed (rowid, session_id, item_id, text)
	SELECT row, session_id, item_id, text FROM (
		SELECT i.rowid AS row, f.session_id, f.item_id, f.text, MAX(f.rowid) FROM items_fts f JOIN items i ON i.session_id = f.session_id AND i.item_id = f.item_id GROUP BY i.rowid);
DROP TABLE items_fts;
ALTER TABLE items_fts_keyed RENAME TO items_fts;
