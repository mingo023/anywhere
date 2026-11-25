package statedb

import (
	"bytes"
	"crypto/sha256"
	"database/sql"
	"encoding/json"
	"errors"
	"io/fs"
	"log"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"

	"pocketd/internal/automation"
	"pocketd/internal/board"
	"pocketd/internal/devices"
	"pocketd/internal/hosts"
	"pocketd/internal/names"
	"pocketd/internal/proto"
	"pocketd/internal/push"
	"pocketd/internal/registry"
	"pocketd/internal/state"
)

// fixture is a home a pocketd from before pocketd.db left, with every file it kept.
type fixture struct {
	home, project, plain, tree, nowhere string
}

func newFixture(t *testing.T) fixture {
	t.Helper()
	base, err := filepath.EvalSymlinks(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	f := fixture{home: filepath.Join(base, "home"), project: filepath.Join(base, "pocket"), plain: filepath.Join(base, "plain"),
		tree: filepath.Join(base, "wt", "Pocket", "calm-otter"), nowhere: filepath.Join(base, "elsewhere")}
	for _, d := range []string{f.home, f.project, f.plain} {
		os.MkdirAll(d, 0o700)
	}
	files := map[string]any{
		"state/registry.json": map[string]any{
			"projects": []any{
				map[string]any{"id": "p1", "path": f.project, "repo": map[string]any{"name": "Pocket", "color": 7, "setup": "make", "launch": map[string]any{"model": "opus"}, "icon": map[string]any{"emoji": "P"}, "accounts": map[string]string{"claude": "w"}, "hidden": []string{"codex"}}},
				map[string]any{"id": "p2", "path": f.plain, "repo": map[string]any{}},
			},
			"tracked":         []string{f.tree, f.tree},
			"agent_worktrees": []string{f.tree, "/wt/x"},
			"settings":        map[string]any{"git": map[string]any{"remote": "fork", "commit_all": false, "force_with_lease": true}, "access": "edits", "worktree_root": filepath.Join(base, "wt")},
		},
		"state/board.json": map[string]any{"version": 1, "limit": 3, "lastNumber": 7, "tickets": []any{
			map[string]any{"id": "t1", "title": "Fix", "prompt": "fix it", "provider": "claude", "folder": f.project, "base": "dev", "state": "todo", "order": 2, "number": 7, "priority": 1, "labels": []string{"Bug"}, "createdAt": 10, "changedAt": 11},
			map[string]any{"id": "t2", "title": "Ship", "prompt": "ship", "provider": "codex", "folder": f.tree, "state": "done", "status": "merged", "why": "w", "agentId": "a", "terminalId": "tm", "worktree": f.tree, "createdAt": 1, "changedAt": 2, "closedAt": 3, "closedWhy": "cw", "relaunch": true},
			map[string]any{"id": "t3", "title": "Lost", "prompt": "lost", "provider": "claude", "folder": f.nowhere, "state": "todo"},
		}},
		"state/automations.json": map[string]any{"version": 1, "automations": []any{
			map[string]any{"id": "a1", "name": "Review", "prompt": "review", "provider": "claude", "folder": f.tree, "schedule": map[string]any{"kind": "days", "days": []int{1, 3}, "time": "09:00"}, "enabled": true, "nextRunAt": 99, "access": "edits", "newWorktree": true, "base": "main", "model": "opus", "effort": "high", "createdBy": "s1", "bound": "s2", "webhook": true, "webhookSecret": "s3cret"},
			map[string]any{"id": "a2", "name": "Gone", "prompt": "p", "provider": "claude", "folder": f.nowhere, "schedule": map[string]any{"kind": "interval", "everyMin": 30}},
		}, "runs": []any{
			map[string]any{"id": "r1", "automationId": "a1", "status": "succeeded", "trigger": "schedule", "why": "y", "summary": "s", "startedAt": 5, "finishedAt": 6, "agentId": "ag", "terminalId": "te", "access": "edits", "worktree": f.tree, "turnAfter": 4, "turnPrompt": "tp", "turnEntry": "te"},
			map[string]any{"id": "r2", "automationId": "a2", "status": "failed", "trigger": "manual", "startedAt": 7},
			map[string]any{"id": "r3", "automationId": "a1", "status": "running", "trigger": "manual", "startedAt": 5},
		}},
		"state/locals.json": []any{
			map[string]any{"id": "l1", "project": f.project, "name": "Main", "auto": true},
			map[string]any{"id": "l2", "project": f.nowhere, "name": "Lost"},
			map[string]any{"id": "l3", "project": f.plain + "/", "name": "Plain"},
		},
		"state/names.json": map[string]any{f.tree: map[string]any{"title": "Calm", "auto": true}, "/wt/x": map[string]any{"title": "X", "auto": false}},
		"state/hosts.json": map[string]any{"version": 1, "hosts": []any{
			map[string]any{"id": "h1", "name": "box", "target": map[string]any{"kind": "address", "addr": "10.0.0.1", "port": 22, "user": "u", "keyFile": "/k"}, "windows": true},
			map[string]any{"id": "h2", "name": "dev", "target": map[string]any{"kind": "alias", "name": "dev"}},
		}},
		"state/host.json": map[string]any{"id": "host-1"},
		"devices.json": map[string]any{"version": 1, "graceEnded": true, "devices": []any{
			map[string]any{"id": "d2", "name": "Pad", "platform": "ios", "scopes": []string{"observe"}, "tokenHash": "h2", "createdAt": 1, "lastSeenAt": 2, "lastAddr": "1.2.3.4:5"},
			map[string]any{"id": "d1", "name": "Phone", "platform": "ios", "scopes": []string{"observe", "drive"}, "tokenHash": "h1", "createdAt": 3, "lastSeenAt": 4, "lastAddr": "a", "legacy": true, "graceEndsAt": 9},
		}},
		"push.json": map[string]any{"records": map[string]any{
			"d1": map[string]any{"device": "d1", "key": []byte{1, 2, 3}, "env": "prod", "alert": map[string]any{"handle": "h", "secret": "s", "expiresAt": 8}, "activity": map[string]any{"id": "act", "startedAt": 4}, "prefs": map[string]any{"needsYou": true, "details": true}, "frequent": true, "updatedAt": 5},
			"d2": map[string]any{"device": "d2", "key": []byte{4}, "env": "dev", "start": map[string]any{"handle": "st", "secret": "ss", "expiresAt": 1}, "prefs": map[string]any{}, "updatedAt": 6},
		}, "sends": []any{
			map[string]any{"at": 1, "device": "d1", "kind": "alert", "agent": "ag", "priority": 10, "outcome": "ok"},
			map[string]any{"at": 1, "device": "d2", "kind": "activity", "priority": 5, "outcome": "gone"},
		}},
		"config.json":  map[string]any{"port": 4600, "listen": "loopback", "phone": map[string]any{"maxAccess": "edits", "terminals": "on"}, "profiles": map[string]any{"work": map[string]any{"cmd": "x"}}, "token": "tok", "empty": map[string]any{}, "phone.maxAccess": "twice"},
		"desktop.json": map[string]any{"projects": []string{"/w/desktop"}, "hosts": []any{map[string]any{"id": "hd", "name": "desk", "target": map[string]any{"kind": "alias", "name": "desk"}}}},
	}
	for name, v := range files {
		writeJSON(t, filepath.Join(f.home, name), v)
	}
	return f
}

func writeJSON(t *testing.T, path string, v any) {
	t.Helper()
	raw, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	os.MkdirAll(filepath.Dir(path), 0o700)
	if err := os.WriteFile(path, raw, 0o600); err != nil {
		t.Fatal(err)
	}
}

// hashes is every file in home but pocketd.db's, by path.
func hashes(t *testing.T, home string) map[string][32]byte {
	t.Helper()
	out := map[string][32]byte{}
	filepath.WalkDir(home, func(path string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() || strings.HasPrefix(d.Name(), "pocketd.db") {
			return err
		}
		raw, _ := os.ReadFile(path)
		out[path] = sha256.Sum256(raw)
		return nil
	})
	return out
}

// copyHome is a copy of home, for the old readers, which may write.
func copyHome(t *testing.T, home string) string {
	t.Helper()
	dst := t.TempDir()
	filepath.WalkDir(home, func(path string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		rel, _ := filepath.Rel(home, path)
		raw, _ := os.ReadFile(path)
		os.MkdirAll(filepath.Dir(filepath.Join(dst, rel)), 0o700)
		return os.WriteFile(filepath.Join(dst, rel), raw, 0o600)
	})
	return dst
}

func open(t *testing.T, home string) *sql.DB {
	t.Helper()
	db, err := Open(home)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { db.Close() })
	return db
}

