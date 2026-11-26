// Package configtest makes Settings for the tests of packages that read them.
package configtest

import (
	"path/filepath"
	"testing"

	"pocketd/internal/config"
	"pocketd/internal/statedb/dbtest"
)

// Settings is the Settings of a new database with an empty config table.
func Settings(t testing.TB) *config.Settings {
	t.Helper()
	s, err := config.NewSettings(dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db")))
	if err != nil {
		t.Fatal(err)
	}
	return s
}
