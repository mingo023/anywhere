package daemon

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"syscall"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/codex/codextest"
	"pocketd/internal/proto"
	"pocketd/internal/shellenv"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

func TestRestoreRecreatesTerminalsWithTheirIDs(t *testing.T) {
	d := newDaemon(t)
	dir := t.TempDir()
	d.Restore(state.File{Version: state.Version, Terminals: []state.Terminal{
		{TerminalID: "t-1", LaunchDir: dir, Cols: 90, Rows: 20},
		{TerminalID: "t-2", LaunchDir: filepath.Join(dir, "gone"), Cols: 80, Rows: 24},
	}}, "/bin/sh")
	term := d.Terminals.Get("t-1")
	if term == nil {
		t.Fatal("t-1 was not recreated")
	}
	t.Cleanup(term.Close)
	if info := term.Info(); info.Cmd != "/bin/sh" || !slices.Equal(info.Args, []string{"-l"}) || info.Cwd != dir || info.Cols != 90 || info.Rows != 20 {
		t.Fatalf("info = %+v", info)
	}
	if d.Terminals.Get("t-2") != nil {
		t.Fatal("t-2 came back though its launch dir is gone")
	}
	want := []state.Terminal{{TerminalID: "t-1", LaunchDir: dir, Cols: 90, Rows: 20}}
	if got := d.Snapshot().Terminals; !slices.Equal(got, want) {
		t.Fatalf("Snapshot = %+v", got)
	}
}

func TestARestoredTerminalStaysInItsLocal(t *testing.T) {
	d := newDaemon(t)
	d.Restore(state.File{Version: state.Version, Terminals: []state.Terminal{{TerminalID: "t-1", LaunchDir: t.TempDir(), Local: "l1"}}}, "/bin/sh")
	term := d.Terminals.Get("t-1")
	if term == nil {
		t.Fatal("t-1 was not recreated")
	}
	t.Cleanup(term.Close)
	if term.Info().Local != "l1" || d.Snapshot().Terminals[0].Local != "l1" {
		t.Fatalf("info = %+v, saved = %+v", term.Info(), d.Snapshot().Terminals)
	}
}

func TestRestoreResumesInTheLoginShellsEnvironment(t *testing.T) {
	d := newDaemon(t)
	d.Capture = func() shellenv.Result {
		return shellenv.Result{Env: []string{"PATH=/usr/bin:/bin", "POCKET_LOGIN=yes"}, Mode: "interactive"}
	}
	var env []string
	d.Resume = func(state.Terminal) (string, []string, string) {
		env = d.LoginEnv()
		return "", nil, "resume_not_accepted"
	}
	d.Restore(savedAgent(t.TempDir(), "claude"), "/bin/sh")
	t.Cleanup(d.Terminals.Get("t-1").Close)
	if !slices.Contains(env, "POCKET_LOGIN=yes") {
		t.Fatal("resume looked up its command in pocketd's own environment")
	}
}

func TestATerminalOpenedAfterRestoringNothingGetsTheLoginShellsEnvironment(t *testing.T) {
	d := newDaemon(t)
	d.Capture = func() shellenv.Result {
		return shellenv.Result{Env: []string{"PATH=/usr/bin:/bin", "POCKET_LOGIN=yes"}, Mode: "interactive"}
	}
	d.Restore(state.File{}, "/bin/sh")
	if !slices.Contains(d.LoginEnv(), "POCKET_LOGIN=yes") {
		t.Fatal("a Terminal opened now gets pocketd's own environment")
	}
}

func TestTheFileHoldsNoTitleEnvArgvOrText(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	path := transcript(t, "fix the login bug")
	hookFrom(d, pr, sessionStart("s1", path))
	eventually(t, "the title", func() bool { return pr.a.Summary().Title == "fix the login bug" })
	f := d.Snapshot()
	if len(f.Terminals) != 1 {
		t.Fatalf("Snapshot = %+v", f)
	}
	e := f.Terminals[0]
	if e.Provider != "claude" || e.ConversationID != "s1" || e.TranscriptPath != path || e.AgentID != pr.a.ID() || e.Launch == nil || e.Launch.Access != "settings" {
		t.Fatalf("entry = %+v", e)
	}
	raw, _ := json.Marshal(f)
	for _, secret := range []string{"login bug", "PATH=", pr.argv[0], "claude-opus"} {
		if strings.Contains(string(raw), secret) {
			t.Errorf("the file holds %q: %s", secret, raw)
		}
	}
}

