//go:build unix

package daemon

import (
	"slices"
	"testing"
	"time"

	"pocketd/internal/acp"
	"pocketd/internal/proc"
	"pocketd/internal/proto"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

func spawnAgent(t *testing.T, d *Daemon, name string, args ...string) (*terminal.Terminal, proto.AgentSummary) {
	t.Helper()
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, name), Args: args, Env: []string{"PATH=/bin:/usr/bin"}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	return term, waitAgent(t, d, term)
}

func TestEachNewAgentInATerminalIsASessionThatShowsOnlyItsTerminal(t *testing.T) {
	for _, c := range []struct {
		provider, cmd string
		args          []string
		access, model string
	}{
		{"cursor", "cursor-agent", []string{"--model", "gpt-6"}, "settings", "gpt-6"},
		{"grok", "grok", []string{"--always-approve"}, "full", ""},
		{"opencode", "opencode", []string{"-m", "opencode/big-pickle"}, "settings", "opencode/big-pickle"},
		{"pi", "pi", nil, "full", ""},
		{"omp", "omp", nil, "full", ""},
		{"fx", "fx", nil, "settings", ""},
		{"hermes", "python3", []string{"-I", "-c", "from hermes_cli.main import main; main()", "chat", "-m", "gpt-6-sol", "--yolo"}, "full", "gpt-6-sol"},
		{"antigravity", "agy", nil, "settings", ""},
	} {
		d := newDaemon(t)
		term, a := spawnAgent(t, d, c.cmd, c.args...)
		if a.Provider != c.provider || a.Attached || a.ChatOnly || a.Access != c.access || a.Model != c.model {
			t.Errorf("%s: got %+v", c.provider, a)
		}
		pr := d.presentIn(term.Info().ID)
		if err := pr.a.Driver().Settings("m", ""); err != errTUISettings {
			t.Errorf("%s: a switch from the chat: %v", c.provider, err)
		}
		// Keys the user types reach its kind without a claude's bookkeeping.
		d.Input(term.Info().ID, []byte("\x1b"))
		if pr.a.Turn() != "idle" {
			t.Errorf("%s: Esc made it %s", c.provider, pr.a.Turn())
		}
	}
}

func TestANotSessionCommandOfANewAgentIsNoAgent(t *testing.T) {
	d := newDaemon(t)
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "opencode"), Args: []string{"serve"}, Env: []string{"PATH=/bin:/usr/bin"}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	eventually(t, "opencode serve to run", func() bool {
		d.poll()
		_, procs := foreground(term)
		return slices.ContainsFunc(procs, func(p proc.Proc) bool { return slices.Contains(p.Argv, "serve") })
	})
	d.poll()
	if a, ok := agentIn(d, term); ok {
		t.Fatalf("got %+v", a)
	}
}

func TestAnAgentPocketdStartedKeepsTheLaunchItsCommandLineNoLongerShows(t *testing.T) {
	// Pi rewrites its command line to its name, so its flags are only in what its Terminal was given.
	args := []string{"-c", `"$1"`, "sh", fakeAgent(t, "pi"), "--model", "a/b", "--thinking", "low", "--tools", "read,grep,find,ls"}
	for origin, want := range map[string]state.Launch{
		"desktop": {Access: "full", Plan: true, Model: "a/b", Effort: "low"},
		// One the user typed has only the command line it shows.
		"": {Access: "full"},
	} {
		d := newDaemon(t)
		term, err := d.Terminals.Spawn(terminal.Spec{Cmd: "/bin/sh", Args: args, Env: []string{"PATH=/bin:/usr/bin"}, Origin: origin})
		if err != nil {
			t.Fatal(err)
		}
		t.Cleanup(term.Close)
		a := waitAgent(t, d, term)
		pr := d.presentIn(term.Info().ID)
		pr.mu.Lock()
		l := pr.launch
		pr.mu.Unlock()
		if a.Provider != "pi" || l != want || a.Model != want.Model || a.Effort != want.Effort {
			t.Errorf("started by %q: launch %+v, agent %+v", origin, l, a)
		}
	}
}

func TestAChatWhoseAgentNeverOpensItShowsWhyAndThePromptItNeverGot(t *testing.T) {
	was := acp.OpenWait
	acp.OpenWait = 500 * time.Millisecond
	t.Cleanup(func() { acp.OpenWait = was })
	d := newDaemon(t)
	// No pipe connects, so nothing answers the session's first call.
	term, a := spawnAgent(t, d, "pocketd", "pipe", "hermes", "acp", "--pocketd", "--access", "ask")
	pr := d.presentIn(term.Info().ID)
	if err := pr.a.Driver().Prompt("do the important thing"); err != nil {
		t.Fatal(err)
	}
	eventually(t, "the failure", func() bool { return pr.a.Summary().Failed && !pr.a.Summary().Attached })
	items, _ := pr.a.Timeline.Page(0, 10)
	const why = "Hermes Agent didn't start, so your prompt was not sent: it didn't answer initialize in 500ms"
	if len(items) != 2 || items[0].Kind != "user" || items[0].Text != "do the important thing" || items[1].Kind != "result" || items[1].Error != why {
		t.Fatalf("got %+v", items)
	}
	if s := pr.a.Summary(); s.ID != a.ID || midTurn(s.Status) || s.Attached {
		t.Fatalf("got %+v", s)
	}
	if err := pr.a.Driver().Prompt("again"); err == nil || err.Error() != "Hermes Agent didn't start: it didn't answer initialize in 500ms" {
		t.Fatalf("the next prompt: %v", err)
	}
}
