package board

import (
	"database/sql"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"testing"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/statedb/dbtest"
)

var at = time.UnixMilli(1_000_000)

// abs is the unix path p made absolute on this OS, which on Windows takes a drive.
func abs(p string) string { return filepath.Join(filepath.VolumeName(os.TempDir()), p) }

// newDB is a database whose registry lists the projects /repo and /other.
func newDB(t *testing.T) *sql.DB {
	t.Helper()
	db := dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db"))
	tx, err := db.Begin()
	if err != nil {
		t.Fatal(err)
	}
	if err := registry.Insert(tx, registry.State{Projects: []proto.RegistryProject{{ID: "p1", Path: abs("/repo")}, {ID: "p2", Path: abs("/other")}}}); err != nil {
		t.Fatal(err)
	}
	if err := tx.Commit(); err != nil {
		t.Fatal(err)
	}
	return db
}

func open(t *testing.T, db *sql.DB) *Store {
	t.Helper()
	reg, err := registry.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	s, err := Open(db, reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	return s
}

func openNew(t *testing.T) *Store { return open(t, newDB(t)) }

func draft(title string) proto.Ticket {
	return proto.Ticket{Title: title, Prompt: "do " + title, Provider: "claude", Folder: abs("/repo")}
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
	s := openNew(t)
	if x := saved(t, s, "a", false); x.State != proto.TicketTodo || x.Order == 0 || x.ID == "" {
		t.Fatalf("got %+v", x)
	}
	if c := s.Claim(at); len(c) != 0 {
		t.Fatalf("claimed %+v", c)
	}
}

func TestSavingWithStartQueuesTheTicket(t *testing.T) {
	s := openNew(t)
	if x := saved(t, s, "a", true); x.State != proto.TicketQueued {
		t.Fatalf("got %+v", x)
	}
}

func TestClaimStartsTheOldestQueuedUpToTheLimit(t *testing.T) {
	s := openNew(t)
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
	s := openNew(t)
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
	s := openNew(t)
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
	s := openNew(t)
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
	s := openNew(t)
	a := saved(t, s, "a", true)
	if err := s.Move(a.ID, proto.TicketTodo, "", at); err != nil {
		t.Fatal(err)
	}
	if got(t, s, a.ID).State != proto.TicketTodo || len(s.Claim(at)) != 0 {
		t.Fatalf("got %+v", got(t, s, a.ID))
	}
}

func TestALiveTicketCanNeitherBeMovedNorDeleted(t *testing.T) {
	s := openNew(t)
	a := saved(t, s, "a", true)
	s.Claim(at)
	for _, err := range []error{s.Move(a.ID, proto.TicketTodo, "", at), s.Delete(a.ID)} {
		if !errors.Is(err, ErrBusy) {
			t.Fatalf("got %v", err)
		}
	}
}

func TestAnEndedTicketStartsAgainAtOnceInItsWorktreeEvenOverTheLimit(t *testing.T) {
	s := openNew(t)
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
	s := openNew(t)
	a := saved(t, s, "a", true)
	s.Claim(at)
	s.Update(a.ID, func(x *proto.Ticket) { x.Status, x.AgentID = proto.TicketFailed, "ag" })
	if err := s.Start(a.ID, at); !errors.Is(err, ErrBusy) {
		t.Fatalf("got %v", err)
	}
}

func TestAStartedTicketSaysSoWhenEdited(t *testing.T) {
	s := openNew(t)
	a := saved(t, s, "a", true)
	s.Claim(at)
	edit := draft("b")
	edit.ID = a.ID
	if _, err := s.Save(edit, false, at); !errors.Is(err, ErrStarted) {
		t.Fatalf("got %v", err)
	}
}

func TestATicketChangesProjectOnlyBeforeItStarts(t *testing.T) {
	s := openNew(t)
	a := saved(t, s, "a", false)
	edit := draft("a")
	edit.ID, edit.Folder = a.ID, abs("/other")
	if got, err := s.Save(edit, false, at); err != nil || got.Folder != abs("/other") {
		t.Fatalf("got %+v, %v", got, err)
	}
	b := saved(t, s, "b", true)
	s.Claim(at)
	late := draft("b")
	late.ID, late.Folder = b.ID, abs("/other")
	if _, err := s.Save(late, false, at); err != ErrStarted {
		t.Fatalf("got %v", err)
	}
}

func TestAStartedTicketStillTakesAPriorityAndLabels(t *testing.T) {
	s := openNew(t)
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
	db := newDB(t)
	s := open(t, db)
	a, b := saved(t, s, "a", false), saved(t, s, "b", false)
	if a.Number != 1 || b.Number != 2 {
		t.Fatalf("got %d, %d", a.Number, b.Number)
	}
}

func TestADeletedTicketsNumberIsNotGivenAgain(t *testing.T) {
	db := newDB(t)
	s := open(t, db)
	a := saved(t, s, "a", false)
	if err := s.Delete(a.ID); err != nil {
		t.Fatal(err)
	}
	if b := saved(t, open(t, db), "b", false); b.Number != a.Number+1 {
		t.Fatalf("got %d after %d", b.Number, a.Number)
	}
}

func TestReopenIsRefusedOnceWhatClosedItStillHolds(t *testing.T) {
	s := openNew(t)
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
	s := openNew(t)
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
	s := openNew(t)
	a := saved(t, s, "a", true)
	s.Claim(at)
	if err := s.Start(a.ID, at); !errors.Is(err, ErrBusy) {
		t.Fatalf("got %v", err)
	}
}

func TestReopenUndoesClose(t *testing.T) {
	s := openNew(t)
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
	s := openNew(t)
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
	s := openNew(t)
	a := saved(t, s, "a", true)
	s.Claim(at)
	s.Update(a.ID, func(x *proto.Ticket) { x.AgentID = "ag" })
	s.Reconcile(at)
	if x := got(t, s, a.ID); x.Status != proto.TicketStarting {
		t.Fatalf("got %+v", x)
	}
}

func TestTheBoardSurvivesARestart(t *testing.T) {
	db := newDB(t)
	s := open(t, db)
	a := saved(t, s, "a", false)
	s.SetLimit(7)
	again := open(t, db)
	tickets, limit := again.Snapshot()
	if limit != 7 || len(tickets) != 1 || tickets[0].ID != a.ID {
		t.Fatalf("got %d %+v", limit, tickets)
	}
}

func TestALimitOutsideTheRangeIsRefused(t *testing.T) {
	s := openNew(t)
	for _, n := range []int{0, 9} {
		if err := s.SetLimit(n); !errors.Is(err, ErrInvalid) {
			t.Fatalf("%d: %v", n, err)
		}
	}
}

func TestEveryFieldOfEveryTicketSurvivesARestart(t *testing.T) {
	db := newDB(t)
	s := open(t, db)
	a := saved(t, s, "a", true)
	b, err := s.Save(proto.Ticket{Title: "b", Prompt: "do b", Provider: "claude", Folder: abs("/other"), Base: "main", Priority: 2, Labels: []string{"Bug", "Docs"}}, false, at)
	if err != nil {
		t.Fatal(err)
	}
	c := saved(t, s, "c", false)
	s.Claim(at)
	s.Update(a.ID, func(x *proto.Ticket) {
		x.AgentID, x.TerminalID, x.Worktree, x.Status, x.Why = "ag", "tm", abs("/repo/wt"), proto.TicketWorking, "because"
	})
	s.Close(a.ID, proto.ClosedByUser, at.Add(time.Minute))
	s.Delete(c.ID)
	before, limit := s.Snapshot()
	after, again := open(t, db).Snapshot()
	if !reflect.DeepEqual(before, after) || limit != again || len(after) != 2 || after[0].ID != a.ID || after[1].ID != b.ID {
		t.Fatalf("before %+v\nafter %+v", before, after)
	}
}

func TestATicketInAWorktreeKeepsItsFolderAcrossARestart(t *testing.T) {
	home := t.TempDir()
	repo := filepath.Join(home, "repo")
	tree := filepath.Join(home, "repo-wt")
	for _, dir := range []string{repo, tree} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.MkdirAll(filepath.Join(repo, ".git", "worktrees", "wt"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(repo, ".git", "worktrees", "wt", "commondir"), []byte("../.."), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tree, ".git"), []byte("gitdir: "+filepath.Join(repo, ".git", "worktrees", "wt")), 0o644); err != nil {
		t.Fatal(err)
	}
	db := dbtest.Open(t, filepath.Join(home, "pocketd.db"))
	reg, err := registry.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	if err := reg.AddProject(repo, nil); err != nil {
		t.Fatal(err)
	}
	s, err := Open(db, reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	a, b := draft("a"), draft("b")
	a.Folder, b.Folder = tree, repo
	for _, d := range []proto.Ticket{a, b} {
		if _, err := s.Save(d, false, at); err != nil {
			t.Fatal(err)
		}
	}
	reopened, err := Open(db, reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	tickets, _ := reopened.Snapshot()
	if len(tickets) != 2 || tickets[0].Folder != tree || tickets[1].Folder != repo {
		t.Fatalf("got %+v", tickets)
	}
	var trees []sql.NullString
	rows, _ := db.Query(`SELECT tree FROM tickets ORDER BY position`)
	defer rows.Close()
	for rows.Next() {
		var tr sql.NullString
		rows.Scan(&tr)
		trees = append(trees, tr)
	}
	if len(trees) != 2 || !trees[0].Valid || trees[0].String != tree || trees[1].Valid {
		t.Fatalf("trees %v", trees)
	}
}

func TestATicketInNoProjectIsRefusedAndSavesNothing(t *testing.T) {
	db := newDB(t)
	s := open(t, db)
	d := draft("a")
	d.Folder = "/nowhere"
	if _, err := s.Save(d, false, at); !errors.Is(err, registry.ErrUnknownProject) {
		t.Fatalf("got %v", err)
	}
	ok := saved(t, s, "b", false)
	ok.Folder = "/nowhere"
	if _, err := s.Save(ok, false, at); !errors.Is(err, registry.ErrUnknownProject) {
		t.Fatalf("moving a ticket: %v", err)
	}
	if tickets, _ := open(t, db).Snapshot(); len(tickets) != 1 || tickets[0].Folder != abs("/repo") {
		t.Fatalf("got %+v", tickets)
	}
}

func TestRemovingAProjectDropsItsTicketsAndWakesWhoWatchesTheBoard(t *testing.T) {
	db := newDB(t)
	reg, err := registry.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	s, err := Open(db, reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	reg.OnRemove(func(string) { s.Reload() })
	saved(t, s, "a", false)
	other := draft("b")
	other.Folder = abs("/other")
	if _, err := s.Save(other, false, at); err != nil {
		t.Fatal(err)
	}
	changed := s.Changed()
	if err := reg.RemoveProject(abs("/repo")); err != nil {
		t.Fatal(err)
	}
	select {
	case <-changed:
	default:
		t.Fatal("the board wasn't told")
	}
	if tickets, _ := s.Snapshot(); !slices.Equal(titles(tickets), []string{"b"}) {
		t.Fatalf("got %v", titles(tickets))
	}
	var n int
	db.QueryRow(`SELECT COUNT(*) FROM tickets`).Scan(&n)
	if n != 1 {
		t.Fatalf("%d rows", n)
	}
}

func TestASaveWritesOnlyTheTicketsThatChanged(t *testing.T) {
	db := newDB(t)
	s := open(t, db)
	a, b := saved(t, s, "a", false), saved(t, s, "b", false)
	if _, err := db.Exec(`UPDATE tickets SET prompt = 'untouched' WHERE id IN (?, ?)`, a.ID, b.ID); err != nil {
		t.Fatal(err)
	}
	s.Update(a.ID, func(x *proto.Ticket) { x.Status = proto.TicketWorking })
	var prompts []string
	rows, _ := db.Query(`SELECT prompt FROM tickets ORDER BY position`)
	defer rows.Close()
	for rows.Next() {
		var p string
		rows.Scan(&p)
		prompts = append(prompts, p)
	}
	if !slices.Equal(prompts, []string{"do a", "untouched"}) {
		t.Fatalf("got %v", prompts)
	}
}

func TestSavingTicketsWhileTheRegistryIsWrittenDoesNotDeadlock(t *testing.T) {
	db := newDB(t)
	reg, err := registry.Open(db)
	if err != nil {
		t.Fatal(err)
	}
	s, err := Open(db, reg.Load)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan struct{})
	go func() {
		defer close(done)
		for i := range 200 {
			reg.Track(abs("/repo/wt"), i%2 == 0)
		}
	}()
	for range 200 {
		saved(t, s, "a", false)
	}
	select {
	case <-done:
	case <-time.After(10 * time.Second):
		t.Fatal("deadlocked")
	}
}
