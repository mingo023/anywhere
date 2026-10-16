//go:build unix

package daemon

import (
	"os"
	"path/filepath"
	"sync/atomic"
	"testing"
	"time"

	"pocketd/internal/accounts"
	"pocketd/internal/agent"
	"pocketd/internal/proto"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

func TestOnlyAnIdleAgentOnAnotherAccountWithAConversationMoves(t *testing.T) {
	for name, c := range map[string]struct {
		s    proto.AgentSummary
		want bool
	}{
		"idle on the old account":     {proto.AgentSummary{Status: "idle", Attached: true, Account: "a", ProviderSessionID: "c1"}, true},
		"done on the old account":     {proto.AgentSummary{Status: "done", Attached: true, Account: "a", ProviderSessionID: "c1"}, true},
		"idle on the default":         {proto.AgentSummary{Status: "idle", Attached: true, Account: "b", ProviderSessionID: "c1"}, false},
		"working":                     {proto.AgentSummary{Status: "working", Attached: true, Account: "a", ProviderSessionID: "c1"}, false},
		"asking for permission":       {proto.AgentSummary{Status: "needsYou", Attached: true, Account: "a", ProviderSessionID: "c1"}, false},
		"failed":                      {proto.AgentSummary{Status: "done", Attached: true, Failed: true, Account: "a", ProviderSessionID: "c1"}, false},
		"unattached":                  {proto.AgentSummary{Status: "idle", Account: "a", ProviderSessionID: "c1"}, false},
		"no conversation to resume":   {proto.AgentSummary{Status: "idle", Attached: true, Account: "a"}, false},
		"built-in to a managed one":   {proto.AgentSummary{Status: "idle", Attached: true, ProviderSessionID: "c1"}, true},
		"managed one back to builtin": {proto.AgentSummary{Status: "idle", Attached: true, Account: "a", ProviderSessionID: "c1"}, true},
	} {
		want := "b"
		if name == "managed one back to builtin" {
			want = ""
		}
		if got := movable(c.s, want); got != c.want {
			t.Errorf("%s: movable = %v", name, got)
		}
	}
}

// useDefault makes id, "" for the built-in login, claude's default account.
func useDefault(t *testing.T, d *Daemon, id string) {
	t.Helper()
	index := `{"accounts":[{"id":"work","provider":"claude"}],"defaults":{"claude":"` + id + `"}}`
	path := filepath.Join(d.Accounts.Root, "accounts.json")
	if err := os.WriteFile(path+".tmp", []byte(index), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Rename(path+".tmp", path); err != nil {
		t.Fatal(err)
	}
}

func signIn(t *testing.T, dir string) {
	t.Helper()
	os.MkdirAll(dir, 0o700)
	creds := `{"claudeAiOauth":{"accessToken":"tok"}}`
	if err := os.WriteFile(filepath.Join(dir, ".credentials.json"), []byte(creds), 0o600); err != nil {
		t.Fatal(err)
	}
}

type resumeFunc = func(state.Terminal) (string, []string, string)

// settleOn is a resumeFunc that starts claude through the shell on config dir dir.
func settleOn(bin, dir string, args ...string) resumeFunc {
	return func(state.Terminal) (string, []string, string) {
		return "/bin/sh", append([]string{"-c", "CLAUDE_CONFIG_DIR=" + dir + ` exec "$0" "$@"`, bin}, args...), ""
	}
}

// followed returns a daemon with a claude on the built-in login, the default,
// that has settled in t-1; the dir of managed account "work"; and use, which
// swaps the Resume the daemon calls without racing the follow a settle starts.
func followed(t *testing.T) (d *Daemon, term *terminal.Terminal, pr *presence, work string, use func(resumeFunc)) {
	t.Helper()
	d = newDaemon(t)
	root := filepath.Join(t.TempDir(), "accounts")
	d.Accounts = &accounts.Store{Root: root, Home: t.TempDir()}
	work = filepath.Join(root, "claude", "work")
	os.MkdirAll(root, 0o700)
	signIn(t, filepath.Join(d.Accounts.Home, ".claude"))
	signIn(t, work)
	useDefault(t, d, "builtin")
	reports := resuming(d, fakeAgent(t, "claude"), "--resume", "c1")
	var current atomic.Pointer[resumeFunc]
	first := d.Resume
	current.Store(&first)
	d.Resume = func(e state.Terminal) (string, []string, string) { return (*current.Load())(e) }
	use = func(f resumeFunc) { current.Store(&f) }
	saved := savedAgent(t.TempDir(), "claude")
	saved.Terminals[0].Status = "idle"
	d.Restore(saved, "/bin/sh")
	term, _ = restoredIn(t, d)
	pr = waitPresent(t, d, term)
	hookFrom(d, pr, sessionStart("c1", transcript(t, "hi")))
	report(t, reports)
	return d, term, pr, work, use
}

func TestAnIdleAgentRestartsOnTheNewAccountInTheSameTerminal(t *testing.T) {
	d, term, pr, work, use := followed(t)
	a := pr.a
	asks := make(chan state.Terminal, 4)
	start := settleOn(fakeAgent(t, "claude"), work, "--resume", "c1")
	use(func(e state.Terminal) (string, []string, string) {
		asks <- e
		return start(e)
	})
	if d.Running("claude", "work") != 0 {
		t.Fatal("work already runs an agent")
	}

	useDefault(t, d, "work")
	d.Follow("claude")

	asked := <-asks
	if asked.Account != "work" || asked.ConversationID != "c1" || asked.AgentID != "a-1" || asked.TerminalID != "t-1" {
		t.Fatalf("resumed %+v", asked)
	}
	if err := a.Driver().Prompt("hi"); err != agent.ErrResuming {
		t.Fatalf("prompt while moving: %v", err)
	}
	var next *presence
	eventually(t, "the agent on the new account", func() bool {
		d.poll()
		next = d.presentIn("t-1")
		return next != nil && next != pr
	})
	if next.a != a {
		t.Fatal("the moved agent got a new Agent")
	}
	if d.Terminals.Get("t-1") != term {
		t.Fatal("the terminal was replaced")
	}
	if got, err := d.Agents.Get("a-1"); err != nil || got != a {
		t.Fatalf("agent listed as %v, %v", got, err)
	}
	if s := a.Summary(); s.Account != "work" || s.Status == "closed" {
		t.Fatalf("summary = %+v", s)
	}
	if d.Running("claude", "work") != 1 || d.Running("claude", "") != 0 {
		t.Fatalf("running: work %d, builtin %d", d.Running("claude", "work"), d.Running("claude", ""))
	}

	hookFrom(d, next, sessionStart("c1", transcript(t, "hi")))
	if err := a.Driver().Prompt("hi"); err == agent.ErrResuming {
		t.Fatal("still resuming after its conversation came back")
	}
	eventually(t, "the move to settle", func() bool {
		d.mu.Lock()
		defer d.mu.Unlock()
		return d.restoring["t-1"] == nil
	})
}

func TestAnAgentStaysWhenItsResumeIsRefused(t *testing.T) {
	d, term, pr, _, use := followed(t)
	use(func(state.Terminal) (string, []string, string) { return "", nil, "resume_not_accepted" })

	d.followTo("claude", "work")

	time.Sleep(300 * time.Millisecond)
	d.poll()
	if d.presentIn("t-1") != pr || term.Info().Cmd == "/bin/sh" {
		t.Fatal("the agent was touched")
	}
}

func TestAMoveThatCannotStartLeavesTheAgentRunning(t *testing.T) {
	d, _, pr, _, use := followed(t)
	use(func(state.Terminal) (string, []string, string) { return "/no/such/binary", nil, "" })

	d.followTo("claude", "work")

	d.poll()
	if d.presentIn("t-1") != pr {
		t.Fatal("the agent was replaced")
	}
	d.mu.Lock()
	_, listed := d.restoring["t-1"]
	d.mu.Unlock()
	if listed {
		t.Fatal("hint outlived the failed move")
	}
	if _, ok := pr.a.Driver().(termDriver); !ok {
		t.Fatalf("driver = %T", pr.a.Driver())
	}
}

func TestAnAgentOnAManagedAccountReturnsToTheBuiltInLogin(t *testing.T) {
	d, _, pr, work, use := followed(t)
	a := pr.a
	bin := fakeAgent(t, "claude")
	use(settleOn(bin, work))
	useDefault(t, d, "work")
	d.Follow("claude")
	var onWork *presence
	eventually(t, "the agent on work", func() bool {
		d.poll()
		onWork = d.presentIn("t-1")
		return onWork != nil && onWork != pr
	})
	hookFrom(d, onWork, sessionStart("c1", transcript(t, "hi")))
	eventually(t, "the move to settle", func() bool {
		d.mu.Lock()
		defer d.mu.Unlock()
		return d.restoring["t-1"] == nil
	})
	asks := make(chan state.Terminal, 4)
	use(func(e state.Terminal) (string, []string, string) {
		asks <- e
		return bin, nil, ""
	})

	useDefault(t, d, "builtin")
	d.Follow("claude")

	asked := <-asks
	if asked.Account != "" || asked.ConversationID != "c1" {
		t.Fatalf("resumed %+v", asked)
	}
	eventually(t, "the agent on the built-in login", func() bool {
		d.poll()
		next := d.presentIn("t-1")
		return next != nil && next != onWork
	})
	if s := a.Summary(); s.Account != "" || s.ID != "a-1" {
		t.Fatalf("summary = %+v", s)
	}
}

func TestAgentsStayWhenTheDefaultAccountIsSignedOut(t *testing.T) {
	d, _, pr, work, use := followed(t)
	asks := make(chan state.Terminal, 4)
	use(func(e state.Terminal) (string, []string, string) {
		asks <- e
		return "/bin/sh", nil, ""
	})
	os.Remove(filepath.Join(work, ".credentials.json"))
	useDefault(t, d, "work")

	d.Follow("claude")

	select {
	case <-asks:
		t.Fatal("moved to a signed-out account")
	case <-time.After(300 * time.Millisecond):
	}
	if d.presentIn("t-1") != pr {
		t.Fatal("the agent was replaced")
	}
}

func TestAnAgentWithoutATranscriptStays(t *testing.T) {
	d, _, pr, _, use := followed(t)
	asks := make(chan state.Terminal, 4)
	use(func(e state.Terminal) (string, []string, string) {
		asks <- e
		return "/bin/sh", nil, ""
	})
	pr.mu.Lock()
	pr.transcript = filepath.Join(t.TempDir(), "never-written.jsonl")
	pr.mu.Unlock()

	d.followTo("claude", "work")

	select {
	case <-asks:
		t.Fatal("resumed a conversation that has no transcript")
	case <-time.After(300 * time.Millisecond):
	}
}

func TestAMovedAgentLosesFullAccessWithANotice(t *testing.T) {
	d, _, pr, work, use := followed(t)
	pr.mu.Lock()
	pr.launch.Access = "full"
	pr.mu.Unlock()
	use(settleOn(fakeAgent(t, "claude"), work, "--resume", "c1"))
	useDefault(t, d, "work")

	d.Follow("claude")
	var next *presence
	eventually(t, "the agent on the new account", func() bool {
		d.poll()
		next = d.presentIn("t-1")
		return next != nil && next != pr
	})
	hookFrom(d, next, sessionStart("c1", transcript(t, "hi")))

	eventually(t, "the notice", func() bool { return pr.a.Summary().Restore == proto.RestoreAccessLowered })
}

func TestAnAgentTheUserStartedStaysOnItsAccount(t *testing.T) {
	d := newDaemon(t)
	asks := make(chan state.Terminal, 4)
	d.Resume = func(e state.Terminal) (string, []string, string) {
		asks <- e
		return "/bin/sh", nil, ""
	}
	_, pr := claudeIn(t, d)
	hookFrom(d, pr, sessionStart("c1", transcript(t, "hi")))
	hookFrom(d, pr, `{"hook_event_name":"Stop","session_id":"c1"}`)
	if s := pr.a.Summary(); !movable(s, "work") {
		t.Fatalf("summary = %+v", s)
	}

	d.followTo("claude", "work")

	select {
	case <-asks:
		t.Fatal("moved an agent the user started")
	case <-time.After(300 * time.Millisecond):
	}
}
