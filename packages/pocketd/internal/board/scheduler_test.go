package board

import (
	"context"
	"sync"
	"testing"
	"time"

	"pocketd/internal/launch"
	"pocketd/internal/proto"
)

func live(status string) proto.Ticket {
	return proto.Ticket{ID: "t", State: proto.TicketLive, Status: status, AgentID: "a"}
}

func TestStepFollowsTheAgent(t *testing.T) {
	cases := []struct {
		name    string
		from    string
		seen    Seen
		present bool
		want    string
	}{
		{"working", proto.TicketStarting, Seen{Status: "working"}, true, proto.TicketWorking},
		{"asks", proto.TicketWorking, Seen{Status: "needsYou"}, true, proto.TicketNeedsYou},
		{"ends a turn", proto.TicketWorking, Seen{Status: "done"}, true, proto.TicketDone},
		{"is opened", proto.TicketDone, Seen{Status: "idle"}, true, proto.TicketIdle},
		{"gets a reply", proto.TicketIdle, Seen{Status: "working"}, true, proto.TicketWorking},
		{"fails", proto.TicketWorking, Seen{Status: "done", Failed: true}, true, proto.TicketFailed},
		{"stays failed when opened", proto.TicketFailed, Seen{Status: "idle", Failed: true}, true, proto.TicketFailed},
		{"is gone", proto.TicketWorking, Seen{}, false, proto.TicketStopped},
		{"is closed", proto.TicketIdle, Seen{Status: "closed"}, true, proto.TicketStopped},
		{"comes back after being restored", proto.TicketStopped, Seen{Status: "working"}, true, proto.TicketWorking},
		{"idle before its first turn", proto.TicketStarting, Seen{Status: "idle"}, true, proto.TicketStarting},
		{"idle after a turn too short to see", proto.TicketStarting, Seen{Status: "idle", Answered: true}, true, proto.TicketIdle},
	}
	for _, c := range cases {
		if next, _ := Step(live(c.from), c.seen, c.present); next.Status != c.want {
			t.Errorf("%s: got %s, want %s", c.name, next.Status, c.want)
		}
	}
}

func TestStepSaysWhenNothingChanged(t *testing.T) {
	if _, changed := Step(live(proto.TicketWorking), Seen{Status: "working"}, true); changed {
		t.Fatal("changed")
	}
}

func TestANewTicketGetsAFreshWorktreeOffItsBase(t *testing.T) {
	x := draft("a")
	x.Base = "dev"
	spec := Spec(x, "0123456789")
	if n := spec.Checkout.New; n == nil || n.Base != "dev" || !n.AutoName || n.Name != "ticket-01234567" || spec.Prompt != x.Prompt {
		t.Fatalf("got %+v", spec)
	}
}

func TestATicketOfAnAgentFollowedOnlyInItsChatStartsAsOne(t *testing.T) {
	x := draft("a")
	for provider, chat := range map[string]bool{"claude": false, "codex": false, "pi": true, "hermes": true} {
		x.Provider = provider
		if spec := Spec(x, "0123456789"); spec.Chat != chat {
			t.Errorf("%s: chat %v", provider, spec.Chat)
		}
	}
}

func TestAStartAgainReusesTheWorktree(t *testing.T) {
	x := draft("a")
	x.Worktree = "/wt"
	if spec := Spec(x, "r"); spec.Checkout.Worktree != "/wt" || spec.Checkout.New != nil {
		t.Fatalf("got %+v", spec)
	}
}

type world struct {
	mu     sync.Mutex
	result launch.Result
	seen   map[string]Seen
	starts []proto.Ticket
	// cwd is the Worktree told about while launching.
	cwd string
}

func (w *world) start(_ string, t proto.Ticket, created func(string)) launch.Result {
	if w.cwd != "" {
		created(w.cwd)
	}
	w.mu.Lock()
	defer w.mu.Unlock()
	w.starts = append(w.starts, t)
	return w.result
}

func (w *world) watch(id string, _ bool) (Seen, bool) {
	w.mu.Lock()
	defer w.mu.Unlock()
	s, ok := w.seen[id]
	return s, ok
}

func running(t *testing.T, s *Store, w *world, merged func(context.Context, string) bool) {
	t.Helper()
	ctx, cancel := context.WithCancel(context.Background())
	sch := &Scheduler{Store: s, Now: time.Now, Poll: 5 * time.Millisecond, Start: w.start, Watch: w.watch, Merged: merged}
	done := make(chan struct{})
	go func() { sch.Run(ctx); close(done) }()
	t.Cleanup(func() { cancel(); <-done })
}

func waitFor(t *testing.T, what string, ok func() bool) {
	t.Helper()
	for end := time.Now().Add(3 * time.Second); time.Now().Before(end); time.Sleep(2 * time.Millisecond) {
		if ok() {
			return
		}
	}
	t.Fatalf("waited for %s", what)
}

