package e2e

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func create(p *Phone, requestID string, spec map[string]any) {
	p.Send(map[string]any{"type": "agent.create", "id": "c-" + requestID, "requestId": requestID, "spec": spec})
}

func inRepo(h *Harness, extra map[string]any) map[string]any {
	spec := map[string]any{"project": h.Repo, "checkout": map[string]any{"worktree": h.Repo}, "provider": "claude", "access": "ask", "plan": false}
	for k, v := range extra {
		spec[k] = v
	}
	return spec
}

func reply(p *Phone, types ...string) Message {
	return p.WaitFor(strings.Join(types, " or "), func(m Message) bool {
		for _, t := range types {
			if m.Type == t && strings.HasPrefix(m.ID, "c-") {
				return true
			}
		}
		return false
	})
}

func TestAnOwnerCreateInANewWorktreeRunsSetupThenTheAgent(t *testing.T) {
	h := Start(t, launchReady("echo setting up"))
	o := h.Owner()
	create(o, "r1", inRepo(h, map[string]any{"checkout": map[string]any{"new": map[string]any{"name": "calm-otter"}}, "prompt": "hello"}))
	c := reply(o, "agent.creating", "error")
	if c.Type != "agent.creating" || !c.Setup || !strings.HasSuffix(c.Cwd, "/wt/calm-otter") {
		t.Fatalf("got %s", c.Raw)
	}
	o.WaitFor("the agent step", func(m Message) bool { return m.Type == "agent.progress" && m.Step == "agent" })
	done := reply(o, "agent.created", "error")
	if done.Type != "agent.created" || done.TerminalID != c.TerminalID || done.AgentID == "" {
		t.Fatalf("got %s", done.Raw)
	}
	h.WaitScreen(c.TerminalID, "setting up")
	h.WaitScreen(c.TerminalID, "echo: hello")
}

func TestABogusModelFailsWithoutWaiting(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner()
	start := time.Now()
	create(o, "r1", inRepo(h, map[string]any{"model": "bogus"}))
	reply(o, "agent.creating")
	e := reply(o, "agent.created", "error")
	if e.Type != "error" || e.Code != "spawn_failed" || e.Message != "The agent exited 1" || !strings.Contains(e.Detail, "fake claude: unknown model bogus") {
		t.Fatalf("got %s", e.Raw)
	}
	if took := time.Since(start); took > 2*time.Second {
		t.Fatalf("took %v", took)
	}
}

func TestASetupFailureIsSpawnFailed(t *testing.T) {
	h := Start(t, launchReady("echo npm ERR! missing script: setup; false"))
	o := h.Owner()
	create(o, "r1", inRepo(h, map[string]any{"checkout": map[string]any{"new": map[string]any{"name": "broken"}}}))
	e := reply(o, "agent.created", "error")
	if e.Code != "spawn_failed" || e.Message != "Setup exited 1" || !strings.Contains(e.Detail, "missing script") {
		t.Fatalf("got %s", e.Raw)
	}
}

func TestPhoneCreateIsRefusedWithoutTheCap(t *testing.T) {
	h := Start(t, launchReady(""))
	p := h.Paired()
	create(p, "r1", inRepo(h, nil))
	e := reply(p, "agent.creating", "error")
	if e.Code != "access_not_allowed" || e.Message != "Update Pocket on your Mac." {
		t.Fatalf("got %s", e.Raw)
	}
}

func TestPocketdConfigSetGoesThroughTheRunningPocketd(t *testing.T) {
	h := Start(t, launchReady(""))
	if out, code := h.Pocketd("config", "set", "phone.maxAccess", "auto"); code != 0 || out != "phone.maxAccess = auto\n" {
		t.Fatalf("exit %d: %q", code, out)
	}
	if out, code := h.Pocketd("config", "set", "phone.maxAccess", "full"); code != 1 || !strings.Contains(out, "pocketd: phone.maxAccess is ask, edits or auto") {
		t.Fatalf("exit %d: %q", code, out)
	}
	o := h.Owner()
	o.Send(map[string]any{"type": "agent.providers", "id": "c-p"})
	got := reply(o, "agent.providers")
	if got.MaxAccess != "full" || got.PhoneMaxAccess != "auto" {
		t.Fatalf("got %s", got.Raw)
	}
}

func TestOwnerSetsPhoneAccessAndThePhoneIsRefusedAbove(t *testing.T) {
	h := Start(t, launchReady(""))
	p := h.Paired("launch.v1")
	create(p, "r1", inRepo(h, map[string]any{"access": "auto"}))
	if e := reply(p, "agent.creating", "error"); e.Message != "On your Mac: ⌘K → Phone access level" {
		t.Fatalf("got %s", e.Raw)
	}
	create(p, "r2", inRepo(h, map[string]any{"access": "full"}))
	if e := reply(p, "agent.creating", "error"); e.Message != "Full access starts only from your Mac" {
		t.Fatalf("got %s", e.Raw)
	}
	if _, code := h.Pocketd("config", "set", "phone.maxAccess", "auto"); code != 0 {
		t.Fatalf("config set exit %d", code)
	}
	create(p, "r3", inRepo(h, map[string]any{"access": "auto", "prompt": "hi"}))
	if c := reply(p, "agent.creating", "error"); c.Type != "agent.creating" {
		t.Fatalf("got %s", c.Raw)
	}
	if done := reply(p, "agent.created", "error"); done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
}

func TestAPhoneCreateInAFreshWorktreeOfATrustedProjectReachesWorking(t *testing.T) {
	h := Start(t, launchReady("echo setting up"))
	p := h.Paired("launch.v1")
	create(p, "r1", inRepo(h, map[string]any{"checkout": map[string]any{"new": map[string]any{"name": "from-phone"}}, "prompt": "hello"}))
	c := reply(p, "agent.creating", "error")
	if c.Type != "agent.creating" || !c.Setup {
		t.Fatalf("got %s", c.Raw)
	}
	done := reply(p, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	p.Send(map[string]any{"type": "agent.view", "id": "v", "agentIds": []string{done.AgentID}})
	h.WaitScreen(c.TerminalID, "echo: hello")
}

func TestAPhoneCreateInAnUntrustedProjectIsRefused(t *testing.T) {
	h := Start(t, launchReady(""))
	os.Remove(filepath.Join(h.ClaudeDir, ".claude.json"))
	p := h.Paired("launch.v1")
	create(p, "r1", inRepo(h, nil))
	if e := reply(p, "agent.creating", "error"); e.Code != "folder_not_trusted" {
		t.Fatalf("got %s", e.Raw)
	}
}