func TestAPermissionModeHookUpdatesTheSavedAccess(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	hookFrom(d, pr, sessionStart("s1", transcript(t, "hi")))
	hookFrom(d, pr, `{"hook_event_name":"UserPromptSubmit","session_id":"s1","permission_mode":"acceptEdits"}`)
	hookFrom(d, pr, `{"hook_event_name":"Stop","session_id":"s1","permission_mode":"plan"}`)
	if got := d.Snapshot().Terminals[0].Launch; *got != (state.Launch{Access: "edits", Plan: true}) {
		t.Fatalf("launch = %+v", got)
	}
}

func TestAPinOutlivesARestart(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	hookFrom(d, pr, sessionStart("s1", transcript(t, "hi")))
	pr.a.SetPinned(true)
	if !d.Snapshot().Terminals[0].Pinned {
		t.Fatal("the file forgets the pin")
	}
	next := newDaemon(t)
	resuming(next, "/bin/sh")
	saved := savedAgent(t.TempDir(), "claude")
	saved.Terminals[0].Pinned = true
	next.Restore(saved, "/bin/sh")
	t.Cleanup(next.Terminals.Get("t-1").Close)
	a, _ := next.Agents.Get("a-1")
	if !a.Summary().Pinned {
		t.Fatal("the restored agent is not pinned")
	}
	a.SetPinned(false)
	if next.Snapshot().Terminals[0].Pinned {
		t.Fatal("a resuming agent's file forgets the unpin")
	}
}

func TestATerminalWithoutAConversationSavesNoAgent(t *testing.T) {
	d := newDaemon(t)
	claudeIn(t, d)
	if e := d.Snapshot().Terminals[0]; e.Provider != "" || e.AgentID != "" || e.Launch != nil {
		t.Fatalf("entry = %+v", e)
	}
}

type restoreReport struct {
	ok              bool
	outcome, reason string
}

// resuming makes d resume each saved Agent by running bin with args in its
// Terminal, and records what OnRestore hears.
func resuming(d *Daemon, bin string, args ...string) <-chan restoreReport {
	reports := make(chan restoreReport, 4)
	d.Resume = func(state.Terminal) (string, []string, string) { return bin, args, "" }
	d.OnRestore = func(_ string, ok bool, _ int64, outcome, reason string) {
		reports <- restoreReport{ok, outcome, reason}
	}
	return reports
}

func savedAgent(dir, provider string) state.File {
	return state.File{Version: state.Version, Terminals: []state.Terminal{{TerminalID: "t-1", LaunchDir: dir, Cols: 80, Rows: 24,
		Provider: provider, ConversationID: "c1", Launch: &state.Launch{Access: "ask"}, AgentID: "a-1", CreatedAt: 7, Status: "working"}}}
}

func report(t *testing.T, reports <-chan restoreReport) restoreReport {
	t.Helper()
	select {
	case r := <-reports:
		return r
	case <-time.After(5 * time.Second):
		t.Fatal("no restore report")
		return restoreReport{}
	}
}

/*
activeUntilReport reports thread c1 active until the restore reports.
Broadcast waits only for the first client, which may be the resume session
rather than pocketd's watcher, so one broadcast can reach nobody who listens.
*/
func activeUntilReport(t *testing.T, srv *codextest.Server, reports <-chan restoreReport) restoreReport {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); {
		srv.Broadcast("thread/status/changed", active("c1"))
		select {
		case r := <-reports:
			return r
		case <-time.After(50 * time.Millisecond):
		}
	}
	t.Fatal("no restore report")
	return restoreReport{}
}

func restoredIn(t *testing.T, d *Daemon) (*terminal.Terminal, *agent.Agent) {
	t.Helper()
	term := d.Terminals.Get("t-1")
	if term == nil {
		t.Fatal("t-1 was not recreated")
	}
	t.Cleanup(term.Close)
	a, err := d.Agents.Get("a-1")
	if err != nil {
		t.Fatal(err)
	}
	return term, a
}

