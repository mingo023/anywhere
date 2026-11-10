package board

import (
	"errors"
	"os"
	"path/filepath"
	"slices"
	"testing"
	"time"

	"pocketd/internal/proto"
)

var at = time.UnixMilli(1_000_000)

func draft(title string) proto.Ticket {
	return proto.Ticket{Title: title, Prompt: "do " + title, Provider: "claude", Folder: "/repo"}
}

func saved(t *testing.T, s *Store, title string, start bool) proto.Ticket {
	t.Helper()
	x, err := s.Save(draft(title), start, at)
	if err != nil {
		t.Fatal(err)
	}
	return x
}

func got(t *testing.T, s *Store, id string) proto.Ticket {
	t.Helper()
	tickets, _ := s.Snapshot()
	i := slices.IndexFunc(tickets, func(x proto.Ticket) bool { return x.ID == id })
	if i < 0 {
		t.Fatalf("no ticket %s", id)
	}
	return tickets[i]
}

func titles(tickets []proto.Ticket) []string {
	var out []string
	for _, t := range tickets {
		out = append(out, t.Title)
	}
	return out
}

func TestANewTicketWaitsInToDoUntilStarted(t *testing.T) {
	s := Open(t.TempDir())
	if x := saved(t, s, "a", false); x.State != proto.TicketTodo || x.Order == 0 || x.ID == "" {
		t.Fatalf("got %+v", x)
	}
	if c := s.Claim(at); len(c) != 0 {
		t.Fatalf("claimed %+v", c)
	}
}

func TestSavingWithStartQueuesTheTicket(t *testing.T) {
	s := Open(t.TempDir())
	if x := saved(t, s, "a", true); x.State != proto.TicketQueued {
		t.Fatalf("got %+v", x)
	}
}

func TestClaimStartsTheOldestQueuedUpToTheLimit(t *testing.T) {
	s := Open(t.TempDir())
	s.SetLimit(2)
	for _, n := range []string{"a", "b", "c"} {
		saved(t, s, n, true)
	}
	if c := titles(s.Claim(at)); !slices.Equal(c, []string{"a", "b"}) {
		t.Fatalf("got %v", c)
	}
	if c := s.Claim(at); len(c) != 0 {
		t.Fatalf("over the limit: %v", titles(c))
	}
}

func TestAFinishedTicketFreesItsSlotForTheNextInLine(t *testing.T) {
	s := Open(t.TempDir())
	s.SetLimit(1)
	a := saved(t, s, "a", true)
	saved(t, s, "b", true)
	s.Claim(at)
	s.Update(a.ID, func(x *proto.Ticket) { x.Status = proto.TicketDone })
	if c := titles(s.Claim(at)); !slices.Equal(c, []string{"b"}) {
		t.Fatalf("got %v", c)
	}
}

func TestAClosedTicketStopsHoldingItsSlot(t *testing.T) {
	s := Open(t.TempDir())
	s.SetLimit(1)
	a := saved(t, s, "a", true)
	saved(t, s, "b", true)
	s.Claim(at)
	s.Update(a.ID, func(x *proto.Ticket) { x.Status = proto.TicketNeedsYou })
	if err := s.Close(a.ID, proto.ClosedByUser, at); err != nil {
		t.Fatal(err)
	}
	if c := titles(s.Claim(at)); !slices.Equal(c, []string{"b"}) {
		t.Fatalf("got %v", c)
	}
}

func TestMoveReordersAQueueAndSoWhatStartsNext(t *testing.T) {
	s := Open(t.TempDir())
	s.SetLimit(1)
	a := saved(t, s, "a", true)
	b := saved(t, s, "b", true)
	if err := s.Move(b.ID, proto.TicketQueued, a.ID, at); err != nil {
		t.Fatal(err)
	}
	if c := titles(s.Claim(at)); !slices.Equal(c, []string{"b"}) {
		t.Fatalf("got %v", c)
	}
}

func TestMoveBackToToDoTakesATicketOutOfTheQueue(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, "a", true)
	if err := s.Move(a.ID, proto.TicketTodo, "", at); err != nil {
		t.Fatal(err)
	}
	if got(t, s, a.ID).State != proto.TicketTodo || len(s.Claim(at)) != 0 {
		t.Fatalf("got %+v", got(t, s, a.ID))
	}
}

func TestALiveTicketCanNeitherBeMovedNorDeleted(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, "a", true)
	s.Claim(at)
	for _, err := range []error{s.Move(a.ID, proto.TicketTodo, "", at), s.Delete(a.ID)} {
		if !errors.Is(err, ErrBusy) {
			t.Fatalf("got %v", err)
		}
	}
}

