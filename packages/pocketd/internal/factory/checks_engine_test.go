//go:build unix

package factory

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/board"
	"pocketd/internal/github"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/team"
)

// rig is an engine with one live Ticket at Checks, in a worktree with a remote, and the fake gh of internal/github.
type rig struct {
	t                         *testing.T
	e                         *Engine
	teams                     *team.Store
	home, project, tree, fake string
	ticket                    string
	mail                      chan string
}

func newRig(t *testing.T, cfg proto.FactoryConfig) *rig {
	t.Helper()
	for k, v := range map[string]string{"GIT_CONFIG_GLOBAL": "/dev/null", "GIT_AUTHOR_NAME": "T", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "T", "GIT_COMMITTER_EMAIL": "t@t"} {
		t.Setenv(k, v)
	}
	// gh and git may still be writing when the test ends, so the folder is removed by hand, with retries.
	made, err := os.MkdirTemp("", "factory-checks")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		for range 50 {
			if os.RemoveAll(made) == nil {
				return
			}
			time.Sleep(20 * time.Millisecond)
		}
	})
	root, _ := filepath.EvalSymlinks(made)
	r := &rig{t: t, home: filepath.Join(root, "home"), project: filepath.Join(root, "app"), tree: filepath.Join(root, "app-fix"), fake: filepath.Join(root, "gh"), mail: make(chan string, 16)}
	for _, d := range []string{r.home, r.project, r.fake} {
		os.MkdirAll(d, 0o755)
	}
	gitIn(t, r.project, "init", "-q", "-b", "main")
	gitIn(t, r.project, "commit", "-q", "--allow-empty", "-m", "first")
	gitIn(t, root, "init", "-q", "--bare", "origin.git")
	gitIn(t, r.project, "remote", "add", "origin", filepath.Join(root, "origin.git"))
	gitIn(t, r.project, "worktree", "add", "-q", "-b", "fix", r.tree)
	gitIn(t, r.tree, "commit", "-q", "--allow-empty", "-m", "Fix it")
	reg := filepath.Join(root, "desktop.json")
	os.WriteFile(reg, []byte(fmt.Sprintf(`{"projects":[%q]}`, r.project)), 0o600)

	r.put("version", "gh version 2.81.0 (2025-10-01)\n")
	r.put("auth", "github.com\n  ✓ Logged in to github.com account t (keyring)\n  - Active account: true\n")
	r.put("nopr", "")
	r.put("created", "https://github.com/acme/app/pull/57\n")
	r.put("threads.json", `{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[]}}}}}`)
	r.view(passedCI)
	bin, _ := filepath.Abs(filepath.Join("..", "github", "testdata", "fakegh"))
	env := func(string) []string {
		return []string{"PATH=" + bin + ":/usr/bin:/bin", "FAKEGH=" + r.fake, "HOME=" + r.fake, "GIT_CONFIG_GLOBAL=/dev/null", "GH_TOKEN=tok"}
	}

	store := board.Open(r.home)
	saved, err := store.Save(proto.Ticket{Title: "Fix it", Prompt: "Fix it.", Provider: "claude", Folder: r.project}, false, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	r.ticket = saved.ID
	store.Update(saved.ID, func(x *proto.Ticket) {
		x.State, x.Status, x.Station, x.AgentID, x.Worktree = proto.TicketLive, proto.TicketWorking, proto.StationChecks, leadID, r.tree
	})
	configs := OpenConfigs(r.home)
	full := proto.DefaultFactoryConfig()
	full.Enabled, full.Fix, full.Checks = true, cfg.Fix, cfg.Checks
	if err := configs.Save(r.project, full); err != nil {
		t.Fatal(err)
	}
	h := hub.New()
	agents := agent.NewRegistry(h)
	agents.Add(leadID, r.tree, "claude", &chat{})
	r.teams, err = team.OpenStore(r.home)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { r.teams.Close() })
	r.e = &Engine{
		Home: r.home, Board: store, Configs: configs, Agents: agents, Now: time.Now, Poll: time.Millisecond, Profiles: OpenProfiles(r.home),
		Teams:  &team.Teams{Store: r.teams, Agents: agents, Hub: h, Poll: time.Millisecond},
		Mail:   func(_, text string) error { r.mail <- text; return nil },
		GitHub: github.New(env, registry.New(reg)),
		Env:    env,
	}
	ctx, cancel := context.WithCancel(context.Background())
	stopped := make(chan struct{})
	t.Cleanup(func() { cancel(); <-stopped })
	go func() { r.e.Run(ctx); close(stopped) }()
	t.Cleanup(func() { r.e.GitHub.Leave("factory") })
	return r
}

