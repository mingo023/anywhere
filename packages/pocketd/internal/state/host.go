package state

import (
	"crypto/rand"
	"database/sql"
	"encoding/hex"
	"errors"
)

// HostID is this host's id, made on first call and kept in the meta table of pocketd.db,
// so a desktop knows the host again under a new address.
func HostID(db *sql.DB) (string, error) {
	var id string
	if err := db.QueryRow(`SELECT value FROM meta WHERE key = 'host_id'`).Scan(&id); err != nil && !errors.Is(err, sql.ErrNoRows) {
		return "", err
	}
	if id != "" {
		return id, nil
	}
	b := make([]byte, 16)
	rand.Read(b)
	id = hex.EncodeToString(b)
	_, err := db.Exec(`INSERT INTO meta (key, value) VALUES ('host_id', ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value`, id)
	return id, err
}
