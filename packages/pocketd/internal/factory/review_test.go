package factory

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"pocketd/internal/broker"
	"pocketd/internal/hub"
	"pocketd/internal/launch"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

func reviewBlock(body string) string {
	return "Here is my review.\n<anywhere_factory_review>\n" + body + "\n</anywhere_factory_review>"
}

func TestParseReviewReadsTheFindingsOfTheLastBlock(t *testing.T) {
	text := reviewBlock(`{"findings":[{"severity":"nit","message":"old"}]}`) + "\nAgain:\n" +
		reviewBlock(`{"findings":[{"severity":"critical","file":"src/a.go","line":12,"message":" Leaks the handle "},{"severity":"minor","message":"Name it"}]}`)
	got, err := ParseReview(text)
	want := []proto.Finding{{Severity: "critical", File: "src/a.go", Line: 12, Message: "Leaks the handle"}, {Severity: "minor", Message: "Name it"}}
	if err != nil || len(got) != 2 || got[0] != want[0] || got[1] != want[1] {
		t.Fatalf("got %+v, %v", got, err)
	}
}

func TestParseReviewTakesAnEmptyListAsNothingFound(t *testing.T) {
	got, err := ParseReview(reviewBlock(`{"findings":[]}`))
	if err != nil || got == nil || len(got) != 0 {
		t.Fatalf("got %#v, %v", got, err)
	}
}

func TestParseReviewRefusesWhatItWasNotAskedFor(t *testing.T) {
	many := `{"findings":[` + strings.TrimSuffix(strings.Repeat(`{"severity":"nit","message":"x"},`, 51), ",") + `]}`
	for name, text := range map[string]string{
		"no block":         "Looks good to me.",
		"not JSON":         reviewBlock(`findings: none`),
		"no list":          reviewBlock(`{}`),
		"unknown key":      reviewBlock(`{"findings":[],"verdict":"approve"}`),
		"unknown severity": reviewBlock(`{"findings":[{"severity":"blocker","message":"x"}]}`),
		"blank message":    reviewBlock(`{"findings":[{"severity":"minor","message":"  "}]}`),
		"negative line":    reviewBlock(`{"findings":[{"severity":"minor","line":-1,"message":"x"}]}`),
		"two objects":      reviewBlock(`{"findings":[]} {"findings":[]}`),
		"too many":         reviewBlock(many),
	} {
		if got, err := ParseReview(text); err == nil {
			t.Errorf("%s: took %+v", name, got)
		}
	}
	fifty := `{"findings":[` + strings.TrimSuffix(strings.Repeat(`{"severity":"nit","message":"x"},`, 50), ",") + `]}`
	if got, err := ParseReview(reviewBlock(fifty)); err != nil || len(got) != 50 {
		t.Errorf("the most findings: got %d, %v", len(got), err)
	}
}

func TestTheReviewPromptNamesTheBaseAndEndsWithTheSpecOrTheTicket(t *testing.T) {
	tk := proto.Ticket{Title: "Fold hunks", Prompt: "Fold the unchanged hunks."}
	with := ReviewRequest(tk, "main", "## Brief\nFold them.")
	if !strings.HasPrefix(with, "<anywhere_factory_review_request>\nReview the changes of this branch against main.") || !strings.HasSuffix(with, "</anywhere_factory_review_request>\n\nSpec:\n## Brief\nFold them.") {
		t.Fatalf("got %q", with)
	}
	without := ReviewRequest(tk, "main", "")
	if !strings.HasSuffix(without, "</anywhere_factory_review_request>\n\nTicket: Fold hunks\n\nFold the unchanged hunks.") {
		t.Fatalf("got %q", without)
	}
	if _, err := ParseReview(with); err != nil {
		t.Fatalf("the prompt's own example does not parse: %v", err)
	}
}

func TestTheRepairPromptSaysWhatWasWrong(t *testing.T) {
	if got := RepairReview(errors.New("no <anywhere_factory_review> block")); got != "Your review block can't be used: no <anywhere_factory_review> block. Send the corrected <anywhere_factory_review> block only." {
		t.Fatalf("got %q", got)
	}
}