func (r *rig) put(name, content string) {
	r.t.Helper()
	if err := os.WriteFile(filepath.Join(r.fake, name), []byte(content), 0o644); err != nil {
		r.t.Fatal(err)
	}
}

const passedCI = `[{"__typename":"CheckRun","name":"ci","status":"COMPLETED","conclusion":"SUCCESS"}]`

// view sets what gh pr view reports for the open PR: its checks, on the commit of the checkout.
func (r *rig) view(rollup string) {
	r.put("view.json", `{"isDraft":true,"number":57,"state":"OPEN","title":"Fix it","url":"https://github.com/acme/app/pull/57","headRefOid":"@HEAD@","baseRefName":"main","mergeable":"MERGEABLE","statusCheckRollup":`+rollup+`}`)
}

func (r *rig) called(prefix string) bool {
	raw, _ := os.ReadFile(filepath.Join(r.fake, "calls"))
	for _, c := range strings.Split(strings.TrimSpace(string(raw)), "\n") {
		if _, args, _ := strings.Cut(c, " "); strings.HasPrefix(args, prefix) {
			return true
		}
	}
	return false
}

func (r *rig) get() proto.Ticket {
	tickets, _ := r.e.Board.Snapshot()
	for _, x := range tickets {
		if x.ID == r.ticket {
			return x
		}
	}
	r.t.Fatal("the ticket is gone")
	return proto.Ticket{}
}

func (r *rig) until(what string, ok func(proto.Ticket) bool) proto.Ticket {
	r.t.Helper()
	for end := time.Now().Add(15 * time.Second); time.Now().Before(end); time.Sleep(10 * time.Millisecond) {
		if x := r.get(); ok(x) {
			return x
		}
	}
	r.t.Fatalf("timed out waiting for %s: %+v", what, r.get())
	return proto.Ticket{}
}

func TestATicketAtChecksGetsADraftPRWithItsSpecAndItsKey(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Enabled: true, Fix: 2})
	d := dir(r.home, r.ticket)
	os.MkdirAll(d, 0o700)
	os.WriteFile(filepath.Join(d, "spec.md"), []byte("## Brief\nAdd the flag.\n\n## Acceptance criteria\n- [ ] The flag exists\n\n## Risks\n- None\n"), 0o600)
	r.e.Recover()
	got := r.until("the draft PR", func(x proto.Ticket) bool { return x.PR == 57 })
	if !r.called("pr create --title Fix it --body-file - --draft") {
		t.Fatal("gh pr create was not called for a draft with the ticket's title")
	}
	body, _ := os.ReadFile(filepath.Join(r.fake, "body"))
	if want := fmt.Sprintf("## Brief\nAdd the flag.\n\n## Acceptance criteria\n- [ ] The flag exists\n\nTicket APP-%d", got.Number); string(body) != want {
		t.Fatalf("body\n%s\nwant\n%s", body, want)
	}
	if remote, local := gitIn(t, filepath.Join(filepath.Dir(r.tree), "origin.git"), "rev-parse", "fix"), gitIn(t, r.tree, "rev-parse", "HEAD"); remote != local {
		t.Fatalf("the remote has %s, the checkout has %s", remote, local)
	}
	events, _ := ReadActivity(r.home, r.ticket)
	if !slices.ContainsFunc(events, func(ev proto.FactoryEvent) bool {
		return ev.Kind == "check" && ev.Actor == "anywhere" && ev.Text == "Opened draft PR #57"
	}) {
		t.Fatalf("activity %+v", events)
	}
}

