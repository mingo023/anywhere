package locals

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"testing"
	"time"

	"pocketd/internal/proto"
)

func TestLocalsAreListedInTheOrderTheyWereCreated(t *testing.T) {
	l := Open(t.TempDir())
	l.Create("b", "/p", "Local 2")
	l.Create("a", "/q", "  Local 3 ")
	want := []proto.Local{{ID: "b", Project: "/p", Name: "Local 2"}, {ID: "a", Project: "/q", Name: "Local 3"}}
	if got := l.List(); !reflect.DeepEqual(got, want) {
		t.Fatalf("%v", got)
	}
	if got, ok := l.Get("a"); !ok || got != want[1] {
		t.Fatalf("%v %v", got, ok)
	}
}

func TestASecondLocalWithTheSameIdOrNoNameIsRefused(t *testing.T) {
	l := Open(t.TempDir())
	l.Create("a", "/p", "Local 2")
	if err := l.Create("a", "/q", "Other"); !errors.Is(err, ErrExists) {
		t.Fatal(err)
	}
	if err := l.Create("b", "/p", "   "); !errors.Is(err, ErrNoName) {
		t.Fatal(err)
	}
	if got := l.List(); len(got) != 1 || got[0].Project != "/p" {
		t.Fatalf("%v", got)
	}
}

func TestARenameTrimsTheTitleAndABlankOneChangesNothing(t *testing.T) {
	l := Open(t.TempDir())
	l.Create("a", "/p", "Local 2")
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
	l := Open(t.TempDir())
	l.Create("a", "/p", "Local 2")
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

func TestLocalsSurviveARestartInAPrivateFile(t *testing.T) {
	home := t.TempDir()
	l := Open(home)
	l.Create("a", "/p", "Local 2")
	l.Create("b", "/p", "Local 3")
	l.Delete("a")
	if got := Open(home).List(); !reflect.DeepEqual(got, []proto.Local{{ID: "b", Project: "/p", Name: "Local 3"}}) {
		t.Fatalf("%v", got)
	}
	if st, err := os.Stat(filepath.Join(home, "state", "locals.json")); err != nil || st.Mode().Perm() != 0o600 {
		t.Fatalf("%v %v", st, err)
	}
}

func TestEachChangeSendsEveryLocal(t *testing.T) {
	l := Open(t.TempDir())
	msgs, stop := l.Subscribe()
	defer stop()
	l.Create("a", "/p", "Local 2")
	l.Rename("a", "")
	l.Delete("a")
	for _, want := range []string{
		`{"type":"local.list","locals":[{"id":"a","project":"/p","name":"Local 2"}]}`,
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