func TestASavedReviewIsReadBackAndARecordOfAnotherHeadIsNoReview(t *testing.T) {
	home := t.TempDir()
	if got := readReview(home, "t1"); got.State != ReviewNone {
		t.Fatalf("got %+v with no file", got)
	}
	rec := reviewRecord{Head: "abc", State: ReviewDone, Agent: "r1", Since: 4, Findings: []proto.Finding{{Severity: "minor", Message: "Name it"}}}
	if err := writeReview(home, "t1", rec); err != nil {
		t.Fatal(err)
	}
	got := readReview(home, "t1")
	if got.Head != "abc" || got.State != ReviewDone || got.Agent != "r1" || got.Since != 4 || len(got.Findings) != 1 {
		t.Fatalf("got %+v", got)
	}
	if same, other, none := got.of("abc"), got.of("def"), got.of(""); same.State != ReviewDone || other.State != ReviewNone || none.State != ReviewNone {
		t.Fatalf("same %+v, other %+v, none %+v", same, other, none)
	}
	os.WriteFile(filepath.Join(home, "state", "factory", "t1", "review.json"), []byte("{"), 0o600)
	if got := readReview(home, "t1"); got.State != ReviewNone {
		t.Fatalf("got %+v from a broken file", got)
	}
}

func TestAReplySettlesAReviewOrAsksForOneRepair(t *testing.T) {
	good, bad := reviewBlock(`{"findings":[{"severity":"nit","message":"x"}]}`), "Looks good."
	for name, c := range map[string]struct {
		state  ReviewState
		reply  string
		failed bool
		want   ReviewState
		n      int
	}{
		"a valid block":                {ReviewRunning, good, false, ReviewDone, 1},
		"a valid block after a repair": {ReviewRepaired, good, false, ReviewDone, 1},
		"a first bad reply":            {ReviewRunning, bad, false, ReviewRunning, 0},
		"a second bad reply":           {ReviewRepaired, bad, false, ReviewFailed, 0},
		"a turn that failed":           {ReviewRunning, bad, true, ReviewFailed, 0},
	} {
		state, findings, _ := verdict(c.state, c.reply, c.failed)
		if state != c.want || len(findings) != c.n {
			t.Errorf("%s: got %s with %d findings", name, state, len(findings))
		}
	}
}

func TestTheFindingsAreCountedBySeverityForTheActivity(t *testing.T) {
	f := func(s string) proto.Finding { return proto.Finding{Severity: s, Message: "x"} }
	if got := tally([]proto.Finding{f("minor"), f("critical"), f("minor")}); got != "1 critical · 2 minor" {
		t.Fatalf("got %q", got)
	}
	if got := tally(nil); got != "No findings" {
		t.Fatalf("got %q", got)
	}
}

func TestOnlyACommandThatReadsIsReadOnly(t *testing.T) {
	for _, c := range []string{"git diff main...HEAD", "git log --oneline -20", "git show HEAD:src/a.go", "git status", "ls -la src", "cat src/a.go", "rg -n TODO src", "grep -rn TODO src", "git diff main | head -50", "/bin/zsh -lc 'git diff main'", "git log --pretty=oneline -5", "git log --oneline", `rg -n "TODO" src`} {
		if !ReadOnly(c) {
			t.Errorf("%q is refused", c)
		}
	}
	for _, c := range []string{
		"rm -rf build", "git push", "git commit -m x", "gh pr review --approve", "gh pr merge 7", "git", "",
		"git diff --output=x.patch", `git diff "--output=x"`, `git diff --out""put=x`, "git log --output x", "git diff --out=x", "git diff --o=x",
		"git diff --ext-diff", "rg --pre ./x.sh TODO", "rg --pre=./x.sh TODO", "rg --hostname-bin ./x.sh TODO",
		"ls; rm x", "cat a > b", "cat a | tee b", "git diff $(rm x)", `git diff \--output=x`, "git diff --out${X}put=x", "git diff --outpu{t,t}=x",
	} {
		if ReadOnly(c) {
			t.Errorf("%q is taken as read-only", c)
		}
	}
}

