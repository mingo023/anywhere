package daemon

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/hub"
	"pocketd/internal/ops"
	"pocketd/internal/shellenv"
	"pocketd/internal/terminal"
	"pocketd/internal/timeline"
)

func newDaemon(t *testing.T) *Daemon {
	h := hub.New()
	d := &Daemon{Terminals: terminal.NewManager(), Agents: agent.NewRegistry(h), Broker: broker.New(h), Home: t.TempDir(), Exe: "/bin/true"}
	d.Terminals.OnInput = d.Input
	return d
}

func eventually(t *testing.T, what string, ok func() bool) {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(20 * time.Millisecond) {
		if ok() {
			return
		}
	}
	t.Fatalf("timed out waiting for %s", what)
}

func TestHookOffersDesktopChoices(t *testing.T) {
	const payload = `{"hook_event_name":"PermissionRequest","session_id":"s1","permission_mode":"default","tool_name":"Bash","tool_input":{"command":"touch c.txt"},
		"permission_suggestions":[{"type":"addDirectories","directories":["/w"],"destination":"session"}]}`
	d := newDaemon(t)
	term, pr := claudeIn(t, d)
	for _, c := range []struct {
		answer broker.Answer
		want   string
		status string
	}{
		{broker.Answer{Decision: "allow"}, `{"behavior":"allow"}`, "working"},
		{broker.Answer{Decision: "allow", Option: optionAlways}, `{"behavior":"allow","updatedPermissions":[{"type":"addDirectories","directories":["/w"],"destination":"session"}]}`, "working"},
		{broker.Answer{Decision: "allow", Option: optionAuto}, `{"behavior":"allow","updatedPermissions":[{"type":"setMode","mode":"auto","destination":"session"}]}`, "working"},
		{broker.Answer{Decision: "deny"}, `{"behavior":"deny","message":"Denied from phone","interrupt":true}`, "idle"},
		{broker.Answer{Decision: "deny", Message: "use b.txt"}, `{"behavior":"deny","message":"Denied from phone","interrupt":true}`, "idle"},
	} {
		out := make(chan []byte, 1)
		go func() { out <- hookFrom(d, pr, payload) }()
		eventually(t, "open request", func() bool { return len(d.Broker.Open()) == 1 })
		if s := pr.a.Summary().Status; s != "needsYou" {
			t.Fatalf("asking: %s", s)
		}
		req := d.Broker.Open()[0]
		opts, _ := json.Marshal(req.Options)
		if string(opts) != `[{"id":"always","label":"Yes, and always allow access to /w"},{"id":"auto","label":"Yes, and switch to auto mode"}]` || !req.Feedback || req.AgentID != pr.a.ID() {
			t.Fatalf("%s feedback=%v agent=%s", opts, req.Feedback, req.AgentID)
		}
		d.Broker.Resolve(req.RequestID, c.answer)
		var got struct {
			HookSpecificOutput struct{ Decision json.RawMessage }
		}
		json.Unmarshal(<-out, &got)
		if s := pr.a.Summary().Status; string(got.HookSpecificOutput.Decision) != c.want || s != c.status {
			t.Errorf("%+v: got %s, %s", c.answer, got.HookSpecificOutput.Decision, s)
		}
		if c.answer.Message != "" {
			pr.a.Record(timeline.Event{Kind: "result", Error: "interrupted"})
			eventually(t, "the feedback typed", func() bool { return strings.Contains(term.Screen(), c.answer.Message) })
		}
	}
}

func TestARequestEndsWithItsAgent(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	d.endAgent(pr)
	out := make(chan []byte, 1)
	go func() {
		out <- d.permission(context.Background(), pr, hookInput{Event: "PermissionRequest", ToolName: "Bash", ToolInput: json.RawMessage(`{"command":"ls"}`)})
	}()
	select {
	case o := <-out:
		if o != nil || len(d.Broker.Open()) != 0 {
			t.Fatalf("replied %s, open %+v", o, d.Broker.Open())
		}
	case <-time.After(5 * time.Second):
		t.Fatal("the request outlived its agent")
	}
}