// waitPresent waits for the agent the poller finds in term: the restored
// Agent is listed before its process is.
func waitPresent(t *testing.T, d *Daemon, term *terminal.Terminal) *presence {
	t.Helper()
	var pr *presence
	eventually(t, "the agent's process", func() bool {
		d.poll()
		pr = d.presentIn(term.Info().ID)
		return pr != nil
	})
	return pr
}

func TestARestoredClaudeAttachesOnItsConversation(t *testing.T) {
	d := newDaemon(t)
	reports := resuming(d, fakeAgent(t, "claude"), "--resume", "c1")
	d.Restore(savedAgent(t.TempDir(), "claude"), "/bin/sh")
	term, a := restoredIn(t, d)
	if s := a.Summary(); s.CreatedAt != 7 || s.TerminalID != "t-1" || s.ProviderSessionID != "c1" || s.Restore != "" {
		t.Fatalf("placeholder = %+v", s)
	}
	if err := a.Driver().Prompt("hi"); err != agent.ErrResuming {
		t.Fatalf("prompt before the claude is back: %v", err)
	}
	pr := waitPresent(t, d, term)
	if pr.a != a {
		t.Fatal("the claude got a new Agent")
	}
	hookFrom(d, pr, sessionStart("c1", transcript(t, "hi")))
	if r := report(t, reports); !r.ok || r.outcome != proto.RestoreInterrupted {
		t.Fatalf("report = %+v", r)
	}
	if s := a.Summary(); s.ID != "a-1" || !s.Attached || s.Restore != proto.RestoreInterrupted {
		t.Fatalf("summary = %+v", s)
	}
}

func TestAnUntrustedRestoredClaudeResumesUnattached(t *testing.T) {
	defer func(w, s time.Duration) { ClaudeAttachWait, RestoreSettle = w, s }(ClaudeAttachWait, RestoreSettle)
	ClaudeAttachWait, RestoreSettle = 100*time.Millisecond, 300*time.Millisecond
	d := newDaemon(t)
	reports := resuming(d, fakeAgent(t, "claude"), "--resume", "c1")
	saved := savedAgent(t.TempDir(), "claude")
	saved.Terminals[0].Status = "idle"
	d.Restore(saved, "/bin/sh")
	term, a := restoredIn(t, d)
	waitPresent(t, d, term)
	if r := report(t, reports); !r.ok || r.outcome != proto.RestoreResumed {
		t.Fatalf("report = %+v", r)
	}
	if s := a.Summary(); s.Attached || s.ProviderSessionID != "c1" || s.Restore != proto.RestoreResumed {
		t.Fatalf("summary = %+v", s)
	}
}

func TestARestoredCodexBindsWithoutEnter(t *testing.T) {
	d := newDaemon(t)
	home, srv := codexHome(t)
	t.Setenv("CODEX_HOME", home)
	reports := resuming(d, fakeAgent(t, "codex"), "resume", "c1")
	d.Restore(savedAgent(t.TempDir(), "codex"), "/bin/sh")
	term, a := restoredIn(t, d)
	waitPresent(t, d, term)
	if got := string(srv.Next("thread/resume").Params); got != `{"threadId":"c1"}` {
		t.Fatalf("resume %s", got)
	}
	if r := activeUntilReport(t, srv, reports); !r.ok || r.outcome != proto.RestoreResumed {
		t.Fatalf("report = %+v", r)
	}
	eventually(t, "the thread's history", func() bool { s := a.Summary(); return s.MaxSeq > 0 && s.Title == "Codex task" })
}

func TestAnExitBeforeAttachFailsTheRestore(t *testing.T) {
	d := newDaemon(t)
	reports := resuming(d, "/bin/sh", "-c", fakeAgent(t, "claude")+" --resume c1; exec sleep 30")
	d.Restore(savedAgent(t.TempDir(), "claude"), "/bin/sh")
	term, a := restoredIn(t, d)
	syscall.Kill(waitPresent(t, d, term).pid, syscall.SIGKILL)
	eventually(t, "the claude gone", func() bool { d.poll(); return d.presentIn("t-1") == nil })
	if r := report(t, reports); r.ok || r.outcome != proto.RestoreFailed || r.reason != "agent_exited" {
		t.Fatalf("report = %+v", r)
	}
	if s := a.Summary(); s.Attached || s.Restore != proto.RestoreFailed {
		t.Fatalf("summary = %+v", s)
	}
	if got, err := d.Agents.Get("a-1"); err != nil || got != a {
		t.Fatal("the placeholder is gone")
	}
}