func TestNoReviewerAskThatEditsWritesOrChangesStateIsEverAllowed(t *testing.T) {
	e := newWorld(t).e
	e.setReviewer("r1", "t1")
	b := broker.New(hub.New())
	b.Deny = e.Deny
	b.Allow = func(proto.PermissionRequest) bool { return true }
	for _, d := range []proto.ToolDetail{
		{Kind: "edit", Path: "src/a.go"},
		{Kind: "write", Path: "src/new.go"},
		{Kind: "shell", Command: "rm -rf build"},
		{Kind: "shell", Command: "git push origin HEAD"},
		{Kind: "shell", Command: "gh pr review --approve"},
		{Kind: "shell", Command: "gh pr merge 7 --squash"},
		{Kind: "shell", Command: "git diff --output=x.patch"},
	} {
		req := proto.PermissionRequest{AgentID: "r1", Detail: d}
		reason, denied := b.Denied(req)
		if !denied || reason != ReadOnlyReason || b.Auto(req) || e.Reads(req) {
			t.Errorf("%+v: denied %v, reason %q, auto %v, reads %v", d, denied, reason, b.Auto(req), e.Reads(req))
		}
	}
	events, _ := ReadActivity(e.Home, "t1")
	noted := func(text string) bool {
		for _, ev := range events {
			if ev.Actor == "reviewer" && ev.Kind == "denied" && ev.Text == text {
				return true
			}
		}
		return false
	}
	if !noted("Refused: rm -rf build") || !noted("Refused to write src/new.go") {
		t.Fatalf("activity %+v", events)
	}
}

func TestAReviewerMayOnlyReadAndSearchSoEveryOtherKindOfAskIsRefused(t *testing.T) {
	e := newWorld(t).e
	e.setReviewer("r1", "t1")
	for _, d := range []proto.ToolDetail{
		{Kind: "other", Name: "mcp__github__merge_pull_request"},
		{Kind: "other", Name: "MultiEdit"},
		{Kind: "task", Description: "fix it"},
		{Kind: "unheard-of"},
	} {
		req := proto.PermissionRequest{AgentID: "r1", Detail: d}
		if reason, denied := e.Deny(req); !denied || reason != ReadOnlyReason || e.Reads(req) {
			t.Errorf("%+v: denied %v, reason %q, reads %v", d, denied, reason, e.Reads(req))
		}
	}
}

func TestAReviewerReadsWithoutAnAskAndEveryOtherSessionIsLeftAlone(t *testing.T) {
	e := newWorld(t).e
	e.setReviewer("r1", "t1")
	for _, d := range []proto.ToolDetail{{Kind: "shell", Command: "git diff main"}, {Kind: "shell", Command: "rg -n TODO"}} {
		req := proto.PermissionRequest{AgentID: "r1", Detail: d}
		if _, denied := e.Deny(req); denied || !e.Reads(req) {
			t.Fatalf("%+v is not read without an ask", d)
		}
	}
	for _, kind := range []string{"read", "search"} {
		req := proto.PermissionRequest{AgentID: "r1", Detail: proto.ToolDetail{Kind: kind}}
		if _, denied := e.Deny(req); denied {
			t.Fatalf("a %s is refused", kind)
		}
	}
	lead := proto.PermissionRequest{AgentID: "lead", Detail: proto.ToolDetail{Kind: "write", Path: "a.go"}}
	if _, denied := e.Deny(lead); denied || e.Reads(lead) {
		t.Fatal("a session that is no reviewer is answered")
	}
	diff := proto.PermissionRequest{AgentID: "r1", Detail: proto.ToolDetail{Kind: "shell", Command: "git diff main"}}
	e.dropReviewer("r1")
	if _, denied := e.Deny(proto.PermissionRequest{AgentID: "r1", Detail: proto.ToolDetail{Kind: "write"}}); denied {
		t.Fatal("a closed reviewer is still held")
	}
	var off *Engine
	if _, denied := off.Deny(lead); denied || off.Reads(diff) {
		t.Fatal("a nil engine answers")
	}
}

// atGate is the Facts of a Ticket that passed Checks: an open draft PR on head abc, the lead idle.
func atGate(edit func(*Facts)) Facts {
	cfg := proto.DefaultFactoryConfig()
	cfg.Enabled = true
	f := Facts{
		Ticket: proto.Ticket{ID: "t1", Title: "Fold hunks", State: proto.TicketLive, Station: proto.StationPR, Status: proto.TicketWorking, AgentID: "lead", PR: 7},
		Config: cfg,
		Lead:   Session{Present: true, Turn: "idle"},
		PR:     &proto.PR{Number: 7, State: "OPEN", Draft: true, Base: "main", Head: "abc", Mergeable: "MERGEABLE"},
		Review: ReviewNone,
	}
	if edit != nil {
		edit(&f)
	}
	return f
}

