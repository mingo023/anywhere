//go:build unix

package daemon

import (
	"encoding/json"
	"flag"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"
	"time"

	"pocketd/internal/claude"
	"pocketd/internal/handoff"
	"pocketd/internal/proto"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
	"pocketd/internal/timeline"
)

var update = flag.Bool("update", false, "rewrite the sub-agent timeline golden")

// The desktop and phone decode this golden too, so it pins what they get.
const subagentGolden = "../proto/testdata/golden/server/timeline_subagents.json"

// TestASubagentSessionPagesEachSubagentUnderItsAgentCall replays a Claude
// session that ran two sub-agents, one at a time so seqs are deterministic.
func TestASubagentSessionPagesEachSubagentUnderItsAgentCall(t *testing.T) {
	src := filepath.Join("testdata", "subagents")
	dir := t.TempDir()
	path := filepath.Join(dir, "s1.jsonl")
	raw, _ := os.ReadFile(filepath.Join(src, "s1.jsonl"))
	// The Agent calls' results come after both sub-agents ran.
	results := strings.Index(string(raw), `{"parentUuid":"00000000-0000-4000-8000-000000000005"`)
	os.WriteFile(path, raw[:results], 0o600)

	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	hookFrom(d, pr, sessionStart("s1", path))
	last := func() proto.Item {
		items, _ := pr.a.Timeline.PageWithSubagents(0, 100)
		if len(items) == 0 {
			return proto.Item{}
		}
		return items[len(items)-1]
	}
	// A sub-agent file read before the session's last Agent call would take its seq.
	eventually(t, "both Agent calls", func() bool { c := last().Call; return c != nil && c.ToolUseID == "toolu_count" })
	os.MkdirAll(filepath.Join(dir, "s1", "subagents"), 0o700)
	for _, sub := range []struct{ name, parent, last string }{
		{"agent-a1157000000000001", "toolu_list", "README.md and src/"},
		{"agent-ac0c0000000000002", "toolu_count", "1, 2, 3"},
	} {
		for _, ext := range []string{".meta.json", ".jsonl"} {
			b, _ := os.ReadFile(filepath.Join(src, "s1", "subagents", sub.name+ext))
			os.WriteFile(filepath.Join(dir, "s1", "subagents", sub.name+ext), b, 0o600)
		}
		eventually(t, sub.name, func() bool { it := last(); return it.ParentToolUseID == sub.parent && it.Text == sub.last })
	}
	appendLine(t, path, strings.TrimSuffix(string(raw[results:]), "\n"))
	eventually(t, "the turn's end", func() bool { return last().Kind == "result" })

	items, older := pr.a.Timeline.PageWithSubagents(0, 100)
	for i := range items {
		items[i].Ts = items[i].Seq
		if c := items[i].Call; c != nil && c.DurationMs != nil {
			call, zero := *c, int64(0)
			call.DurationMs = &zero
			items[i].Call = &call
		}
	}
	epoch, maxSeq := pr.a.Timeline.State()
	got, _ := json.MarshalIndent(proto.NewAgentTimeline("t1", "a1", epoch, items, older, maxSeq), "", "  ")
	if *update {
		os.WriteFile(subagentGolden, append(got, '\n'), 0o644)
		return
	}
	want, _ := os.ReadFile(subagentGolden)
	if strings.TrimSpace(string(want)) != string(got) {
		t.Fatalf("got %s\nwant %s", got, want)
	}
}

// TestASessionReadAgainKeepsEachSubagentRightAfterItsCall replays a finished
// session, as pocketd does after an upgrade, with one sub-agent that ran
// another: each sub-agent's items come right after the call that spawned it.
func TestASessionReadAgainKeepsEachSubagentRightAfterItsCall(t *testing.T) {
	src := filepath.Join("testdata", "subagents")
	dir := t.TempDir()
	os.MkdirAll(filepath.Join(dir, "s1", "subagents"), 0o700)
	for _, name := range []string{"s1.jsonl", "s1/subagents/agent-a1157000000000001.meta.json", "s1/subagents/agent-a1157000000000001.jsonl", "s1/subagents/agent-ac0c0000000000002.meta.json", "s1/subagents/agent-ac0c0000000000002.jsonl"} {
		b, _ := os.ReadFile(filepath.Join(src, name))
		os.WriteFile(filepath.Join(dir, name), b, 0o600)
	}
	os.WriteFile(filepath.Join(dir, "s1", "subagents", "agent-nested.meta.json"), []byte(`{"toolUseId":"toolu_ls"}`), 0o600)
	os.WriteFile(filepath.Join(dir, "s1", "subagents", "agent-nested.jsonl"), []byte(`{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"deep"}]}}`+"\n"), 0o600)

	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	hookFrom(d, pr, sessionStart("s1", filepath.Join(dir, "s1.jsonl")))
	want := []string{"user", "toolu_list", "  toolu_list", "  toolu_list", "  toolu_ls", "  toolu_list", "toolu_count", "  toolu_count", "  toolu_count", "assistant", "result"}
	eventually(t, "every item", func() bool { items, _ := pr.a.Timeline.PageWithSubagents(0, 100); return len(items) == len(want) })

	items, _ := pr.a.Timeline.PageWithSubagents(0, 100)
	var got []string
	for _, it := range items {
		switch {
		case it.ParentToolUseID != "":
			got = append(got, "  "+it.ParentToolUseID)
		case it.Call != nil:
			got = append(got, it.Call.ToolUseID)
		default:
			got = append(got, it.Kind)
		}
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %q\nwant %q", got, want)
	}
}

