//go:build unix

package daemon

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/claude"
	"pocketd/internal/ops"
	"pocketd/internal/peer"
	"pocketd/internal/proc"
	"pocketd/internal/proto"
	"pocketd/internal/terminal"
)

// claudeIn runs claude as the terminal's own process, not as a shell job:
// under load, macOS can fail both of sh's setpgid calls, leaving the job in
// the shell's group while an empty group holds the foreground, where no poll finds it.
func claudeIn(t *testing.T, d *Daemon) (*terminal.Terminal, *presence) {
	t.Helper()
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "claude"), Env: []string{"PATH=/bin:/usr/bin"}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	waitAgent(t, d, term)
	return term, d.presentIn(term.Info().ID)
}

func hookFrom(d *Daemon, pr *presence, payload string) []byte {
	id := pr.t.Info().ID
	out, _ := d.Hook(context.Background(), peer.Principal{Kind: peer.PTY, Pid: pr.pid, Terminal: id}, ops.Msg{ID: id, Data: []byte(payload)})
	return out
}

// userLine is a prompt typed into Claude's own input, which Claude marks with
// entrypoint "cli", stamped now as Claude stamps its lines.
func userLine(prompt string) string {
	return fmt.Sprintf(`{"type":"user","entrypoint":"cli","timestamp":%q,"message":{"role":"user","content":%q}}`, time.Now().UTC().Format(time.RFC3339Nano), prompt)
}

// prompts are texts received at no known time.
func prompts(texts ...string) []received {
	var out []received
	for _, t := range texts {
		out = append(out, received{text: t})
	}
	return out
}

// prompt has pr's claude sent text from the chat.
func prompt(t *testing.T, pr *presence, text string) {
	t.Helper()
	if err := pr.a.Driver().Prompt(text); err != nil {
		t.Fatal(err)
	}
}

func transcript(t *testing.T, prompt string) string {
	path := filepath.Join(t.TempDir(), "s.jsonl")
	os.WriteFile(path, []byte(userLine(prompt)+"\n"), 0o600)
	return path
}

func appendLine(t *testing.T, path, line string) {
	f, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	f.WriteString(line + "\n")
}

func sessionStart(id, path string) string {
	return fmt.Sprintf(`{"hook_event_name":"SessionStart","session_id":%q,"transcript_path":%q,"cwd":"/w","source":"startup","model":"claude-opus-5-5"}`, id, path)
}

func TestSessionStartBindsTheClaudeThatSentIt(t *testing.T) {
	d := newDaemon(t)
	term := shell(t, d)
	term.Write([]byte(fakeAgent(t, "claude") + "\r"))
	eventually(t, "claude", func() bool { return agentPid(term) != 0 })
	pid := agentPid(term)
	hook := func(pid int, payload string) {
		id := term.Info().ID
		d.Hook(context.Background(), peer.Principal{Kind: peer.PTY, Pid: pid, Terminal: id}, ops.Msg{ID: id, Data: []byte(payload)})
	}
	hook(pid, sessionStart("s1", transcript(t, "fix the login bug")))
	a, ok := agentIn(d, term)
	if !ok || a.ProviderSessionID != "s1" || !a.Attached || a.Model != "claude-opus-5-5" || a.Cwd != "/w" {
		t.Fatalf("agent = %+v, %v", a, ok)
	}
	eventually(t, "the transcript", func() bool { a, _ := agentIn(d, term); return a.Title == "fix the login bug" })
	hook(pid+1, sessionStart("s2", transcript(t, "nested")))
	if a, _ := agentIn(d, term); a.ProviderSessionID != "s1" {
		t.Fatalf("a nested claude's hook took the agent: %+v", a)
	}
}

func TestANewSessionStartsTheTimelineOver(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	first := transcript(t, "first")
	hookFrom(d, pr, sessionStart("s1", first))
	eventually(t, "first", func() bool { return pr.a.Summary().Title == "first" })
	hookFrom(d, pr, sessionStart("s2", transcript(t, "second")))
	eventually(t, "second", func() bool { return pr.a.Summary().Title == "second" })
	appendLine(t, first, userLine("late"))
	time.Sleep(300 * time.Millisecond)
	items, _ := pr.a.Timeline.Page(0, 10)
	if s := pr.a.Summary(); s.ProviderSessionID != "s2" || len(items) != 1 || items[0].Text != "second" {
		t.Fatalf("summary = %+v, items = %+v", s, items)
	}
}

func TestACompactKeepsTheTranscriptWhereItWas(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	path := transcript(t, "first")
	hookFrom(d, pr, sessionStart("s1", path))
	eventually(t, "first", func() bool { return pr.a.Summary().Title == "first" })
	hookFrom(d, pr, fmt.Sprintf(`{"hook_event_name":"SessionStart","session_id":"s1","transcript_path":%q,"source":"compact"}`, path))
	appendLine(t, path, userLine("second"))
	var items []proto.Item
	eventually(t, "second", func() bool {
		items, _ = pr.a.Timeline.Page(0, 10)
		return items[len(items)-1].Text == "second"
	})
	if len(items) != 2 {
		t.Fatalf("items = %+v", items)
	}
}

func TestAnEndingClaudeKeepsItsLastTitle(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	path := filepath.Join(t.TempDir(), "s.jsonl")
	os.WriteFile(path, nil, 0o600)
	hookFrom(d, pr, sessionStart("s1", path))
	appendLine(t, path, strings.Repeat(`{"type":"progress"}`+"\n", 20000)+userLine("fix the login bug"))
	d.endAgent(pr)
	if s := pr.a.Summary(); s.Status != "closed" || s.Title != "fix the login bug" {
		t.Fatalf("summary = %+v", s)
	}
}

func TestAnInterruptInTheTranscriptClearsNeedsYou(t *testing.T) {
	const interrupt = `{"type":"user","message":{"content":"[Request interrupted by user for tool use]"}}`
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	path := transcript(t, "hi")
	appendLine(t, path, interrupt)
	appendLine(t, path, `{"type":"assistant","message":{"content":[{"type":"text","text":"Old"}]}}`)
	hookFrom(d, pr, `{"hook_event_name":"PreToolUse"}`)
	hookFrom(d, pr, sessionStart("s1", path))
	eventually(t, "the transcript", func() bool {
		items, _ := pr.a.Timeline.Page(0, 10)
		return len(items) > 0 && items[len(items)-1].Text == "Old"
	})
	if s := pr.a.Summary().Status; s != "needsYou" {
		t.Fatalf("an old interrupt left %s", s)
	}
	appendLine(t, path, interrupt)
	eventually(t, "idle", func() bool { return pr.a.Summary().Status == "idle" })
}

func TestACodexSendsNoClaudeHooks(t *testing.T) {
	d := newDaemon(t)
	home, _ := codexHome(t)
	_, pr := codexIn(t, d, home)
	if out := hookFrom(d, pr, `{"hook_event_name":"UserPromptSubmit"}`); out != nil || pr.a.Summary().Status != "idle" {
		t.Fatalf("replied %s, status %s", out, pr.a.Summary().Status)
	}
}

func TestTheTranscriptLeavesTheStatus(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	hookFrom(d, pr, sessionStart("s1", transcript(t, "hi")))
	eventually(t, "the transcript", func() bool { return pr.a.Summary().Title == "hi" })
	if s := pr.a.Summary().Status; s != "idle" {
		t.Fatalf("status %s", s)
	}
}

func TestHooksDriveTheStatus(t *testing.T) {
	for _, c := range []struct {
		events []string
		want   string
	}{
		{[]string{"UserPromptSubmit"}, "working"},
		{[]string{"UserPromptSubmit", "PreToolUse"}, "needsYou"},
		{[]string{"UserPromptSubmit", "Notification"}, "needsYou"},
		{[]string{"UserPromptSubmit", "PreToolUse", "PostToolUse"}, "working"},
		{[]string{"UserPromptSubmit", "Notification", "PostToolUseFailure"}, "working"},
		{[]string{"UserPromptSubmit", "Notification", "PermissionDenied"}, "working"},
		{[]string{"UserPromptSubmit", "Stop"}, "done"},
		{[]string{"UserPromptSubmit", "StopFailure"}, "done failed"},
		{[]string{"Stop"}, "idle"},
		{[]string{"PreCompact"}, "working compacting"},
		{[]string{"PreCompact", "SessionStart compact"}, "done"},
	} {
		d := newDaemon(t)
		_, pr := claudeIn(t, d)
		for _, e := range c.events {
			event, source, _ := strings.Cut(e, " ")
			if out := hookFrom(d, pr, fmt.Sprintf(`{"hook_event_name":%q,"session_id":"s1","source":%q}`, event, source)); out != nil {
				t.Errorf("%s replied %s", e, out)
			}
		}
		s := pr.a.Summary()
		got := s.Status
		if s.Failed {
			got += " failed"
		}
		if s.Compacting {
			got += " compacting"
		}
		if got != c.want {
			t.Errorf("%v: %s, want %s", c.events, got, c.want)
		}
	}
}