func logged(t *testing.T) *bytes.Buffer {
	t.Helper()
	var b bytes.Buffer
	log.SetOutput(&b)
	t.Cleanup(func() { log.SetOutput(os.Stderr) })
	return &b
}

func count(t *testing.T, db *sql.DB, table string) int {
	t.Helper()
	var n int
	if err := db.QueryRow(`SELECT COUNT(*) FROM ` + table).Scan(&n); err != nil {
		t.Fatal(err)
	}
	return n
}

func meta(t *testing.T, db *sql.DB) map[string]string {
	t.Helper()
	out := map[string]string{}
	rows, err := db.Query(`SELECT key, value FROM meta`)
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	for rows.Next() {
		var k, v string
		rows.Scan(&k, &v)
		out[k] = v
	}
	return out
}

func decode(t *testing.T, raw string, v any) {
	t.Helper()
	if err := json.Unmarshal([]byte(raw), v); err != nil {
		t.Fatalf("%s: %v", raw, err)
	}
}

func readFile(t *testing.T, path string, v any) {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	decode(t, string(raw), v)
}

// folder is a Ticket's or an Automation's folder as the wire carries it: its tree, else its project's path.
func folder(t *testing.T, db *sql.DB, project string, tree sql.NullString) string {
	t.Helper()
	if tree.Valid {
		return tree.String
	}
	var path string
	if err := db.QueryRow(`SELECT path FROM projects WHERE id = ?`, project).Scan(&path); err != nil {
		t.Fatal(err)
	}
	return path
}

