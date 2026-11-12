//go:build unix

package daemon

import (
	"encoding/json"
	"flag"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"pocketd/internal/proto"
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
