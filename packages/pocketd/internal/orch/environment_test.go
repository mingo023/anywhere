package orch

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

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
	if v < 3 {
		t.Errorf("schema version = %d, want 3 or later", v)
	}
}

// storeAt is a home whose store an older pocketd left at version, holding the rows put in by inserts.
func storeAt(t *testing.T, version int, inserts ...string) string {
	t.Helper()
	dir := t.TempDir()
	path := filepath.Join(dir, "state", "orchestrator.db")
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatal(err)
	}
	db, err := sql.Open("sqlite", path)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	names, _ := fs.Glob(migrations, "migrations/*.sql")
	slices.Sort(names)
	stmts := []string{`CREATE TABLE schema_version (version INTEGER NOT NULL)`}
	for n, name := range names[:version] {
		body, err := migrations.ReadFile(name)
		if err != nil {
			t.Fatal(err)
		}
		stmts = append(stmts, string(body), fmt.Sprintf(`INSERT INTO schema_version (version) VALUES (%d)`, n+1))
	}
	for _, q := range append(stmts, inserts...) {
		if _, err := db.Exec(q); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

// migrated opens dir's store and fails t unless it reached the latest schema.
func migrated(t *testing.T, dir string) *Store {
	t.Helper()
	s, err := Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { s.Close() })
	names, _ := fs.Glob(migrations, "migrations/*.sql")
	var v int
	s.db.QueryRow(`SELECT MAX(version) FROM schema_version`).Scan(&v)
	if v != len(names) {
		t.Fatalf("schema version = %d, want %d", v, len(names))
	}
	return s
}

func TestAStoreAtSchema1KeepsItsSessionsAndDelegationsWhenItMigratesToTheLatest(t *testing.T) {
	s := migrated(t, storeAt(t, 1,
		`INSERT INTO sessions (id, title, status) VALUES ('s-1', 'Old', 'idle')`,
		`INSERT INTO delegations (task_id, parent_id, title, status, created_at) VALUES ('task-1', 's-1', 'Old', 'working', 5)`))

	var title, saved string
	var imported bool
	if err := s.db.QueryRow(`SELECT title, imported, saved FROM sessions WHERE id = 's-1'`).Scan(&title, &imported, &saved); err != nil || title != "Old" || imported || saved != "" {
		t.Fatalf("session = %q imported %v saved %q, %v", title, imported, saved, err)
	}
	ts, err := s.Delegations(`task_id = 'task-1'`)
	if err != nil || len(ts) != 1 || ts[0].Title != "Old" || ts[0].Status != TaskWorking || ts[0].Environment != "" {
		t.Fatalf("delegations = %+v, %v", ts, err)
	}
}

func TestAStoreAtSchema3KeepsItsSessionsWhenItMigratesToTheLatest(t *testing.T) {
	s := migrated(t, storeAt(t, 3, `INSERT INTO sessions (id, title, imported) VALUES ('s-1', 'Old', 1)`))

	var title, saved string
	var imported bool
	if err := s.db.QueryRow(`SELECT title, imported, saved FROM sessions WHERE id = 's-1'`).Scan(&title, &imported, &saved); err != nil || title != "Old" || !imported || saved != "" {
		t.Fatalf("session = %q imported %v saved %q, %v", title, imported, saved, err)
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

func TestAHostChildPromptedOnItsHostOpensAFollowUpAndATurnOfItsOwnDoesNot(t *testing.T) {
	s := openTest(t)
	putSession(t, s, Session{ID: "p", Agent: "claude", Access: "auto", Status: "working", Project: "proj", Cwd: "/r"})
	f := &fakeEffector{result: func(e Effect) (map[string]any, error) {
		if e.Kind == EffLaunch {
			return map[string]any{"childId": "devbox/c1"}, nil
		}
		return nil, nil
	}}
	e := NewEngine(s, f)
	e.Agent = agents
	if _, err := e.Dispatch(NewCommand("d1", CmdDelegate, "p", DelegatePayload{Agent: "codex", Brief: "fix it", Environment: "devbox", HostProject: "/srv/app"})); err != nil {
		t.Fatal(err)
	}
	tasks := func() []Delegation {
		ts, _ := s.Delegations(`child_id = 'devbox/c1' ORDER BY created_at, rowid`)
		return ts
	}
	waitFor(t, func() bool { ts := tasks(); return len(ts) == 1 && ts[0].Status == TaskWorking })
	k := NewSink(s)
	defer k.Close()
	k.OnTurn = e.OnTurn
	turn := func(items ...proto.Item) {
		k.Summary(proto.AgentSummary{ID: "devbox/c1", Provider: "codex", Status: "working"})
		for _, it := range items {
			k.Item("devbox/c1", 1, it)
		}
		k.Summary(proto.AgentSummary{ID: "devbox/c1", Provider: "codex", Status: "done"})
		k.Flush()
	}
	turn(proto.Item{ID: "i1", Seq: 1, Kind: "assistant", Text: "Fixed"}, proto.Item{ID: "i2", Seq: 2, Kind: "result", OK: true})
	waitFor(t, func() bool { return tasks()[0].Status == TaskCompleted })

	turn(proto.Item{ID: "i3", Seq: 3, Kind: "user", Text: "Add a test"}, proto.Item{ID: "i4", Seq: 4, Kind: "assistant", Text: "Test added"}, proto.Item{ID: "i5", Seq: 5, Kind: "result", OK: true})
	waitFor(t, func() bool { ts := tasks(); return len(ts) == 2 && ts[1].Status == TaskCompleted })
	if ts := tasks(); ts[0].Summary != "Fixed" || ts[1].Summary != "Test added" || ts[1].Brief != "Add a test" {
		t.Fatalf("tasks %+v", ts)
	}

	turn(proto.Item{ID: "i6", Seq: 6, Kind: "notice", Text: "Monitor: CI · green"}, proto.Item{ID: "i7", Seq: 7, Kind: "assistant", Text: "CI went green"}, proto.Item{ID: "i8", Seq: 8, Kind: "result", OK: true})
	time.Sleep(200 * time.Millisecond)
	if ts := tasks(); len(ts) != 2 || ts[1].Summary != "Test added" {
		t.Fatalf("a turn of its own changed the tasks: %+v", ts)
	}
}