func TestATicketAtPRReviewGetsAReviewerThatIsRepairedOnce(t *testing.T) {
	working, ended := Session{Present: true, Turn: "working"}, Session{Present: true, Turn: "done"}
	for name, c := range map[string]struct {
		edit func(*Facts)
		want string
	}{
		"no review yet starts the reviewer":       {nil, "startReview"},
		"a reviewer at work is waited for":        {func(f *Facts) { f.Review, f.Reviewer = ReviewRunning, working }, ""},
		"a reviewer being launched is waited for": {func(f *Facts) { f.Review = ReviewRunning }, ""},
		"a reviewer that asks you is waited for":  {func(f *Facts) { f.Review, f.Reviewer = ReviewRunning, Session{Present: true, Turn: "needsYou"} }, ""},
		"a first reply without a block":           {func(f *Facts) { f.Review, f.Reviewer = ReviewRunning, ended }, "repairReview"},
		"a repair under way is waited for":        {func(f *Facts) { f.Review, f.Reviewer = ReviewRepaired, working }, ""},
		"a reply to the repair is waited for":     {func(f *Facts) { f.Review, f.Reviewer = ReviewRepaired, ended }, ""},
		"no pull request yet":                     {func(f *Facts) { f.PR = nil }, ""},
	} {
		got, acts := Next(atGate(c.edit))
		if did(acts) != c.want || got.Station != proto.StationPR || got.Status != proto.TicketWorking {
			t.Errorf("%s: got %q, station %s, status %s", name, did(acts), got.Station, got.Status)
		}
	}
}

// gateOpen is atGate with the gate open: the review is done and the Ticket waits on you.
func gateOpen(edit func(*Facts)) Facts {
	return atGate(func(f *Facts) {
		f.Review = ReviewDone
		f.Ticket.Gate, f.Ticket.Status = proto.GatePR, proto.TicketNeedsYou
		f.PR.Draft = false
		if edit != nil {
			edit(f)
		}
	})
}

func TestThePRReviewGateOpensOnceTheReviewIsOver(t *testing.T) {
	ended := Session{Present: true, Turn: "done"}
	for name, c := range map[string]struct {
		facts Facts
		want  string
	}{
		"a finished review":                     {atGate(func(f *Facts) { f.Review, f.Reviewer, f.Ready = ReviewDone, ended, ReadyDone }), "closeReviewer,await"},
		"a failed review does not block":        {atGate(func(f *Facts) { f.Review, f.Reviewer, f.Ready = ReviewFailed, ended, ReadyDone }), "closeReviewer,await"},
		"a project with the reviewer off":       {atGate(func(f *Facts) { f.Config.Reviewer, f.Ready = false, ReadyDone }), "await"},
		"a pull request that is ready":          {atGate(func(f *Facts) { f.Review = ReviewDone; f.PR.Draft = false }), "await"},
		"a draft is marked ready once":          {gateOpen(func(f *Facts) { f.PR.Draft = true }), "await"},
		"after a restart the wait is set again": {gateOpen(nil), "await"},
		"a lead that is gone":                   {gateOpen(func(f *Facts) { f.Lead = Session{} }), "await"},
	} {
		got, acts := Next(c.facts)
		if did(acts) != c.want || got.Station != proto.StationPR || got.Gate != proto.GatePR || got.Status != proto.TicketNeedsYou || got.Why != "" {
			t.Errorf("%s: got %q, %+v", name, did(acts), got)
		}
	}
}

func TestTheGateStaysShutWhileTheDraftIsNotReadyAndAFailureFailsTheTicket(t *testing.T) {
	for name, c := range map[string]struct {
		ready  ReadyState
		want   string
		status string
		why    string
	}{
		"not asked yet": {ReadyNone, "markReady", proto.TicketWorking, ""},
		"asked":         {ReadyPending, "", proto.TicketWorking, ""},
		"failed":        {ReadyFailed, "", proto.TicketFailed, "The pull request couldn't be marked ready: gh said no"},
	} {
		got, acts := Next(atGate(func(f *Facts) { f.Config.Reviewer, f.Ready, f.ReadyErr = false, c.ready, "gh said no" }))
		if did(acts) != c.want || got.Gate != "" || got.Status != c.status || got.Why != c.why || got.Station != proto.StationPR {
			t.Errorf("%s: got %q, %+v", name, did(acts), got)
		}
	}
}