func tickets(t *testing.T, db *sql.DB) []proto.Ticket {
	t.Helper()
	rows, err := db.Query(`SELECT id, project_id, tree, title, prompt, provider, base, state, sort_order, status, why, agent_id, terminal_id,
		worktree, created_at, changed_at, closed_at, closed_why, relaunch, number, priority, labels FROM tickets ORDER BY position`)
	if err != nil {
		t.Fatal(err)
	}
	type row struct {
		x               proto.Ticket
		project, labels string
		tree            sql.NullString
	}
	var got []row
	for rows.Next() {
		var r row
		x := &r.x
		if err := rows.Scan(&x.ID, &r.project, &r.tree, &x.Title, &x.Prompt, &x.Provider, &x.Base, &x.State, &x.Order, &x.Status, &x.Why, &x.AgentID, &x.TerminalID,
			&x.Worktree, &x.CreatedAt, &x.ChangedAt, &x.ClosedAt, &x.ClosedWhy, &x.Relaunch, &x.Number, &x.Priority, &r.labels); err != nil {
			t.Fatal(err)
		}
		got = append(got, r)
	}
	rows.Close()
	var out []proto.Ticket
	for _, r := range got {
		r.x.Folder = folder(t, db, r.project, r.tree)
		if decode(t, r.labels, &r.x.Labels); len(r.x.Labels) == 0 {
			r.x.Labels = nil
		}
		out = append(out, r.x)
	}
	return out
}