func TestASubagentHookLeavesTheSessionAlone(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	for _, e := range []string{"PermissionRequest", "Notification", "PreToolUse", "PostToolUse"} {
		out := make(chan []byte, 1)
		go func() {
			out <- hookFrom(d, pr, fmt.Sprintf(`{"hook_event_name":%q,"agent_id":"a1","tool_name":"Bash","tool_input":{"command":"ls"}}`, e))
		}()
		select {
		case b := <-out:
			if b != nil {
				t.Errorf("%s replied %s", e, b)
			}
		case <-time.After(time.Second):
			t.Fatalf("%s is held open", e)
		}
	}
	if s := pr.a.Summary().Status; s != "idle" {
		t.Fatalf("status = %s", s)
	}
}

func TestEscOrCtrlCClearsTheTurn(t *testing.T) {
	for _, key := range []string{"\x1b", "\x1b[27u", "\x03", "\x1b[99;5u"} {
		d := newDaemon(t)
		term, pr := claudeIn(t, d)
		hookFrom(d, pr, `{"hook_event_name":"UserPromptSubmit"}`)
		term.Write([]byte(key))
		if s := pr.a.Summary(); s.Status != "idle" {
			t.Errorf("%q: %s", key, s.Status)
		}
	}
}

func TestTypingKeepsTheTurn(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	hookFrom(d, pr, `{"hook_event_name":"UserPromptSubmit"}`)
	for _, b := range []string{"hello", "\r", "\x1b[A", "\x1b\r"} {
		term.Write([]byte(b))
	}
	if s := pr.a.Summary().Status; s != "working" {
		t.Fatalf("status = %s", s)
	}
}

func TestAClaudeWithoutASessionStartIsNotAttached(t *testing.T) {
	defer func(w time.Duration) { ClaudeAttachWait = w }(ClaudeAttachWait)
	ClaudeAttachWait = 100 * time.Millisecond
	d := newDaemon(t)
	_, hooked := claudeIn(t, d)
	hookFrom(d, hooked, sessionStart("s1", transcript(t, "hi")))
	_, silent := claudeIn(t, d)
	time.Sleep(2 * ClaudeAttachWait)
	if !hooked.a.Summary().Attached || silent.a.Summary().Attached {
		t.Fatalf("hooked = %+v, silent = %+v", hooked.a.Summary(), silent.a.Summary())
	}
	hookFrom(d, silent, sessionStart("s2", transcript(t, "trusted")))
	if !silent.a.Summary().Attached {
		t.Fatalf("a late SessionStart left %+v", silent.a.Summary())
	}
}

func TestAnAskIsOpenWhileAPermissionHookWaits(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	id := term.Info().ID
	if d.AskOpen(id) {
		t.Fatal("idle agent has an open ask")
	}
	go hookFrom(d, pr, `{"hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"ls"}}`)
	eventually(t, "open ask", func() bool { return d.AskOpen(id) && len(d.Broker.Open()) == 1 })
	d.Broker.Resolve(d.Broker.Open()[0].RequestID, broker.Answer{Decision: "allow"})
	eventually(t, "ask closed", func() bool { return !d.AskOpen(id) })
}

func TestNearestClaudeOwnsTheHook(t *testing.T) {
	for _, c := range []struct {
		name  string
		chain []proc.Proc
		want  int
	}{
		{"claude's shell", []proc.Proc{{Pid: 30, Argv: []string{"sh", "-c", "pocketd hook"}}, {Pid: 20, Argv: []string{"claude"}}, {Pid: 10, Argv: []string{"-zsh"}}}, 20},
		{"nested claude -p", []proc.Proc{{Pid: 50, Argv: []string{"claude", "-p", "hi"}}, {Pid: 40, Argv: []string{"bash"}}, {Pid: 20, Argv: []string{"claude"}}}, 50},
		{"claude mcp is not claude", []proc.Proc{{Pid: 60, Argv: []string{"claude", "mcp", "serve"}}, {Pid: 20, Argv: []string{"/Users/me/.local/bin/claude"}}}, 20},
		{"no claude", []proc.Proc{{Pid: 10, Argv: []string{"-zsh"}}}, 0},
		{"a chat claude is its pipe's", []proc.Proc{{Pid: 30, Argv: []string{"pocketd", "hook"}}, {Pid: 20, Argv: []string{"/bin/claude", "-p"}}, {Pid: 15, Argv: []string{"/bin/zsh", "-c", `"$@"`}}, {Pid: 12, Argv: []string{"/bin/pocketd", "pipe", "claude", "-p"}}}, 12},
		{"a chat claude's claude -p is its own", []proc.Proc{{Pid: 50, Argv: []string{"claude", "-p", "hi"}}, {Pid: 40, Argv: []string{"bash"}}, {Pid: 20, Argv: []string{"/bin/claude", "-p"}}, {Pid: 12, Argv: []string{"/bin/pocketd", "pipe", "/bin/claude", "-p"}}}, 50},
	} {
		if got := NearestClaude(c.chain); got != c.want {
			t.Errorf("%s: pid %d, want %d", c.name, got, c.want)
		}
	}
}

func hookCode(t *testing.T, d *Daemon, p peer.Principal, id string) string {
	t.Helper()
	_, err := d.Hook(context.Background(), p, ops.Msg{ID: id, Data: []byte(`{"hook_event_name":"PreToolUse"}`)})
	r, ok := errors.AsType[*peer.Refusal](err)
	if !ok {
		t.Fatalf("hook accepted: %v", err)
	}
	return r.Code
}

func TestAHookFromAnotherTerminalIsRefusedAsNotOwnTerminal(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	p := peer.Principal{Kind: peer.PTY, Pid: pr.pid, Terminal: "elsewhere", Scopes: peer.PTYScopes}
	if code := hookCode(t, d, p, term.Info().ID); code != "not_own_terminal" {
		t.Fatalf("code %q", code)
	}
}

func TestAHookWhoseNearestClaudeIsNotInThatTerminalIsRefusedAsHookForged(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	other := exec.Command(fakeAgent(t, "claude"))
	if err := other.Start(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { other.Process.Kill(); other.Wait() })
	id := term.Info().ID
	for name, p := range map[string]peer.Principal{
		"a claude outside the terminal": {Kind: peer.PTY, Pid: other.Process.Pid, Terminal: id},
		"the owner":                     peer.OwnerOf(pr.pid),
	} {
		if code := hookCode(t, d, p, id); code != "hook_forged" {
			t.Errorf("%s: code %q", name, code)
		}
	}
	if s := pr.a.Summary().Status; s == "needsYou" {
		t.Fatal("a refused hook changed the agent")
	}
}

func TestEachTypedPromptIsHeardEvenWhenReportsDriveTheStatus(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	var heard []string
	d.OnPrompt = func(a *agent.Agent, prompt string, env []string) {
		if a == pr.a && slices.Equal(env, pr.env) {
			heard = append(heard, prompt)
		}
	}
	hookFrom(d, pr, `{"hook_event_name":"UserPromptSubmit","prompt":"fix the login"}`)
	reportState(d, pr, "working")
	hookFrom(d, pr, `{"hook_event_name":"UserPromptSubmit","prompt":"/review 12"}`)
	if !slices.Equal(heard, []string{"fix the login", "/review 12"}) {
		t.Fatalf("heard %q", heard)
	}
}

func reportState(d *Daemon, pr *presence, state string) {
	d.Report(pr.t.Info().ID, terminal.Report{State: state, App: "claude-code"})
}

func statusOf(d *Daemon, term *terminal.Terminal) string { a, _ := agentIn(d, term); return a.Status }

func TestEachReportReplacesTheActivity(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	d.Report(pr.t.Info().ID, terminal.Report{State: "working", Msg: "Removing marker file"})
	if a, _ := agentIn(d, term); a.Activity != "Removing marker file" {
		t.Fatalf("activity = %q", a.Activity)
	}
	reportState(d, pr, "done")
	if a, _ := agentIn(d, term); a.Activity != "" {
		t.Fatalf("activity = %q, want cleared: a report replaces its record", a.Activity)
	}
}

func TestAClearReportEmptiesTheActivity(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	d.Report(pr.t.Info().ID, terminal.Report{State: "working", Msg: "Removing marker file"})
	d.Report(pr.t.Info().ID, terminal.Report{State: "clear", Msg: "stale"})
	if a, _ := agentIn(d, term); a.Activity != "" {
		t.Fatalf("activity = %q", a.Activity)
	}
}