func TestAMergedPullRequestMovesTheTicketToMerged(t *testing.T) {
	for name, c := range map[string]struct {
		facts Facts
		want  string
	}{
		"at the gate":        {gateOpen(func(f *Facts) { f.PR.State = "MERGED" }), "close"},
		"with the lead gone": {gateOpen(func(f *Facts) { f.PR.State, f.Lead = "MERGED", Session{} }), "close"},
		"while the reviewer reads": {atGate(func(f *Facts) {
			f.Review, f.Reviewer, f.PR.State = ReviewRunning, Session{Present: true, Turn: "working"}, "MERGED"
		}), "closeReviewer,close"},
	} {
		got, acts := Next(c.facts)
		if did(acts) != c.want || got.Station != proto.StationMerged || got.Gate != "" || got.Status != proto.TicketDone {
			t.Errorf("%s: got %q, %+v", name, did(acts), got)
		}
	}
}

func TestALeadThatStartsATurnAtPRReviewTakesTheTicketBackToImplement(t *testing.T) {
	got, acts := Next(gateOpen(func(f *Facts) { f.Lead.Turn = "working" }))
	if got.Station != proto.StationImplement || got.Gate != "" || got.Status != proto.TicketWorking || did(acts) != "" {
		t.Fatalf("got %q, %+v", did(acts), got)
	}
	got, acts = Next(atGate(func(f *Facts) {
		f.Review, f.Reviewer, f.Lead.Turn = ReviewRunning, Session{Present: true, Turn: "working"}, "working"
	}))
	if got.Station != proto.StationImplement || did(acts) != "closeReviewer" {
		t.Fatalf("got %q, %+v", did(acts), got)
	}
}

func TestALeadThatWorkedWhileNoStepSawItTakesTheTicketBackToImplement(t *testing.T) {
	got, acts := Next(gateOpen(func(f *Facts) { f.GateSeq, f.Gated, f.Lead.Seq = 4, true, 9 }))
	if got.Station != proto.StationImplement || got.Gate != "" || got.Status != proto.TicketWorking || did(acts) != "" {
		t.Fatalf("got %q, %+v", did(acts), got)
	}
	for name, edit := range map[string]func(*Facts){
		"a lead that did nothing":        func(f *Facts) { f.GateSeq, f.Gated, f.Lead.Seq = 9, true, 9 },
		"a gate that is not yet noticed": func(f *Facts) { f.GateSeq, f.Gated, f.Lead.Seq = 0, false, 9 },
	} {
		if got, _ := Next(gateOpen(edit)); got.Station != proto.StationPR || got.Gate != proto.GatePR {
			t.Errorf("%s: got %+v", name, got)
		}
	}
}

func TestAChangeRequestThatTheLeadAnswersBetweenTwoStepsIsNotMissed(t *testing.T) {
	w := newWorld(t)
	tk := w.ticket(false)
	w.reg.Add(leadID, "/w/x", "claude", w.chat)
	w.e.Board.Update(tk.ID, func(x *proto.Ticket) {
		x.State, x.Station, x.Gate, x.Status, x.AgentID, x.Worktree, x.PR = proto.TicketLive, proto.StationPR, proto.GatePR, proto.TicketNeedsYou, leadID, "/w/x", 7
	})
	if err := writeReview(w.home, tk.ID, reviewRecord{Head: "abc", State: ReviewDone, Findings: []proto.Finding{}}); err != nil {
		t.Fatal(err)
	}
	w.e.recovered.Store(true)
	w.settle()
	w.e.prs["/w/x"] = &proto.PR{Number: 7, State: "OPEN", Base: "main", Head: "abc", Mergeable: "MERGEABLE"}
	w.e.post(event{ticket: tk.ID})
	w.settle()
	if got := w.got(tk.ID); got.Station != proto.StationPR || got.Gate != proto.GatePR {
		t.Fatalf("the gate did not hold: %+v", got)
	}
	// The whole turn happens where no step looks.
	lead := w.lead()
	lead.Record(timeline.Event{Kind: "user", Text: "Rename the helper"})
	lead.Record(timeline.Event{Kind: "assistant_text", Text: "Renamed."})
	lead.Record(timeline.Event{Kind: "result", OK: true})
	w.e.post(event{ticket: tk.ID})
	w.settle()
	if got := w.got(tk.ID); got.Station != proto.StationImplement || got.Gate != "" {
		t.Fatalf("the rework was missed: %+v", got)
	}
}