func TestAMergeConflictFailsTheTicketWithTheFilesAndUsesNoAttempt(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2})
	tm := proto.Team{Lead: leadID, State: "conflict", Conflict: []string{"a.rs", "b.rs"}, Tasks: make([]proto.TeamTask, 2)}
	if err := r.teams.Put(tm); err != nil {
		t.Fatal(err)
	}
	r.e.TeamChanged(tm)
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Station != proto.StationChecks || got.Why != "Conflict in a.rs, b.rs" || got.Fix != 0 || got.PR != 0 {
		t.Fatalf("got %+v", got)
	}
	if r.called("pr create") {
		t.Fatal("a PR was opened for a branch that did not merge")
	}
}

func TestAPushThatFailsFailsTheTicketWithoutAnAttempt(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Enabled: true, Fix: 2})
	gitIn(t, r.project, "remote", "set-url", "origin", filepath.Join(r.home, "nowhere.git"))
	r.e.Recover()
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Station != proto.StationChecks || got.Fix != 0 || !strings.HasPrefix(got.Why, "Couldn't open the pull request") {
		t.Fatalf("got %+v", got)
	}
}

// record is the Ticket's newest check record once ok holds.
func (r *rig) record(what string, ok func(Record) bool) Record {
	r.t.Helper()
	for end := time.Now().Add(15 * time.Second); time.Now().Before(end); time.Sleep(10 * time.Millisecond) {
		if rec, has := ReadRecord(r.home, r.ticket); has && ok(rec) {
			return rec
		}
	}
	rec, _ := ReadRecord(r.home, r.ticket)
	r.t.Fatalf("timed out waiting for %s: %+v", what, rec)
	return Record{}
}

func TestTheCommandsRunInOrderInTheCheckoutAndTheirOutputIsSaved(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Checks: []string{"echo one > first.txt", "cat first.txt; pwd"}})
	r.e.Recover()
	rec := r.record("both commands", func(rec Record) bool { return rec.State == LocalPassed })
	if rec.Total != 2 || len(rec.Checks) != 2 || rec.Checks[1].Exit != 0 || rec.Checks[1].Output != "one\n"+r.tree+"\n" || rec.Checks[1].Attempt != 0 || rec.Checks[1].At == 0 {
		t.Fatalf("got %+v", rec)
	}
	if rec.Head != gitIn(t, r.tree, "rev-parse", "HEAD") {
		t.Fatalf("the record is for %s", rec.Head)
	}
}

func TestTheCommandsStopAtTheFirstFailureAndTheTicketFails(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Checks: []string{"true", "sh -c 'echo boom; exit 3'", "touch never.txt"}})
	r.e.Recover()
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	rec, _ := ReadRecord(r.home, r.ticket)
	if got.Station != proto.StationChecks || !strings.HasPrefix(got.Why, "sh -c 'echo boom; exit 3'") || rec.State != LocalFailed || len(rec.Checks) != 2 || rec.Checks[1].Exit != 3 || rec.Checks[1].Output != "boom\n" {
		t.Fatalf("got %+v\n%+v", got, rec)
	}
	if _, err := os.Stat(filepath.Join(r.tree, "never.txt")); err == nil {
		t.Fatal("the command after the failure ran")
	}
	events, _ := ReadActivity(r.home, r.ticket)
	var texts []string
	for _, ev := range events {
		if ev.Kind == "check" {
			texts = append(texts, ev.Text)
		}
	}
	if got := strings.Join(texts, " | "); !strings.HasSuffix(got, "Passed: true | Failed (exit 3): sh -c 'echo boom; exit 3'") {
		t.Fatalf("activity: %s", got)
	}
}

