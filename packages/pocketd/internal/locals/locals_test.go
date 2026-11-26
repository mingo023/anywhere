package locals

import (
	"database/sql"
	"errors"
	"path/filepath"
	"reflect"
	"testing"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/statedb/dbtest"
)

type home struct {
	db   *sql.DB
	reg  *registry.Registry
	p, q string
}

func newHome(t *testing.T) home {
	t.Helper()
	db := dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db"))
	reg, err := registry.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	h := home{db: db, reg: reg, p: t.TempDir(), q: t.TempDir()}
	for _, dir := range []string{h.p, h.q} {
		if err := reg.AddProject(dir, nil); err != nil {
			t.Fatal(err)
		}
	}
	return h
}

func (h home) open(t *testing.T) *Locals {
	t.Helper()
	l, err := Open(h.db, h.reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	return l
}

func TestLocalsAreListedInTheOrderTheyWereCreated(t *testing.T) {
	h := newHome(t)
	l := h.open(t)
	l.Create("b", h.p, "Local 2")
	l.Create("a", h.q, "  Local 3 ")
	want := []proto.Local{{ID: "b", Project: h.p, Name: "Local 2"}, {ID: "a", Project: h.q, Name: "Local 3"}}
	if got := l.List(); !reflect.DeepEqual(got, want) {
		t.Fatalf("%v", got)
	}
	if got, ok := l.Get("a"); !ok || got != want[1] {
		t.Fatalf("%v %v", got, ok)
	}
}

func TestASecondLocalWithTheSameIdOrNoNameIsRefused(t *testing.T) {
	h := newHome(t)
	l := h.open(t)
	l.Create("a", h.p, "Local 2")
	if err := l.Create("a", h.q, "Other"); !errors.Is(err, ErrExists) {
		t.Fatal(err)
	}
	if err := l.Create("b", h.p, "   "); !errors.Is(err, ErrNoName) {
		t.Fatal(err)
	}
	if got := l.List(); len(got) != 1 || got[0].Project != h.p {
		t.Fatalf("%v", got)
	}
}

func TestALocalInAFolderThatIsNoProjectIsRefusedAndSavesNothing(t *testing.T) {
	h := newHome(t)
	l := h.open(t)
	if err := l.Create("a", t.TempDir(), "Local 2"); !errors.Is(err, registry.ErrUnknownProject) {
		t.Fatal(err)
	}
	if got := l.List(); len(got) != 0 {
		t.Fatalf("%v", got)
	}
	if got := h.open(t).List(); len(got) != 0 {
		t.Fatalf("after a restart %v", got)
	}
}

func TestARenameTrimsTheTitleAndABlankOneChangesNothing(t *testing.T) {
	h := newHome(t)
	l := h.open(t)
	l.Create("a", h.p, "Local 2")
	if err := l.Rename("a", " Review "); err != nil {
		t.Fatal(err)
	}
	if err := l.Rename("a", "  "); err != nil {
		t.Fatal(err)
	}
	if got, _ := l.Get("a"); got.Name != "Review" {
		t.Fatalf("%q", got.Name)
	}
	if err := l.Rename("zz", "x"); !errors.Is(err, ErrUnknown) {
		t.Fatal(err)
	}
}

func TestDeletingALocalForgetsItAndAnUnknownOneIsAnError(t *testing.T) {
	h := newHome(t)
	l := h.open(t)
	l.Create("a", h.p, "Local 2")
	if err := l.Delete("a"); err != nil {
		t.Fatal(err)
	}
	if _, ok := l.Get("a"); ok || len(l.List()) != 0 {
		t.Fatalf("%v", l.List())
	}
	if err := l.Delete("a"); !errors.Is(err, ErrUnknown) {
		t.Fatal(err)
	}
}

func TestLocalsSurviveARestart(t *testing.T) {
	h := newHome(t)
	l := h.open(t)
	l.Create("a", h.p, "Local 2")
	l.Create("b", h.p, "Local 3")
	l.Create("c", h.q, "Local 4")
	l.Delete("a")
	want := []proto.Local{{ID: "b", Project: h.p, Name: "Local 3"}, {ID: "c", Project: h.q, Name: "Local 4"}}
	if got := h.open(t).List(); !reflect.DeepEqual(got, want) {
		t.Fatalf("%v", got)
	}
}

func TestEachChangeSendsEveryLocal(t *testing.T) {
	h := newHome(t)
	l := h.open(t)
	msgs, stop := l.Subscribe()
	defer stop()
	l.Create("a", h.p, "Local 2")
	l.Rename("a", "")
	l.Delete("a")
	for _, want := range []string{
		`{"type":"local.list","locals":[{"id":"a","project":"` + h.p + `","name":"Local 2"}]}`,
		`{"type":"local.list","locals":[]}`,
	} {
		select {
		case raw := <-msgs:
			if string(raw) != want {
				t.Fatalf("got %s\nwant %s", raw, want)
			}
		case <-time.After(time.Second):
			t.Fatal("nothing published")
		}
	}
	select {
	case raw := <-msgs:
		t.Fatalf("extra %s", raw)
	default:
	}
}

func TestNamingRenamesANewLocalUntilItSettlesOrTheUserRenamesIt(t *testing.T) {
	h := newHome(t)
	l := h.open(t)
	l.Create("a", h.p, "Local 2")
	l.Create("b", h.p, "Local 3")
	if !l.Auto("a", "Fix login") || !l.Auto("a", "Fix the login form") {
		t.Fatal("refused")
	}
	l.Settle("a")
	l.Rename("b", "Mine")
	if l.Auto("a", "Other") || l.Auto("b", "Other") {
		t.Fatal("renamed")
	}
	want := []proto.Local{{ID: "a", Project: h.p, Name: "Fix the login form"}, {ID: "b", Project: h.p, Name: "Mine"}}
	if got := h.open(t).List(); !reflect.DeepEqual(got, want) {
		t.Fatalf("%v", got)
	}
}

func TestALocalStillAPlaceholderAfterARestartIsNamedLater(t *testing.T) {
	h := newHome(t)
	h.open(t).Create("a", h.p, "Local 2")
	l := h.open(t)
	if !l.Auto("a", "Fix login") {
		t.Fatal("refused")
	}
	if got, _ := l.Get("a"); got.Name != "Fix login" {
		t.Fatalf("%q", got.Name)
	}
}

func TestRemovingAProjectDropsItsLocalsAndTellsSubscribers(t *testing.T) {
	h := newHome(t)
	l := h.open(t)
	h.reg.OnRemove(func(string) { l.Reload() })
	l.Create("a", h.p, "Local 2")
	l.Create("b", h.q, "Local 3")
	msgs, stop := l.Subscribe()
	defer stop()
	if err := h.reg.RemoveProject(h.p); err != nil {
		t.Fatal(err)
	}
	want := `{"type":"local.list","locals":[{"id":"b","project":"` + h.q + `","name":"Local 3"}]}`
	select {
	case raw := <-msgs:
		if string(raw) != want {
			t.Fatalf("got %s\nwant %s", raw, want)
		}
	case <-time.After(time.Second):
		t.Fatal("nothing published")
	}
	if got := h.open(t).List(); len(got) != 1 || got[0].ID != "b" {
		t.Fatalf("after a restart %v", got)
	}
}