func TestTheWrapperReportsAnExitBeforeDetection(t *testing.T) {
	d := newDaemon(t)
	reports := resuming(d, "/bin/sh", "-c", "sleep 30")
	d.Restore(savedAgent(t.TempDir(), "claude"), "/bin/sh")
	restoredIn(t, d)
	d.RestoreExited("t-1")
	if r := report(t, reports); r.ok || r.reason != "agent_exited" {
		t.Fatalf("report = %+v", r)
	}
}

func TestAFailedRestoreNoLongerSaysResuming(t *testing.T) {
	d := newDaemon(t)
	reports := resuming(d, "/bin/sh", "-c", "sleep 30")
	d.Restore(savedAgent(t.TempDir(), "claude"), "/bin/sh")
	_, a := restoredIn(t, d)
	d.RestoreExited("t-1")
	report(t, reports)
	if err := a.Driver().Prompt("hi"); err == nil || errors.Is(err, agent.ErrResuming) {
		t.Fatalf("prompt after the restore failed: %v", err)
	}
}

func TestAMissingLaunchDirFailsTheRestore(t *testing.T) {
	d := newDaemon(t)
	reports := resuming(d, "/bin/sh")
	d.Restore(savedAgent(filepath.Join(t.TempDir(), "gone"), "claude"), "/bin/sh")
	if r := report(t, reports); r.ok || r.reason != "launch_dir_missing" {
		t.Fatalf("report = %+v", r)
	}
	a, err := d.Agents.Get("a-1")
	if err != nil {
		t.Fatal(err)
	}
	if s := a.Summary(); s.Attached || s.Restore != proto.RestoreFailed || s.TerminalID != "" {
		t.Fatalf("summary = %+v", s)
	}
	a.Driver().Close()
	if _, err := d.Agents.Get("a-1"); err == nil {
		t.Fatal("closing the placeholder left it listed")
	}
}

func TestNoPresenceInTimeFailsTheRestore(t *testing.T) {
	old := RestoreWait
	RestoreWait = 200 * time.Millisecond
	t.Cleanup(func() { RestoreWait = old })
	d := newDaemon(t)
	reports := resuming(d, "/bin/sh", "-c", "sleep 30")
	d.Restore(savedAgent(t.TempDir(), "claude"), "/bin/sh")
	restoredIn(t, d)
	if r := report(t, reports); r.ok || r.reason != "resume_timeout" {
		t.Fatalf("report = %+v", r)
	}
}

func TestResumeOffReturnsShellsOnly(t *testing.T) {
	d := newDaemon(t)
	d.Restore(savedAgent(t.TempDir(), "claude"), "/bin/sh")
	term := d.Terminals.Get("t-1")
	if term == nil {
		t.Fatal("t-1 was not recreated")
	}
	t.Cleanup(term.Close)
	if info := term.Info(); info.Cmd != "/bin/sh" {
		t.Fatalf("info = %+v", info)
	}
	if all := d.Agents.List(); len(all) != 0 {
		t.Fatalf("agents = %+v", all)
	}
	if e := d.Snapshot().Terminals[0]; e.AgentID != "" {
		t.Fatalf("entry = %+v", e)
	}
}

func TestAResumingAgentKeepsItsEntry(t *testing.T) {
	d := newDaemon(t)
	resuming(d, "/bin/sh", "-c", "sleep 30")
	saved := savedAgent(t.TempDir(), "claude")
	d.Restore(saved, "/bin/sh")
	restoredIn(t, d)
	if got := d.Snapshot().Terminals; !reflect.DeepEqual(got, saved.Terminals) {
		t.Fatalf("Snapshot = %+v", got)
	}
}

func projectTranscript(t *testing.T) string {
	cfg := t.TempDir()
	t.Setenv("CLAUDE_CONFIG_DIR", cfg)
	path := filepath.Join(cfg, "projects", "-w", "s1.jsonl")
	os.MkdirAll(filepath.Dir(path), 0o700)
	lines := userLine("fix the tests") + "\n" + `{"type":"assistant","message":{"content":[{"type":"text","text":"on it"}]}}` + "\n"
	os.WriteFile(path, []byte(lines), 0o600)
	return path
}

