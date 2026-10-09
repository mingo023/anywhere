package e2e

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"

	"pocketd/internal/automation"
	"pocketd/internal/proto"
)

func draft(h *Harness, prompt string) map[string]any {
	return map[string]any{
		"name": "Morning review", "prompt": prompt, "provider": "claude", "folder": h.Repo, "enabled": true,
		"schedule": map[string]any{"kind": "interval", "everyMin": 60},
	}
}

func (p *Phone) Automation(verb, id string, fields map[string]any) Message {
	p.t.Helper()
	msg := map[string]any{"type": "automation." + verb, "id": id}
	for k, v := range fields {
		msg[k] = v
	}
	p.Send(msg)
	return p.WaitFor("the answer to "+id, func(m Message) bool { return m.ID == id && (m.Type == "ack" || m.Type == "error") })
}

func (p *Phone) WaitRun(what string, ok func(proto.Run) bool) proto.Run {
	p.t.Helper()
	var found proto.Run
	p.WaitFor(what, func(m Message) bool {
		if m.Type != "automations" {
			return false
		}
		for _, r := range m.Runs {
			if ok(r) {
				found = r
				return true
			}
		}
		return false
	})
	return found
}

func withState(file automation.File) func(*Harness) {
	return func(h *Harness) {
		file.Version = automation.Version
		for i := range file.Automations {
			file.Automations[i].Folder = h.Repo
		}
		raw, _ := json.Marshal(file)
		os.MkdirAll(filepath.Join(h.Home, "state"), 0o700)
		if err := os.WriteFile(filepath.Join(h.Home, "state", "automations.json"), raw, 0o600); err != nil {
			h.t.Fatal(err)
		}
	}
}

func dueAutomation(due time.Time) automation.File {
	return automation.File{Automations: []proto.Automation{{
		ID: "au1", Name: "Due", Prompt: "hello", Provider: "claude", Enabled: true,
		Schedule:  proto.Schedule{Kind: "interval", EveryMin: 60},
		NextRunAt: due.UnixMilli(),
	}}}
}

func TestRunNowLaunchesASessionAndRecordsItDone(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapAutomations)
	saved := o.Automation("save", "s1", map[string]any{"automation": draft(h, "review the diff")})
	if saved.Type != "ack" {
		t.Fatalf("got %s", saved.Raw)
	}
	snap := o.WaitFor("the saved automation", func(m Message) bool { return m.Type == "automations" && len(m.Automations) == 1 })
	id := snap.Automations[0].ID
	if snap.Automations[0].NextRunAt == 0 {
		t.Fatalf("not scheduled: %s", snap.Raw)
	}

	if a := o.Automation("run", "r1", map[string]any{"automationId": id}); a.Type != "ack" {
		t.Fatalf("got %s", a.Raw)
	}
	started := o.WaitRun("a started run", func(r proto.Run) bool { return r.Status == "pending" || r.Status == "running" })
	if started.Trigger != "manual" || started.AutomationID != id {
		t.Fatalf("got %+v", started)
	}
	done := o.WaitRun("a succeeded run", func(r proto.Run) bool { return r.ID == started.ID && r.Status == "succeeded" })
	if done.TerminalID == "" || done.AgentID == "" || done.Summary != "echo: review the diff" {
		t.Fatalf("got %+v", done)
	}
	h.WaitScreen(done.TerminalID, "echo: review the diff")
}

func TestARunInANewWorktreeRecordsItsModeAndWorktree(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapAutomations)
	d := draft(h, "review the diff")
	d["access"], d["newWorktree"] = "edits", true
	o.Automation("save", "s1", map[string]any{"automation": d})
	id := o.WaitFor("the saved automation", func(m Message) bool { return m.Type == "automations" && len(m.Automations) == 1 }).Automations[0].ID
	o.Automation("run", "r1", map[string]any{"automationId": id})
	done := o.WaitRun("a succeeded run", func(r proto.Run) bool { return r.Status == "succeeded" })
	home, _ := filepath.EvalSymlinks(h.Home)
	if done.Access != "edits" || filepath.Dir(done.Worktree) != filepath.Join(home, "wt") {
		t.Fatalf("got %+v", done)
	}
	if _, err := os.Stat(filepath.Join(done.Worktree, ".git")); err != nil {
		t.Fatalf("no worktree at %s: %v", done.Worktree, err)
	}
}