func automations(t *testing.T, db *sql.DB) ([]proto.Automation, []proto.Run) {
	t.Helper()
	rows, err := db.Query(`SELECT id, project_id, tree, name, prompt, provider, schedule, enabled, next_run_at, access, new_worktree,
		base, model, effort, created_by, bound, webhook, webhook_secret FROM automations ORDER BY position`)
	if err != nil {
		t.Fatal(err)
	}
	type row struct {
		a                 proto.Automation
		project, schedule string
		tree              sql.NullString
	}
	var got []row
	for rows.Next() {
		var r row
		a := &r.a
		if err := rows.Scan(&a.ID, &r.project, &r.tree, &a.Name, &a.Prompt, &a.Provider, &r.schedule, &a.Enabled, &a.NextRunAt, &a.Access, &a.NewWorktree,
			&a.Base, &a.Model, &a.Effort, &a.CreatedBy, &a.Bound, &a.Webhook, &a.WebhookSecret); err != nil {
			t.Fatal(err)
		}
		got = append(got, r)
	}
	rows.Close()
	var autos []proto.Automation
	for _, r := range got {
		r.a.Folder = folder(t, db, r.project, r.tree)
		decode(t, r.schedule, &r.a.Schedule)
		autos = append(autos, r.a)
	}
	rows, err = db.Query(`SELECT id, automation_id, status, trigger, why, summary, started_at, finished_at, agent_id, terminal_id,
		access, worktree, turn_after, turn_prompt, turn_entry FROM runs ORDER BY position`)
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	var runs []proto.Run
	for rows.Next() {
		var r proto.Run
		if err := rows.Scan(&r.ID, &r.AutomationID, &r.Status, &r.Trigger, &r.Why, &r.Summary, &r.StartedAt, &r.FinishedAt, &r.AgentID, &r.TerminalID,
			&r.Access, &r.Worktree, &r.TurnAfter, &r.TurnPrompt, &r.TurnEntry); err != nil {
			t.Fatal(err)
		}
		runs = append(runs, r)
	}
	return autos, runs
}

