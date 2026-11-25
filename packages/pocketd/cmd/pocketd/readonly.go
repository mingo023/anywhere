package main

import (
	"database/sql"
	"errors"

	"pocketd/internal/statedb"
)

// openReadOnly is pocketd.db for a command that only reads it, beside a running daemon or without one. Before a
// daemon made the database it returns nil, which reads as empty state.
func openReadOnly(home string) (*sql.DB, error) {
	db, err := statedb.OpenReadOnly(home)
	if errors.Is(err, statedb.ErrNoDatabase) {
		return nil, nil
	}
	return db, err
}