func TestAnEndedTicketStartsAgainAtOnceInItsWorktreeEvenOverTheLimit(t *testing.T) {
	s := Open(t.TempDir())
	s.SetLimit(1)
	a := saved(t, s, "a", true)
	s.Claim(at)
	s.Update(a.ID, func(x *proto.Ticket) {
		x.Status, x.Why, x.AgentID, x.Worktree = proto.TicketStopped, "Session closed", "ag", "/wt"
	})
	b := saved(t, s, "b", true)
	s.Claim(at)
	if err := s.Start(a.ID, at); err != nil {
		t.Fatal(err)
	}
	x := got(t, s, a.ID)
	if x.State != proto.TicketLive || x.Status != proto.TicketStarting || x.Worktree != "/wt" || x.AgentID != "" || x.Why != "" {
		t.Fatalf("got %+v", x)
	}
	if c := s.Claim(at); len(c) != 1 || c[0].ID != a.ID {
		t.Fatalf("claimed %v", titles(c))
	}
	if c := s.Claim(at); len(c) != 0 {
		t.Fatalf("launched twice: %v", titles(c))
	}
	if got(t, s, b.ID).State != proto.TicketLive {
		t.Fatal("b should have started when a stopped")
	}
}

func TestAFailedTicketWhoseAgentLivesCannotStartAgain(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, "a", true)
	s.Claim(at)
	s.Update(a.ID, func(x *proto.Ticket) { x.Status, x.AgentID = proto.TicketFailed, "ag" })
	if err := s.Start(a.ID, at); !errors.Is(err, ErrBusy) {
		t.Fatalf("got %v", err)
	}
}

func TestAStartedTicketSaysSoWhenEdited(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, "a", true)
	s.Claim(at)
	edit := draft("b")
	edit.ID = a.ID
	if _, err := s.Save(edit, false, at); !errors.Is(err, ErrStarted) {
		t.Fatalf("got %v", err)
	}
}

func TestATicketChangesProjectOnlyBeforeItStarts(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, "a", false)
	edit := draft("a")
	edit.ID, edit.Folder = a.ID, "/other"
	if got, err := s.Save(edit, false, at); err != nil || got.Folder != "/other" {
		t.Fatalf("got %+v, %v", got, err)
	}
	b := saved(t, s, "b", true)
	s.Claim(at)
	late := draft("b")
	late.ID, late.Folder = b.ID, "/other"
	if _, err := s.Save(late, false, at); err != ErrStarted {
		t.Fatalf("got %v", err)
	}
}

func TestAStartedTicketStillTakesAPriorityAndLabels(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, "a", true)
	s.Claim(at)
	edit := draft("a")
	edit.ID, edit.Priority, edit.Labels = a.ID, 2, []string{"Bug"}
	got, err := s.Save(edit, false, at.Add(time.Hour))
	if err != nil || got.Priority != 2 || !slices.Equal(got.Labels, []string{"Bug"}) || got.ChangedAt != a.ChangedAt {
		t.Fatalf("got %+v, %v", got, err)
	}
}

func TestEveryTicketGetsANumberOfItsOwn(t *testing.T) {
	home := t.TempDir()
	s := Open(home)
	a, b := saved(t, s, "a", false), saved(t, s, "b", false)
	if a.Number != 1 || b.Number != 2 {
		t.Fatalf("got %d, %d", a.Number, b.Number)
	}
}

func TestADeletedTicketsNumberIsNotGivenAgain(t *testing.T) {
	home := t.TempDir()
	s := Open(home)
	a := saved(t, s, "a", false)
	if err := s.Delete(a.ID); err != nil {
		t.Fatal(err)
	}
	if b := saved(t, Open(home), "b", false); b.Number != a.Number+1 {
		t.Fatalf("got %d after %d", b.Number, a.Number)
	}
}

func TestTicketsFromBeforeNumbersGetOneOnOpen(t *testing.T) {
	home := t.TempDir()
	path := filepath.Join(home, "state", "board.json")
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatal(err)
	}
	raw := `{"version":1,"limit":4,"tickets":[{"id":"a","title":"a","prompt":"p","provider":"claude","folder":"/r","state":"todo","order":1,"createdAt":1,"changedAt":1},{"id":"b","title":"b","prompt":"p","provider":"claude","folder":"/r","state":"todo","order":2,"createdAt":2,"changedAt":2,"number":5}]}`
	if err := os.WriteFile(path, []byte(raw), 0o600); err != nil {
		t.Fatal(err)
	}
	tickets, _ := Open(home).Snapshot()
	if tickets[0].Number != 6 || tickets[1].Number != 5 {
		t.Fatalf("got %d, %d", tickets[0].Number, tickets[1].Number)
	}
}

