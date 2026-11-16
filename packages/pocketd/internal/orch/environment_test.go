package orch

import (
	"database/sql"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func launchPayload(t *testing.T, e Effect) LaunchPayload {
	t.Helper()
	var p LaunchPayload
	if err := json.Unmarshal(e.Payload, &p); err != nil {
		t.Fatal(err)
	}
	return p
}

func TestADelegationToAHostLaunchesOnThatHostInItsProject(t *testing.T) {
	d, err := decide(parentState("working", 0), NewCommand("c", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "fix it", Environment: "devbox", HostProject: "/srv/app"}))
	if err != nil {
		t.Fatal(err)
	}
	if got := d.Delegations[0]; got.Environment != "devbox" || got.HostProject != "/srv/app" {
		t.Errorf("task = %+v, want devbox and /srv/app kept", got)
	}
	if p := launchPayload(t, d.Effects[0]); p.Env != "devbox" || p.HostProject != "/srv/app" {
		t.Errorf("launch = %+v, want devbox and /srv/app", p)
	}
}

func TestAQueuedHostDelegationLaunchesOnItsHostWhenRoomFrees(t *testing.T) {
	full := parentState("working", 0)
	full.Running = full.Limits.PerMac
	d, err := decide(full, NewCommand("c", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "fix it", Environment: "devbox", HostProject: "/srv/app"}))
	if err != nil {
		t.Fatal(err)
	}
	if got := d.Delegations[0]; got.Status != TaskQueued || len(d.Effects) != 0 {
		t.Fatalf("task %s with %d effects, want queued with none", got.Status, len(d.Effects))
	}
	room := parentState("working", 0)
	room.Children = d.Delegations
	d, err = decide(room, Command{Kind: CmdDelegatePump, Session: "p"})
	if err != nil {
		t.Fatal(err)
	}
	if len(d.Effects) != 1 {
		t.Fatalf("pump launched %d, want 1", len(d.Effects))
	}
	if p := launchPayload(t, d.Effects[0]); p.Env != "devbox" || p.HostProject != "/srv/app" {
		t.Errorf("launch = %+v, want devbox and /srv/app", p)
	}
}

func TestAHostDelegationKeepsItsEnvironmentAcrossARestart(t *testing.T) {
	dir := t.TempDir()
	s, err := Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	e := NewEngine(s, &fakeEffector{})
	e.Agent = agents
	e.Limits = func() Limits { return Limits{Depth: 3, PerSession: 6, PerMac: 0} }
	r, err := e.Dispatch(NewCommand("", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "x", Environment: "devbox", HostProject: "/srv/app"}))
	if err != nil {
		t.Fatal(err)
	}
	var task Delegation
	json.Unmarshal(r.Result, &task)
	e.Close()
	s.Close()

	s, err = Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	ts, err := s.Delegations(`task_id = ?`, task.TaskID)
	if err != nil || len(ts) != 1 || ts[0].Status != TaskQueued || ts[0].Environment != "devbox" || ts[0].HostProject != "/srv/app" {
		t.Fatalf("after reopen = %+v, %v; want the queued task on devbox in /srv/app", ts, err)
	}
	f := &fakeEffector{}
	e = NewEngine(s, f)
	e.Agent = agents
	defer e.Close()
	if _, err := e.Dispatch(Command{Kind: CmdDelegatePump, Session: "p"}); err != nil {
		t.Fatal(err)
	}
	waitFor(t, func() bool {
		f.mu.Lock()
		defer f.mu.Unlock()
		if len(f.ran) == 0 {
			return false
		}
		p := launchPayload(t, f.ran[0])
		return p.Env == "devbox" && p.HostProject == "/srv/app"
	})
}

func TestAStoreAtSchema2KeepsItsDelegationsWhenItMigratesToThree(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "state", "orchestrator.db")
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatal(err)
	}
	db, err := sql.Open("sqlite", path)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := db.Exec(`CREATE TABLE schema_version (version INTEGER NOT NULL)`); err != nil {
		t.Fatal(err)
	}
	for n, name := range []string{"migrations/001_init.sql", "migrations/002_imported.sql"} {
		body, err := migrations.ReadFile(name)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := db.Exec(string(body)); err != nil {
			t.Fatal(err)
		}
		if _, err := db.Exec(`INSERT INTO schema_version (version) VALUES (?)`, n+1); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := db.Exec(`INSERT INTO delegations (task_id, parent_id, title, status, created_at) VALUES ('task-1', 'p', 'Old', 'working', 5)`); err != nil {
		t.Fatal(err)
	}
	db.Close()

	s, err := Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	ts, err := s.Delegations(`task_id = 'task-1'`)
	if err != nil || len(ts) != 1 || ts[0].Title != "Old" || ts[0].Status != TaskWorking || ts[0].Environment != "" || ts[0].HostProject != "" {
		t.Fatalf("after migrating = %+v, %v; want the row kept with no environment", ts, err)
	}
	var v int
	s.db.QueryRow(`SELECT MAX(version) FROM schema_version`).Scan(&v)
	if v != 3 {
		t.Errorf("schema version = %d, want 3", v)
	}
}

func coldRestartedChild(t *testing.T, id string, mirrored func(string) bool) Delegation {
	t.Helper()
	s := openTest(t)
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	e := NewEngine(s, &fakeEffector{result: func(Effect) (map[string]any, error) { return map[string]any{"childId": id}, nil }})
	e.Agent = agents
	defer e.Close()
	r, err := e.Dispatch(NewCommand("", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "x"}))
	if err != nil {
		t.Fatal(err)
	}
	var task Delegation
	json.Unmarshal(r.Result, &task)
	before := NewSink(s)
	before.Summary(proto.AgentSummary{ID: id, Provider: "codex", Status: "working"})
	before.Item(id, 1, proto.Item{ID: "i1", Seq: 1, Kind: "assistant", Text: "Done on the host"})
	before.Item(id, 1, proto.Item{ID: "i2", Seq: 2, Kind: "result", OK: true})
	before.Close()
	waitFor(t, func() bool {
		ts, _ := s.Delegations(`task_id = ?`, task.TaskID)
		return ts[0].Status == TaskWorking
	})
	after := NewSink(s)
	defer after.Close()
	after.Mirrored = mirrored
	after.OnTurn = e.OnTurn
	after.Summary(proto.AgentSummary{ID: id, Provider: "codex", Status: "idle"})
	var got Delegation
	waitFor(t, func() bool {
		ts, _ := s.Delegations(`task_id = ?`, task.TaskID)
		got = ts[0]
		return got.Terminal()
	})
	return got
}

func tagged(id string) bool { return strings.Contains(id, "/") }

func TestAMirroredChildThatFinishedWhileTheLinkWasDownCompletes(t *testing.T) {
	if got := coldRestartedChild(t, "devbox/a2", tagged); got.Status != TaskCompleted || got.Summary != "Done on the host" {
		t.Fatalf("task = %s %q summary %q, want completed with the host's reply", got.Status, got.Error, got.Summary)
	}
}

func TestALocalChildBackIdleAfterAColdRestartStillFails(t *testing.T) {
	got := coldRestartedChild(t, "child-1", tagged)
	if got.Status != TaskFailed || !strings.Contains(got.Error, "restarted") {
		t.Fatalf("task = %s %q, want failed as restarted", got.Status, got.Error)
	}
}
