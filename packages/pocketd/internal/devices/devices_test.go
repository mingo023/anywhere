package devices

import (
	"errors"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"pocketd/internal/statedb/dbtest"
)

func openAt(t *testing.T, path, legacyToken string) (*Store, error) {
	t.Helper()
	return Open(dbtest.Open(t, path), legacyToken)
}

// deviceRows is every column of every device, as text.
func deviceRows(t *testing.T, path string) string {
	t.Helper()
	var rows string
	err := dbtest.Open(t, path).QueryRow(`SELECT coalesce(group_concat(id || '|' || name || '|' || platform || '|' || scopes || '|' || token_hash || '|' || last_addr, char(10)), '') FROM devices`).Scan(&rows)
	if err != nil {
		t.Fatal(err)
	}
	return rows
}

func open(t *testing.T) (*Store, string) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "pocketd.db")
	s, err := openAt(t, path, "legacy-token")
	if err != nil {
		t.Fatal(err)
	}
	return s, path
}

func TestATokenIsStoredOnlyAsItsHash(t *testing.T) {
	s, path := open(t)
	d, token, err := s.Add("iPhone", "ios", PhoneScopes)
	if err != nil || len(token) != 43 || d.TokenHash != "" {
		t.Fatalf("%+v %q %v", d, token, err)
	}
	if rows := deviceRows(t, path); strings.Contains(rows, token) || !strings.Contains(rows, hash(token)) {
		t.Fatalf("%s", rows)
	}
}

func TestLookupFindsPairedDevicesAndTheSharedToken(t *testing.T) {
	s, path := open(t)
	d, token, _ := s.Add("iPhone", "ios", PhoneScopes)
	again, err := openAt(t, path, "legacy-token")
	if err != nil {
		t.Fatal(err)
	}
	if got, ok := again.Lookup(token); !ok || got.ID != d.ID || got.TokenHash != "" {
		t.Fatalf("%+v %v", got, ok)
	}
	if got, ok := again.Lookup("legacy-token"); !ok || got.ID != LegacyID || len(got.Scopes) != 3 {
		t.Fatalf("%+v %v", got, ok)
	}
	for _, bad := range []string{"", "nope", token[:42] + "x", "legacy-token "} {
		if _, ok := again.Lookup(bad); ok {
			t.Fatalf("%q got in", bad)
		}
	}
	if s, _ := openAt(t, filepath.Join(t.TempDir(), "pocketd.db"), ""); s.Has(LegacyID) || len(s.List()) != 0 {
		t.Fatal("a legacy device without a legacy token")
	}
}

func TestRevokeForgetsTheToken(t *testing.T) {
	s, _ := open(t)
	d, token, _ := s.Add("iPhone", "ios", PhoneScopes)
	if got, err := s.Revoke(d.ID); err != nil || got.Name != "iPhone" {
		t.Fatalf("%+v %v", got, err)
	}
	if _, ok := s.Lookup(token); ok || s.Has(d.ID) {
		t.Fatal("revoked token still works")
	}
	if _, err := s.Revoke(d.ID); !errors.Is(err, ErrNotFound) {
		t.Fatal(err)
	}
}

func TestTheSharedTokenCantBeRenamedOrRevoked(t *testing.T) {
	s, _ := open(t)
	if _, err := s.Rename(LegacyID, "Mine"); !errors.Is(err, ErrLegacy) {
		t.Fatal(err)
	}
	if _, err := s.Revoke(LegacyID); !errors.Is(err, ErrLegacy) {
		t.Fatal(err)
	}
	if list := s.List(); len(list) != 1 || list[0].Name != "Shared token (legacy)" || !list[0].Legacy {
		t.Fatalf("%+v", list)
	}
}

func TestResolveTakesAUniquePrefix(t *testing.T) {
	s, _ := open(t)
	s.devices = append(s.devices, Device{ID: "3fa9c1d2aa"}, Device{ID: "3fb0000000"})
	for prefix, want := range map[string]string{"3fa": "3fa9c1d2aa", "3fb0000000": "3fb0000000", "legacy": LegacyID} {
		if got, err := s.Resolve(prefix); err != nil || got != want {
			t.Errorf("%q: %q %v", prefix, got, err)
		}
	}
	if _, err := s.Resolve("3f"); !errors.Is(err, ErrAmbiguous) {
		t.Error(err)
	}
	for _, prefix := range []string{"", "9"} {
		if _, err := s.Resolve(prefix); !errors.Is(err, ErrNotFound) {
			t.Errorf("%q: %v", prefix, err)
		}
	}
}