func TestACommandThatWasRunningWhenPocketdStoppedRunsAgain(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Checks: []string{"echo again >> ran.txt"}})
	head := gitIn(t, r.tree, "rev-parse", "HEAD")
	if err := WriteRecord(r.home, r.ticket, Record{Head: head, Pushed: time.Now().UnixMilli(), State: LocalRunning, Total: 1}, true); err != nil {
		t.Fatal(err)
	}
	r.e.Board.Update(r.ticket, func(x *proto.Ticket) { x.PR, x.Fix = 57, 1 })
	r.e.Recover()
	r.record("the command", func(rec Record) bool { return rec.State == LocalPassed })
	if got := r.get(); got.Fix != 1 {
		t.Fatalf("a restart changed the attempts: %+v", got)
	}
	if raw, _ := os.ReadFile(filepath.Join(r.tree, "ran.txt")); string(raw) != "again\n" {
		t.Fatalf("ran.txt: %q", raw)
	}
}

// asked makes the PR cache ask gh again now. In production the poller asks every 30 seconds.
func (r *rig) asked() {
	r.t.Helper()
	if err := r.e.GitHub.Push(context.Background(), r.tree); err != nil {
		r.t.Fatal(err)
	}
	r.e.PRChanged(r.tree)
}

func TestATicketWhoseCommandsAndGitHubChecksPassMovesToPRReview(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Checks: []string{"true"}})
	r.e.Recover()
	got := r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
	if got.Status != proto.TicketWorking || got.Why != "" || got.PR != 57 || got.Fix != 0 {
		t.Fatalf("got %+v", got)
	}
}

func TestAFailedGitHubCheckFailsTheTicket(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Checks: []string{"true"}})
	r.view(`[{"__typename":"CheckRun","name":"e2e","status":"COMPLETED","conclusion":"FAILURE"}]`)
	r.e.Recover()
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Station != proto.StationChecks || !strings.HasPrefix(got.Why, "e2e") {
		t.Fatalf("got %+v", got)
	}
}

func TestPendingGitHubChecksHoldTheTicketUntilTheyPass(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{})
	r.view(`[{"__typename":"CheckRun","name":"ci","status":"IN_PROGRESS","conclusion":""}]`)
	r.e.Recover()
	r.until("the wait", func(x proto.Ticket) bool { return x.Why == "GitHub checks · 0/1" && x.Status == proto.TicketWorking })
	r.view(passedCI)
	r.asked()
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
}

func TestWithNoCommandsAndNoGitHubChecksTheTicketPassesAfterTheWait(t *testing.T) {
	quietAfter = 300 * time.Millisecond
	t.Cleanup(func() { quietAfter = 3 * github.WatchedEvery })
	r := newRig(t, proto.FactoryConfig{})
	r.view(`[]`)
	r.e.Recover()
	r.until("the wait", func(x proto.Ticket) bool { return x.Why == "Looking for GitHub checks" })
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
	if rec, _ := ReadRecord(r.home, r.ticket); !rec.Quiet || rec.State != LocalPassed || rec.Total != 0 {
		t.Fatalf("got %+v", rec)
	}
}

func TestAfterARestartTheWaitForGitHubCountsFromThePush(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{})
	r.view(`[]`)
	os.Remove(filepath.Join(r.fake, "nopr"))
	head := gitIn(t, r.tree, "rev-parse", "HEAD")
	pushed := time.Now().Add(-2 * quietAfter).UnixMilli()
	if err := WriteRecord(r.home, r.ticket, Record{Head: head, Pushed: pushed, State: LocalPassed}, true); err != nil {
		t.Fatal(err)
	}
	r.e.Board.Update(r.ticket, func(x *proto.Ticket) { x.PR = 57 })
	r.e.Recover()
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
}

