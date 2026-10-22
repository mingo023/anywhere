//go:build unix

package daemon

import (
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"slices"
	"sync/atomic"
	"testing"
	"time"

	"pocketd/internal/accounts"
	"pocketd/internal/agent"
	"pocketd/internal/proto"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

func TestOnlyAnAgentBetweenTurnsOnAnotherAccountWithAConversationMoves(t *testing.T) {
	for name, c := range map[string]struct {
		s    proto.AgentSummary
		want bool
	}{
		"idle on the old account":     {proto.AgentSummary{Status: "idle", Attached: true, Account: "a", ProviderSessionID: "c1"}, true},
		"done on the old account":     {proto.AgentSummary{Status: "done", Attached: true, Account: "a", ProviderSessionID: "c1"}, true},
		"idle on the one picked":      {proto.AgentSummary{Status: "idle", Attached: true, Account: "b", ProviderSessionID: "c1"}, false},
		"working":                     {proto.AgentSummary{Status: "working", Attached: true, Account: "a", ProviderSessionID: "c1"}, false},
		"asking for permission":       {proto.AgentSummary{Status: "needsYou", Attached: true, Account: "a", ProviderSessionID: "c1"}, false},
		"failed at a limit":           {proto.AgentSummary{Status: "done", Attached: true, Failed: true, Account: "a", ProviderSessionID: "c1"}, true},
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

// listWork makes managed account "work" one of claude's.
func listWork(t *testing.T, d *Daemon) {
	t.Helper()
	index := `{"accounts":[{"id":"work","provider":"claude"}]}`
	if err := os.WriteFile(filepath.Join(d.Accounts.Root, "accounts.json"), []byte(index), 0o600); err != nil {
		t.Fatal(err)
	}
}

// signIn signs claude in at dir with a token named after it.
func signIn(t *testing.T, dir string) {
	t.Helper()
	os.MkdirAll(dir, 0o700)
	creds := `{"claudeAiOauth":{"accessToken":"tok-` + filepath.Base(dir) + `"}}`
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

// switchable is a daemon with a claude on the built-in login that has
// settled in t-1 on transcript; work is the dir of managed account "work";
// use swaps the Resume the daemon calls without racing a switch.
type switchable struct {
	d          *Daemon
	term       *terminal.Terminal
	pr         *presence
	work       string
	transcript string
	use        func(resumeFunc)
}

func onBuiltIn(t *testing.T) switchable {
	t.Helper()
	d := newDaemon(t)
	root := filepath.Join(t.TempDir(), "accounts")
	d.Accounts = &accounts.Store{Root: root, Home: t.TempDir()}
	work := filepath.Join(root, "claude", "work")
	os.MkdirAll(root, 0o700)
	signIn(t, filepath.Join(d.Accounts.Home, ".claude"))
	signIn(t, work)
	listWork(t, d)
	reports := resuming(d, fakeAgent(t, "claude"), "--resume", "c1")
	var current atomic.Pointer[resumeFunc]
	first := d.Resume
	current.Store(&first)
	d.Resume = func(e state.Terminal) (string, []string, string) { return (*current.Load())(e) }
	saved := savedAgent(t.TempDir(), "claude")
	saved.Terminals[0].Status = "idle"
	d.Restore(saved, "/bin/sh")
	term, _ := restoredIn(t, d)
	pr := waitPresent(t, d, term)
	path := transcript(t, "hi")
	hookFrom(d, pr, sessionStart("c1", path))
	report(t, reports)
	return switchable{d, term, pr, work, path, func(f resumeFunc) { current.Store(&f) }}
}

// next waits for the agent in t-1 to run in a process other than pr's.
func (s switchable) next(t *testing.T, pr *presence) *presence {
	t.Helper()
	var next *presence
	eventually(t, "the agent in its new process", func() bool {
		s.d.poll()
		next = s.d.presentIn("t-1")
		return next != nil && next != pr
	})
	return next
}

func (s switchable) settle(t *testing.T, next *presence) {
	t.Helper()
	hookFrom(s.d, next, sessionStart("c1", s.transcript))
	eventually(t, "the move to settle", func() bool {
		s.d.mu.Lock()
		defer s.d.mu.Unlock()
		return s.d.restoring["t-1"] == nil
	})
}

func asking(s switchable, start resumeFunc) <-chan state.Terminal {
	asks := make(chan state.Terminal, 4)
	s.use(func(e state.Terminal) (string, []string, string) {
		asks <- e
		return start(e)
	})
	return asks
}

func noAsk(t *testing.T, asks <-chan state.Terminal, why string) {
	t.Helper()
	select {
	case <-asks:
		t.Fatal(why)
	case <-time.After(300 * time.Millisecond):
	}
}

func TestAnIdleAgentRestartsOnTheAccountPickedInTheSameTerminal(t *testing.T) {
	s := onBuiltIn(t)
	a := s.pr.a
	asks := asking(s, settleOn(fakeAgent(t, "claude"), s.work, "--resume", "c1"))
	if s.d.Running("claude", "work") != 0 {
		t.Fatal("work already runs an agent")
	}

	if err := s.d.Switch("a-1", "work"); err != nil {
		t.Fatal(err)
	}

	asked := <-asks
	if asked.Account != "work" || asked.ConversationID != "c1" || asked.AgentID != "a-1" || asked.TerminalID != "t-1" {
		t.Fatalf("resumed %+v", asked)
	}
	if err := a.Driver().Prompt("hi"); err != agent.ErrResuming {
		t.Fatalf("prompt while moving: %v", err)
	}
	next := s.next(t, s.pr)
	if next.a != a {
		t.Fatal("the moved agent got a new Agent")
	}
	if s.d.Terminals.Get("t-1") != s.term {
		t.Fatal("the terminal was replaced")
	}
	if got, err := s.d.Agents.Get("a-1"); err != nil || got != a {
		t.Fatalf("agent listed as %v, %v", got, err)
	}
	if sum := a.Summary(); sum.Account != "work" || sum.Switching != "" || sum.Status == "closed" {
		t.Fatalf("summary = %+v", sum)
	}
	if s.d.Running("claude", "work") != 1 || s.d.Running("claude", "") != 0 {
		t.Fatalf("running: work %d, builtin %d", s.d.Running("claude", "work"), s.d.Running("claude", ""))
	}

	s.settle(t, next)
	if err := a.Driver().Prompt("hi"); err == agent.ErrResuming {
		t.Fatal("still resuming after its conversation came back")
	}
}

func TestABusyAgentSwitchesOnceItsTurnEnds(t *testing.T) {
	s := onBuiltIn(t)
	asks := asking(s, settleOn(fakeAgent(t, "claude"), s.work, "--resume", "c1"))
	hookFrom(s.d, s.pr, `{"hook_event_name":"UserPromptSubmit","session_id":"c1","prompt":"go on"}`)
	eventually(t, "the turn", func() bool { return s.pr.a.Summary().Status == "working" })

	if err := s.d.Switch("a-1", "work"); err != nil {
		t.Fatal(err)
	}

	if sum := s.pr.a.Summary(); sum.Switching != "work" || sum.Account != "" {
		t.Fatalf("summary = %+v", sum)
	}
	noAsk(t, asks, "moved mid-turn")
	hookFrom(s.d, s.pr, `{"hook_event_name":"Stop","session_id":"c1"}`)
	if asked := <-asks; asked.Account != "work" {
		t.Fatalf("resumed %+v", asked)
	}
}

func TestPickingTheAccountItRunsOnDropsASwitch(t *testing.T) {
	s := onBuiltIn(t)
	asks := asking(s, settleOn(fakeAgent(t, "claude"), s.work, "--resume", "c1"))
	hookFrom(s.d, s.pr, `{"hook_event_name":"UserPromptSubmit","session_id":"c1","prompt":"go on"}`)
	eventually(t, "the turn", func() bool { return s.pr.a.Summary().Status == "working" })
	s.d.Switch("a-1", "work")

	if err := s.d.Switch("a-1", accounts.BuiltIn); err != nil {
		t.Fatal(err)
	}

	if sum := s.pr.a.Summary(); sum.Switching != "" {
		t.Fatalf("summary = %+v", sum)
	}
	hookFrom(s.d, s.pr, `{"hook_event_name":"Stop","session_id":"c1"}`)
	noAsk(t, asks, "moved after all")
}

func TestAnAgentStaysWhenItsResumeIsRefused(t *testing.T) {
	s := onBuiltIn(t)
	s.use(func(state.Terminal) (string, []string, string) { return "", nil, "resume_not_accepted" })

	s.d.Switch("a-1", "work")

	eventually(t, "the switch dropped", func() bool { return s.pr.a.Summary().Switching == "" })
	s.d.poll()
	if s.d.presentIn("t-1") != s.pr || s.term.Info().Cmd == "/bin/sh" {
		t.Fatal("the agent was touched")
	}
}

func TestAMoveThatCannotStartLeavesTheAgentRunning(t *testing.T) {
	s := onBuiltIn(t)
	s.use(func(state.Terminal) (string, []string, string) { return "/no/such/binary", nil, "" })

	s.d.Switch("a-1", "work")

	eventually(t, "the switch dropped", func() bool { return s.pr.a.Summary().Switching == "" })
	s.d.poll()
	if s.d.presentIn("t-1") != s.pr {
		t.Fatal("the agent was replaced")
	}
	s.d.mu.Lock()
	_, listed := s.d.restoring["t-1"]
	s.d.mu.Unlock()
	if listed {
		t.Fatal("hint outlived the failed move")
	}
	if _, ok := s.pr.a.Driver().(termDriver); !ok {
		t.Fatalf("driver = %T", s.pr.a.Driver())
	}
}

func TestAnAgentOnAManagedAccountReturnsToTheBuiltInLogin(t *testing.T) {
	s := onBuiltIn(t)
	a := s.pr.a
	bin := fakeAgent(t, "claude")
	s.use(settleOn(bin, s.work))
	s.d.Switch("a-1", "work")
	onWork := s.next(t, s.pr)
	s.settle(t, onWork)
	asks := asking(s, func(state.Terminal) (string, []string, string) { return bin, nil, "" })

	if err := s.d.Switch("a-1", accounts.BuiltIn); err != nil {
		t.Fatal(err)
	}

	asked := <-asks
	if asked.Account != "" || asked.ConversationID != "c1" {
		t.Fatalf("resumed %+v", asked)
	}
	s.next(t, onWork)
	if sum := a.Summary(); sum.Account != "" || sum.ID != "a-1" {
		t.Fatalf("summary = %+v", sum)
	}
}

func TestASignedOutAccountCannotBePicked(t *testing.T) {
	s := onBuiltIn(t)
	asks := asking(s, func(state.Terminal) (string, []string, string) { return "/bin/sh", nil, "" })
	os.Remove(filepath.Join(s.work, ".credentials.json"))

	if err := s.d.Switch("a-1", "work"); err != errSwitchToOut {
		t.Fatalf("err = %v", err)
	}

	noAsk(t, asks, "moved to a signed-out account")
	if s.pr.a.Summary().Switching != "" {
		t.Fatal("a switch waits")
	}
}

func TestAChatWithNoConversationYetSwitchesOnceItHasOne(t *testing.T) {
	s := onBuiltIn(t)
	asks := asking(s, func(state.Terminal) (string, []string, string) { return "/bin/sh", nil, "" })
	transcript := filepath.Join(t.TempDir(), "first-turn.jsonl")
	s.pr.mu.Lock()
	s.pr.transcript = transcript
	s.pr.mu.Unlock()

	s.d.Switch("a-1", "work")

	noAsk(t, asks, "resumed a conversation that has no transcript")
	if got := s.pr.a.Summary().Switching; got != "work" {
		t.Fatalf("switching %q before the first turn ended", got)
	}
	os.WriteFile(transcript, []byte("{}\n"), 0o600)
	s.d.switchPending(s.pr)
	if asked := <-asks; asked.Account != "work" {
		t.Fatalf("resumed on %q", asked.Account)
	}
}

func TestAMovedAgentKeepsFullAccessWithoutANotice(t *testing.T) {
	s := onBuiltIn(t)
	s.pr.mu.Lock()
	s.pr.launch.Access = "full"
	s.pr.mu.Unlock()
	asks := asking(s, settleOn(fakeAgent(t, "claude"), s.work, "--resume", "c1"))

	s.d.Switch("a-1", "work")

	if asked := <-asks; !asked.KeepAccess || asked.Launch.Access != "full" {
		t.Fatalf("resumed %+v", asked)
	}
	s.settle(t, s.next(t, s.pr))
	if r := s.pr.a.Summary().Restore; r == proto.RestoreAccessLowered {
		t.Fatalf("notice %q", r)
	}
}

func TestAnAgentTheUserStartedCannotSwitch(t *testing.T) {
	d := newDaemon(t)
	d.Accounts = &accounts.Store{Root: t.TempDir(), Home: t.TempDir()}
	asks := make(chan state.Terminal, 4)
	d.Resume = func(e state.Terminal) (string, []string, string) {
		asks <- e
		return "/bin/sh", nil, ""
	}
	_, pr := claudeIn(t, d)
	hookFrom(d, pr, sessionStart("c1", transcript(t, "hi")))
	hookFrom(d, pr, `{"hook_event_name":"Stop","session_id":"c1"}`)

	if err := d.Switch(pr.a.ID(), accounts.BuiltIn); err != errNoSwitch {
		t.Fatalf("err = %v", err)
	}
	d.limitHit(pr)

	noAsk(t, asks, "moved an agent the user started")
}

// usageOf serves claude's usage by token: the built-in login at its 5-hour
// limit, and work at peak.
func usageOf(t *testing.T, peak int) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		used := 100
		if r.Header.Get("Authorization") == "Bearer tok-work" {
			used = peak
		}
		fmt.Fprintf(w, `{"five_hour":{"utilization":%d},"seven_day":{"utilization":5}}`, used)
	}))
	t.Cleanup(srv.Close)
	old := accounts.ClaudeUsageURL
	accounts.ClaudeUsageURL = srv.URL
	t.Cleanup(func() { accounts.ClaudeUsageURL = old })
}