func TestAnOpenRequestKeepsNeedsYou(t *testing.T) {
	const bash = `"tool_name":"Bash","tool_input":{"command":"ls"}`
	for _, answeredAtTheDesk := range []bool{false, true} {
		d := newDaemon(t)
		_, pr := claudeIn(t, d)
		out := make(chan []byte, 1)
		go func() { out <- hookFrom(d, pr, `{"hook_event_name":"PermissionRequest",`+bash+`}`) }()
		eventually(t, "open request", func() bool { return len(d.Broker.Open()) == 1 })
		for _, e := range []string{"UserPromptSubmit", "PostToolUse", "PostToolUseFailure", "PermissionDenied"} {
			hookFrom(d, pr, fmt.Sprintf(`{"hook_event_name":%q,"tool_name":"Read","tool_input":{"file_path":"/w/a"}}`, e))
			if s := pr.a.Summary().Status; s != "needsYou" {
				t.Errorf("%s during the request: %s", e, s)
			}
		}
		if answeredAtTheDesk {
			hookFrom(d, pr, `{"hook_event_name":"PostToolUse",`+bash+`}`)
		} else {
			d.Broker.Resolve(d.Broker.Open()[0].RequestID, broker.Answer{Decision: "allow"})
			<-out
		}
		if s := pr.a.Summary().Status; s != "working" {
			t.Errorf("answered at the desk %v: %s", answeredAtTheDesk, s)
		}
	}
}

func TestFeedbackFollowsATurnThatEndsDone(t *testing.T) {
	d := newDaemon(t)
	drv := &promptRecorder{prompts: make(chan string, 1)}
	a := d.Agents.Add("s1", "/w", "claude", drv)
	a.Record(timeline.Event{Kind: "user", Text: "go"})
	a.Working()
	go promptAfterTurn(a, a.Summary().MaxSeq, "use b.txt")
	a.Record(timeline.Event{Kind: "result", OK: true})
	a.TurnEnded(false)
	select {
	case p := <-drv.prompts:
		if p != "use b.txt" {
			t.Fatalf("prompted %q", p)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("no prompt after the turn ended Done")
	}
}

type promptRecorder struct {
	agent.Driver
	prompts chan string
}

func (p *promptRecorder) Prompt(text string) error {
	p.prompts <- text
	return nil
}

func TestSpawnWithoutEnvUsesTheLoginEnv(t *testing.T) {
	d := newDaemon(t)
	d.Capture = func() shellenv.Result {
		return shellenv.Result{Env: []string{"PATH=/usr/bin:/bin", "POCKET_LOGIN=yes"}, Mode: "interactive"}
	}
	d.Recapture()
	term, err := d.Spawn(ops.Msg{Cmd: "sh", Args: []string{"-c", "echo login=$POCKET_LOGIN.; sleep 30"}})
	if err != nil {
		t.Fatal(err)
	}
	defer term.Close()
	eventually(t, "login env in the terminal", func() bool { return strings.Contains(term.Screen(), "login=yes.") })
	if d.ShellEnv() != "interactive 0s" {
		t.Fatalf("status %q", d.ShellEnv())
	}
}

func TestSpawnRecapturesOnceWhenTheCommandIsMissing(t *testing.T) {
	bin := t.TempDir()
	os.WriteFile(filepath.Join(bin, "newtool"), []byte("#!/bin/sh\nsleep 30\n"), 0o755)
	d := newDaemon(t)
	captures := 0
	d.Capture = func() shellenv.Result {
		captures++
		path := "PATH=/usr/bin:/bin"
		if captures > 1 {
			path += ":" + bin
		}
		return shellenv.Result{Env: []string{path}, Mode: "interactive"}
	}
	d.Recapture()
	term, err := d.Spawn(ops.Msg{Cmd: "newtool"})
	if err != nil {
		t.Fatal(err)
	}
	defer term.Close()
	if captures != 2 {
		t.Fatalf("captures %d, want 2: installed after the first capture", captures)
	}
	if _, err := d.Spawn(ops.Msg{Cmd: "nosuchtool"}); !errors.Is(err, exec.ErrNotFound) {
		t.Fatalf("err %v", err)
	}
	if captures != 3 {
		t.Fatalf("captures %d, want 3: one retry per missing command", captures)
	}
}