func TestReportsDriveTheClaudeStatus(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	for _, c := range []struct{ state, want string }{{"working", "working"}, {"blocked", "needsYou"}, {"working", "working"}, {"done", "done"}, {"working", "working"}, {"idle", "idle"}} {
		reportState(d, pr, c.state)
		if got := statusOf(d, term); got != c.want {
			t.Fatalf("after %s: %s, want %s", c.state, got, c.want)
		}
	}
}

func TestAnErrorReportEndsFailed(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	reportState(d, pr, "working")
	reportState(d, pr, "error")
	if a, _ := agentIn(d, term); a.Status != "done" || !a.Failed {
		t.Fatalf("agent = %+v", a)
	}
}

func TestAWorkingReportDoesNotHideAnOpenPhoneAsk(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	pr.mu.Lock()
	pr.asks["k"] = 1
	pr.mu.Unlock()
	pr.a.NeedsYou()
	reportState(d, pr, "working")
	if got := statusOf(d, term); got != "needsYou" {
		t.Fatalf("status = %s", got)
	}
}

func TestAReportInAnyOtherProcessIsIgnored(t *testing.T) {
	d := newDaemon(t)
	term := shell(t, d)
	d.Report(term.Info().ID, terminal.Report{State: "working"})
	if _, ok := agentIn(d, term); ok {
		t.Fatal("a shell got an agent")
	}
}

func TestHooksStopDrivingStatusOnceTheClaudeReports(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	hookFrom(d, pr, `{"hook_event_name":"UserPromptSubmit"}`)
	if got := statusOf(d, term); got != "working" {
		t.Fatalf("before a report: %s", got)
	}
	reportState(d, pr, "idle")
	hookFrom(d, pr, `{"hook_event_name":"UserPromptSubmit"}`)
	hookFrom(d, pr, `{"hook_event_name":"Stop"}`)
	if got := statusOf(d, term); got != "idle" {
		t.Fatalf("after a report: %s (a Stop hook must not make it Done)", got)
	}
}

func TestEscDoesNotClearAReportingClaude(t *testing.T) {
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	reportState(d, pr, "working")
	d.Input(term.Info().ID, []byte("\x1b"))
	if got := statusOf(d, term); got != "working" {
		t.Fatalf("status = %s; the idle report clears it, not the key", got)
	}
}

func TestAReportKeepsAClaudeAttachedWithoutAHook(t *testing.T) {
	defer func(w time.Duration) { ClaudeAttachWait = w }(ClaudeAttachWait)
	ClaudeAttachWait = 100 * time.Millisecond
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	reportState(d, pr, "idle")
	time.Sleep(2 * ClaudeAttachWait)
	if a, _ := agentIn(d, term); !a.Attached {
		t.Fatal("a reporting claude was marked not attached")
	}
}

func TestAClaudeStartsInTheStateItAlreadyReported(t *testing.T) {
	d := newDaemon(t)
	term, applied := reportingClaude(t, d, "working")
	waitApplied(t, applied, "working")
	if _, ok := agentIn(d, term); ok {
		t.Fatal("the agent was found before the poll")
	}
	waitAgent(t, d, term)
	if got := statusOf(d, term); got != "working" {
		t.Fatalf("status = %s", got)
	}
}

func printReport(term *terminal.Terminal, fields string) {
	term.Write([]byte(`printf '\033]7501;` + fields + `\033\\'` + "\r"))
}

func TestANewClaudeDoesNotInheritTheReportOfOneThatEnded(t *testing.T) {
	d := newDaemon(t)
	term := shell(t, d)
	claude := fakeAgent(t, "claude")
	printReport(term, "state=working:app=claude-code")
	eventually(t, "the report", func() bool { return term.Program() != nil })
	term.Write([]byte(claude + "\r"))
	first := waitAgent(t, d, term)
	if first.Status != "working" {
		t.Fatalf("the first claude: %s, want working from the report that beat the poll", first.Status)
	}
	term.Write([]byte{0x03})
	waitGone(t, d, first.ID)
	term.Write([]byte(claude + "\r"))
	second := waitAgent(t, d, term)
	if second.Status != "idle" || d.presentIn(term.Info().ID).isReported() {
		t.Fatalf("the second claude started from the first's report: %+v", second)
	}
}

func TestOnlyAClaudeCodeReportIsReplayedIntoANewClaude(t *testing.T) {
	d := newDaemon(t)
	term := shell(t, d)
	printReport(term, "state=working:app=other")
	eventually(t, "the report", func() bool { return term.Program() != nil })
	term.Write([]byte(fakeAgent(t, "claude") + "\r"))
	if a := waitAgent(t, d, term); a.Status != "idle" {
		t.Fatalf("status = %s, want a report from another app left unreplayed", a.Status)
	}
}

func reportingClaude(t *testing.T, d *Daemon, states string) (*terminal.Terminal, <-chan string) {
	t.Helper()
	applied := make(chan string, 8)
	d.Terminals.OnReport = func(id string, r terminal.Report) {
		d.Report(id, r)
		applied <- r.State
	}
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "claude"), Env: []string{"PATH=/bin:/usr/bin", "FAKE_AGENT_REPORT=" + states}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	return term, applied
}

func waitApplied(t *testing.T, applied <-chan string, state string) {
	t.Helper()
	select {
	case got := <-applied:
		if got != state {
			t.Fatalf("report %q, want %q", got, state)
		}
	case <-time.After(5 * time.Second):
		t.Fatalf("no %q report", state)
	}
}

func TestAPhoneAskAnsweredAtTheDeskFollowsTheClaudesLastReport(t *testing.T) {
	d := newDaemon(t)
	term, applied := reportingClaude(t, d, "blocked working")
	waitApplied(t, applied, "blocked")
	waitAgent(t, d, term)
	pr := d.presentIn(term.Info().ID)
	out := make(chan []byte, 1)
	go func() {
		out <- hookFrom(d, pr, `{"hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"ls"}}`)
	}()
	eventually(t, "open request", func() bool { return len(d.Broker.Open()) == 1 })
	term.Write([]byte("go\n"))
	waitApplied(t, applied, "working")
	if got := statusOf(d, term); got != "needsYou" {
		t.Fatalf("with the ask open: %s", got)
	}
	d.Broker.Dismiss(permissionKey(pr.a.ID(), "Bash", json.RawMessage(`{"command":"ls"}`)), "allow")
	<-out
	if got := statusOf(d, term); got != "working" {
		t.Fatalf("after the desk answered: %s, want the claude's last report", got)
	}
}

func results(pr *presence) []proto.Item {
	items, _ := pr.a.Timeline.Page(0, 100)
	var out []proto.Item
	for _, it := range items {
		if it.Kind == "result" {
			out = append(out, it)
		}
	}
	return out
}

func TestAStopBeforeAnyReplyEndsTheTurnInterrupted(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	hookFrom(d, pr, sessionStart("s1", transcript(t, "write a story")))
	eventually(t, "the prompt", func() bool { return pr.a.Summary().Title == "write a story" })
	reportState(d, pr, "working")
	pr.t.Write([]byte("\x1b"))
	reportState(d, pr, "idle")
	eventually(t, "an interrupted end", func() bool {
		r := results(pr)
		return len(r) == 1 && r[0].Error == "interrupted"
	})
}

func TestAStopFromAClientBeforeAnyReplyEndsTheTurnInterrupted(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	hookFrom(d, pr, sessionStart("s1", transcript(t, "write a story")))
	eventually(t, "the prompt", func() bool { return pr.a.Summary().Title == "write a story" })
	reportState(d, pr, "blocked")
	if err := pr.a.Driver().Interrupt(); err != nil {
		t.Fatal(err)
	}
	reportState(d, pr, "idle")
	eventually(t, "an interrupted end", func() bool {
		r := results(pr)
		return len(r) == 1 && r[0].Error == "interrupted"
	})
}

// noResultsAfter fails when pr's timeline ends a turn within a few settles.
func noResultsAfter(t *testing.T, pr *presence) {
	t.Helper()
	time.Sleep(4 * StopSettle)
	if r := results(pr); len(r) != 0 {
		t.Fatalf("results = %+v", r)
	}
}

func TestAnIdleAfterATurnThatEndedOnItsOwnEndsNothing(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	path := transcript(t, "write a story")
	hookFrom(d, pr, sessionStart("s1", path))
	reportState(d, pr, "working")
	appendLine(t, path, `{"type":"assistant","message":{"content":[{"type":"text","text":"Once"}]}}`)
	eventually(t, "the reply", func() bool { _, seq := pr.a.Timeline.State(); return seq == 2 })
	reportState(d, pr, "idle")
	noResultsAfter(t, pr)
}

