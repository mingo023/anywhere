package github

import (
	"testing"
	"time"

	"pocketd/internal/proto"
)

func pr(n int) *PR {
	return &PR{Number: n, Title: "PR", URL: "https://github.com/acme/app/pull/1", State: "OPEN"}
}

// reply starts a gh run for tree at now and applies its answer.
func reply(c *cache, tree string, pr *PR, problem Problem, now time.Time) bool {
	c.start(tree, now)
	return c.apply(tree, now, pr, problem, now)
}

var trees = []string{"/a", "/b", "/c"}

func on(tree string) map[string]bool { return map[string]bool{tree: true} }

func dueIs(t *testing.T, c *cache, watched map[string]bool, now time.Time, want string) {
	t.Helper()
	got, ok := c.due(watched, trees, now)
	if want == "" && ok || want != "" && got != want {
		t.Fatalf("due %q (%v), want %q", got, ok, want)
	}
}

func TestTheWatchedTreeIsAskedAboutFirstThenTheRestInOrder(t *testing.T) {
	c, now := newCache(), time.Now()
	dueIs(t, c, on("/b"), now, "/b")
	reply(c, "/b", nil, "", now)
	dueIs(t, c, on("/b"), now, "/a")
	reply(c, "/a", pr(1), "", now)
	dueIs(t, c, on("/b"), now, "/c")
	reply(c, "/c", nil, "", now)
	dueIs(t, c, on("/b"), now, "")
	dueIs(t, c, on("/main"), now, "")
}

func TestGhRunsOneAtATimeAHungOneGivesUpItsTurnAndAMissingOrLoggedOutGhIsAskedAgainAfter5Minutes(t *testing.T) {
	c, now := newCache(), time.Now()
	c.start("/a", now)
	dueIs(t, c, nil, now, "")
	dueIs(t, c, nil, now.Add(time.Minute), "/a")
	c.apply("/a", now, nil, SignedOut, now)
	if c.running != "" {
		t.Fatal("still running")
	}
	dueIs(t, c, nil, now.Add(299*time.Second), "")
	later := now.Add(300 * time.Second)
	dueIs(t, c, nil, later, "/a")
	if !reply(c, "/a", nil, "", later) {
		t.Fatal("an answer after a problem changes nothing")
	}
}

func TestTheWatchedTreeIsAskedAgainAfter30sAndTheOthersAfter5Minutes(t *testing.T) {
	c, t0 := newCache(), time.Now()
	for _, tree := range trees {
		reply(c, tree, nil, "", t0)
	}
	at := func(s int) time.Time { return t0.Add(time.Duration(s) * time.Second) }
	dueIs(t, c, on("/b"), at(29), "")
	dueIs(t, c, on("/b"), at(30), "/b")
	reply(c, "/b", nil, "", at(290))
	dueIs(t, c, on("/b"), at(299), "")
	dueIs(t, c, on("/b"), at(300), "/a")
}

func TestEveryWatchedTreeIsAskedAboutBeforeTheOthers(t *testing.T) {
	c, now := newCache(), time.Now()
	watched := map[string]bool{"/b": true, "/c": true}
	dueIs(t, c, watched, now, "/b")
	reply(c, "/b", nil, "", now)
	dueIs(t, c, watched, now, "/c")
}

func TestApplyingTheSamePRAgainChangesNothing(t *testing.T) {
	c, now := newCache(), time.Now()
	steps := []struct {
		pr      *PR
		problem Problem
		changed bool
	}{
		{nil, "", true},
		{nil, "", false},
		{pr(1), "", true},
		{pr(1), "", false},
		{&PR{Number: 1, Title: "PR", URL: "https://github.com/acme/app/pull/1", State: "OPEN", Checks: proto.Checks{Failed: 1}}, "", true},
		{nil, Missing, true},
		{nil, Missing, false},
	}
	for i, s := range steps {
		if got := reply(c, "/a", s.pr, s.problem, now); got != s.changed {
			t.Fatalf("step %d: changed %v", i, got)
		}
	}
}

func TestAReadyGhLetsPRPollingResumeAtOnce(t *testing.T) {
	c, now := newCache(), time.Now()
	reply(c, "/a", nil, SignedOut, now)
	c.checked(Status{State: "signedOut", Version: "2.81.0"})
	dueIs(t, c, nil, now, "")
	c.checked(Status{State: "ready", Version: "2.81.0", Login: "me", Accounts: []string{"me"}})
	dueIs(t, c, nil, now, "/a")
}

func TestAForgottenTreeIsDueAtOnce(t *testing.T) {
	c, now := newCache(), time.Now()
	reply(c, "/a", nil, "", now)
	reply(c, "/b", nil, "", now)
	reply(c, "/c", nil, "", now)
	dueIs(t, c, on("/a"), now, "")
	c.forget("/a")
	dueIs(t, c, on("/a"), now, "/a")
}

func TestAnAnswerFromAForgottenOrSupersededRunDoesNotLand(t *testing.T) {
	c, t0 := newCache(), time.Now()
	t1 := t0.Add(time.Minute)
	c.start("/a", t0)
	c.forget("/a")
	if c.apply("/a", t0, pr(1), "", t0) {
		t.Fatal("a forgotten run landed")
	}
	if _, ok := c.get("/a"); ok {
		t.Fatal("kept")
	}
	c.start("/a", t1)
	if c.apply("/a", t0, pr(1), "", t1) {
		t.Fatal("a superseded run landed")
	}
	if !c.apply("/a", t1, nil, "", t1) {
		t.Fatal("the current run did not land")
	}
}

func TestAnAnswerAskedForOutsideThePollOutlivesThePollsOlderRun(t *testing.T) {
	c, now := newCache(), time.Now()
	c.start("/a", now)
	c.record("/a", pr(2), "", now)
	if c.apply("/a", now, pr(1), "", now) {
		t.Fatal("the poll's older answer landed")
	}
	if got, _ := c.get("/a"); got.Number != 2 {
		t.Fatal(got)
	}
}

func TestAPRJustOpenedShowsAtOnceAndIsAskedAboutAtTheNextPoll(t *testing.T) {
	c, now := newCache(), time.Now()
	c.put("/a", pr(7), now)
	if got, _ := c.get("/a"); got == nil || got.Number != 7 {
		t.Fatal(got)
	}
	dueIs(t, c, on("/a"), now, "/a")
}

func TestAnEditOutlivesTheAnswerOfARunAskedForBeforeIt(t *testing.T) {
	c, now := newCache(), time.Now()
	reply(c, "/a", pr(1), "", now)
	c.start("/a", now)
	c.edit("/a", func(p *PR) { p.State = "MERGED" })
	if c.apply("/a", now, pr(1), "", now) {
		t.Fatal("the older answer landed")
	}
	if got, _ := c.get("/a"); got.State != "MERGED" {
		t.Fatal(got.State)
	}
}

func TestAnExpiredPRStaysShownUntilGhAnswersAgain(t *testing.T) {
	c, now := newCache(), time.Now().Add(10*time.Minute)
	reply(c, "/a", pr(1), "", now)
	dueIs(t, c, on("/a"), now, "/b")
	c.expire("/a", now)
	if got, _ := c.get("/a"); got == nil || got.Number != 1 {
		t.Fatal(got)
	}
	dueIs(t, c, on("/a"), now, "/a")
}