func TestTheTicketsDetailHoldsTheCommandsOfTheNewestRecord(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Checks: []string{"true", "sh -c 'echo boom; exit 3'"}})
	r.e.Recover()
	r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	d, err := r.e.Detail(r.ticket)
	if err != nil || len(d.Checks) != 2 || d.Checks[0].Command != "true" || d.Checks[1].Exit != 3 || d.Checks[1].Output != "boom\n" || d.NoChecks {
		t.Fatalf("got %+v %v", d, err)
	}
}

func TestATicketThatPassedWithNothingToRunSaysNoChecksRan(t *testing.T) {
	quietAfter = 100 * time.Millisecond
	t.Cleanup(func() { quietAfter = 3 * github.WatchedEvery })
	r := newRig(t, proto.FactoryConfig{})
	r.view(`[]`)
	r.e.Recover()
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
	if d, _ := r.e.Detail(r.ticket); !d.NoChecks || len(d.Checks) != 0 {
		t.Fatalf("got %+v", d)
	}
}

func TestATicketWhoseGitHubChecksPassedDoesNotSayNoChecksRan(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{})
	r.e.Recover()
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
	if d, _ := r.e.Detail(r.ticket); d.NoChecks {
		t.Fatalf("got %+v", d)
	}
}

// note is the next note mailed to the lead.
func (r *rig) note() string {
	r.t.Helper()
	select {
	case text := <-r.mail:
		return text
	case <-time.After(15 * time.Second):
		r.t.Fatalf("no note was mailed: %+v", r.get())
		return ""
	}
}

// silent fails when a note is mailed within d.
func (r *rig) silent(d time.Duration) {
	r.t.Helper()
	select {
	case text := <-r.mail:
		r.t.Fatalf("mailed %q", text)
	case <-time.After(d):
	}
}

// fixes is the lead's fix turn: it writes file in the checkout, then ends normally.
func (r *rig) fixes(file, body string) {
	r.t.Helper()
	if file != "" {
		if err := os.WriteFile(filepath.Join(r.tree, file), []byte(body), 0o644); err != nil {
			r.t.Fatal(err)
		}
	}
	r.e.TurnEnded(leadID, true)
}

const needsFixed = `test -f fixed.txt || { echo "fixed.txt is missing"; exit 1; }`

func TestAFailedCommandIsMailedToTheLeadAndItsFixPassesOnTheFirstAttempt(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{needsFixed}})
	r.e.Recover()
	note := r.note()
	if want := "<anywhere_factory_fix attempt=\"1\" of=\"2\">\nThe checks failed. Fix the cause in this checkout. Don't commit and don't push: Anywhere does both when you finish.\n$ " + needsFixed + "   (exit 1)\nfixed.txt is missing\n</anywhere_factory_fix>"; note != want {
		t.Fatalf("note\n%s\nwant\n%s", note, want)
	}
	if got := r.get(); got.Fix != 1 || got.Status != proto.TicketWorking || got.Why != "Fixing · attempt 1" {
		t.Fatalf("while fixing: %+v", got)
	}
	r.fixes("fixed.txt", "ok\n")
	got := r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
	if got.Fix != 1 || got.Status != proto.TicketWorking {
		t.Fatalf("got %+v", got)
	}
	if msg := gitIn(t, r.tree, "log", "-1", "--format=%s"); msg != "Fix the failed checks" {
		t.Fatalf("the last commit is %q", msg)
	}
	if remote, local := gitIn(t, filepath.Join(filepath.Dir(r.tree), "origin.git"), "rev-parse", "fix"), gitIn(t, r.tree, "rev-parse", "HEAD"); remote != local {
		t.Fatalf("the fix was not pushed: %s, %s", remote, local)
	}
	if d, _ := r.e.Detail(r.ticket); len(d.Checks) != 1 || d.Checks[0].Exit != 0 || d.Checks[0].Attempt != 1 {
		t.Fatalf("the newest record: %+v", d.Checks)
	}
}