func TestNamesAreCleaned(t *testing.T) {
	s, _ := open(t)
	d, _, _ := s.Add("  Work\x1b[31m phone\n ", "ios", PhoneScopes)
	if d.Name != "Work[31m phone" {
		t.Fatalf("%q", d.Name)
	}
	if d, _, _ = s.Add(" \t", "ios", PhoneScopes); d.Name != "iPhone" {
		t.Fatalf("%q", d.Name)
	}
	if d, _ = s.Rename(d.ID, strings.Repeat("é", 70)); d.Name != strings.Repeat("é", 64) {
		t.Fatalf("%q", d.Name)
	}
	if _, err := s.Rename(d.ID, "\n"); err == nil {
		t.Fatal("renamed to nothing")
	}
}

func TestLastSeenIsSavedAtMostOncePerMinutePerDevice(t *testing.T) {
	s, path := open(t)
	d, _, _ := s.Add("iPhone", "ios", PhoneScopes)
	e, _, _ := s.Add("iPad", "ios", PhoneScopes)
	saved := func(i int) int64 {
		again, _ := openAt(t, path, "")
		return again.List()[i].LastSeenAt
	}
	t0 := time.UnixMilli(1_000_000_000_000)
	s.Seen(d.ID, "100.77.122.90", t0)
	s.Seen(d.ID, "100.77.122.90", t0.Add(30*time.Second))
	if got := saved(1); got != t0.UnixMilli() {
		t.Fatalf("saved %d", got)
	}
	if got := s.List()[1].LastSeenAt; got != t0.Add(30*time.Second).UnixMilli() {
		t.Fatalf("in memory %d", got)
	}
	s.Seen(e.ID, "100.77.122.91", t0.Add(40*time.Second))
	if got := saved(2); got != t0.Add(40*time.Second).UnixMilli() {
		t.Fatalf("second device saved %d", got)
	}
	s.Seen(d.ID, "100.77.122.90", t0.Add(61*time.Second))
	if got := saved(1); got != t0.Add(61*time.Second).UnixMilli() {
		t.Fatalf("saved %d", got)
	}
	s.Seen(LegacyID, "127.0.0.1", t0)
	if got := s.List()[0]; got.LastSeenAt != t0.UnixMilli() || got.LastAddr != "127.0.0.1" {
		t.Fatalf("%+v", got)
	}
}

func TestADeviceWhoseScopesAreNotJSONRefusesToOpen(t *testing.T) {
	path := filepath.Join(t.TempDir(), "pocketd.db")
	db := dbtest.Open(t, path)
	if _, err := db.Exec(`INSERT INTO devices (id, name, scopes, position) VALUES ('a', 'x', '{', 0)`); err != nil {
		t.Fatal(err)
	}
	if _, err := Open(db, "t"); err == nil {
		t.Fatal("opened")
	}
}

func TestEveryDeviceAndTheGraceFlagSurviveAReopen(t *testing.T) {
	s, path := open(t)
	phone, _, _ := s.Add("iPhone", "ios", PhoneScopes)
	pad, _, _ := s.Add("iPad", "ios", LegacyScopes)
	s.Rename(phone.ID, "Mine")
	s.Revoke(pad.ID)
	if _, err := s.StartGrace(time.UnixMilli(1)); err != nil {
		t.Fatal(err)
	}
	if over, err := s.EndGrace(time.UnixMilli(1).Add(Grace)); !over || err != nil {
		t.Fatal(over, err)
	}
	again, err := openAt(t, path, "legacy-token")
	if err != nil {
		t.Fatal(err)
	}
	got := again.List()
	if len(got) != 1 || got[0].ID != phone.ID || got[0].Name != "Mine" || !reflect.DeepEqual(got[0].Scopes, PhoneScopes) || again.Has(LegacyID) {
		t.Fatalf("%+v", got)
	}
}