func TestALaunchedTicketRecordsItsSessionAndWorktree(t *testing.T) {
	s := Open(t.TempDir())
	dir := t.TempDir()
	w := &world{result: launch.Result{AgentID: "a", TerminalID: "tm", Cwd: dir}, seen: map[string]Seen{"a": {Status: "working"}}}
	x := saved(t, s, "a", true)
	running(t, s, w, nil)
	waitFor(t, "working", func() bool { return got(t, s, x.ID).Status == proto.TicketWorking })
	if g := got(t, s, x.ID); g.AgentID != "a" || g.TerminalID != "tm" || g.Worktree != dir {
		t.Fatalf("got %+v", g)
	}
}

func TestALaunchThatFailsSaysWhyAndFreesTheSlot(t *testing.T) {
	s := Open(t.TempDir())
	s.SetLimit(1)
	w := &world{result: launch.Result{Err: &launch.Failure{Message: "no   such\nbranch"}}}
	a := saved(t, s, "a", true)
	b := saved(t, s, "b", true)
	running(t, s, w, nil)
	waitFor(t, "both failed", func() bool {
		return got(t, s, a.ID).Status == proto.TicketFailed && got(t, s, b.ID).Status == proto.TicketFailed
	})
	if g := got(t, s, a.ID); g.Why != "no such branch" {
		t.Fatalf("got %+v", g)
	}
}

func TestALaunchThatDiesKeepsTheWorktreeItMade(t *testing.T) {
	s := Open(t.TempDir())
	dir := t.TempDir()
	w := &world{cwd: dir, result: launch.Result{Err: &launch.Failure{Message: "boom"}}}
	x := saved(t, s, "a", true)
	running(t, s, w, nil)
	waitFor(t, "failed", func() bool { return got(t, s, x.ID).Status == proto.TicketFailed })
	if g := got(t, s, x.ID); g.Worktree != dir {
		t.Fatalf("got %+v", g)
	}
}

func TestAMissingAgentRightAfterARestartIsGivenTimeToComeBack(t *testing.T) {
	s := Open(t.TempDir())
	x := saved(t, s, "a", true)
	s.Claim(at)
	s.Update(x.ID, func(t *proto.Ticket) { t.AgentID, t.Status = "gone", proto.TicketWorking })
	running(t, s, &world{seen: map[string]Seen{}}, nil)
	time.Sleep(40 * time.Millisecond)
	if g := got(t, s, x.ID); g.Status != proto.TicketWorking {
		t.Fatalf("got %+v", g)
	}
}

func TestAMergedPullRequestClosesTheTicket(t *testing.T) {
	s := Open(t.TempDir())
	dir := t.TempDir()
	x := saved(t, s, "a", true)
	s.Claim(at)
	s.Update(x.ID, func(t *proto.Ticket) { t.AgentID, t.Status, t.Worktree = "a", proto.TicketIdle, dir })
	w := &world{seen: map[string]Seen{"a": {Status: "idle"}}}
	running(t, s, w, func(_ context.Context, tree string) bool { return tree == dir })
	waitFor(t, "merged", func() bool { return got(t, s, x.ID).ClosedWhy == proto.ClosedMerged })
	if g := got(t, s, x.ID); g.State != proto.TicketClosed {
		t.Fatalf("got %+v", g)
	}
}

func TestADeletedWorktreeClosesAnEndedTicket(t *testing.T) {
	s := Open(t.TempDir())
	x := saved(t, s, "a", true)
	s.Claim(at)
	s.Update(x.ID, func(t *proto.Ticket) { t.AgentID, t.Status, t.Worktree = "a", proto.TicketStopped, "/nowhere/at/all" })
	(&Scheduler{Store: s, Now: time.Now}).sweep(at)
	if g := got(t, s, x.ID); g.State != proto.TicketClosed || g.ClosedWhy != proto.ClosedDeleted || g.Worktree != "" {
		t.Fatalf("got %+v", g)
	}
}

func TestDeletingTheWorktreeOfAMergedTicketKeepsItMerged(t *testing.T) {
	s := Open(t.TempDir())
	x := saved(t, s, "a", true)
	s.Claim(at)
	s.Update(x.ID, func(t *proto.Ticket) {
		t.State, t.ClosedWhy, t.Status, t.Worktree = proto.TicketClosed, proto.ClosedMerged, proto.TicketIdle, "/nowhere/at/all"
	})
	(&Scheduler{Store: s, Now: time.Now}).sweep(at)
	if g := got(t, s, x.ID); g.ClosedWhy != proto.ClosedMerged || g.Worktree != "" {
		t.Fatalf("got %+v", g)
	}
}

func TestAWorktreeThatStillExistsIsLeftAlone(t *testing.T) {
	s := Open(t.TempDir())
	dir := t.TempDir()
	x := saved(t, s, "a", true)
	s.Claim(at)
	s.Update(x.ID, func(t *proto.Ticket) { t.Status, t.Worktree = proto.TicketIdle, dir })
	(&Scheduler{Store: s, Now: time.Now}).sweep(at)
	if g := got(t, s, x.ID); g.State != proto.TicketLive || g.Worktree != dir {
		t.Fatalf("got %+v", g)
	}
}