func TestAReworkedTicketIsCommittedPushedAndCheckedOnceTheLeadsTurnEnds(t *testing.T) {
	at := func(turn string) Facts {
		return atGate(func(f *Facts) { f.Ticket.Station, f.Lead.Turn = proto.StationImplement, turn })
	}
	if got, acts := Next(at("working")); got.Station != proto.StationImplement || got.Status != proto.TicketWorking || len(acts) != 0 {
		t.Fatalf("working: got %q, %+v", did(acts), got)
	}
	if got, acts := Next(at("needsYou")); got.Station != proto.StationImplement || got.Gate != proto.GateAsk || len(acts) != 0 {
		t.Fatalf("needsYou: got %q, %+v", did(acts), got)
	}
	if got, acts := Next(at("done")); got.Station != proto.StationChecks || got.Status != proto.TicketWorking || did(acts) != "commitPush" {
		t.Fatalf("done: got %q, %+v", did(acts), got)
	}
}

func TestAClosedOrConflictingPullRequestLeavesTheGate(t *testing.T) {
	got, acts := Next(gateOpen(func(f *Facts) { f.PR.State = "CLOSED" }))
	if got.Station != proto.StationPR || got.Status != proto.TicketFailed || got.Why != "The pull request was closed" || got.Gate != "" || len(acts) != 0 {
		t.Fatalf("closed: got %q, %+v", did(acts), got)
	}
	got, acts = Next(gateOpen(func(f *Facts) {
		f.PR.Mergeable = "CONFLICTING"
		f.Local = LocalChecks{Head: "abc", Pushed: true, State: LocalPassed}
	}))
	if got.Station != proto.StationChecks || got.Gate != "" || got.Fix != 1 || did(acts) != "sendFix" {
		t.Fatalf("conflicting: got %q, %+v", did(acts), got)
	}
	got, _ = Next(gateOpen(func(f *Facts) {
		f.PR.Mergeable, f.Ticket.Fix = "CONFLICTING", f.Config.Fix
		f.Local = LocalChecks{Head: "abc", Pushed: true, State: LocalPassed}
	}))
	if got.Station != proto.StationChecks || got.Status != proto.TicketFailed || !strings.HasPrefix(got.Why, "Conflicts with main") {
		t.Fatalf("conflicting with no attempt left: got %+v", got)
	}
}

func TestChangesYouAskForAtThePRGateAreNoted(t *testing.T) {
	before := proto.Ticket{Station: proto.StationPR, Gate: proto.GatePR, Status: proto.TicketNeedsYou}
	after := proto.Ticket{Station: proto.StationImplement, Status: proto.TicketWorking}
	got := Notes(before, after, nil, 5)
	want := proto.FactoryEvent{At: 5, Station: proto.StationPR, Actor: "you", Kind: "changes"}
	if len(got) != 2 || got[0] != want || got[1].Kind != "moved" {
		t.Fatalf("got %+v", got)
	}
	if got := Notes(before, proto.Ticket{Station: proto.StationMerged, Status: proto.TicketDone}, nil, 5); len(got) != 1 || got[0].Kind != "moved" {
		t.Fatalf("a merge is no request for changes: %+v", got)
	}
}

// atReview puts the rig's Ticket at PR review, with a finished review of the checkout's head. view is what
// gh reports for the PR.
func atReview(t *testing.T, r *rig, view string) {
	t.Helper()
	os.Remove(filepath.Join(r.fake, "nopr"))
	r.put("view.json", view)
	rec := reviewRecord{Head: gitIn(t, r.tree, "rev-parse", "HEAD"), State: ReviewDone, Findings: []proto.Finding{}}
	if err := writeReview(r.home, r.ticket, rec); err != nil {
		t.Fatal(err)
	}
	r.e.Board.Update(r.ticket, func(x *proto.Ticket) { x.Station, x.PR = proto.StationPR, 57 })
	r.e.PRChanged(r.tree)
}

func prView(state string, draft bool) string {
	return fmt.Sprintf(`{"isDraft":%t,"number":57,"state":%q,"title":"Fix it","url":"https://github.com/acme/app/pull/57","headRefOid":"@HEAD@","baseRefName":"main","mergeable":"MERGEABLE","statusCheckRollup":[]}`, draft, state)
}

func lastEvent(t *testing.T, r *rig) proto.FactoryEvent {
	t.Helper()
	events, err := ReadActivity(r.home, r.ticket)
	if err != nil || len(events) == 0 {
		t.Fatalf("activity %+v, %v", events, err)
	}
	return events[len(events)-1]
}

