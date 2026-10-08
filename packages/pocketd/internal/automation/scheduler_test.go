package automation

import (
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"pocketd/internal/launch"
	"pocketd/internal/proto"
)

func eventually(t *testing.T, what string, ok func() bool) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		if ok() {
			return
		}
		time.Sleep(5 * time.Millisecond)
	}
	t.Fatalf("timed out waiting for %s", what)
}

func runOf(t *testing.T, s *Store, id string) proto.Run {
	t.Helper()
	r, ok := s.Run(id)
	if !ok {
		t.Fatalf("no run %s", id)
	}
	return r
}

func scheduler(s *Store, now time.Time, start Starter, watch Watcher) *Scheduler {
	return &Scheduler{Store: s, Now: func() time.Time { return now }, Poll: 5 * time.Millisecond, Start: start, Watch: watch}
}

func refuse(message string) Starter {
	return func(string, proto.Automation) launch.Result {
		return launch.Result{Err: &launch.Failure{Code: "spawn_failed", Message: message}}
	}
}

func TestADueAutomationStartsExactlyOnceAcrossTwoSchedulers(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, at(9, 8, 0))
	var starts atomic.Int32
	start := func(string, proto.Automation) launch.Result {
		starts.Add(1)
		return launch.Result{Err: &launch.Failure{Message: "stop here"}}
	}
	now := at(9, 9, 0)
	var wg sync.WaitGroup
	for range 2 {
		wg.Go(scheduler(s, now, start, nil).check)
	}
	wg.Wait()
	eventually(t, "the run to end", func() bool {
		_, runs := s.Snapshot()
		return len(runs) == 1 && finished(runs[0].Status)
	})
	time.Sleep(50 * time.Millisecond)
	if got := starts.Load(); got != 1 {
		t.Fatalf("started %d times", got)
	}
	if got, _ := s.Automation(a.ID); got.NextRunAt <= now.UnixMilli() {
		t.Fatalf("next run %v", time.UnixMilli(got.NextRunAt))
	}
}

func TestAnAutomationThatIsNotDueYetDoesNotStart(t *testing.T) {
	s := Open(t.TempDir())
	saved(t, s, at(9, 8, 0))
	scheduler(s, at(9, 8, 59), refuse("no"), nil).check()
	if _, runs := s.Snapshot(); len(runs) != 0 {
		t.Fatalf("%+v", runs)
	}
}

func TestAFailedLaunchRecordsWhy(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, at(9, 8, 0))
	sched := scheduler(s, at(9, 8, 30), refuse("claude isn't installed on your Mac"), nil)
	run, err := sched.RunNow(a.ID)
	if err != nil {
		t.Fatal(err)
	}
	eventually(t, "the run to fail", func() bool { return runOf(t, s, run.ID).Status == "failed" })
	got := runOf(t, s, run.ID)
	if got.Why != "claude isn't installed on your Mac" || got.FinishedAt != at(9, 8, 30).UnixMilli() || got.AgentID != "" {
		t.Fatalf("%+v", got)
	}
}

func TestALaunchFailureMessageIsCutToEightyRunes(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, at(9, 8, 0))
	long := ""
	for range 100 {
		long += "é"
	}
	run, _ := scheduler(s, at(9, 8, 30), refuse(long), nil).RunNow(a.ID)
	eventually(t, "the run to fail", func() bool { return runOf(t, s, run.ID).Status == "failed" })
	if n := len([]rune(runOf(t, s, run.ID).Why)); n != 80 {
		t.Fatalf("%d runes", n)
	}
}

func TestARunLaunchesThroughTheSpecAndFollowsItsAgentToTheEnd(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, at(9, 8, 0))
	var specs []proto.LaunchSpec
	var mu sync.Mutex
	start := func(id string, a proto.Automation) launch.Result {
		mu.Lock()
		specs = append(specs, Spec(a))
		mu.Unlock()
		return launch.Result{AgentID: "ag1", TerminalID: "t1"}
	}
	var phase atomic.Value
	phase.Store(Seen{Status: "idle"})
	watch := func(id string) (Seen, bool) { return phase.Load().(Seen), id == "ag1" }
	run, _ := scheduler(s, at(9, 8, 30), start, watch).RunNow(a.ID)
	status := func() string { return runOf(t, s, run.ID).Status }
	eventually(t, "running", func() bool { return status() == "running" })
	if got := runOf(t, s, run.ID); got.AgentID != "ag1" || got.TerminalID != "t1" {
		t.Fatalf("%+v", got)
	}
	time.Sleep(30 * time.Millisecond)
	if status() != "running" {
		t.Fatalf("an idle agent that never worked ended the run as %q", status())
	}
	phase.Store(Seen{Status: "needsYou"})
	eventually(t, "waiting", func() bool { return status() == "waiting" })
	phase.Store(Seen{Status: "working"})
	eventually(t, "running again", func() bool { return status() == "running" })
	phase.Store(Seen{Status: "done", Summary: "Reviewed 3 PRs."})
	eventually(t, "succeeded", func() bool { return status() == "succeeded" })
	got := runOf(t, s, run.ID)
	if got.Summary != "Reviewed 3 PRs." || got.FinishedAt != at(9, 8, 30).UnixMilli() {
		t.Fatalf("%+v", got)
	}
	mu.Lock()
	defer mu.Unlock()
	want := proto.LaunchSpec{Project: "/f", Checkout: proto.Checkout{Worktree: "/f"}, Provider: "claude", Access: "settings", Prompt: "Review new PRs"}
	if len(specs) != 1 || specs[0] != want {
		t.Fatalf("%+v", specs)
	}
}