func TestAHomeWithEveryFileImportsEveryValueTheOldReadersRead(t *testing.T) {
	f := newFixture(t)
	old := copyHome(t, f.home)
	db := open(t, f.home)

	reg := must(registry.ReadLegacy(old))
	r := must(registry.Open(db))
	if got, want := marshal(r.List()), marshal(proto.NewRegistryList(reg.Projects, []string{f.tree}, reg.Settings)); got != want {
		t.Errorf("registry.list\n got %s\nwant %s", got, want)
	}
	if got := r.Load().AgentWorktrees; !slices.Equal(got, reg.AgentWorktrees) {
		t.Errorf("agent worktrees %v, want %v", got, reg.AgentWorktrees)
	}

	var bf board.File
	readFile(t, filepath.Join(old, "state", "board.json"), &bf)
	for i := range bf.Tickets {
		if bf.Tickets[i].Number == 0 {
			for _, t := range bf.Tickets {
				bf.LastNumber = max(bf.LastNumber, t.Number)
			}
			bf.LastNumber++
			bf.Tickets[i].Number = bf.LastNumber
		}
	}
	ts, limit := bf.Tickets, bf.Limit
	if got, want := tickets(t, db), ts[:2]; !reflect.DeepEqual(got, want) {
		t.Errorf("tickets\n got %+v\nwant %+v", got, want)
	}
	if m := meta(t, db); m["board.limit"] != "3" || limit != 3 || m["board.last_number"] != "9" || m["host_id"] != "host-1" || m["devices.grace_ended"] != "true" || m["imported_at"] == "" {
		t.Errorf("meta %v", m)
	}

	var af automation.File
	readFile(t, filepath.Join(old, "state", "automations.json"), &af)
	autos, runs := automations(t, db)
	if want := af.Automations[:1]; !reflect.DeepEqual(autos, want) {
		t.Errorf("automations\n got %+v\nwant %+v", autos, want)
	}
	if want := []proto.Run{af.Runs[0], af.Runs[2]}; !reflect.DeepEqual(runs, want) {
		t.Errorf("runs\n got %+v\nwant %+v", runs, want)
	}
	// Beside the decoder both sides share, the values it must not lose.
	if len(autos) != 1 || autos[0].WebhookSecret != "s3cret" || len(runs) != 2 || runs[0].TurnAfter != 4 || runs[0].TurnPrompt != "tp" || runs[0].TurnEntry != "te" {
		t.Errorf("lost what the wire never carries: %+v %+v", autos, runs)
	}

	var locals []struct {
		proto.Local
		Auto bool `json:"auto,omitempty"`
	}
	readFile(t, filepath.Join(old, "state", "locals.json"), &locals)
	var gotLocals []string
	rows := must(db.Query(`SELECT l.id, p.id, l.name, l.auto FROM locals l JOIN projects p ON p.id = l.project_id ORDER BY l.position`))
	for rows.Next() {
		var id, project, name string
		var auto bool
		rows.Scan(&id, &project, &name, &auto)
		gotLocals = append(gotLocals, marshal([]any{id, project, name, auto}))
	}
	rows.Close()
	if want := []string{marshal([]any{"l1", "p1", locals[0].Name, locals[0].Auto}), marshal([]any{"l3", "p2", locals[2].Name, locals[2].Auto})}; !slices.Equal(gotLocals, want) {
		t.Errorf("locals %v, want %v", gotLocals, want)
	}

	var wantNames map[string]names.Entry
	readFile(t, filepath.Join(old, "state", "names.json"), &wantNames)
	gotNames := map[string]names.Entry{}
	rows = must(db.Query(`SELECT path, title, auto FROM names`))
	for rows.Next() {
		var p string
		var e names.Entry
		rows.Scan(&p, &e.Title, &e.Auto)
		gotNames[p] = e
	}
	rows.Close()
	if !reflect.DeepEqual(gotNames, wantNames) {
		t.Errorf("names %v, want %v", gotNames, wantNames)
	}

	var gotHosts []hosts.Host
	rows = must(db.Query(`SELECT id, name, target, windows FROM hosts ORDER BY position`))
	for rows.Next() {
		var h hosts.Host
		var target string
		rows.Scan(&h.ID, &h.Name, &target, &h.Windows)
		decode(t, target, &h.Target)
		gotHosts = append(gotHosts, h)
	}
	rows.Close()
	var hf struct{ Hosts []hosts.Host }
	readFile(t, filepath.Join(old, "state", "hosts.json"), &hf)
	if !reflect.DeepEqual(gotHosts, hf.Hosts) {
		t.Errorf("hosts %+v, want %+v", gotHosts, hf.Hosts)
	}
	if loaded := must(hosts.Load(db)); !reflect.DeepEqual(loaded, hf.Hosts) {
		t.Errorf("hosts.Load %+v, want %+v", loaded, hf.Hosts)
	}
	if id := must(state.HostID(db)); id != "host-1" {
		t.Errorf("state.HostID %q", id)
	}

	var df struct{ Devices []devices.Device }
	readFile(t, filepath.Join(old, "devices.json"), &df)
	var gotDevices []devices.Device
	rows = must(db.Query(`SELECT id, name, platform, scopes, token_hash, created_at, last_seen_at, last_addr, legacy, grace_ends_at FROM devices ORDER BY position`))
	for rows.Next() {
		var d devices.Device
		var scopes string
		rows.Scan(&d.ID, &d.Name, &d.Platform, &scopes, &d.TokenHash, &d.CreatedAt, &d.LastSeenAt, &d.LastAddr, &d.Legacy, &d.GraceEndsAt)
		decode(t, scopes, &d.Scopes)
		gotDevices = append(gotDevices, d)
	}
	rows.Close()
	if len(gotDevices) != 2 || gotDevices[1].TokenHash != "h1" || !gotDevices[1].Legacy {
		t.Errorf("devices %+v", gotDevices)
	}
	if !reflect.DeepEqual(gotDevices, df.Devices) {
		t.Errorf("devices %+v, want %+v", gotDevices, df.Devices)
	}

	var pf struct {
		Records map[string]*push.Record
		Sends   []push.Sent
	}
	readFile(t, push.Path(old), &pf)
	wantRecords := []push.Record{}
	for _, r := range pf.Records {
		wantRecords = append(wantRecords, *r)
	}
	slices.SortFunc(wantRecords, func(a, b push.Record) int { return int(a.UpdatedAt - b.UpdatedAt) })
	if opened := must(push.Open(db)); !reflect.DeepEqual(opened.Records(), wantRecords) || !reflect.DeepEqual(opened.Sends(), pf.Sends) {
		t.Errorf("push.Open\n got %s %s\nwant %s %s", marshal(opened.Records()), marshal(opened.Sends()), marshal(wantRecords), marshal(pf.Sends))
	}
	var gotRecords []push.Record
	rows = must(db.Query(`SELECT device, key, env, alert, start, activity, prefs, frequent, updated_at FROM push_records ORDER BY updated_at`))
	for rows.Next() {
		var p push.Record
		var alert, start, activity sql.NullString
		var prefs string
		rows.Scan(&p.Device, &p.Key, &p.Env, &alert, &start, &activity, &prefs, &p.Frequent, &p.UpdatedAt)
		for raw, into := range map[sql.NullString]any{alert: &p.Alert, start: &p.Start, activity: &p.Activity} {
			if raw.Valid {
				decode(t, raw.String, into)
			}
		}
		decode(t, prefs, &p.Prefs)
		gotRecords = append(gotRecords, p)
	}
	rows.Close()
	if want := wantRecords; !reflect.DeepEqual(gotRecords, want) {
		t.Errorf("push records\n got %s\nwant %s", marshal(gotRecords), marshal(want))
	}
	var gotSends []push.Sent
	rows = must(db.Query(`SELECT at, device, kind, agent, priority, outcome FROM push_sends ORDER BY position`))
	for rows.Next() {
		var s push.Sent
		rows.Scan(&s.At, &s.Device, &s.Kind, &s.Agent, &s.Priority, &s.Outcome)
		gotSends = append(gotSends, s)
	}
	rows.Close()
	if want := pf.Sends; !reflect.DeepEqual(gotSends, want) {
		t.Errorf("push sends %+v, want %+v", gotSends, want)
	}

	gotConfig := map[string]string{}
	rows = must(db.Query(`SELECT key, value FROM config`))
	for rows.Next() {
		var k, v string
		rows.Scan(&k, &v)
		gotConfig[k] = v
	}
	rows.Close()
	if len(gotRecords) != 2 || string(gotRecords[0].Key) != "\x01\x02\x03" || gotRecords[0].Alert.Secret != "s" {
		t.Errorf("push records %+v", gotRecords)
	}
	wantConfig := map[string]string{"port": "4600", "listen": `"loopback"`, "phone.maxAccess": `"edits"`, "phone.terminals": `"on"`, "profiles.work": `{"cmd":"x"}`, "token": `"tok"`, "empty": "{}"}
	if !reflect.DeepEqual(gotConfig, wantConfig) {
		t.Errorf("config %v, want %v", gotConfig, wantConfig)
	}
}