func TestAnEscWhileIdleEndsNothing(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	hookFrom(d, pr, sessionStart("s1", transcript(t, "write a story")))
	eventually(t, "the prompt", func() bool { return pr.a.Summary().Title == "write a story" })
	pr.t.Write([]byte("\x1b"))
	noResultsAfter(t, pr)
}

func TestAStopAfterTheReplyStartedLeavesTheEndToClaude(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	path := transcript(t, "write a story")
	hookFrom(d, pr, sessionStart("s1", path))
	reportState(d, pr, "working")
	appendLine(t, path, `{"type":"assistant","message":{"content":[{"type":"text","text":"Once"}]}}`)
	eventually(t, "the reply", func() bool { _, seq := pr.a.Timeline.State(); return seq == 2 })
	pr.t.Write([]byte("\x1b"))
	reportState(d, pr, "idle")
	noResultsAfter(t, pr)
}

func TestABashModeCommandStartsATurnOfItsOwn(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	path := transcript(t, "write a story")
	hookFrom(d, pr, sessionStart("s1", path))
	appendLine(t, path, `{"type":"system","subtype":"turn_duration","durationMs":7}`)
	appendLine(t, path, `{"type":"user","uuid":"b1","message":{"role":"user","content":"<bash-input>echo hi</bash-input>"}}`)
	appendLine(t, path, `{"type":"user","parentUuid":"b1","message":{"role":"user","content":"<bash-stdout>hi</bash-stdout><bash-stderr></bash-stderr>"}}`)
	appendLine(t, path, `{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}`)
	appendLine(t, path, `{"type":"system","subtype":"turn_duration","durationMs":9}`)
	eventually(t, "the second turn end", func() bool { return len(results(pr)) == 2 })
	items, _ := pr.a.Timeline.Page(0, 100)
	var kinds []string
	for _, it := range items {
		kinds = append(kinds, it.Kind)
	}
	if !slices.Equal(kinds, []string{"user", "result", "user", "tool", "assistant", "result"}) || items[2].Text != "! echo hi" || items[3].Call.Status != "ok" {
		t.Fatalf("items = %+v", items)
	}
}

// typed collects what reaches the terminals of d's claude, so a test sees each write.
func typed(d *Daemon) func() string {
	var mu sync.Mutex
	var b strings.Builder
	d.Terminals.OnInput = func(id string, in []byte) {
		mu.Lock()
		b.Write(in)
		mu.Unlock()
		d.Input(id, in)
	}
	return func() string {
		mu.Lock()
		defer mu.Unlock()
		return b.String()
	}
}

func TestAPromptAfterAStopClearsTheInputClaudeRefilled(t *testing.T) {
	d := newDaemon(t)
	got := typed(d)
	_, pr := claudeIn(t, d)
	reportState(d, pr, "working")
	pr.t.Write([]byte("\x1b"))
	reportState(d, pr, "idle")
	if err := pr.a.Driver().Prompt("next"); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(got(), strings.Repeat("\x15", 16)+"next") {
		t.Fatalf("typed %q", got())
	}
	if err := pr.a.Driver().Prompt("then"); err != nil {
		t.Fatal(err)
	}
	if strings.Count(got(), "\x15") != 64 {
		t.Fatalf("the prompt after cleared again: %q", got())
	}
}

func TestAPlainPromptTypesNoClear(t *testing.T) {
	d := newDaemon(t)
	got := typed(d)
	_, pr := claudeIn(t, d)
	if err := pr.a.Driver().Prompt("hi"); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(got(), "hi") || strings.Contains(got(), "\x15") {
		t.Fatalf("typed %q", got())
	}
}

func TestAPromptStashesWhatTheUserLeftInClaudesInput(t *testing.T) {
	d := newDaemon(t)
	got := typed(d)
	_, pr := claudeIn(t, d)
	pr.t.Write([]byte("draft"))
	if err := pr.a.Driver().Prompt("next"); err != nil {
		t.Fatal(err)
	}
	if !strings.HasSuffix(got(), "draft\x13next") {
		t.Fatalf("typed %q", got())
	}
	// Claude puts the stash back once the prompt is sent.
	if err := pr.a.Driver().Prompt("then"); err != nil {
		t.Fatal(err)
	}
	if !strings.HasSuffix(got(), "\r\x13then") {
		t.Fatalf("the prompt after kept the draft: %q", got())
	}
}

// The desktop sends a paste, and a file dropped on the terminal, bracketed.
func TestAPromptStashesWhatTheUserPastedIntoClaudesInput(t *testing.T) {
	d := newDaemon(t)
	got := typed(d)
	_, pr := claudeIn(t, d)
	pr.t.Write([]byte("\x1b[200~/Users/me/shot.png\x1b[201~"))
	prompt(t, pr, "next")
	if !strings.HasSuffix(got(), "\x1b[201~\x13next") {
		t.Fatalf("typed %q", got())
	}
}

func TestAnEmptyPasteLeavesNoDraft(t *testing.T) {
	d := newDaemon(t)
	got := typed(d)
	_, pr := claudeIn(t, d)
	pr.t.Write([]byte("\x1b[200~\x1b[201~"))
	prompt(t, pr, "next")
	if strings.Contains(got(), "\x13") {
		t.Fatalf("typed %q", got())
	}
}

func TestAPromptAfterTheUserSentAStoppedPromptClearsNothing(t *testing.T) {
	d := newDaemon(t)
	got := typed(d)
	_, pr := claudeIn(t, d)
	reportState(d, pr, "working")
	pr.t.Write([]byte("\x1b"))
	reportState(d, pr, "idle")
	pr.t.Write([]byte("\r"))
	reportState(d, pr, "working")
	reportState(d, pr, "done")
	prompt(t, pr, "next")
	if strings.Contains(got(), "\x15") {
		t.Fatalf("typed %q", got())
	}
}

func TestAPromptAfterAStopPutsAsideWhatTheUserTypedThere(t *testing.T) {
	d := newDaemon(t)
	got := typed(d)
	_, pr := claudeIn(t, d)
	reportState(d, pr, "working")
	pr.t.Write([]byte("\x1b"))
	reportState(d, pr, "idle")
	pr.t.Write([]byte(" and more"))
	prompt(t, pr, "next")
	if strings.Contains(got(), "\x15") || !strings.HasSuffix(got(), "\x13next") {
		t.Fatalf("typed %q", got())
	}
}

func TestAnOldStopDoesNotWipeALaterDraft(t *testing.T) {
	d := newDaemon(t)
	got := typed(d)
	_, pr := claudeIn(t, d)
	reportState(d, pr, "working")
	pr.t.Write([]byte("\x1b"))
	reportState(d, pr, "idle")
	pr.t.Write([]byte("redo it"))
	pr.t.Write([]byte("\r"))
	reportState(d, pr, "working")
	reportState(d, pr, "done")
	pr.t.Write([]byte("my half written draft"))
	prompt(t, pr, "next")
	if strings.Contains(got(), "\x15") || !strings.HasSuffix(got(), "draft\x13next") {
		t.Fatalf("typed %q", got())
	}
}

func TestAPromptAfterTheUserSentTheirInputStashesNothing(t *testing.T) {
	d := newDaemon(t)
	got := typed(d)
	_, pr := claudeIn(t, d)
	pr.t.Write([]byte("draft"))
	pr.t.Write([]byte("\r"))
	if err := pr.a.Driver().Prompt("next"); err != nil {
		t.Fatal(err)
	}
	if strings.Contains(got(), "\x13") {
		t.Fatalf("typed %q", got())
	}
}

func TestAStopClaudeMarksEndsTheTurnOnce(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	path := transcript(t, "write a story")
	hookFrom(d, pr, sessionStart("s1", path))
	reportState(d, pr, "working")
	appendLine(t, path, `{"type":"assistant","message":{"content":[{"type":"text","text":"Once"}]}}`)
	appendLine(t, path, `{"type":"user","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}}`)
	eventually(t, "Claude's mark", func() bool { return len(results(pr)) == 1 })
	reportState(d, pr, "idle")
	time.Sleep(4 * StopSettle)
	if r := results(pr); len(r) != 1 {
		t.Fatalf("results = %+v", r)
	}
}

// hookless gives the test a HOME of its own and a cwd reached through a
// symlink, like /tmp on macOS, and looks for transcripts often.
func hookless(t *testing.T) (home, cwd string) {
	w := DiscoverEvery
	t.Cleanup(func() { DiscoverEvery = w })
	DiscoverEvery = 50 * time.Millisecond
	home = t.TempDir()
	t.Setenv("HOME", home)
	cwd = filepath.Join(t.TempDir(), "link")
	if err := os.Symlink(t.TempDir(), cwd); err != nil {
		t.Fatal(err)
	}
	return home, cwd
}