func TestARunNowOnABusyAutomationIsRefused(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapAutomations)
	o.Automation("save", "s1", map[string]any{"automation": draft(h, "run ls")})
	id := o.WaitFor("the saved automation", func(m Message) bool { return m.Type == "automations" && len(m.Automations) == 1 }).Automations[0].ID
	o.Automation("run", "r1", map[string]any{"automationId": id})
	waiting := o.WaitRun("a waiting run", func(r proto.Run) bool { return r.Status == "waiting" })
	if waiting.AgentID == "" {
		t.Fatalf("got %+v", waiting)
	}
	if e := o.Automation("run", "r2", map[string]any{"automationId": id}); e.Type != "error" || e.Code != proto.CodeAutomationBusy {
		t.Fatalf("got %s", e.Raw)
	}
}

func TestADueRunFiresOnceAndARestartDoesNotRepeatIt(t *testing.T) {
	h := Start(t, launchReady(""), func(h *Harness) { withState(dueAutomation(time.Now().Add(-time.Minute)))(h) })
	o := h.Owner(proto.CapAutomations)
	run := o.WaitRun("the scheduled run done", func(r proto.Run) bool { return r.Trigger == "schedule" && r.Status == "succeeded" })

	h.Restart()
	o = h.Owner(proto.CapAutomations)
	time.Sleep(1500 * time.Millisecond)
	o.Automation("enable", "e1", map[string]any{"automationId": "au1", "enabled": false})
	snap := o.WaitFor("a fresh snapshot", func(m Message) bool {
		return m.Type == "automations" && len(m.Automations) == 1 && !m.Automations[0].Enabled
	})
	if len(snap.Runs) != 1 || snap.Runs[0].ID != run.ID {
		t.Fatalf("got %s", snap.Raw)
	}
}

func TestARunDueBeyondTheGraceWindowIsSkipped(t *testing.T) {
	h := Start(t, launchReady(""), func(h *Harness) { withState(dueAutomation(time.Now().Add(-13 * time.Hour)))(h) })
	o := h.Owner(proto.CapAutomations)
	skipped := o.WaitRun("a skipped run", func(r proto.Run) bool { return r.Status == "skipped" })
	if skipped.Why != "Mac asleep" || skipped.AgentID != "" || skipped.Trigger != "schedule" {
		t.Fatalf("got %+v", skipped)
	}
	if got := len(h.Status().Agents); got != 0 {
		t.Fatalf("%d agents started", got)
	}
}

func TestAutomationsSurviveARestart(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapAutomations)
	o.Automation("save", "s1", map[string]any{"automation": draft(h, "hello")})
	want := o.WaitFor("the saved automation", func(m Message) bool { return m.Type == "automations" && len(m.Automations) == 1 }).Automations[0]

	h.Restart()
	o = h.Owner(proto.CapAutomations)
	got := o.WaitFor("a snapshot", func(m Message) bool { return m.Type == "automations" })
	if len(got.Automations) != 1 || got.Automations[0].ID != want.ID || got.Automations[0].Name != "Morning review" {
		t.Fatalf("got %s", got.Raw)
	}
}

func TestAnAutomationInAFolderThatIsNoProjectIsRefused(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapAutomations)
	d := draft(h, "hello")
	d["folder"] = t.TempDir()
	if e := o.Automation("save", "s1", map[string]any{"automation": d}); e.Type != "error" || e.Code != proto.CodeUnknownProject {
		t.Fatalf("got %s", e.Raw)
	}
}

func TestAPhoneCannotSeeOrSaveAutomations(t *testing.T) {
	h := Start(t, launchReady(""))
	p := h.Paired(proto.CapAutomations)
	e := p.Automation("save", "s1", map[string]any{"automation": draft(h, "hello")})
	if e.Type != "error" || e.Code != "scope_denied" {
		t.Fatalf("got %s", e.Raw)
	}
	seen := false
	p.Send(map[string]any{"type": "agent.list", "id": "l1"})
	p.WaitFor("the agent list", func(m Message) bool {
		seen = seen || m.Type == "automations"
		return m.Type == "agent.list" && m.ID == "l1"
	})
	if seen {
		t.Fatal("a phone was sent automations")
	}
}