func must[T any](v T, err error) T {
	if err != nil {
		panic(err)
	}
	return v
}

func TestTheImportLeavesEveryFileAsItWasAndALaterChangeToo(t *testing.T) {
	f := newFixture(t)
	before := hashes(t, f.home)
	db := open(t, f.home)
	if after := hashes(t, f.home); !reflect.DeepEqual(after, before) {
		t.Fatalf("files changed: %d before, %d after", len(before), len(after))
	}
	r := must(registry.Open(db))
	if err := r.Track("/wt/new", true); err != nil {
		t.Fatal(err)
	}
	if err := r.RemoveProject(f.plain); err != nil {
		t.Fatal(err)
	}
	if after := hashes(t, f.home); !reflect.DeepEqual(after, before) {
		t.Fatal("a registry change wrote a file")
	}
}

func TestARestartKeepsEveryRowAndImportsNothingAgain(t *testing.T) {
	f := newFixture(t)
	db := must(Open(f.home))
	want := map[string]int{}
	tables := []string{"projects", "worktrees", "shared_settings", "config", "locals", "names", "tickets", "automations", "runs", "hosts", "devices", "push_records", "push_sends", "meta"}
	for _, table := range tables {
		want[table] = count(t, db, table)
	}
	at := meta(t, db)["imported_at"]
	db.Close()
	writeJSON(t, filepath.Join(f.home, "state", "registry.json"), map[string]any{"projects": []any{}})
	os.Remove(filepath.Join(f.home, "state", "orchestrator.db"))
	db = open(t, f.home)
	for _, table := range tables {
		if n := count(t, db, table); n != want[table] || n == 0 {
			t.Errorf("%s: %d rows, want %d", table, n, want[table])
		}
	}
	if got := meta(t, db)["imported_at"]; got != at {
		t.Errorf("imported again: %s, then %s", at, got)
	}
}

