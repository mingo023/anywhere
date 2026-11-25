package hosts

import (
	"database/sql"
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"pocketd/internal/statedb/dbtest"
)

func newDB(t *testing.T) *sql.DB {
	t.Helper()
	return dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db"))
}

func openManager(t *testing.T, paths Paths, env func() []string, version, name string) (*Manager, error) {
	t.Helper()
	return Open(newDB(t), paths, env, version, name)
}

func TestHostsRoundTripThroughTheDatabase(t *testing.T) {
	db := newDB(t)
	devbox := New(alias("devbox"), nil)
	box := New(Target{Kind: KindAddress, Addr: "10.0.4.12", Port: 22, User: "u", KeyFile: "~/.ssh/k"}, []Host{devbox})
	box.Windows = true
	want := []Host{devbox, box}
	if err := Save(db, nil, want); err != nil {
		t.Fatal(err)
	}
	got, err := Load(db)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("%+v %v", got, err)
	}
}

func TestSavingKeepsTheOrderDropsRemovedHostsAndRewritesAChangedOne(t *testing.T) {
	db := newDB(t)
	a, b, c := Host{ID: "a", Name: "a", Target: alias("a")}, Host{ID: "b", Name: "b", Target: alias("b")}, Host{ID: "c", Name: "c", Target: alias("c")}
	if err := Save(db, nil, []Host{a, b, c}); err != nil {
		t.Fatal(err)
	}
	prev := []Host{a, b, c}
	b.Windows = true
	next := []Host{c, b}
	if err := Save(db, prev, next); err != nil {
		t.Fatal(err)
	}
	if got, err := Load(db); err != nil || !reflect.DeepEqual(got, next) {
		t.Fatalf("%+v %v", got, err)
	}
}

func TestNoHostsRowsMeansNoHosts(t *testing.T) {
	got, err := Load(newDB(t))
	if len(got) != 0 || err != nil {
		t.Fatalf("%+v %v", got, err)
	}
}

func TestEveryHostTheManagerAddedSurvivesARestartAndARemovedOneStaysGone(t *testing.T) {
	db := newDB(t)
	paths := Paths{Home: t.TempDir(), RemoteDir: ".cp"}
	m, err := Open(db, paths, os.Environ, "1.2.0", "Mac")
	if err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"devbox", "gpu"} {
		if _, err := m.Add(alias(name), ""); err != nil {
			t.Fatal(err)
		}
	}
	m.setWindows("gpu", true)
	if err := m.Remove("devbox", false); err != nil {
		t.Fatal(err)
	}
	again, err := Open(db, paths, os.Environ, "1.2.0", "Mac")
	if err != nil {
		t.Fatal(err)
	}
	if l := again.List(); len(l) != 1 || l[0].ID != "gpu" || !l[0].Windows {
		t.Fatalf("%+v", l)
	}
}

func TestHostsFromDesktopJsonAreReadAndTheFileIsLeftAlone(t *testing.T) {
	home := t.TempDir()
	// Saved by the desktop's Store before hosts moved to pocketd.
	sample, err := os.ReadFile("testdata/desktop.json")
	if err != nil {
		t.Fatal(err)
	}
	desktop := filepath.Join(home, "desktop.json")
	if err := os.WriteFile(desktop, sample, 0o600); err != nil {
		t.Fatal(err)
	}
	want := []Host{
		{ID: "devbox", Name: "devbox", Target: alias("devbox")},
		{ID: "10-0-4-12", Name: "10.0.4.12", Target: Target{Kind: KindAddress, Addr: "10.0.4.12", Port: 2222, User: "dev", KeyFile: "~/.ssh/id_ed25519"}, Windows: true},
		{ID: "gpu-1", Name: "gpu-1", Target: Target{Kind: KindAddress, Addr: "gpu-1.lab.example.com", Port: 22}},
	}
	if got, err := DesktopHosts(home); err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("read %+v %v", got, err)
	}
	if got, _ := os.ReadFile(desktop); string(got) != string(sample) {
		t.Fatal("desktop.json changed")
	}
	if entries, _ := os.ReadDir(home); len(entries) != 1 {
		t.Fatalf("wrote %v", entries)
	}
}

func TestADesktopJsonItCannotReadIsAnErrorAndAMissingOneIsNoHosts(t *testing.T) {
	home := t.TempDir()
	if got, err := DesktopHosts(home); got != nil || err != nil {
		t.Fatalf("%+v %v", got, err)
	}
	os.WriteFile(filepath.Join(home, "desktop.json"), []byte("{"), 0o600)
	if _, err := DesktopHosts(home); err == nil {
		t.Fatal("read")
	}
}