func claudeInDir(t *testing.T, d *Daemon, cwd string, args ...string) *presence {
	t.Helper()
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "claude"), Args: args, Cwd: cwd, Env: []string{"PATH=/bin:/usr/bin"}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	waitAgent(t, d, term)
	return d.presentIn(term.Info().ID)
}

// writeSession writes transcript id where Claude keeps cwd's: a dir named
// after the resolved path, every character but a letter or digit a dash.
func writeSession(t *testing.T, home, cwd, id string, lines ...string) {
	t.Helper()
	real, err := filepath.EvalSymlinks(cwd)
	if err != nil {
		t.Fatal(err)
	}
	dir := filepath.Join(home, ".claude", "projects", regexp.MustCompile(`[^A-Za-z0-9]`).ReplaceAllString(real, "-"))
	os.MkdirAll(dir, 0o700)
	if err := os.WriteFile(filepath.Join(dir, id+".jsonl"), []byte(strings.Join(lines, "\n")+"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
}

const clearLine = `{"type":"user","entrypoint":"cli","message":{"role":"user","content":"<command-name>/clear</command-name>\n            <command-message>clear</command-message>\n            <command-args></command-args>"}}`

func appendSession(t *testing.T, home, cwd, id, line string) {
	t.Helper()
	real, err := filepath.EvalSymlinks(cwd)
	if err != nil {
		t.Fatal(err)
	}
	appendLine(t, filepath.Join(home, ".claude", "projects", regexp.MustCompile(`[^A-Za-z0-9]`).ReplaceAllString(real, "-"), id+".jsonl"), line)
}

func sessionOf(pr *presence) string { return pr.a.Summary().ProviderSessionID }

func TestAClaudeWithoutHooksBindsTheOneTranscriptItStarted(t *testing.T) {
	home, cwd := hookless(t)
	writeSession(t, home, cwd, "old", userLine("an older session"))
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd, "--", "hi")
	writeSession(t, home, cwd, "s1", `{"type":"mode","mode":"normal","sessionId":"s1"}`, userLine("hi"))
	eventually(t, "s1 bound", func() bool { return sessionOf(pr) == "s1" && pr.a.Summary().Title == "hi" })
}

func TestAClaudeWithoutHooksBindsTheTranscriptOfAPromptTypedInItsTerminal(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd)
	pr.t.Write([]byte("fix the build"))
	pr.t.Write([]byte("\r"))
	writeSession(t, home, cwd, "s1", userLine("fix the build"))
	eventually(t, "s1 bound", func() bool { return sessionOf(pr) == "s1" })
}

func TestAClearAndAPromptTypedInTheTerminalMoveAClaudeWithoutHooks(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd)
	prompt(t, pr, "hi")
	writeSession(t, home, cwd, "s1", userLine("hi"))
	eventually(t, "s1 bound", func() bool { return sessionOf(pr) == "s1" })
	pr.t.Write([]byte("/clear"))
	pr.t.Write([]byte("\r"))
	pr.t.Write([]byte("something new"))
	pr.t.Write([]byte("\r"))
	writeSession(t, home, cwd, "s2", clearLine, userLine("something new"))
	eventually(t, "s2 bound", func() bool { return sessionOf(pr) == "s2" })
}

// Claude writes the transcript a /clear starts at once, before any prompt,
// and only that prompt tells it from another claude's /clear.
func TestAClearMovesAClaudeWithoutHooksOnceItsNextPromptIsIn(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd)
	prompt(t, pr, "hi")
	writeSession(t, home, cwd, "s1", userLine("hi"))
	eventually(t, "s1 bound", func() bool { return sessionOf(pr) == "s1" })
	prompt(t, pr, "/clear")
	writeSession(t, home, cwd, "s2", `{"type":"user","isMeta":true,"entrypoint":"cli","message":{"role":"user","content":"<local-command-caveat>Caveat</local-command-caveat>"}}`, clearLine)
	time.Sleep(10 * DiscoverEvery)
	if s := sessionOf(pr); s != "s1" {
		t.Fatalf("moved to %q before its prompt", s)
	}
	prompt(t, pr, "next")
	appendSession(t, home, cwd, "s2", userLine("next"))
	eventually(t, "s2 bound", func() bool {
		items, _ := pr.a.Timeline.Page(0, 10)
		return sessionOf(pr) == "s2" && len(items) == 1 && items[0].Text == "next"
	})
}

func TestAClaudeWithoutHooksDoesNotGuessBetweenTwoNewTranscriptsOpeningWithItsPrompt(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd)
	prompt(t, pr, "one")
	writeSession(t, home, cwd, "s1", userLine("one"))
	writeSession(t, home, cwd, "s2", userLine("one"))
	time.Sleep(10 * DiscoverEvery)
	if s := sessionOf(pr); s != "" {
		t.Fatalf("guessed %q", s)
	}
}

func TestAClaudeWithoutHooksTakesNoTranscriptItSentNothingTo(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	a, b := claudeInDir(t, d, cwd), claudeInDir(t, d, cwd)
	writeSession(t, home, cwd, "s1", userLine("whose"))
	time.Sleep(10 * DiscoverEvery)
	if sessionOf(a) != "" || sessionOf(b) != "" {
		t.Fatalf("guessed a = %q, b = %q", sessionOf(a), sessionOf(b))
	}
}

func TestTwoClaudesInOneFolderWithoutHooksEachBindTheTranscriptOfTheirOwnPrompt(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	a, b := claudeInDir(t, d, cwd), claudeInDir(t, d, cwd)
	prompt(t, a, "for a")
	prompt(t, b, "for b")
	writeSession(t, home, cwd, "sb", userLine("for b"))
	writeSession(t, home, cwd, "nobody", userLine("whose"))
	writeSession(t, home, cwd, "sa", userLine("for a"))
	eventually(t, "each bound", func() bool { return sessionOf(a) == "sa" && sessionOf(b) == "sb" })
}

// Claude names a project's folder after its path with every character but a
// letter or digit a dash, so /x/my_app and /x/my-app share one.
func TestTwoClaudesWhoseFoldersClaudeNamesTheSameBindTheirOwnTranscripts(t *testing.T) {
	home, _ := hookless(t)
	base := t.TempDir()
	cwdA, cwdB := filepath.Join(base, "my_app"), filepath.Join(base, "my-app")
	os.Mkdir(cwdA, 0o700)
	os.Mkdir(cwdB, 0o700)
	d := newDaemon(t)
	a, b := claudeInDir(t, d, cwdA), claudeInDir(t, d, cwdB)
	prompt(t, a, "A's first prompt")
	prompt(t, b, "B's first prompt")
	writeSession(t, home, cwdB, "sb", userLine("B's first prompt"))
	time.Sleep(5 * DiscoverEvery)
	if s := sessionOf(a); s != "" {
		t.Fatalf("a took %q", s)
	}
	eventually(t, "b bound", func() bool { return sessionOf(b) == "sb" })
	writeSession(t, home, cwdA, "sa", userLine("A's first prompt"))
	eventually(t, "a bound", func() bool { return sessionOf(a) == "sa" && sessionOf(b) == "sb" })
}

// The desktop's commit message and pull request description run claude -p
// in the agent's folder; Claude marks theirs with entrypoint "sdk-cli".
func TestAClaudePRunInTheFolderIsNotTakenForTheAgent(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd)
	prompt(t, pr, "fix the login bug")
	writeSession(t, home, cwd, "commitmsg",
		`{"type":"queue-operation","operation":"enqueue"}`,
		`{"type":"user","entrypoint":"sdk-cli","message":{"role":"user","content":"Write a git commit message for the diff on stdin. Reply with the message only."}}`)
	time.Sleep(5 * DiscoverEvery)
	if s := sessionOf(pr); s != "" {
		t.Fatalf("took %q", s)
	}
	writeSession(t, home, cwd, "mine", userLine("fix the login bug"))
	eventually(t, "mine bound", func() bool { return sessionOf(pr) == "mine" && pr.a.Summary().Title == "fix the login bug" })
}

func TestAClaudePRunWithTheAgentsPromptIsNotTakenForIt(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd)
	prompt(t, pr, "fix the login bug")
	writeSession(t, home, cwd, "sdk", `{"type":"user","entrypoint":"sdk-cli","message":{"role":"user","content":"fix the login bug"}}`)
	time.Sleep(10 * DiscoverEvery)
	if s := sessionOf(pr); s != "" {
		t.Fatalf("took %q", s)
	}
}

