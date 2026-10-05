package daemon

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"pocketd/internal/broker"
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

func userLine(prompt string) string {
	return fmt.Sprintf(`{"type":"user","message":{"role":"user","content":%q}}`, prompt)
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
	path := transcript(t, "hi")
	hookFrom(d, pr, sessionStart("s1", path))
	eventually(t, "the transcript", func() bool { return pr.a.Summary().Title == "hi" })
	appendLine(t, path, strings.Repeat(`{"type":"progress"}`+"\n", 20000)+`{"type":"ai-title","aiTitle":"Fix the login bug"}`)
	d.endAgent(pr)
	if s := pr.a.Summary(); s.Status != "closed" || s.Title != "Fix the login bug" {
		t.Fatalf("summary = %+v", s)
	}
}

func TestAnInterruptInTheTranscriptClearsNeedsYou(t *testing.T) {
	const interrupt = `{"type":"user","message":{"content":"[Request interrupted by user for tool use]"}}`
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	path := transcript(t, "hi")
	appendLine(t, path, interrupt)
	appendLine(t, path, `{"type":"ai-title","aiTitle":"Old"}`)
	hookFrom(d, pr, `{"hook_event_name":"PreToolUse"}`)
	hookFrom(d, pr, sessionStart("s1", path))
	eventually(t, "the transcript", func() bool { return pr.a.Summary().Title == "Old" })
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
