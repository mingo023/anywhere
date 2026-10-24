package push

import (
	"encoding/json"
	"strings"
	"testing"
	"time"

	"pocketd/internal/proto"
)

func agentAt(id, status string, updated int64) proto.AgentSummary {
	return proto.AgentSummary{ID: id, Status: status, Attached: true, UpdatedAt: updated}
}

func TestTheActivityCountsSessionsAndShowsTheOldestNeedsYouFirst(t *testing.T) {
	failed := agentAt("f", "done", 1000)
	failed.Failed = true
	detached := agentAt("x", "needsYou", 10)
	detached.Attached = false
	agents := []proto.AgentSummary{agentAt("w1", "working", 3000), agentAt("w2", "working", 2000), agentAt("n2", "needsYou", 5000),
		agentAt("n1", "needsYou", 4000), agentAt("d", "done", 500), failed, agentAt("i", "idle", 1), detached}
	ask := proto.PermissionRequest{RequestID: "r1", AgentID: "n1", ToolName: "Bash", Detail: proto.ToolDetail{Kind: "shell", Command: "npm test"}}
	st := Build(agents, []proto.PermissionRequest{ask}, false)
	want := State{Working: 2, NeedsYou: 2, Unseen: 2, Top: &Top{Slot: "n1", Status: NeedsYou, SinceUnix: 4, Answerable: true, Request: "r1"}}
	if a, b := jsonOf(st), jsonOf(want); a != b {
		t.Fatalf("%s\nwant %s", a, b)
	}
	if Relevance(st) != 100 || Badge(agents) != 4 {
		t.Fatalf("%d %d", Relevance(st), Badge(agents))
	}
}

func TestTheAsksVerbAndSubjectShowOnlyWithDetails(t *testing.T) {
	agents := []proto.AgentSummary{agentAt("n", "needsYou", 1000)}
	ask := proto.PermissionRequest{RequestID: "r", AgentID: "n", ToolName: "Bash", Detail: proto.ToolDetail{Kind: "shell", Command: "echo " + strings.Repeat("abcdefghij", 7)}}
	top := Build(agents, []proto.PermissionRequest{ask}, true).Top
	if top.Verb != "Run" || len([]rune(top.Subject)) != 60 || []rune(top.Subject)[59] != '…' {
		t.Fatalf("%+v", top)
	}
	if hidden := Build(agents, []proto.PermissionRequest{ask}, false).Top; hidden.Verb != "" || hidden.Subject != "" {
		t.Fatalf("%+v", hidden)
	}
	ask.Options = []proto.PermissionOption{{ID: "always", Label: "Always allow"}}
	if top := Build(agents, []proto.PermissionRequest{ask}, true).Top; top.Answerable || top.Request != "" {
		t.Fatalf("an ask with options is answerable: %+v", top)
	}
}

func TestWithoutNeedsYouTheLongestWorkingThenFailedThenDoneIsOnTop(t *testing.T) {
	failed := agentAt("f", "done", 1000)
	failed.Failed = true
	for _, c := range []struct {
		agents    []proto.AgentSummary
		top       string
		relevance int
	}{
		{[]proto.AgentSummary{agentAt("w1", "working", 3000), agentAt("w2", "working", 2000), failed}, "w2", 50},
		{[]proto.AgentSummary{agentAt("d", "done", 500), failed}, "f", 75},
		{[]proto.AgentSummary{agentAt("d2", "done", 900), agentAt("d1", "done", 500)}, "d1", 10},
	} {
		st := Build(c.agents, nil, false)
		if st.Top == nil || st.Top.Slot != c.top || Relevance(st) != c.relevance {
			t.Errorf("%+v %d, want %s", st.Top, Relevance(st), c.top)
		}
	}
	if st := Build(nil, nil, false); st.Top != nil || st.Busy() || Relevance(st) != 10 {
		t.Fatalf("%+v", st)
	}
}

func jsonOf(v any) string {
	raw, _ := json.Marshal(v)
	return string(raw)
}

func TestTheActivityPayloadsAreTheOnesThePhoneDecodes(t *testing.T) {
	now := time.Unix(1761300000, 0)
	st := State{Working: 2, NeedsYou: 1, Unseen: 1, Top: &Top{Slot: "k7", Status: NeedsYou, SinceUnix: 1761299950.25, Verb: "Run", Subject: "npm test", Answerable: true, Request: "r_8f2"}}
	golden(t, "activity_update.json", UpdatePayload(st, now, 5*time.Minute, 0, false))
	golden(t, "activity_start.json", StartPayload(st, "studio", "Studio", now, 5*time.Minute))
	golden(t, "activity_end.json", UpdatePayload(State{Unseen: 3}, now, 5*time.Minute, 15*time.Minute, true))
}
