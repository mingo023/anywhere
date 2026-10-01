package devices

import (
	"bytes"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func open(t *testing.T) (*Store, string) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "devices.json")
	s, err := Open(path, "legacy-token")
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
	raw, _ := os.ReadFile(path)
	if bytes.Contains(raw, []byte(token)) || !bytes.Contains(raw, []byte(hash(token))) {
		t.Fatalf("%s", raw)
	}
	if st, _ := os.Stat(path); st.Mode().Perm() != 0o600 {
		t.Fatalf("mode %v", st.Mode())
	}
	if entries, _ := os.ReadDir(filepath.Dir(path)); len(entries) != 1 {
		t.Fatalf("%v", entries)
	}
}

func TestLookupFindsPairedDevicesAndTheSharedToken(t *testing.T) {
	s, path := open(t)
	d, token, _ := s.Add("iPhone", "ios", PhoneScopes)
	again, err := Open(path, "legacy-token")
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
	if s, _ := Open(filepath.Join(t.TempDir(), "devices.json"), ""); s.Has(LegacyID) || len(s.List()) != 0 {
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
		again, _ := Open(path, "")
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

func TestACorruptFileRefusesToOpen(t *testing.T) {
	path := filepath.Join(t.TempDir(), "devices.json")
	for _, raw := range []string{"{", `{"version":2,"devices":[]}`} {
		os.WriteFile(path, []byte(raw), 0o600)
		if _, err := Open(path, "t"); err == nil {
			t.Fatalf("%s opened", raw)
		}
	}
}
