package devices

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestTheLegacyTokenIsStoredHashedOnceAndListedFirst(t *testing.T) {
	path := filepath.Join(t.TempDir(), "pocketd.db")
	s, err := openAt(t, path, "")
	if err != nil {
		t.Fatal(err)
	}
	s.Add("iPhone", "ios", PhoneScopes)
	for range 2 {
		if err := s.EnsureLegacy("old-token"); err != nil {
			t.Fatal(err)
		}
	}
	if rows := deviceRows(t, path); strings.Contains(rows, "old-token") || strings.Count(rows, "legacy|") != 1 {
		t.Fatalf("devices = %s", rows)
	}
	again, _ := openAt(t, path, "")
	d, ok := again.Lookup("old-token")
	if !ok || d.ID != LegacyID || !d.Legacy || len(d.Scopes) != len(LegacyScopes) {
		t.Fatalf("lookup = %+v %v", d, ok)
	}
	if list := again.List(); len(list) != 2 || list[0].ID != LegacyID {
		t.Fatalf("%+v", list)
	}
	if _, ok := again.Lookup(""); ok {
		t.Fatal("an empty token found a device")
	}
}
