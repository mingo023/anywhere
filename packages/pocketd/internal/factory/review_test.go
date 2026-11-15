package factory

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/broker"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
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

func TestAReviewerReadsWithoutAnAskAndEveryOtherSessionIsLeftAlone(t *testing.T) {
	e := newWorld(t).e
	e.setReviewer("r1", "t1")
	for _, d := range []proto.ToolDetail{{Kind: "shell", Command: "git diff main"}, {Kind: "shell", Command: "rg -n TODO"}} {
		req := proto.PermissionRequest{AgentID: "r1", Detail: d}
		if _, denied := e.Deny(req); denied || !e.Reads(req) {
			t.Fatalf("%+v is not read without an ask", d)
		}
	}
	other := proto.PermissionRequest{AgentID: "r1", Detail: proto.ToolDetail{Kind: "other"}}
	if _, denied := e.Deny(other); denied || e.Reads(other) {
		t.Fatal("an ask that is no edit, write or shell command is not left to the human")
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

func TestATicketAtPRReviewGetsAReviewerThatIsRepairedOnceAndThenClosed(t *testing.T) {
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
		"a finished review closes the reviewer":   {func(f *Facts) { f.Review, f.Reviewer = ReviewDone, ended }, "closeReviewer"},
		"a failed review closes the reviewer":     {func(f *Facts) { f.Review, f.Reviewer = ReviewFailed, ended }, "closeReviewer"},
		"a finished review with no reviewer":      {func(f *Facts) { f.Review = ReviewDone }, ""},
		"a project with the reviewer off":         {func(f *Facts) { f.Config.Reviewer = false }, ""},
		"no pull request yet":                     {func(f *Facts) { f.PR = nil }, ""},
	} {
		got, acts := Next(atGate(c.edit))
		if did(acts) != c.want || got.Station != proto.StationPR || got.Status != proto.TicketWorking {
			t.Errorf("%s: got %q, station %s, status %s", name, did(acts), got.Station, got.Status)
		}
	}
}