func TestTheGateMarksThePRReadyAndSetsTheWaitOnTheLead(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Enabled: true, Reviewer: true, Fix: 2})
	atReview(t, r, prView("OPEN", true))
	got := r.until("the PR review gate", func(x proto.Ticket) bool { return x.Gate == proto.GatePR })
	if got.Station != proto.StationPR || got.Status != proto.TicketNeedsYou {
		t.Fatalf("got %+v", got)
	}
	lead, _ := r.e.Agents.Get(leadID)
	r.until("gh pr ready and the wait", func(proto.Ticket) bool {
		return r.called("pr ready 57") && lead.Summary().Status == "needsYou"
	})
	if r.called("pr merge") || r.called("pr review") {
		t.Fatal("the engine merged or reviewed on GitHub")
	}
}

func TestAPRThatCannotBeMarkedReadyFailsTheTicketInsteadOfOpeningTheGate(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Enabled: true, Reviewer: true, Fix: 2})
	r.put("fail", "GraphQL: no permission\n")
	atReview(t, r, prView("OPEN", true))
	got := r.until("the failed ticket", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Station != proto.StationPR || got.Gate != "" || !strings.HasPrefix(got.Why, "The pull request couldn't be marked ready: ") {
		t.Fatalf("got %+v", got)
	}
	if ev := lastEvent(t, r); ev.Kind != "failed" || ev.Text != got.Why {
		t.Fatalf("got %+v", ev)
	}
}

func TestAPRThatGhCouldNotReadIsNotedOnceInTheTicketsActivity(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Enabled: true, Reviewer: true, Fix: 2})
	os.Remove(filepath.Join(r.fake, "nopr"))
	r.put("view.err", "HTTP 502: Bad Gateway\n")
	r.e.Board.Update(r.ticket, func(x *proto.Ticket) { x.Station = proto.StationPR })
	noted := func() (n int) {
		events, _ := ReadActivity(r.home, r.ticket)
		for _, ev := range events {
			if ev.Kind == "check" && strings.HasPrefix(ev.Text, "Couldn't read the pull request") {
				n++
			}
		}
		return n
	}
	r.e.PRChanged(r.tree)
	r.until("the note", func(proto.Ticket) bool { return noted() == 1 })
	r.e.PRChanged(r.tree)
	time.Sleep(300 * time.Millisecond)
	if n := noted(); n != 1 {
		t.Fatalf("the same failure was noted %d times", n)
	}
}

func TestAPRMergedOnGitHubClosesTheTicketAsMergedOutsideAnywhere(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Enabled: true, Reviewer: true, Fix: 2})
	atReview(t, r, prView("MERGED", false))
	got := r.until("the closed ticket", func(x proto.Ticket) bool { return x.State == proto.TicketClosed })
	if got.ClosedWhy != proto.ClosedMerged || got.Station != proto.StationMerged || got.Gate != "" || got.Status != proto.TicketDone {
		t.Fatalf("got %+v", got)
	}
	r.until("the note", func(proto.Ticket) bool { return lastEvent(t, r).Kind == "merged" })
	if ev := lastEvent(t, r); ev.Actor != "anywhere" || ev.Text != "Merged outside Anywhere" {
		t.Fatalf("got %+v", ev)
	}
	if r.called("pr merge") {
		t.Fatal("the engine merged")
	}
}

func TestAMergeYouAskedForClosesTheTicketAndTakesTheWaitOffTheLead(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Enabled: true, Reviewer: true, Fix: 2})
	atReview(t, r, prView("OPEN", false))
	r.until("the PR review gate", func(x proto.Ticket) bool { return x.Gate == proto.GatePR })
	// What wsserver does for gh.pr.merge.
	if err := r.e.GitHub.Merge(context.Background(), r.tree, 57, "squash"); err != nil {
		t.Fatal(err)
	}
	// Merge expired the cache entry, so the next read asks gh, which now says merged.
	r.put("view.json", prView("MERGED", false))
	r.e.PRChanged(r.tree)
	got := r.until("the closed ticket", func(x proto.Ticket) bool { return x.State == proto.TicketClosed })
	if got.ClosedWhy != proto.ClosedMerged || got.Station != proto.StationMerged {
		t.Fatalf("got %+v", got)
	}
	r.until("the note", func(proto.Ticket) bool { return lastEvent(t, r).Kind == "merged" })
	lead, _ := r.e.Agents.Get(leadID)
	if ev := lastEvent(t, r); ev.Actor != "you" || ev.Text != "Merged" || lead.Summary().Status == "needsYou" {
		t.Fatalf("got %+v, lead %s", ev, lead.Summary().Status)
	}
}