func TestAnAgentThatSawTwoNewTranscriptsAtOnceBindsTheOneItsPromptOpens(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd)
	prompt(t, pr, "mine")
	writeSession(t, home, cwd, "s1", userLine("mine"))
	writeSession(t, home, cwd, "p1", userLine("another claude's"))
	eventually(t, "s1 bound", func() bool { return sessionOf(pr) == "s1" })
}

func TestAnAgentThatTookNoneOfTwoNewTranscriptsBindsTheOneAClearStarts(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd)
	writeSession(t, home, cwd, "s1", userLine("typed at the terminal"))
	writeSession(t, home, cwd, "p1", userLine("another claude's"))
	time.Sleep(5 * DiscoverEvery)
	prompt(t, pr, "/clear")
	prompt(t, pr, "after clear")
	writeSession(t, home, cwd, "s2", clearLine, userLine("after clear"))
	eventually(t, "s2 bound", func() bool { return sessionOf(pr) == "s2" })
}

func TestAnAgentThatTookNoneOfTwoNewTranscriptsBindsTheOneAClearTypedInTheTerminalStarts(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd)
	writeSession(t, home, cwd, "s1", userLine("typed at the terminal"))
	writeSession(t, home, cwd, "p1", userLine("another claude's"))
	time.Sleep(5 * DiscoverEvery)
	for _, k := range []string{"/clear", "\r", "after clear", "\r"} {
		pr.t.Write([]byte(k))
	}
	writeSession(t, home, cwd, "s2", clearLine, userLine("after clear"))
	eventually(t, "s2 bound", func() bool { return sessionOf(pr) == "s2" })
}

func TestAHookOutranksTheTranscriptFoundWithoutOne(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd)
	prompt(t, pr, "found")
	writeSession(t, home, cwd, "s1", userLine("found"))
	eventually(t, "s1 bound", func() bool { return sessionOf(pr) == "s1" })
	prompt(t, pr, "/clear")
	prompt(t, pr, "after clear")
	hookFrom(d, pr, fmt.Sprintf(`{"hook_event_name":"SessionStart","session_id":"s2","transcript_path":%q,"cwd":%q,"source":"startup"}`, transcript(t, "hooked"), cwd))
	eventually(t, "s2 bound", func() bool { return pr.a.Summary().Title == "hooked" })
	writeSession(t, home, cwd, "s3", clearLine, userLine("after clear"))
	time.Sleep(10 * DiscoverEvery)
	if s := pr.a.Summary(); s.ProviderSessionID != "s2" || s.Title != "hooked" {
		t.Fatalf("summary = %+v", s)
	}
}

func TestAClearMovesAClaudeWithoutHooksToItsNewTranscript(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd)
	prompt(t, pr, "before clear")
	writeSession(t, home, cwd, "s1", userLine("before clear"))
	eventually(t, "s1 bound", func() bool { return sessionOf(pr) == "s1" })
	prompt(t, pr, "again")
	writeSession(t, home, cwd, "nested", userLine("again"))
	time.Sleep(10 * DiscoverEvery)
	if s := sessionOf(pr); s != "s1" {
		t.Fatalf("moved to %q, which no /clear started", s)
	}
	prompt(t, pr, "/clear")
	prompt(t, pr, "after clear")
	writeSession(t, home, cwd, "s2", clearLine)
	time.Sleep(5 * DiscoverEvery)
	appendSession(t, home, cwd, "s2", userLine("after clear"))
	eventually(t, "s2 bound", func() bool {
		items, _ := pr.a.Timeline.Page(0, 10)
		return sessionOf(pr) == "s2" && len(items) == 1 && items[0].Text == "after clear"
	})
}

// A /clear typed at the terminal is one pocketd doesn't see; with another
// claude in the folder, the prompts its transcript showed before it can't
// open the next one.
func TestAClearTypedAtTheTerminalMovesToTheTranscriptOfThePromptSentAfter(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd)
	claudeInDir(t, d, cwd)
	prompt(t, pr, "first")
	writeSession(t, home, cwd, "s1", userLine("first"))
	eventually(t, "s1 bound", func() bool { return sessionOf(pr) == "s1" })
	writeSession(t, home, cwd, "again", clearLine, userLine("first"))
	time.Sleep(10 * DiscoverEvery)
	if s := sessionOf(pr); s != "s1" {
		t.Fatalf("moved to %q, which opens with a prompt s1 already showed", s)
	}
	prompt(t, pr, "second")
	writeSession(t, home, cwd, "s2", clearLine, userLine("second"))
	eventually(t, "s2 bound", func() bool { return sessionOf(pr) == "s2" })
}

// closeAgent ends pr as the watch loop ends a closed terminal's agent.
func closeAgent(t *testing.T, d *Daemon, pr *presence) {
	t.Helper()
	id := pr.t.Info().ID
	pr.t.Close()
	d.endAgent(pr)
	eventually(t, "agent ended", func() bool { return d.presentIn(id) == nil })
}

func TestABoundAgentDoesNotMoveToTheClearedTranscriptOfAClosedAgent(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	a := claudeInDir(t, d, cwd)
	prompt(t, a, "a first")
	writeSession(t, home, cwd, "sa", userLine("a first"))
	eventually(t, "a on sa", func() bool { return sessionOf(a) == "sa" })
	b := claudeInDir(t, d, cwd)
	prompt(t, b, "b first")
	writeSession(t, home, cwd, "sb1", userLine("b first"))
	eventually(t, "b on sb1", func() bool { return sessionOf(b) == "sb1" })
	prompt(t, b, "/clear")
	prompt(t, b, "b second")
	writeSession(t, home, cwd, "sb2", clearLine, userLine("b second"))
	eventually(t, "b on sb2", func() bool { return sessionOf(b) == "sb2" })
	closeAgent(t, d, b)
	time.Sleep(10 * DiscoverEvery)
	if s := sessionOf(a); s != "sa" {
		t.Fatalf("a moved to %q, the closed agent's transcript", s)
	}
}

func TestAnAgentSentNothingDoesNotTakeTheTranscriptOfAClosedAgent(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	a, b := claudeInDir(t, d, cwd), claudeInDir(t, d, cwd)
	prompt(t, b, "b first")
	writeSession(t, home, cwd, "sb1", userLine("b first"))
	eventually(t, "b on sb1", func() bool { return sessionOf(b) == "sb1" })
	closeAgent(t, d, b)
	time.Sleep(10 * DiscoverEvery)
	if s := sessionOf(a); s != "" {
		t.Fatalf("a took %q, the closed agent's transcript", s)
	}
}

func TestABoundAgentDoesNotMoveToTheClearOfAClaudeOutsidePocket(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	a := claudeInDir(t, d, cwd)
	prompt(t, a, "a first")
	writeSession(t, home, cwd, "sa", userLine("a first"))
	eventually(t, "a on sa", func() bool { return sessionOf(a) == "sa" })
	writeSession(t, home, cwd, "ext", clearLine, userLine("work done in iTerm"))
	time.Sleep(10 * DiscoverEvery)
	if s := sessionOf(a); s != "sa" {
		t.Fatalf("a moved to %q, an outside claude's transcript", s)
	}
}

func TestAnAgentLaunchedWithAPromptBesideAnotherBindsItsOwnTranscript(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	other := claudeInDir(t, d, cwd)
	prompt(t, other, "other first")
	writeSession(t, home, cwd, "so", userLine("other first"))
	eventually(t, "other on so", func() bool { return sessionOf(other) == "so" })
	b := claudeInDir(t, d, cwd, "-n", "add-a-dark-mode", "--", "add a dark mode toggle")
	writeSession(t, home, cwd, "sb", userLine("add a dark mode toggle"))
	eventually(t, "b on sb", func() bool { return sessionOf(b) == "sb" && sessionOf(other) == "so" })
}

func TestAPromptTypedInTheTerminalBesideAnotherAgentBindsItsOwnTranscript(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	other := claudeInDir(t, d, cwd)
	pr := claudeInDir(t, d, cwd)
	for _, k := range []string{"f", "ix", " teh", "\x7f\x7f", "he build", "\x1b[200~ now\x1b[201~", "\r"} {
		d.Input(pr.t.Info().ID, []byte(k))
	}
	writeSession(t, home, cwd, "s1", userLine("fix the build now"))
	eventually(t, "s1 bound", func() bool { return sessionOf(pr) == "s1" })
	if s := sessionOf(other); s != "" {
		t.Fatalf("other took %q", s)
	}
}