func kinds(a *agent.Agent) []string {
	items, _ := a.Timeline.Page(0, 100)
	var out []string
	for _, it := range items {
		out = append(out, it.Kind+":"+it.Text)
	}
	return out
}

func TestTailFromZeroEqualsTheLiveTimeline(t *testing.T) {
	path := projectTranscript(t)
	live := newDaemon(t)
	_, pr := claudeIn(t, live)
	hookFrom(live, pr, sessionStart("c1", path))
	eventually(t, "the live timeline", func() bool { return len(kinds(pr.a)) == 2 })

	d := newDaemon(t)
	resuming(d, fakeAgent(t, "claude"), "--resume", "c1")
	saved := savedAgent(t.TempDir(), "claude")
	saved.Terminals[0].TranscriptPath = path
	d.Restore(saved, "/bin/sh")
	term, a := restoredIn(t, d)
	waitPresent(t, d, term)
	eventually(t, "the restored timeline", func() bool { return slices.Equal(kinds(a), kinds(pr.a)) })
}

func TestAnOutsideTranscriptPathIsNotTailed(t *testing.T) {
	projectTranscript(t)
	d := newDaemon(t)
	resuming(d, fakeAgent(t, "claude"), "--resume", "c1")
	saved := savedAgent(t.TempDir(), "claude")
	saved.Terminals[0].TranscriptPath = transcript(t, "not yours")
	d.Restore(saved, "/bin/sh")
	term, _ := restoredIn(t, d)
	pr := waitPresent(t, d, term)
	pr.mu.Lock()
	defer pr.mu.Unlock()
	if pr.transcript != "" {
		t.Fatalf("tailing %s", pr.transcript)
	}
}

func TestPromptToAResumingAgentIsRefused(t *testing.T) {
	d := newDaemon(t)
	reports := resuming(d, fakeAgent(t, "claude"), "--resume", "c1")
	d.Restore(savedAgent(t.TempDir(), "claude"), "/bin/sh")
	term, a := restoredIn(t, d)
	pr := waitPresent(t, d, term)
	if err := a.Driver().Prompt("hi"); err != agent.ErrResuming {
		t.Fatalf("prompt to an adopted claude still resuming: %v", err)
	}
	hookFrom(d, pr, sessionStart("c1", transcript(t, "hi")))
	report(t, reports)
	if err := a.Driver().Prompt("hi"); err != nil {
		t.Fatalf("prompt after the restore settled: %v", err)
	}
}

func TestACodexRejoiningARunningTurnShowsNoNotice(t *testing.T) {
	d := newDaemon(t)
	home, srv := codexHome(t)
	t.Setenv("CODEX_HOME", home)
	reports := resuming(d, fakeAgent(t, "codex"), "resume", "c1")
	d.Restore(savedAgent(t.TempDir(), "codex"), "/bin/sh")
	term, a := restoredIn(t, d)
	waitPresent(t, d, term)
	srv.Next("thread/resume")
	if r := activeUntilReport(t, srv, reports); !r.ok || r.outcome != proto.RestoreResumed {
		t.Fatalf("report = %+v", r)
	}
	if s := a.Summary(); s.Status != "working" || s.Restore != "" {
		t.Fatalf("summary = %+v", s)
	}
}

func TestAPhoneCreatedAgentComesBackFromThePhone(t *testing.T) {
	d := newDaemon(t)
	resuming(d, "/bin/sh", "-c", "sleep 30")
	saved := savedAgent(t.TempDir(), "claude")
	saved.Terminals[0].Origin = "phone"
	d.Restore(saved, "/bin/sh")
	_, a := restoredIn(t, d)
	if s := a.Summary(); s.Origin != "phone" {
		t.Fatalf("origin = %q", s.Origin)
	}
}

func TestTheFileKeepsWhoStartedTheAgent(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	pr.a.SetOrigin("phone")
	hookFrom(d, pr, sessionStart("s1", transcript(t, "hi")))
	if e := d.Snapshot().Terminals[0]; e.Origin != "phone" {
		t.Fatalf("entry = %+v", e)
	}
}