const (
	subBrief  = `{"type":"user","isSidechain":true,"message":{"role":"user","content":"look around"}}`
	subFirst  = `{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"first"}]}}`
	subLs     = `{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"tool_use","id":"s1","name":"Bash","input":{"command":"ls"}}]}}`
	subLsDone = `{"type":"user","isSidechain":true,"message":{"content":[{"type":"tool_result","tool_use_id":"s1","content":"a b"}]}}`
	subSecond = `{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"second"}]}}`
	subPwd    = `{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"tool_use","id":"s2","name":"Bash","input":{"command":"pwd"}}]}}`
)

// runningSubagent writes a conversation whose background Agent call t1 runs
// sub-agent a1, which wrote sub; the store kept it up to the first stored
// lines of sub.
func runningSubagent(t *testing.T, d *Daemon, sub []string, stored int) (path, subPath string) {
	t.Helper()
	path = projectTranscript(t)
	appendLine(t, path, `{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"look around","prompt":"look around","run_in_background":true}}]}}`)
	appendLine(t, path, `{"type":"user","toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a1"},"message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Async agent launched successfully."}]}}`)
	dir := filepath.Join(strings.TrimSuffix(path, ".jsonl"), "subagents")
	os.MkdirAll(dir, 0o700)
	os.WriteFile(filepath.Join(dir, "agent-a1.meta.json"), []byte(`{"toolUseId":"t1"}`), 0o600)
	subPath = filepath.Join(dir, "agent-a1.jsonl")
	os.WriteFile(subPath, []byte(strings.Join(sub, "\n")+"\n"), 0o600)

	tl := timeline.New()
	raw, _ := os.ReadFile(path)
	for _, line := range strings.Split(strings.TrimSpace(string(raw)), "\n") {
		for _, e := range claude.Map([]byte(line)) {
			tl.Apply(e, 1)
		}
	}
	for _, line := range sub[:stored] {
		for _, e := range claude.MapSubagent([]byte(line), "t1") {
			tl.Apply(e, 1)
		}
	}
	items, _ := tl.PageWithSubagents(0, 100)
	d.Agents.History = func(string) ([]proto.Item, bool) { return items, true }
	return path, subPath
}

// under is what the timeline shows under call t1, and t1's own status.
func under(pr *presence) (rows []string, status string) {
	items, _ := pr.a.Timeline.PageWithSubagents(0, 100)
	for _, it := range items {
		switch {
		case it.ParentToolUseID == "t1" && it.Call != nil:
			rows = append(rows, "tool "+it.Call.ToolUseID+" "+it.Call.Status)
		case it.ParentToolUseID == "t1":
			rows = append(rows, it.Kind+" "+it.Text)
		case it.Call != nil && it.Call.ToolUseID == "t1":
			status = it.Call.Status
		}
	}
	return rows, status
}

func TestAnUpgradeFollowsARunningSubAgentFromWhatTheStoreKeptAndItsNoticeEndsItsCall(t *testing.T) {
	d := newDaemon(t)
	path, subPath := runningSubagent(t, d, []string{subBrief, subFirst, subLs, subLsDone, subSecond}, 2)
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "claude"), Env: []string{"PATH=/bin:/usr/bin", "CLAUDE_CONFIG_DIR=" + os.Getenv("CLAUDE_CONFIG_DIR")}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	saved := state.Terminal{TerminalID: term.Info().ID, LaunchDir: t.TempDir(), Cols: 80, Rows: 24, Provider: "claude", ConversationID: "s1",
		TranscriptPath: path, Launch: &state.Launch{Access: "ask"}, AgentID: "a-1", CreatedAt: 7, Status: "working"}

	d.expect(saved, term, "", &handoff.Agent{Phase: "working", Seq: 1, Read: written(path), Background: timeline.Background{Tasks: map[string]string{"a1": "t1"}, Lasting: []string{"t1"}}})
	pr := waitPresent(t, d, term)

	caught := []string{"user look around", "assistant first", "tool s1 ok", "assistant second"}
	eventually(t, "what the sub-agent wrote past the store", func() bool { rows, _ := under(pr); return len(rows) >= len(caught) })
	appendLine(t, subPath, subPwd)
	eventually(t, "what it writes next", func() bool { rows, _ := under(pr); return len(rows) > len(caught) })
	appendLine(t, path, `{"type":"user","origin":{"kind":"task-notification"},"message":{"content":"<task-notification>\n<task-id>a1</task-id>\n<status>completed</status>\n<result>looked</result>\n</task-notification>"}}`)
	eventually(t, "its call's end", func() bool { _, status := under(pr); return status == "ok" })

	if rows, _ := under(pr); !slices.Equal(rows, append(caught, "tool s2 running")) {
		t.Fatalf("under the call: %q", rows)
	}
}