func TestANewSessionIsNoneAnotherAgentFollows(t *testing.T) {
	hi := claude.Opening{Prompt: "hi", Interactive: true}
	opens := map[string]claude.Opening{"/p/old.jsonl": hi, "/p/theirs.jsonl": hi, "/p/mine.jsonl": hi}
	known := map[string]bool{"/p/old.jsonl": true}
	taken := map[string]bool{"/p/theirs.jsonl": true}
	if got := newSession([]string{"/p/old.jsonl", "/p/theirs.jsonl", "/p/mine.jsonl"}, known, taken, opens, claimant{sent: prompts("hi")}, nil); got != "/p/mine.jsonl" {
		t.Fatalf("got %q", got)
	}
	if got := newSession([]string{"/p/theirs.jsonl"}, known, taken, opens, claimant{sent: prompts("hi")}, nil); got != "" {
		t.Fatalf("took %q", got)
	}
}

func TestANewSessionIsUpToAPromptClaudeReceived(t *testing.T) {
	paths := []string{"/p/s.jsonl"}
	for _, c := range []struct {
		open  claude.Opening
		sent  []string
		bound bool
		want  string
	}{
		{claude.Opening{Prompt: "fix  the\nbug", Interactive: true}, []string{"fix the bug"}, false, "/p/s.jsonl"},
		{claude.Opening{Prompt: "draft fix the bug", Interactive: true}, []string{"earlier", "fix the bug"}, false, "/p/s.jsonl"},
		{claude.Opening{Prompt: "! ls", Interactive: true}, []string{"!ls"}, false, "/p/s.jsonl"},
		{claude.Opening{Prompt: "fix the bug"}, []string{"fix the bug"}, false, ""},
		{claude.Opening{Prompt: "fix the bug"}, nil, false, ""},
		{claude.Opening{Prompt: "fix the bug", Interactive: true}, []string{"other"}, false, ""},
		{claude.Opening{Prompt: "fix the bug", Interactive: true}, nil, false, ""},
		{claude.Opening{Interactive: true}, []string{"fix the bug"}, false, ""},
		{claude.Opening{Prompt: "fix the bug", Interactive: true}, []string{"fix the bug"}, true, ""},
		{claude.Opening{Prompt: "fix the bug", Interactive: true}, nil, true, ""},
		{claude.Opening{Cleared: true, Prompt: "fix the bug", Interactive: true}, []string{"fix the bug"}, true, "/p/s.jsonl"},
		{claude.Opening{Cleared: true, Prompt: "work done elsewhere", Interactive: true}, []string{"fix the bug"}, true, ""},
		{claude.Opening{Cleared: true, Interactive: true}, nil, true, ""},
	} {
		if got := newSession(paths, nil, nil, map[string]claude.Opening{paths[0]: c.open}, claimant{bound: c.bound, sent: prompts(c.sent...)}, nil); got != c.want {
			t.Errorf("newSession(%+v, sent %q, bound %v) = %q", c.open, c.sent, c.bound, got)
		}
	}
}

func TestANewSessionIsTheOneOfSeveralUpToItsPrompt(t *testing.T) {
	paths := []string{"/p/a.jsonl", "/p/b.jsonl", "/p/c.jsonl"}
	opens := map[string]claude.Opening{
		"/p/a.jsonl": {Prompt: "other", Interactive: true},
		"/p/b.jsonl": {Prompt: "mine", Interactive: true},
		"/p/c.jsonl": {Cleared: true, Prompt: "after clear", Interactive: true},
	}
	if got := newSession(paths, nil, nil, opens, claimant{sent: prompts("mine")}, nil); got != "/p/b.jsonl" {
		t.Fatalf("got %q", got)
	}
	if got := newSession(paths, nil, nil, opens, claimant{}, nil); got != "" {
		t.Fatalf("guessed %q", got)
	}
	if got := newSession(paths, nil, nil, opens, claimant{bound: true, sent: prompts("after clear")}, nil); got != "/p/c.jsonl" {
		t.Fatalf("got %q, not the one a /clear started", got)
	}
}

func TestANewSessionIsNoneARivalClaimsToo(t *testing.T) {
	paths := []string{"/p/s.jsonl"}
	opens := map[string]claude.Opening{"/p/s.jsonl": {Prompt: "fix the flaky test", Interactive: true}}
	mine := claimant{sent: prompts("fix the flaky test")}
	if got := newSession(paths, nil, nil, opens, mine, []claimant{{sent: prompts("fix the flaky test")}}); got != "" {
		t.Fatalf("took %q, which the rival's prompt may have opened", got)
	}
	if got := newSession(paths, nil, nil, opens, mine, []claimant{{bound: true, sent: prompts("fix the flaky test")}}); got != "/p/s.jsonl" {
		t.Fatalf("got %q; a bound rival takes only a transcript a /clear started", got)
	}
	if got := newSession(paths, nil, nil, opens, mine, []claimant{{sent: prompts("other")}}); got != "/p/s.jsonl" {
		t.Fatalf("got %q", got)
	}
}

func TestALineFollowsTheKeysTypedIntoClaudesInput(t *testing.T) {
	for _, c := range []struct {
		keys []string
		want line
	}{
		{[]string{"fix", " teh", "\x7f\x7f", "he bug"}, line{text: "fix the bug"}},
		{[]string{"café", "\x7f", "e"}, line{text: "cafe"}},
		{[]string{"look at ", "\x1b[200~/tmp/a b.png\x1b[201~"}, line{text: "look at /tmp/a b.png"}},
		{[]string{"one", "\x1b\r", "two", "\x1b[13;2u", "three"}, line{text: "one\ntwo\nthree"}},
		{[]string{"fix", "\x1b[D", "x"}, line{text: "fixx", lost: true}},
		{[]string{"fix", "\x15", "redo"}, line{text: "fixredo", lost: true}},
		{[]string{"\x1b[Z", "plan it"}, line{text: "plan it"}},
		{[]string{"fix", "\x1b[Z", " it"}, line{text: "fix it"}},
		{[]string{"\x1b", "fix it"}, line{text: "fix it"}},
		{[]string{"\x1b[27u", "fix it"}, line{text: "fix it"}},
		{[]string{"fix teh", "\x03", "fix the build"}, line{text: "fix the build"}},
		{[]string{"fix", "\x1b[D", "\x1b[99;5u", "redo"}, line{text: "redo"}},
	} {
		var l line
		for _, k := range c.keys {
			l = l.typed(k)
		}
		if l != c.want {
			t.Errorf("keys %q = %+v, want %+v", c.keys, l, c.want)
		}
	}
}

func TestALaunchPromptIsTheLastWordAfterTheDashesOrTheFlags(t *testing.T) {
	for _, c := range []struct {
		argv []string
		want string
	}{
		{[]string{"claude", "-n", "fix-it", "--", "fix it"}, "fix it"},
		{[]string{"claude", "-n", "fix-it"}, ""},
		{[]string{"claude", "--resume", "s1"}, ""},
		{[]string{"claude", "--", "a", "b"}, ""},
		{[]string{"claude", "fix the login bug"}, "fix the login bug"},
		{[]string{"claude", "--model", "opus", "fix it"}, "fix it"},
		{[]string{"claude", "--model", "opus"}, ""},
		{[]string{"claude", "-c"}, ""},
		{[]string{"claude"}, ""},
	} {
		if got := launchPrompt(c.argv); got != c.want {
			t.Errorf("launchPrompt(%q) = %q, want %q", c.argv, got, c.want)
		}
	}
}

func typeKeys(d *Daemon, pr *presence, keys ...string) {
	for _, k := range keys {
		d.Input(pr.t.Info().ID, []byte(k))
	}
}

func TestAPromptTypedAfterShiftTabOrEscBindsItsTranscript(t *testing.T) {
	for _, key := range []string{"\x1b[Z", "\x1b"} {
		home, cwd := hookless(t)
		d := newDaemon(t)
		pr := claudeInDir(t, d, cwd)
		typeKeys(d, pr, key, "plan the refactor", "\r")
		writeSession(t, home, cwd, "s1", userLine("plan the refactor"))
		eventually(t, fmt.Sprintf("s1 bound after %q", key), func() bool { return sessionOf(pr) == "s1" })
	}
}

func TestAPromptSentAfterALostLineBindsTheTranscriptItReached(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd)
	typeKeys(d, pr, "fix the buid", "\x1b[D", "l", "\r")
	writeSession(t, home, cwd, "s1", userLine("fix the build"))
	time.Sleep(5 * DiscoverEvery)
	prompt(t, pr, "and add a test")
	appendSession(t, home, cwd, "s1", userLine("and add a test"))
	eventually(t, "s1 bound", func() bool {
		items, _ := pr.a.Timeline.Page(0, 10)
		return sessionOf(pr) == "s1" && len(items) == 2
	})
	if u := pr.unshown(); len(u) != 0 {
		t.Fatalf("unshown = %v", u)
	}
}