func TestAHomeWithOnlyDesktopJSONImportsItsProjectsAndHosts(t *testing.T) {
	home := t.TempDir()
	writeJSON(t, filepath.Join(home, "desktop.json"), map[string]any{
		"projects": []string{"/w/app", "//box/w/other"}, "repos": map[string]any{"/w/app": map[string]any{"name": "App"}}, "tracked": []string{"/wt/a", "/wt/a"}, "worktree": map[string]any{"root": "/wt"},
		"hosts": []any{map[string]any{"id": "h1", "name": "box", "target": map[string]any{"kind": "address", "addr": "10.0.0.1", "port": 22, "key_file": "/k"}}},
	})
	db := open(t, home)
	l := must(registry.Open(db)).List()
	if len(l.Projects) != 1 || l.Projects[0].Path != "/w/app" || l.Projects[0].Repo.Name != "App" || len(l.Projects[0].ID) != 16 ||
		!slices.Equal(l.Tracked, []string{"/wt/a"}) || l.Settings.WorktreeRoot != "/wt" {
		t.Fatalf("%+v", l)
	}
	var id, target string
	if err := db.QueryRow(`SELECT id, target FROM hosts`).Scan(&id, &target); err != nil || id != "h1" || target != `{"kind":"address","addr":"10.0.0.1","port":22,"keyFile":"/k"}` {
		t.Fatalf("%s %s %v", id, target, err)
	}
	if _, err := os.Stat(filepath.Join(home, "state", "hosts.json")); !errors.Is(err, fs.ErrNotExist) {
		t.Fatalf("hosts.json: %v", err)
	}
	if n := count(t, db, "meta"); n != 1 {
		t.Fatalf("meta holds %v", meta(t, db))
	}
}

func TestATicketInAWorktreeKeepsItsFolderAndOneInNoProjectIsDroppedByName(t *testing.T) {
	f := newFixture(t)
	logs := logged(t)
	db := open(t, f.home)
	var tree sql.NullString
	if err := db.QueryRow(`SELECT tree FROM tickets WHERE id = 't2'`).Scan(&tree); err != nil || tree.String != f.tree {
		t.Fatalf("t2's tree %v %v", tree, err)
	}
	if err := db.QueryRow(`SELECT tree FROM tickets WHERE id = 't1'`).Scan(&tree); err != nil || tree.Valid {
		t.Fatalf("t1's tree %v %v", tree, err)
	}
	for _, want := range []string{"ticket t3: its folder " + f.nowhere, "automation a2: its folder " + f.nowhere, "run r2", "local l2"} {
		if !strings.Contains(logs.String(), want) {
			t.Errorf("the log doesn't say %q:\n%s", want, logs)
		}
	}
}

func TestAnImportThatFailsHalfwayLeavesNothingAndRunsAgainAtTheNextOpen(t *testing.T) {
	f := newFixture(t)
	pushes := push.Path(f.home)
	raw, _ := os.ReadFile(pushes)
	os.Remove(pushes)
	os.Mkdir(pushes, 0o700)
	if db, err := Open(f.home); err == nil {
		db.Close()
		t.Fatal("imported a home whose push.json can't be read")
	}
	db, err := sql.Open("sqlite", Path(f.home))
	if err != nil {
		t.Fatal(err)
	}
	for _, table := range []string{"meta", "projects", "tickets", "hosts", "devices", "config"} {
		if n := count(t, db, table); n != 0 {
			t.Errorf("%s holds %d rows", table, n)
		}
	}
	db.Close()
	os.Remove(pushes)
	os.WriteFile(pushes, raw, 0o600)
	db = open(t, f.home)
	if n := count(t, db, "push_records"); n != 2 || meta(t, db)["imported_at"] == "" {
		t.Fatalf("%d push records, meta %v", n, meta(t, db))
	}
}

