//go:build unix

package e2e

import (
	"os"
	"path/filepath"
	"testing"
)

// heldSetup is a Repo setup that runs until release exists, holding a create in a new Worktree in flight.
// It runs in sh, as the login shell may be fish.
func heldSetup(t *testing.T) (setup, release string) {
	release = filepath.Join(t.TempDir(), "release")
	return `/bin/sh -c "while [ ! -f '` + release + `' ]; do sleep 0.05; done"`, release
}

func agentCount(p *Phone) int {
	p.Send(map[string]any{"type": "agent.list", "id": "c-list"})
	return len(reply(p, "agent.list").Agents)
}

func TestARetryOfARunningCreateJoinsItInsteadOfStartingASecondAgent(t *testing.T) {
	setup, release := heldSetup(t)
	h := Start(t, launchReady(setup))
	o := h.Owner()
	spec := inRepo(h, map[string]any{"checkout": map[string]any{"new": map[string]any{"name": "calm-otter"}}})
	create(o, "r1", spec)
	c := reply(o, "agent.creating", "error")
	if c.Type != "agent.creating" || !c.Setup {
		t.Fatalf("got %s", c.Raw)
	}
	o.Send(map[string]any{"type": "agent.create", "id": "c-again", "requestId": "r1", "spec": spec})
	os.WriteFile(release, nil, 0o644)
	var first, joined, opened Message
	o.WaitFor("both creates' replies", func(m Message) bool {
		switch {
		case m.ID == "c-again" && m.Type == "agent.progress":
			t.Errorf("a retry got progress: %s", m.Raw)
		case m.ID == "c-again" && m.Type == "agent.creating":
			opened = m
		case m.ID == "c-again" && (m.Type == "agent.created" || m.Type == "error"):
			joined = m
		case m.ID == "c-r1" && (m.Type == "agent.created" || m.Type == "error"):
			first = m
		}
		return first.Type != "" && joined.Type != ""
	})
	if first.Type != "agent.created" || joined.AgentID != first.AgentID || joined.TerminalID != first.TerminalID || opened.TerminalID != c.TerminalID || opened.Cwd != c.Cwd {
		t.Fatalf("first %s, retry %s after %s", first.Raw, joined.Raw, opened.Raw)
	}
	if n := agentCount(o); n != 1 {
		t.Fatalf("%d agents", n)
	}
}

func TestADuplicateReturnsTheFirstResult(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner()
	create(o, "r1", inRepo(h, nil))
	c := reply(o, "agent.creating", "error")
	done := reply(o, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	create(o, "r1", inRepo(h, nil))
	if again := reply(o, "agent.creating", "error"); again.TerminalID != c.TerminalID || again.Cwd != c.Cwd {
		t.Fatalf("retry opened %s, first %s", again.Raw, c.Raw)
	}
	if again := reply(o, "agent.created", "error"); again.AgentID != done.AgentID || again.TerminalID != done.TerminalID {
		t.Fatalf("retry got %s, first %s", again.Raw, done.Raw)
	}
	if n := agentCount(o); n != 1 {
		t.Fatalf("%d agents", n)
	}
}

func TestADifferentSpecUnderTheSameRequestIsADuplicate(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner()
	create(o, "r1", inRepo(h, nil))
	if done := reply(o, "agent.created", "error"); done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	create(o, "r1", inRepo(h, map[string]any{"access": "full"}))
	if e := reply(o, "agent.creating", "agent.created", "error"); e.Code != "duplicate" || e.Message != "This request was already sent with a different spec" {
		t.Fatalf("got %s", e.Raw)
	}
	p := h.Paired("launch.v1")
	create(p, "r1", inRepo(h, map[string]any{"access": "full"}))
	if done := reply(p, "agent.created", "error"); done.Type != "agent.created" {
		t.Fatalf("another principal's request id collided: %s", done.Raw)
	}
}

func TestARetryOfAFailedCreateGetsItsFailure(t *testing.T) {
	h := Start(t, launchReady("echo npm ERR! missing script: setup; false"))
	o := h.Owner()
	spec := inRepo(h, map[string]any{"checkout": map[string]any{"new": map[string]any{"name": "broken"}}})
	create(o, "r1", spec)
	e := reply(o, "agent.created", "error")
	if e.Code != "spawn_failed" || e.Message != "Setup exited 1" {
		t.Fatalf("got %s", e.Raw)
	}
	create(o, "r1", spec)
	if again := reply(o, "agent.created", "error"); again.Code != e.Code || again.Message != e.Message || again.Detail != e.Detail {
		t.Fatalf("retry got %s, first %s", again.Raw, e.Raw)
	}
}