func TestARunWhoseShortTurnWasNeverSeenWorkingStillSucceeds(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, at(9, 8, 0))
	start := func(string, proto.Automation) launch.Result { return launch.Result{AgentID: "ag1", TerminalID: "t1"} }
	var polls atomic.Int32
	watch := func(string) (Seen, bool) {
		if polls.Add(1) < 3 {
			return Seen{Status: "idle"}, true
		}
		return Seen{Status: "idle", Summary: "Hi.", Answered: true}, true
	}
	run, _ := scheduler(s, at(9, 8, 30), start, watch).RunNow(a.ID)
	eventually(t, "succeeded", func() bool { return runOf(t, s, run.ID).Status == "succeeded" })
	if got := runOf(t, s, run.ID).Summary; got != "Hi." {
		t.Fatalf("summary %q", got)
	}
}

func TestARunWaitsForTheSummaryOfATurnThatJustEnded(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, at(9, 8, 0))
	start := func(string, proto.Automation) launch.Result { return launch.Result{AgentID: "ag1", TerminalID: "t1"} }
	var polls atomic.Int32
	watch := func(string) (Seen, bool) {
		if polls.Add(1) < 3 {
			return Seen{Status: "done"}, true
		}
		return Seen{Status: "done", Summary: "Reviewed 3 PRs."}, true
	}
	run, _ := scheduler(s, at(9, 8, 30), start, watch).RunNow(a.ID)
	eventually(t, "succeeded", func() bool { return runOf(t, s, run.ID).Status == "succeeded" })
	if got := runOf(t, s, run.ID).Summary; got != "Reviewed 3 PRs." {
		t.Fatalf("summary %q", got)
	}
}

func TestARunWhoseSessionWasClosedIsCancelled(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, at(9, 8, 0))
	var gone atomic.Bool
	start := func(string, proto.Automation) launch.Result { return launch.Result{AgentID: "ag1", TerminalID: "t1"} }
	watch := func(string) (Seen, bool) { return Seen{Status: "working"}, !gone.Load() }
	run, _ := scheduler(s, at(9, 8, 30), start, watch).RunNow(a.ID)
	eventually(t, "running", func() bool { return runOf(t, s, run.ID).Status == "running" })
	gone.Store(true)
	eventually(t, "cancelled", func() bool { return runOf(t, s, run.ID).Status == "cancelled" })
	if got := runOf(t, s, run.ID); got.Why != "Session closed" || got.FinishedAt == 0 {
		t.Fatalf("%+v", got)
	}
}

func TestResumeFollowsTheRunsARestartLeftGoing(t *testing.T) {
	home := t.TempDir()
	s := Open(home)
	a := saved(t, s, at(9, 8, 0))
	waiting, _ := s.Begin(a.ID, at(9, 8, 1))
	s.Update(waiting.ID, func(r *proto.Run) { r.Status, r.AgentID = "waiting", "ag1" })
	s = Open(home)
	sched := scheduler(s, at(9, 8, 30), nil, func(string) (Seen, bool) { return Seen{Status: "idle", Summary: "ok"}, true })
	sched.Resume()
	eventually(t, "succeeded", func() bool { return runOf(t, s, waiting.ID).Status == "succeeded" })
}

func TestARunNowOnABusyAutomationIsRefused(t *testing.T) {
	s := Open(t.TempDir())
	a := saved(t, s, at(9, 8, 0))
	block := make(chan struct{})
	start := func(string, proto.Automation) launch.Result {
		<-block
		return launch.Result{Err: &launch.Failure{Message: "late"}}
	}
	sched := scheduler(s, at(9, 8, 30), start, nil)
	run, err := sched.RunNow(a.ID)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := sched.RunNow(a.ID); err != ErrBusy {
		t.Fatalf("got %v", err)
	}
	close(block)
	eventually(t, "the run to end", func() bool { return runOf(t, s, run.ID).Status == "failed" })
}