func TestAFileThatCantBeParsedImportsAsEmptyAndIsLeftAsItWas(t *testing.T) {
	f := newFixture(t)
	for _, name := range []string{"state/registry.json", "state/board.json", "devices.json"} {
		os.WriteFile(filepath.Join(f.home, name), []byte("{"), 0o600)
	}
	writeJSON(t, filepath.Join(f.home, "state", "automations.json"), map[string]any{"version": 2})
	logs := logged(t)
	db := open(t, f.home)
	if l := must(registry.Open(db)).List(); len(l.Projects) != 1 || l.Projects[0].Path != "/w/desktop" {
		t.Fatalf("not desktop.json's projects: %+v", l.Projects)
	}
	for _, table := range []string{"tickets", "automations", "devices"} {
		if n := count(t, db, table); n != 0 {
			t.Errorf("%s holds %d rows", table, n)
		}
	}
	for _, name := range []string{"board.json", "devices.json", "automations.json"} {
		if !strings.Contains(logs.String(), name) {
			t.Errorf("the log doesn't name %s:\n%s", name, logs)
		}
	}
	entries, _ := filepath.Glob(filepath.Join(f.home, "*", "*.b*"))
	if top, _ := filepath.Glob(filepath.Join(f.home, "*.b*")); len(entries)+len(top) != 0 {
		t.Fatalf("set aside: %v %v", entries, top)
	}
	if raw, _ := os.ReadFile(filepath.Join(f.home, "state", "board.json")); string(raw) != "{" {
		t.Fatalf("board.json is %q", raw)
	}
}

func TestALocalsOrNamesFileWithAMistypedFieldImportsAsEmpty(t *testing.T) {
	f := newFixture(t)
	writeJSON(t, filepath.Join(f.home, "state", "locals.json"), []any{
		map[string]any{"id": "good", "project": f.project, "name": "Local 2"},
		map[string]any{"id": "bad", "project": f.project, "name": 7},
	})
	writeJSON(t, filepath.Join(f.home, "state", "names.json"), map[string]any{"/a": map[string]any{"title": "A"}, "/b": map[string]any{"title": 5}})
	db := open(t, f.home)
	for _, table := range []string{"locals", "names"} {
		if n := count(t, db, table); n != 0 {
			t.Errorf("%s holds %d rows", table, n)
		}
	}
}

func TestRemovingAProjectDeletesItsTicketsAutomationsRunsAndLocals(t *testing.T) {
	f := newFixture(t)
	db := open(t, f.home)
	r := must(registry.Open(db))
	var heard []string
	r.OnRemove(func(id string) { heard = append(heard, id) })
	if err := r.RemoveProject(f.project); err != nil {
		t.Fatal(err)
	}
	for table, want := range map[string]int{"tickets": 0, "automations": 0, "runs": 0, "locals": 1, "projects": 1} {
		if n := count(t, db, table); n != want {
			t.Errorf("%s holds %d rows, want %d", table, n, want)
		}
	}
	if !slices.Equal(heard, []string{"p1"}) {
		t.Fatalf("heard %v", heard)
	}
}

func TestOpenReadOnlyReadsBesideTheDaemonAndAfterItAndWritesNothing(t *testing.T) {
	home := t.TempDir()
	if _, err := OpenReadOnly(home); !errors.Is(err, ErrNoDatabase) {
		t.Fatalf("err = %v", err)
	}
	if _, err := os.Stat(Path(home)); !errors.Is(err, fs.ErrNotExist) {
		t.Fatalf("made %s: %v", Path(home), err)
	}
	writeJSON(t, filepath.Join(home, "desktop.json"), map[string]any{"projects": []string{"/w/app"}})
	daemon := must(Open(home))
	read := func() {
		t.Helper()
		ro, err := OpenReadOnly(home)
		if err != nil {
			t.Fatal(err)
		}
		defer ro.Close()
		if l := must(registry.Open(ro)).Load(); !slices.Equal(l.Projects, []string{"/w/app"}) {
			t.Fatalf("%+v", l)
		}
		if _, err := ro.Exec(`INSERT INTO meta (key, value) VALUES ('x', 'y')`); err == nil {
			t.Fatal("wrote through a read-only handle")
		}
	}
	read()
	if _, err := daemon.Exec(`INSERT INTO names (path, title) VALUES ('/wt/a', 'A')`); err != nil {
		t.Fatal(err)
	}
	read()
	daemon.Close()
	read()
}