func TestATicketFailsAtChecksAfterTheLastAttemptAndSaysSo(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{needsFixed}})
	r.e.Recover()
	if note := r.note(); !strings.HasPrefix(note, `<anywhere_factory_fix attempt="1" of="2">`) {
		t.Fatalf("first note %q", note)
	}
	r.fixes("a.txt", "one\n")
	if note := r.note(); !strings.HasPrefix(note, `<anywhere_factory_fix attempt="2" of="2">`) {
		t.Fatalf("second note %q", note)
	}
	r.fixes("a.txt", "two\n")
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Station != proto.StationChecks || got.Fix != 2 || got.Why != needsFixed+" · Auto-fix 2 of 2 used" {
		t.Fatalf("got %+v", got)
	}
	r.silent(300 * time.Millisecond)
}

func TestAFixTurnThatChangesNothingIsCheckedAgainButNotSentAgain(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{needsFixed}})
	r.e.Recover()
	r.note()
	r.fixes("", "")
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Fix != 1 || got.Why != needsFixed+" · The lead changed nothing" {
		t.Fatalf("got %+v", got)
	}
	if rec, _ := ReadRecord(r.home, r.ticket); !rec.Same || rec.State != LocalFailed {
		t.Fatalf("the commands did not run again on the same commit: %+v", rec)
	}
	r.silent(300 * time.Millisecond)
}

func TestAStoppedFixTurnFailsTheTicketAndTheLeadsNextNormalEndCarriesOn(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{needsFixed}})
	r.e.Recover()
	r.note()
	r.e.TurnEnded(leadID, false)
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Fix != 1 || got.Why != "The lead's fix turn failed or was stopped" {
		t.Fatalf("got %+v", got)
	}
	r.fixes("fixed.txt", "ok\n")
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
}

func TestAFailedGitHubCheckIsMailedWithItsLink(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 1})
	r.view(`[{"__typename":"CheckRun","name":"e2e","status":"COMPLETED","conclusion":"FAILURE","detailsUrl":"https://github.com/acme/app/actions/runs/9"}]`)
	r.e.Recover()
	if note := r.note(); !strings.Contains(note, "\nGitHub check failed: e2e https://github.com/acme/app/actions/runs/9\n") {
		t.Fatalf("note %q", note)
	}
	r.view(passedCI)
	r.fixes("b.txt", "x\n")
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
}

// spent is a rig whose Ticket failed at Checks after both attempts.
func spent(t *testing.T) *rig {
	t.Helper()
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{needsFixed}})
	r.e.Recover()
	r.note()
	r.fixes("a.txt", "one\n")
	r.note()
	r.fixes("a.txt", "two\n")
	r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	return r
}

func TestAHintSendsYourTextToTheLeadAndGrantsExactlyOneMoreAttempt(t *testing.T) {
	r := spent(t)
	if err := r.e.Hint(r.ticket, "  Create fixed.txt.  "); err != nil {
		t.Fatal(err)
	}
	note := r.note()
	if !strings.HasPrefix(note, "<anywhere_factory_fix attempt=\"3\" of=\"2\" by=\"you\">\nCreate fixed.txt.\nThe checks failed.") || !strings.Contains(note, "fixed.txt is missing") {
		t.Fatalf("note %q", note)
	}
	if got := r.get(); !got.Hint || got.Fix != 2 || got.Status != proto.TicketWorking || got.Why != "Fixing · your hint" {
		t.Fatalf("while fixing: %+v", got)
	}
	r.fixes("a.txt", "three\n")
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Fix != 2 || !got.Hint || got.Why != needsFixed+" · Auto-fix 2 of 2 used" {
		t.Fatalf("got %+v", got)
	}
	r.silent(300 * time.Millisecond)

	if err := r.e.Hint(r.ticket, "Really, create fixed.txt."); err != nil {
		t.Fatal(err)
	}
	r.note()
	r.fixes("fixed.txt", "ok\n")
	got = r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
	if got.Hint || got.Fix != 2 {
		t.Fatalf("got %+v", got)
	}
	events, _ := ReadActivity(r.home, r.ticket)
	hints := slices.DeleteFunc(slices.Clone(events), func(ev proto.FactoryEvent) bool { return ev.Kind != "hint" })
	if len(hints) != 2 || hints[0].Actor != "you" || hints[0].Station != proto.StationChecks {
		t.Fatalf("hints in the activity: %+v", hints)
	}
}