func TestAColdRestartShowsOnceWhatASubAgentWroteBeforeItAndEndsItsCall(t *testing.T) {
	d := newDaemon(t)
	path, _ := runningSubagent(t, d, []string{subBrief, subFirst, subLs, subLsDone, subSecond, subPwd}, 2)
	resuming(d, fakeAgent(t, "claude"), "--resume", "s1")
	saved := savedAgent(t.TempDir(), "claude")
	saved.Terminals[0].TranscriptPath = path
	d.Restore(saved, "/bin/sh")
	term, _ := restoredIn(t, d)
	pr := waitPresent(t, d, term)

	want := []string{"user look around", "assistant first", "tool s1 ok", "assistant second", "tool s2 ok"}
	eventually(t, "what the sub-agent wrote past the store", func() bool { rows, _ := under(pr); return len(rows) >= len(want) })
	time.Sleep(200 * time.Millisecond)
	if rows, status := under(pr); !slices.Equal(rows, want) || status != "ok" {
		t.Fatalf("under the call: %q, the call %s", rows, status)
	}
}

// shape is the items a timeline shows, sub-agents' included, without their ids and times.
func shape(a interface {
	PageWithSubagents(int64, int) ([]proto.Item, bool)
}) []string {
	items, _ := a.PageWithSubagents(0, 1000)
	var out []string
	for _, it := range items {
		row := it.ParentToolUseID + " " + it.Kind + " " + it.Text
		if c := it.Call; c != nil {
			row += c.ToolUseID + " " + c.Status
		}
		out = append(out, row)
	}
	return out
}

func TestARestoreAddsNothingToAFinishedConversationWithSubAgents(t *testing.T) {
	cfg := t.TempDir()
	t.Setenv("CLAUDE_CONFIG_DIR", cfg)
	dir := filepath.Join(cfg, "projects", "-w")
	src := filepath.Join("testdata", "subagents")
	os.MkdirAll(filepath.Join(dir, "s1", "subagents"), 0o700)
	for _, name := range []string{"s1.jsonl", "s1/subagents/agent-a1157000000000001.meta.json", "s1/subagents/agent-a1157000000000001.jsonl", "s1/subagents/agent-ac0c0000000000002.meta.json", "s1/subagents/agent-ac0c0000000000002.jsonl"} {
		b, _ := os.ReadFile(filepath.Join(src, name))
		os.WriteFile(filepath.Join(dir, name), b, 0o600)
	}
	path := filepath.Join(dir, "s1.jsonl")
	live := newDaemon(t)
	_, pr := claudeIn(t, live)
	hookFrom(live, pr, sessionStart("s1", path))
	eventually(t, "the live timeline", func() bool { return len(shape(pr.a.Timeline)) == 10 })
	stored, _ := pr.a.Timeline.PageWithSubagents(0, 1000)
	want := shape(pr.a.Timeline)

	for _, handed := range []bool{false, true} {
		d := newDaemon(t)
		d.Agents.History = func(string) ([]proto.Item, bool) { return stored, true }
		var term *terminal.Terminal
		if handed {
			var err error
			term, err = d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "claude"), Env: []string{"PATH=/bin:/usr/bin", "CLAUDE_CONFIG_DIR=" + cfg}})
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(term.Close)
			saved := state.Terminal{TerminalID: term.Info().ID, LaunchDir: t.TempDir(), Cols: 80, Rows: 24, Provider: "claude", ConversationID: "s1",
				TranscriptPath: path, Launch: &state.Launch{Access: "ask"}, AgentID: "a-1", CreatedAt: 7, Status: "idle"}
			d.expect(saved, term, "", &handoff.Agent{Phase: "idle", Seq: stored[len(stored)-1].Seq, Read: written(path)})
		} else {
			resuming(d, fakeAgent(t, "claude"), "--resume", "s1")
			saved := savedAgent(t.TempDir(), "claude")
			saved.Terminals[0].TranscriptPath = path
			d.Restore(saved, "/bin/sh")
			term, _ = restoredIn(t, d)
		}
		again := waitPresent(t, d, term)
		time.Sleep(700 * time.Millisecond)
		if got := shape(again.a.Timeline); !slices.Equal(got, want) {
			t.Fatalf("handed %v:\n%q\nwant\n%q", handed, got, want)
		}
	}
}