const limitLine = `{"type":"assistant","isApiErrorMessage":true,"error":"rate_limit","message":{"model":"<synthetic>","content":[{"type":"text","text":"You've hit your session limit · resets 9:20am"}]}}`

func TestAnAgentAtItsLimitMovesToTheAccountWithTheMostLeftAndSendsItsPromptAgain(t *testing.T) {
	usageOf(t, 30)
	s := onBuiltIn(t)
	asks := asking(s, settleOn(fakeAgent(t, "claude"), s.work, "--resume", "c1"))

	appendLine(t, s.transcript, limitLine)
	hookFrom(s.d, s.pr, `{"hook_event_name":"StopFailure","session_id":"c1"}`)

	if asked := <-asks; asked.Account != "work" {
		t.Fatalf("resumed %+v", asked)
	}
	next := s.next(t, s.pr)
	s.settle(t, next)
	eventually(t, "the prompt sent again", func() bool {
		next.sending.Lock()
		defer next.sending.Unlock()
		return slices.Contains(next.typing, "hi")
	})
}

func TestAtEveryAccountsLimitTheAgentStays(t *testing.T) {
	usageOf(t, 100)
	s := onBuiltIn(t)
	asks := asking(s, settleOn(fakeAgent(t, "claude"), s.work, "--resume", "c1"))

	appendLine(t, s.transcript, limitLine)
	hookFrom(s.d, s.pr, `{"hook_event_name":"StopFailure","session_id":"c1"}`)

	noAsk(t, asks, "moved with every account at its limit")
}

func TestWithSwitchingAtALimitOffTheAgentStays(t *testing.T) {
	usageOf(t, 30)
	s := onBuiltIn(t)
	s.d.Accounts.SetAutoSwitch(false)
	asks := asking(s, settleOn(fakeAgent(t, "claude"), s.work, "--resume", "c1"))

	appendLine(t, s.transcript, limitLine)
	hookFrom(s.d, s.pr, `{"hook_event_name":"StopFailure","session_id":"c1"}`)

	noAsk(t, asks, "moved with switching off")
}
