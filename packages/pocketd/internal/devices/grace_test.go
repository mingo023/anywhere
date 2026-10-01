package devices

import (
	"path/filepath"
	"testing"
	"time"
)

func legacyStore(t *testing.T) (*Store, string) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "devices.json")
	s, err := Open(path, "")
	if err != nil {
		t.Fatal(err)
	}
	if err := s.EnsureLegacy("old-token"); err != nil {
		t.Fatal(err)
	}
	return s, path
}

func TestTheGraceStartsOnceAndLastsSevenDays(t *testing.T) {
	s, path := legacyStore(t)
	first := time.Date(2026, 10, 1, 9, 0, 0, 0, time.UTC)
	end, err := s.StartGrace(first)
	if err != nil || !end.Equal(first.Add(7*24*time.Hour)) {
		t.Fatalf("%v %v", end, err)
	}
	again, _ := Open(path, "")
	if later, _ := again.StartGrace(first.Add(48 * time.Hour)); !later.Equal(end) {
		t.Fatalf("a restart moved the grace to %v", later)
	}
}

func TestTheLegacyDeviceIsGoneOnceItsGraceEnds(t *testing.T) {
	s, path := legacyStore(t)
	first := time.Date(2026, 10, 1, 9, 0, 0, 0, time.UTC)
	end, _ := s.StartGrace(first)
	if ended, _ := s.EndGrace(end.Add(-time.Minute)); ended {
		t.Fatal("ended early")
	}
	if ended, err := s.EndGrace(end); !ended || err != nil {
		t.Fatalf("%v %v", ended, err)
	}
	again, _ := Open(path, "")
	if _, ok := again.Lookup("old-token"); ok {
		t.Fatal("the old token still works")
	}
}

func TestTheOldTokenIsRefusedOnceTheGraceIsOverEvenBeforeItIsForgotten(t *testing.T) {
	s, _ := legacyStore(t)
	if _, err := s.StartGrace(time.Now().Add(-Grace)); err != nil {
		t.Fatal(err)
	}
	if _, ok := s.Lookup("old-token"); ok {
		t.Fatal("the old token works after its grace")
	}
}

func TestAStoreWithoutALegacyDeviceHasNoGrace(t *testing.T) {
	s, err := Open(filepath.Join(t.TempDir(), "devices.json"), "")
	if err != nil {
		t.Fatal(err)
	}
	if end, err := s.StartGrace(time.Now()); !end.IsZero() || err != nil {
		t.Fatalf("%v %v", end, err)
	}
}

func TestAnEndedGraceStaysEndedWhenTheOldTokenTurnsUpAgain(t *testing.T) {
	s, path := legacyStore(t)
	end, _ := s.StartGrace(time.Now().Add(-Grace))
	if over, err := s.EndGrace(end); !over || err != nil {
		t.Fatalf("%v %v", over, err)
	}
	if err := s.EnsureLegacy("old-token"); err != nil {
		t.Fatal(err)
	}
	again, err := Open(path, "old-token")
	if err != nil {
		t.Fatal(err)
	}
	if s.Has(LegacyID) || again.Has(LegacyID) {
		t.Fatal("the legacy device came back with a fresh grace")
	}
}