func TestReopenIsRefusedOnceWhatClosedItStillHolds(t *testing.T) {
	s := Open(t.TempDir())
	for _, c := range []struct{ why, tree string }{{proto.ClosedMerged, ""}, {proto.ClosedDeleted, ""}, {proto.ClosedByUser, "/nowhere/at/all"}} {
		a := saved(t, s, "a", true)
		s.Claim(at)
		s.Update(a.ID, func(x *proto.Ticket) { x.Status, x.Worktree = proto.TicketIdle, c.tree })
		s.Close(a.ID, c.why, at)
		if err := s.Reopen(a.ID, at); !errors.Is(err, ErrBusy) {
			t.Fatalf("%s: got %v", c.why, err)
		}
	}
}

func TestReconcileClearsAStartAgainARestartInterrupted(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, "a", true)
	s.Claim(at)
	s.Update(a.ID, func(x *proto.Ticket) { x.Status = proto.TicketStopped })
	s.Start(a.ID, at)
	s.Reconcile(at)
	if x := got(t, s, a.ID); x.Status != proto.TicketFailed || x.Relaunch {
		t.Fatalf("got %+v", x)
	}
	if c := s.Claim(at); len(c) != 0 {
		t.Fatalf("claimed %v", titles(c))
	}
}

func TestAWorkingTicketCannotBeStartedAgain(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, "a", true)
	s.Claim(at)
	if err := s.Start(a.ID, at); !errors.Is(err, ErrBusy) {
		t.Fatalf("got %v", err)
	}
}

func TestReopenUndoesClose(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, "a", true)
	s.Claim(at)
	s.Update(a.ID, func(x *proto.Ticket) { x.Status = proto.TicketDone })
	s.Close(a.ID, proto.ClosedByUser, at)
	if err := s.Reopen(a.ID, at); err != nil {
		t.Fatal(err)
	}
	if x := got(t, s, a.ID); x.State != proto.TicketLive || x.Status != proto.TicketDone || x.ClosedWhy != "" || x.ClosedAt != 0 {
		t.Fatalf("got %+v", x)
	}
}

func TestReconcileFailsWhatARestartLeftStarting(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, "a", true)
	s.Claim(at)
	if err := s.Reconcile(at); err != nil {
		t.Fatal(err)
	}
	if x := got(t, s, a.ID); x.Status != proto.TicketFailed || x.Why != "Interrupted" {
		t.Fatalf("got %+v", x)
	}
}

func TestReconcileLeavesAStartingTicketThatHasItsAgent(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, "a", true)
	s.Claim(at)
	s.Update(a.ID, func(x *proto.Ticket) { x.AgentID = "ag" })
	s.Reconcile(at)
	if x := got(t, s, a.ID); x.Status != proto.TicketStarting {
		t.Fatalf("got %+v", x)
	}
}

func TestTheBoardSurvivesARestart(t *testing.T) {
	home := t.TempDir()
	s := Open(home)
	a := saved(t, s, "a", false)
	s.SetLimit(7)
	again := Open(home)
	tickets, limit := again.Snapshot()
	if limit != 7 || len(tickets) != 1 || tickets[0].ID != a.ID {
		t.Fatalf("got %d %+v", limit, tickets)
	}
}

func TestAnUnreadableFileIsKeptAsideAndTheBoardStartsEmpty(t *testing.T) {
	home := t.TempDir()
	path := filepath.Join(home, "state", "board.json")
	os.MkdirAll(filepath.Dir(path), 0o700)
	os.WriteFile(path, []byte("{"), 0o600)
	tickets, limit := Open(home).Snapshot()
	if len(tickets) != 0 || limit != proto.DefaultBoardLimit {
		t.Fatalf("got %d %+v", limit, tickets)
	}
	if _, err := os.Stat(path + ".bad"); err != nil {
		t.Fatal(err)
	}
}

func TestALimitOutsideTheRangeIsRefused(t *testing.T) {
	s := Open(t.TempDir())
	for _, n := range []int{0, 9} {
		if err := s.SetLimit(n); !errors.Is(err, ErrInvalid) {
			t.Fatalf("%d: %v", n, err)
		}
	}
}
