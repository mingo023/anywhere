package mailbox

import (
	"reflect"
	"testing"
	"time"
)

func open(t *testing.T, home string) *Store {
	t.Helper()
	s, err := Open(home)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { s.Close() })
	return s
}

func send(t *testing.T, s *Store, agent, text string) int64 {
	t.Helper()
	id, err := s.Send(agent, text)
	if err != nil {
		t.Fatal(err)
	}
	return id
}

func shown(t *testing.T, s *Store, id int64, ref string) {
	t.Helper()
	if err := s.Shown(id, ref); err != nil {
		t.Fatal(err)
	}
}

func settle(t *testing.T, s *Store, agent, ref string, ok bool) {
	t.Helper()
	if err := s.Settle(agent, ref, ok); err != nil {
		t.Fatal(err)
	}
}

// queued is agent's queued mail without Created, which TestMailIsStampedWhenSent checks.
func queued(t *testing.T, s *Store, agent string) []Mail {
	t.Helper()
	q, err := s.Queued(agent)
	if err != nil {
		t.Fatal(err)
	}
	for i := range q {
		q[i].Created = time.Time{}
	}
	return q
}

// Proves: Queued is per agent and oldest first.
func TestMailWaitsInItsAgentsQueueOldestFirst(t *testing.T) {
	s := open(t, t.TempDir())
	a := send(t, s, "lead", "first")
	send(t, s, "worker", "elsewhere")
	b := send(t, s, "lead", "second")

	want := []Mail{{ID: a, Agent: "lead", Text: "first", State: "queued"}, {ID: b, Agent: "lead", Text: "second", State: "queued"}}
	if got := queued(t, s, "lead"); !reflect.DeepEqual(got, want) {
		t.Fatalf("got  %+v\nwant %+v", got, want)
	}
}

// Proves: Created is the send time.
func TestMailIsStampedWhenSent(t *testing.T) {
	s := open(t, t.TempDir())
	before := time.Now().Truncate(time.Millisecond)
	send(t, s, "lead", "hi")
	after := time.Now()

	q, err := s.Queued("lead")
	if err != nil {
		t.Fatal(err)
	}
	if len(q) != 1 || q[0].Created.Before(before) || q[0].Created.After(after) {
		t.Fatalf("got %+v, want one stamped between %v and %v", q, before, after)
	}
}

// Proves: shown mail is no longer queued.
func TestShownMailLeavesTheQueue(t *testing.T) {
	s := open(t, t.TempDir())
	a := send(t, s, "lead", "first")
	b := send(t, s, "lead", "second")
	shown(t, s, a, "r1")

	want := []Mail{{ID: b, Agent: "lead", Text: "second", State: "queued"}}
	if got := queued(t, s, "lead"); !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}

// Proves: settled ok is acked, and a restart doesn't bring it back.
func TestMailSettledOkIsAckedForGood(t *testing.T) {
	home := t.TempDir()
	s, err := Open(home)
	if err != nil {
		t.Fatal(err)
	}
	id := send(t, s, "lead", "hi")
	shown(t, s, id, "r1")
	settle(t, s, "lead", "r1", true)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}

	if got := queued(t, open(t, home), "lead"); got != nil {
		t.Fatalf("got %+v", got)
	}
}

// Proves: settled not ok is queued again, in its old place, with no ref.
func TestMailSettledNotOkGoesBackToTheQueue(t *testing.T) {
	s := open(t, t.TempDir())
	a := send(t, s, "lead", "first")
	b := send(t, s, "lead", "second")
	shown(t, s, a, "r1")
	settle(t, s, "lead", "r1", false)

	want := []Mail{{ID: a, Agent: "lead", Text: "first", State: "queued"}, {ID: b, Agent: "lead", Text: "second", State: "queued"}}
	if got := queued(t, s, "lead"); !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}

// Proves: a settle touches only its agent's mail shown under its ref.
func TestSettlingTouchesOnlyTheMailItsRefShowed(t *testing.T) {
	s := open(t, t.TempDir())
	a := send(t, s, "lead", "a")
	b := send(t, s, "lead", "b")
	c := send(t, s, "worker", "c")
	shown(t, s, a, "r1")
	shown(t, s, b, "r2")
	shown(t, s, c, "r1")

	settle(t, s, "lead", "r1", false)

	if got, want := queued(t, s, "lead"), []Mail{{ID: a, Agent: "lead", Text: "a", State: "queued"}}; !reflect.DeepEqual(got, want) {
		t.Fatalf("lead: got %+v", got)
	}
	if got := queued(t, s, "worker"); got != nil {
		t.Fatalf("worker: got %+v", got)
	}
}

// Proves: ref "" settles every shown mail of the agent, as a session exit does.
func TestSettlingWithNoRefSettlesAllTheAgentsShownMail(t *testing.T) {
	s := open(t, t.TempDir())
	a := send(t, s, "lead", "a")
	b := send(t, s, "lead", "b")
	shown(t, s, a, "r1")
	shown(t, s, b, "r2")

	settle(t, s, "lead", "", false)

	want := []Mail{{ID: a, Agent: "lead", Text: "a", State: "queued"}, {ID: b, Agent: "lead", Text: "b", State: "queued"}}
	if got := queued(t, s, "lead"); !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}

// Proves: Open puts mail shown before back in the queue.
func TestReopeningPutsShownMailBackInTheQueue(t *testing.T) {
	home := t.TempDir()
	s, err := Open(home)
	if err != nil {
		t.Fatal(err)
	}
	id := send(t, s, "lead", "hi")
	shown(t, s, id, "r1")
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}

	want := []Mail{{ID: id, Agent: "lead", Text: "hi", State: "queued"}}
	if got := queued(t, open(t, home), "lead"); !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}