// The pipeline never merges and never reviews on GitHub. never.go lists the commands in order to
// refuse them, and the Factory rules name them for the agent.
func TestNoFactoryCodeMergesOrReviewsAPullRequest(t *testing.T) {
	files, err := filepath.Glob("*.go")
	if err != nil || len(files) == 0 {
		t.Fatalf("files %v, %v", files, err)
	}
	for _, name := range files {
		if strings.HasSuffix(name, "_test.go") || name == "never.go" {
			continue
		}
		raw, err := os.ReadFile(name)
		if err != nil {
			t.Fatal(err)
		}
		text := string(raw)
		if from, to := strings.Index(text, "<anywhere_factory_rules>"), strings.Index(text, "</anywhere_factory_rules>"); from >= 0 && to > from {
			text = text[:from] + text[to:]
		}
		for _, banned := range []string{"pr review", "pr merge", "GitHub.Merge(", "--approve"} {
			if strings.Contains(text, banned) {
				t.Errorf("%s holds %q", name, banned)
			}
		}
	}
}

// slowLaunches makes each reviewer launch wait until the test lets it go. started has each launch's agent id,
// and chats has each reviewer's session.
type slowLaunches struct {
	mu      sync.Mutex
	started chan string
	release []chan struct{}
	chats   map[string]*chat
}

func slowReviewerLaunches(w *world, n int) *slowLaunches {
	l := &slowLaunches{started: make(chan string, n), chats: map[string]*chat{}}
	for range n {
		l.release = append(l.release, make(chan struct{}))
	}
	var calls int
	w.e.Launch = func(key, requestID string, s proto.LaunchSpec, created func(string)) launch.Result {
		l.mu.Lock()
		i := calls
		calls++
		id := fmt.Sprintf("reviewer-%d", i)
		c := &chat{}
		l.chats[id] = c
		l.mu.Unlock()
		w.reg.Add(id, "/w", s.Provider, c)
		l.started <- id
		<-l.release[i]
		return launch.Result{AgentID: id}
	}
	return l
}

func (l *slowLaunches) closed(id string) bool {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.chats[id].closed.Load()
}

func reviewing(w *world) (proto.Ticket, proto.FactoryConfig, *proto.PR) {
	w.t.Helper()
	tk := w.ticket(false)
	cfg := w.e.Configs.Get(tk.Folder)
	return tk, cfg, &proto.PR{Number: 7, State: "OPEN", Base: "main", Head: "abc"}
}

func waitFor(t *testing.T, what string, ok func() bool) {
	t.Helper()
	for end := time.Now().Add(5 * time.Second); time.Now().Before(end); time.Sleep(5 * time.Millisecond) {
		if ok() {
			return
		}
	}
	t.Fatalf("timed out waiting for %s", what)
}

func TestAReviewerThatLaunchesAfterItsReviewWasClosedDoesNotBringTheReviewBack(t *testing.T) {
	w := newWorld(t)
	slow := slowReviewerLaunches(w, 1)
	tk, cfg, pr := reviewing(w)
	w.e.startReview(tk, cfg, pr)
	<-slow.started
	w.e.closeReviewer(tk)
	close(slow.release[0])
	waitFor(t, "the late reviewer to be closed", func() bool { return slow.closed("reviewer-0") })
	if rec := readReview(w.home, tk.ID); rec.State != ReviewNone {
		t.Fatalf("the review is back: %+v", rec)
	}
	if _, ok := w.e.reviewerOf("reviewer-0"); ok {
		t.Fatal("the late reviewer is still held")
	}
}

func TestAReviewerOfAClosedLaunchDoesNotTakeTheRecordOfTheNextLaunchOfTheSameHead(t *testing.T) {
	w := newWorld(t)
	slow := slowReviewerLaunches(w, 2)
	tk, cfg, pr := reviewing(w)
	w.e.startReview(tk, cfg, pr)
	<-slow.started
	w.e.closeReviewer(tk)
	w.e.startReview(tk, cfg, pr)
	<-slow.started
	close(slow.release[0])
	waitFor(t, "the first launch to end", func() bool {
		return slow.closed("reviewer-0") || readReview(w.home, tk.ID).Agent == "reviewer-0"
	})
	if rec := readReview(w.home, tk.ID); rec.Agent != "" || !slow.closed("reviewer-0") {
		t.Fatalf("the closed launch took the record: %+v", rec)
	}
	close(slow.release[1])
	waitFor(t, "the second launch to be recorded", func() bool { return readReview(w.home, tk.ID).Agent == "reviewer-1" })
	if slow.closed("reviewer-1") {
		t.Fatal("the live reviewer was closed")
	}
}
