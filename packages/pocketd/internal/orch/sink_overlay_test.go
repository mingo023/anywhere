package orch

import (
	"slices"
	"testing"

	"pocketd/internal/proto"
)

// held takes the store's write lock, so the sink's writes wait until release while reads go on.
func held(t *testing.T, s *Store) (release func()) {
	t.Helper()
	s.DB().SetMaxOpenConns(2)
	tx, err := s.DB().Begin()
	if err != nil {
		t.Fatal(err)
	}
	if _, err := tx.Exec(`DELETE FROM held`); err != nil {
		t.Fatal(err)
	}
	return func() { tx.Rollback() }
}

// openAfter is the sink's open sessions once queue has run with its writes held back.
func openAfter(t *testing.T, s *Store, k *Sink, queue func()) []Session {
	t.Helper()
	k.Flush()
	release := held(t, s)
	defer release()
	queue()
	open, err := k.OpenSessions()
	if err != nil {
		t.Fatal(err)
	}
	return open
}

func ids(open []Session) []string {
	var out []string
	for _, x := range open {
		out = append(out, x.ID)
	}
	return out
}

func TestASessionWhoseCloseIsQueuedIsNotListedAsOpen(t *testing.T) {
	s := openTest(t)
	k := NewSink(s)
	defer k.Close()
	k.Summary(proto.AgentSummary{ID: "a", Status: "idle", CreatedAt: 1})
	k.Summary(proto.AgentSummary{ID: "b", Status: "idle", CreatedAt: 2})
	open := openAfter(t, s, k, func() { k.Summary(proto.AgentSummary{ID: "a", Status: "closed", CreatedAt: 1}) })
	if got := ids(open); !slices.Equal(got, []string{"b"}) {
		t.Fatalf("open %q", got)
	}
}

func TestAQueuedStatusWinsOverTheStoredOne(t *testing.T) {
	s := openTest(t)
	k := NewSink(s)
	defer k.Close()
	k.Summary(proto.AgentSummary{ID: "a", Status: "idle", Title: "Old", CreatedAt: 1})
	open := openAfter(t, s, k, func() {
		k.Summary(proto.AgentSummary{ID: "a", Status: "working", Title: "New", Pinned: true, CreatedAt: 1})
	})
	if len(open) != 1 || open[0].Status != "working" || open[0].Title != "New" || !open[0].Pinned {
		t.Fatalf("%+v", open)
	}
}

func TestASessionTheStoreHasNotHeardOfYetIsListedFromItsQueuedSummary(t *testing.T) {
	s := openTest(t)
	k := NewSink(s)
	defer k.Close()
	k.Summary(proto.AgentSummary{ID: "a", Status: "idle", CreatedAt: 1})
	open := openAfter(t, s, k, func() {
		k.Summary(proto.AgentSummary{ID: "z", Status: "working", CreatedAt: 5, Provider: "claude", Parent: "a", Relationship: "fork"})
		k.Summary(proto.AgentSummary{ID: "gone", Status: "closed", CreatedAt: 3})
		k.Summary(proto.AgentSummary{ID: "host/x", Status: "idle", CreatedAt: 4})
	})
	if got := ids(open); !slices.Equal(got, []string{"a", "z"}) || open[1].Agent != "claude" || open[1].ParentID != "a" {
		t.Fatalf("open %q %+v", got, open)
	}
}

func TestASummaryStaysOverlaidUntilItsTransactionCommits(t *testing.T) {
	s := openTest(t)
	k := NewSink(s)
	defer k.Close()
	release := held(t, s)
	k.Summary(proto.AgentSummary{ID: "a", Status: "working", CreatedAt: 1})
	if len(k.unwritten()) != 1 {
		t.Fatal("nothing pending")
	}
	release()
	k.Flush()
	if len(k.unwritten()) != 0 {
		t.Fatalf("pending %v after the commit", k.unwritten())
	}
	got, _ := s.Session("a")
	if got == nil || got.Status != "working" {
		t.Fatalf("stored %+v", got)
	}
}

func TestACommitDoesNotForgetANewerSummaryQueuedMeanwhile(t *testing.T) {
	s := openTest(t)
	k := NewSink(s)
	defer k.Close()
	release := held(t, s)
	defer release()
	first := proto.AgentSummary{ID: "a", Status: "idle"}
	k.Summary(first)
	k.Summary(proto.AgentSummary{ID: "a", Status: "working"})
	k.written([]sinkOp{{summary: &first, session: "a", n: 1}})
	if p, ok := k.unwritten()["a"]; !ok || p.sum.Status != "working" {
		t.Fatalf("pending %+v", k.unwritten())
	}
}

func TestARecapIsKeptForASessionNoLongerRunning(t *testing.T) {
	s := openTest(t)
	k := NewSink(s)
	want := proto.Recap{Asked: "fix it", Read: 2, Edited: 1, Said: "Fixed."}
	k.Summary(proto.AgentSummary{ID: "a", Status: "idle", CreatedAt: 1, Recap: want})
	k.Summary(proto.AgentSummary{ID: "a", Status: "working", CreatedAt: 1})
	k.Close()
	open, err := s.OpenSessions()
	if err != nil || len(open) != 1 || open[0].Recap != want {
		t.Fatalf("%+v %v", open, err)
	}
}

func TestAFailedSessionKeepsItsErrorWrittenOrQueued(t *testing.T) {
	s := openTest(t)
	k := NewSink(s)
	defer k.Close()
	k.Summary(proto.AgentSummary{ID: "a", Status: "done", Failed: true, Error: "exit 1", CreatedAt: 1})
	open := openAfter(t, s, k, func() {
		k.Summary(proto.AgentSummary{ID: "b", Status: "done", Failed: true, Error: "API Error: 529", CreatedAt: 2})
	})
	if len(open) != 2 || open[0].Error != "exit 1" || open[1].Error != "API Error: 529" {
		t.Fatalf("open %+v", open)
	}
}