func TestAHintIsRefusedUnlessTheTicketFailedAtChecksAndTheTextSaysSomething(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{"sleep 5"}})
	r.e.Recover()
	r.until("the commands", func(x proto.Ticket) bool { return strings.HasPrefix(x.Why, "Checks · ") })
	if err := r.e.Hint(r.ticket, "try harder"); !errors.Is(err, ErrNoHint) {
		t.Fatalf("a working ticket: %v", err)
	}
	if err := r.e.Hint("nope", "try harder"); !errors.Is(err, board.ErrUnknown) {
		t.Fatalf("an unknown ticket: %v", err)
	}
	r.e.Board.Update(r.ticket, func(x *proto.Ticket) { x.Status = proto.TicketFailed })
	if err := r.e.Hint(r.ticket, " \n "); !errors.Is(err, ErrNoHint) {
		t.Fatalf("a blank hint: %v", err)
	}
	var off *Engine
	if err := off.Hint(r.ticket, "try harder"); !errors.Is(err, ErrOff) {
		t.Fatalf("a nil engine: %v", err)
	}
}

func TestARestartGrantsNoAttemptToATicketThatUsedThemAll(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{needsFixed}})
	head := gitIn(t, r.tree, "rev-parse", "HEAD")
	failed := Record{Head: head, Pushed: time.Now().UnixMilli(), State: LocalFailed, Total: 1, Checks: []proto.FactoryCheck{{Command: needsFixed, Exit: 1, Output: "fixed.txt is missing\n", Attempt: 2}}}
	if err := WriteRecord(r.home, r.ticket, failed, true); err != nil {
		t.Fatal(err)
	}
	r.e.Board.Update(r.ticket, func(x *proto.Ticket) {
		x.PR, x.Fix, x.Status, x.Why = 57, 2, proto.TicketFailed, needsFixed+" · Auto-fix 2 of 2 used"
	})
	r.e.Recover()
	r.silent(500 * time.Millisecond)
	if got := r.get(); got.Status != proto.TicketFailed || got.Fix != 2 || got.Station != proto.StationChecks {
		t.Fatalf("got %+v", got)
	}
}

func TestAFixNoteThatWasOutWhenPocketdStoppedIsNotSentAgain(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2, Checks: []string{needsFixed}})
	os.Remove(filepath.Join(r.fake, "nopr"))
	head := gitIn(t, r.tree, "rev-parse", "HEAD")
	out := Record{Head: head, Pushed: time.Now().UnixMilli(), State: LocalFailed, Total: 1, Fix: FixMailed, Checks: []proto.FactoryCheck{{Command: needsFixed, Exit: 1, Attempt: 0}}}
	if err := WriteRecord(r.home, r.ticket, out, true); err != nil {
		t.Fatal(err)
	}
	r.e.Board.Update(r.ticket, func(x *proto.Ticket) { x.PR, x.Fix, x.Why = 57, 1, "Fixing · attempt 1" })
	r.e.Recover()
	r.silent(500 * time.Millisecond)
	if got := r.get(); got.Status != proto.TicketWorking || got.Fix != 1 || got.Why != "Fixing · attempt 1" {
		t.Fatalf("got %+v", got)
	}
	r.fixes("fixed.txt", "ok\n")
	r.until("PR review", func(x proto.Ticket) bool { return x.Station == proto.StationPR })
}
