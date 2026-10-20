package wsserver

import (
	"testing"
	"time"

	"pocketd/internal/names"
	"pocketd/internal/peer"
	"pocketd/internal/terminal"
)

type retitled struct {
	match func(terminal.Info) bool
	title string
}

func retitles() (func(*Server), <-chan retitled) {
	got := make(chan retitled, 1)
	return func(s *Server) {
		s.Retitle = func(match func(terminal.Info) bool, title string) { got <- retitled{match, title} }
	}, got
}

func next(t *testing.T, got <-chan retitled) retitled {
	t.Helper()
	select {
	case r := <-got:
		return r
	case <-time.After(5 * time.Second):
		t.Fatal("no retitle")
		return retitled{}
	}
}

func TestAWorktreeRenameRetitlesTheClaudesInItsTerminals(t *testing.T) {
	retitle, got := retitles()
	_, _, p := local(t, peer.OwnerOf(1), func(s *Server) { s.Names = names.Open(t.TempDir()) }, retitle)
	p.send(helloWithNames)
	p.send(`{"type":"worktree.rename","id":"r","path":"/w/a","title":"Fix login"}`)
	r := next(t, got)
	if r.title != "Fix login" {
		t.Fatal(r.title)
	}
	for _, c := range []struct {
		info terminal.Info
		want bool
	}{
		{terminal.Info{Cwd: "/w/a"}, true},
		{terminal.Info{Cwd: "/w/a/src"}, true},
		{terminal.Info{Cwd: "/w/ab"}, false},
		{terminal.Info{Cwd: "/w"}, false},
		{terminal.Info{Cwd: "/w/a", Local: "l1"}, false},
	} {
		if r.match(c.info) != c.want {
			t.Errorf("match(%+v) = %v, want %v", c.info, !c.want, c.want)
		}
	}
}

func TestALocalRenameRetitlesTheClaudesInItsTerminals(t *testing.T) {
	retitle, got := retitles()
	_, _, p := local(t, peer.OwnerOf(1), withLocals(t, terminal.NewManager()), retitle)
	p.send(helloWithLocals)
	p.send(`{"type":"local.create","id":"c","localId":"l1","project":"/p","name":"Local 2"}`)
	p.send(`{"type":"local.rename","id":"r","localId":"l1","title":"Review"}`)
	r := next(t, got)
	if r.title != "Review" || !r.match(terminal.Info{Cwd: "/p", Local: "l1"}) || r.match(terminal.Info{Cwd: "/p"}) || r.match(terminal.Info{Cwd: "/p", Local: "l2"}) {
		t.Fatalf("%+v", r)
	}
}
