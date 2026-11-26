// Package sqlmigrate brings a SQLite database up to the newest of a set of numbered migrations.
package sqlmigrate

import (
	"database/sql"
	"fmt"
	"io/fs"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
)

// Run applies each migrations/NNN_*.sql in fsys that db lacks, in order, each in its own
// transaction with its row in schema_version.
func Run(db *sql.DB, fsys fs.FS) error {
	if _, err := db.Exec(`CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)`); err != nil {
		return err
	}
	var have int
	if err := db.QueryRow(`SELECT COALESCE(MAX(version), 0) FROM schema_version`).Scan(&have); err != nil {
		return err
	}
	names, err := fs.Glob(fsys, "migrations/*.sql")
	if err != nil {
		return err
	}
	slices.Sort(names)
	for _, name := range names {
		n, err := strconv.Atoi(strings.SplitN(filepath.Base(name), "_", 2)[0])
		if err != nil {
			return fmt.Errorf("migration %s: %w", name, err)
		}
		if n <= have {
			continue
		}
		body, err := fs.ReadFile(fsys, name)
		if err != nil {
			return err
		}
		if err := apply(db, name, string(body), n); err != nil {
			return err
		}
	}
	return nil
}

func apply(db *sql.DB, name, body string, n int) error {
	tx, err := db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if _, err := tx.Exec(body); err != nil {
		return fmt.Errorf("migration %s: %w", name, err)
	}
	if _, err := tx.Exec(`INSERT INTO schema_version (version) VALUES (?)`, n); err != nil {
		return err
	}
	return tx.Commit()
}
