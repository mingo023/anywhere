package orch

import (
	"maps"
	"slices"
	"strings"
	"testing"
)

func TestWhatARestartBringsBackIsTheLastSetSaved(t *testing.T) {
	s := openTest(t)
	if err := s.SetRestorable(map[string][]byte{"a": []byte(`{"agentId":"a"}`), "b": []byte(`{"agentId":"b","createdAt":7}`)}); err != nil {
		t.Fatal(err)
	}
	want := map[string][]byte{"b": []byte(`{"agentId":"b","createdAt":7}`)}
	if err := s.SetRestorable(want); err != nil {
		t.Fatal(err)
	}
	got, err := s.Restorable()
	if err != nil || !maps.EqualFunc(got, want, func(a, b []byte) bool { return string(a) == string(b) }) {
		t.Fatalf("restorable %q, %v", got, err)
	}
	if b, _ := s.Session("b"); b == nil || b.CreatedAt != 7 {
		t.Fatalf("session b = %+v", b)
	}
}

func TestAWorktreesSubagentTreesReachEveryGenerationButNotOnesSomeoneElseWorksIn(t *testing.T) {
	s := openTest(t)
	execAll(t, s,
		`INSERT INTO sessions (id, status, cwd, parent_id, relationship, closed_at) VALUES
			('p', 'closed', '/wt/src', '', '', 5),
			('c', 'closed', '/wt-c', 'p', 'subagent', 5),
			('g', 'closed', '/wt-g', 'c', 'subagent', 5),
			('shared', 'idle', '/wt', 'p', 'subagent', 0),
			('fork', 'idle', '/wt-f', 'p', 'fork', 0),
			('busy', 'idle', '/wt-b', 'p', 'subagent', 0),
			('user', 'idle', '/wt-b', '', '', 0),
			('was', 'closed', '/wt-c', '', '', 5),
			('host/x', 'idle', '/wt-h', 'p', 'subagent', 0),
			('devbox/hc', 'closed', '/home/wt-hc', 'c', 'subagent', 5),
			('devbox/shared', 'idle', '/home/app', 'p', 'subagent', 0),
			('old', 'closed', '/wt-n', 'p', 'subagent', 5)`,
	)
	// /wt-n was made again at 10, after its old session closed.
	execAll(t, s, `INSERT INTO delegations (task_id, parent_id, child_id, isolation, created_at) VALUES ('t1', 'c', 'devbox/hc', 'worktree', 1), ('t2', 'p', 'devbox/shared', 'shared', 1)`)
	treeOf := func(cwd string, at int64) string {
		if cwd == "/wt-n" && at < 10 {
			return ""
		}
		return strings.TrimSuffix(cwd, "/src")
	}
	got, err := s.SubagentTrees("/wt", treeOf)
	if err != nil || !slices.Equal(got.Local, []string{"/wt-c", "/wt-g"}) || !slices.Equal(got.Hosts, []HostTree{{"devbox", "/home/wt-hc"}}) {
		t.Fatalf("trees %+v, %v", got, err)
	}
}

func TestAStartClosesTheSessionsItBroughtBackNeitherRestoredNorHandedDown(t *testing.T) {
	s := openTest(t)
	execAll(t, s,
		`INSERT INTO sessions (id, status) VALUES ('back', 'idle'), ('gone', 'working'), ('host/x', 'idle')`,
		`INSERT INTO sessions (id, status, closed_at) VALUES ('closed', 'closed', 5)`,
	)
	closed, err := s.CloseAbsent(func(id string) bool { return id == "back" })
	if err != nil || len(closed) != 1 || closed[0] != "gone" {
		t.Fatalf("closed %q, %v", closed, err)
	}
	open, err := s.OpenSessions()
	if err != nil || len(open) != 1 || open[0].ID != "back" {
		t.Fatalf("open %+v, %v", open, err)
	}
	if gone, _ := s.Session("gone"); gone.Status != "closed" || gone.ClosedAt == 0 {
		t.Fatalf("gone = %+v", gone)
	}
	if remote, _ := s.Session("host/x"); remote.Status != "idle" {
		t.Fatalf("a host's session was closed: %+v", remote)
	}
}