func TestAClaudeTypedWithAPromptBindsItsTranscript(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd, "fix the login bug")
	writeSession(t, home, cwd, "s1", userLine("fix the login bug"))
	eventually(t, "s1 bound", func() bool { return sessionOf(pr) == "s1" })
}

func TestTwoAgentsLaunchedWithOnePromptTakeNeitherTranscriptUntilTheirPromptsDiffer(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	a := claudeInDir(t, d, cwd, "--", "fix the flaky test")
	b := claudeInDir(t, d, cwd, "--", "fix the flaky test")
	writeSession(t, home, cwd, "sb", userLine("fix the flaky test"))
	time.Sleep(5 * DiscoverEvery)
	writeSession(t, home, cwd, "sa", userLine("fix the flaky test"))
	time.Sleep(5 * DiscoverEvery)
	if sessionOf(a) != "" || sessionOf(b) != "" {
		t.Fatalf("guessed a = %q, b = %q", sessionOf(a), sessionOf(b))
	}
	prompt(t, a, "use a retry")
	appendSession(t, home, cwd, "sa", userLine("use a retry"))
	prompt(t, b, "use a mock")
	appendSession(t, home, cwd, "sb", userLine("use a mock"))
	eventually(t, "each bound", func() bool { return sessionOf(a) == "sa" && sessionOf(b) == "sb" })
}

func TestPromptsNoTranscriptShowsAreForgottenOldestFirst(t *testing.T) {
	_, cwd := hookless(t)
	d := newDaemon(t)
	pr := claudeInDir(t, d, cwd)
	for i := range 200 {
		typeKeys(d, pr, fmt.Sprintf("prompt %d", i), "\r")
	}
	if u := pr.unshown(); len(u) > 20 || u[len(u)-1].text != "prompt 199" {
		t.Fatalf("unshown = %d, last %q", len(u), u[len(u)-1].text)
	}
}

// enterLag is how long after prompt() returns Claude may first write the
// prompt: termDriver sends Enter 150ms after the text.
const enterLag = 200 * time.Millisecond

func TestAnAgentDoesNotTakeAClosedAgentsTranscriptThatEndsOnTheSamePrompt(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	a := claudeInDir(t, d, cwd)
	b := claudeInDir(t, d, cwd)
	prompt(t, b, "b first")
	writeSession(t, home, cwd, "sb", userLine("b first"))
	eventually(t, "b on sb", func() bool { return sessionOf(b) == "sb" })
	prompt(t, b, "run the tests")
	appendSession(t, home, cwd, "sb", userLine("run the tests"))
	time.Sleep(5 * DiscoverEvery)
	closeAgent(t, d, b)

	prompt(t, a, "run the tests")
	time.Sleep(enterLag)
	writeSession(t, home, cwd, "sa", userLine("run the tests"))
	time.Sleep(10 * DiscoverEvery)
	if s := sessionOf(a); s != "sa" {
		t.Fatalf("a follows %q, not its own sa", s)
	}
}

func TestAClearedAgentDoesNotMoveToAClosedAgentsClearThatEndsOnTheSamePrompt(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	a := claudeInDir(t, d, cwd)
	prompt(t, a, "a first")
	writeSession(t, home, cwd, "sa", userLine("a first"))
	eventually(t, "a on sa", func() bool { return sessionOf(a) == "sa" })
	b := claudeInDir(t, d, cwd)
	prompt(t, b, "b first")
	writeSession(t, home, cwd, "sb", userLine("b first"))
	eventually(t, "b on sb", func() bool { return sessionOf(b) == "sb" })
	prompt(t, b, "/clear")
	prompt(t, b, "summarize the repo")
	writeSession(t, home, cwd, "sb2", clearLine, userLine("summarize the repo"))
	eventually(t, "b on sb2", func() bool { return sessionOf(b) == "sb2" })
	closeAgent(t, d, b)

	prompt(t, a, "/clear")
	time.Sleep(enterLag)
	writeSession(t, home, cwd, "sa2", clearLine)
	prompt(t, a, "summarize the repo")
	time.Sleep(enterLag)
	appendSession(t, home, cwd, "sa2", userLine("summarize the repo"))
	time.Sleep(10 * DiscoverEvery)
	if s := sessionOf(a); s != "sa2" {
		t.Fatalf("after /clear a follows %q, not sa2", s)
	}
}

func TestAnAgentDoesNotTakeAnOutsideClaudesTranscriptThatEndsOnTheSamePrompt(t *testing.T) {
	home, cwd := hookless(t)
	d := newDaemon(t)
	a := claudeInDir(t, d, cwd)
	writeSession(t, home, cwd, "iterm", userLine("look at the diff"))
	appendSession(t, home, cwd, "iterm", userLine("commit this"))
	time.Sleep(5 * DiscoverEvery)
	prompt(t, a, "commit this")
	time.Sleep(enterLag)
	writeSession(t, home, cwd, "sa", userLine("commit this"))
	time.Sleep(10 * DiscoverEvery)
	if s := sessionOf(a); s != "sa" {
		t.Fatalf("a follows %q, not its own sa", s)
	}
}

func TestOnePromptToAgentsInFoldersClaudeNamesTheSameIsNotGuessed(t *testing.T) {
	home, _ := hookless(t)
	base := t.TempDir()
	cwdA, cwdB := filepath.Join(base, "my_app"), filepath.Join(base, "my-app")
	os.Mkdir(cwdA, 0o700)
	os.Mkdir(cwdB, 0o700)
	d := newDaemon(t)
	a, b := claudeInDir(t, d, cwdA), claudeInDir(t, d, cwdB)
	prompt(t, a, "fix the build")
	prompt(t, b, "fix the build")
	time.Sleep(enterLag)
	writeSession(t, home, cwdB, "sb", userLine("fix the build"))
	time.Sleep(5 * DiscoverEvery)
	writeSession(t, home, cwdA, "sa", userLine("fix the build"))
	time.Sleep(10 * DiscoverEvery)
	if sessionOf(a) == "sb" || sessionOf(b) == "sa" {
		t.Fatalf("a follows %q, b follows %q: one took the other's", sessionOf(a), sessionOf(b))
	}
}

func TestATranscriptUpToAPromptFromBeforeItWasSentIsNotTheAgents(t *testing.T) {
	now := time.Now()
	sent := claimant{sent: []received{{"run the tests", now}}}
	open := func(at time.Time) map[string]claude.Opening {
		return map[string]claude.Opening{"/p/s.jsonl": {Prompt: "run the tests", Interactive: true, At: at}}
	}
	if got := newSession([]string{"/p/s.jsonl"}, nil, nil, open(now.Add(-time.Second)), sent, nil); got != "" {
		t.Fatalf("took %q, written before the prompt was sent", got)
	}
	if got := newSession([]string{"/p/s.jsonl"}, nil, nil, open(now.Add(-time.Millisecond)), sent, nil); got != "/p/s.jsonl" {
		t.Fatalf("got %q; Claude stamps milliseconds", got)
	}
}

func TestRetitleNamesTheConversationOfEachClaudeThatMatches(t *testing.T) {
	d := newDaemon(t)
	cfg := t.TempDir()
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "claude"), Env: []string{"PATH=/bin:/usr/bin", "CLAUDE_CONFIG_DIR=" + cfg}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	waitAgent(t, d, term)
	pr := d.presentIn(term.Info().ID)
	path := filepath.Join(cfg, "projects", "-w", "s1.jsonl")
	os.MkdirAll(filepath.Dir(path), 0o700)
	os.WriteFile(path, []byte(userLine("hi")+"\n"), 0o600)
	hookFrom(d, pr, sessionStart("s1", path))

	d.Retitle(func(i terminal.Info) bool { return i.ID != term.Info().ID }, "Not this one")
	d.Retitle(func(i terminal.Info) bool { return i.ID == term.Info().ID }, "Fix login")
	raw, _ := os.ReadFile(path)
	lines := strings.Split(strings.TrimSpace(string(raw)), "\n")
	if len(lines) != 2 || lines[1] != `{"type":"custom-title","customTitle":"Fix login","sessionId":"s1"}` {
		t.Fatalf("%s", raw)
	}
}

func TestRetitleLeavesATranscriptOutsideClaudesProjectsAlone(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	path := transcript(t, "hi")
	hookFrom(d, pr, sessionStart("s1", path))
	d.Retitle(func(terminal.Info) bool { return true }, "Fix login")
	if raw, _ := os.ReadFile(path); strings.Contains(string(raw), "custom-title") {
		t.Fatalf("%s", raw)
	}
}
