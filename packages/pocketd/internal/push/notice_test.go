package push

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func TestTheLineIsCutTo240CharactersAsTheDesktopCutsIt(t *testing.T) {
	long := strings.Repeat("é", 300)
	if got := Cut(long); got != strings.Repeat("é", 240)+"…" {
		t.Fatalf("%q", got)
	}
	if got := Cut("Wants to run ls"); got != "Wants to run ls" {
		t.Fatalf("%q", got)
	}
}

func TestTheLineIsTheAskTheReplysFirstParagraphOrTheStatus(t *testing.T) {
	shell := proto.PermissionRequest{ToolName: "Bash", Detail: proto.ToolDetail{Kind: "shell", Command: "npm test"}}
	mcp := proto.PermissionRequest{ToolName: "mcp__linear__create"}
	for _, c := range []struct {
		kind  string
		ask   *proto.PermissionRequest
		reply string
		want  string
	}{
		{NeedsYou, &shell, "", "Wants to run npm test"},
		{NeedsYou, &mcp, "", "Wants to use mcp__linear__create"},
		{NeedsYou, nil, "", "Needs you"},
		{Done, nil, "\n\n  All   tests\npass now.\n\nSecond paragraph.", "All tests pass now."},
		{Done, nil, "", "Done"},
		{Failed, nil, "Something", "Failed"},
	} {
		if got := Line(c.kind, c.ask, c.reply); got != c.want {
			t.Errorf("%s: %q, want %q", c.kind, got, c.want)
		}
	}
}

func TestTheTitleAndPlaceNameTheSessionAndWhereItRuns(t *testing.T) {
	a := proto.AgentSummary{Provider: "claude", Project: "anywhere", Worktree: "calm-otter"}
	if Title(a) != "Claude Code" || Place(a) != "anywhere · calm-otter" {
		t.Fatalf("%q %q", Title(a), Place(a))
	}
	a.Title, a.Worktree = "Fix the flaky test", ""
	if Title(a) != "Fix the flaky test" || Place(a) != "anywhere" {
		t.Fatalf("%q %q", Title(a), Place(a))
	}
	if Place(proto.AgentSummary{}) != "" || Title(proto.AgentSummary{Provider: "zsh"}) != "Shell" {
		t.Fatal("outside a project")
	}
}

func TestEachAskGetsTheCategoryWhoseActionsCanAnswerIt(t *testing.T) {
	plain := proto.PermissionRequest{ToolName: "Bash"}
	note := proto.PermissionRequest{ToolName: "Bash", Feedback: true}
	options := proto.PermissionRequest{ToolName: "Edit", Options: []proto.PermissionOption{{ID: "always", Label: "Always"}}}
	question := proto.PermissionRequest{ToolName: "AskUserQuestion"}
	for _, c := range []struct {
		kind string
		ask  *proto.PermissionRequest
		want string
	}{
		{NeedsYou, &plain, "PERM"}, {NeedsYou, &note, "PERM_NOTE"}, {NeedsYou, &options, "QUESTION"},
		{NeedsYou, &question, "QUESTION"}, {NeedsYou, nil, "QUESTION"},
		{Done, nil, "DONE"}, {Failed, nil, "FAILED"}, {Automation, nil, "AUTOMATION"}, {Test, nil, ""},
	} {
		if got := Category(c.kind, c.ask); got != c.want {
			t.Errorf("%s %+v: %q", c.kind, c.ask, got)
		}
	}
	if Level(NeedsYou, false) != "time-sensitive" || Level(Done, false) != "active" || Level(Failed, false) != "active" ||
		Level(Automation, false) != "passive" || Level(Automation, true) != "active" {
		t.Fatal("levels")
	}
}

func TestAnAlertShowsOnlyGenericTextAndSealsTheNotice(t *testing.T) {
	key := bytes.Repeat([]byte{9}, 32)
	n := Notice{Mac: "studio", Agent: "a1", Kind: NeedsYou, Title: "Secret project", Place: "p · w", Line: "Wants to run rm -rf /", Request: "r1", Badge: 3}
	raw, err := AlertPayload(key, n, "PERM", "time-sensitive")
	if err != nil {
		t.Fatal(err)
	}
	if bytes.Contains(raw, []byte("Secret")) || bytes.Contains(raw, []byte("rm -rf")) || bytes.Contains(raw, []byte(`"a1"`)) {
		t.Fatalf("leaks: %s", raw)
	}
	var p struct {
		Aps map[string]any `json:"aps"`
		E   string         `json:"e"`
	}
	json.Unmarshal(raw, &p)
	alert := p.Aps["alert"].(map[string]any)
	if alert["title"] != "Anywhere" || alert["body"] != "A session needs you" || p.Aps["mutable-content"] != 1.0 || p.Aps["category"] != "PERM" ||
		p.Aps["interruption-level"] != "time-sensitive" || p.Aps["sound"] != "default" || p.Aps["thread-id"] != Opaque(key, "a1") || p.Aps["badge"] != 3.0 {
		t.Fatalf("%v", p.Aps)
	}
	plain, err := Unseal(key, "studio", p.E)
	var got Notice
	if err != nil || json.Unmarshal(plain, &got) != nil || got != n {
		t.Fatalf("%+v %v", got, err)
	}
	quiet, _ := AlertPayload(key, Notice{Mac: "studio", Kind: Automation}, "AUTOMATION", "passive")
	if bytes.Contains(quiet, []byte("sound")) || bytes.Contains(quiet, []byte("thread-id")) {
		t.Fatalf("%s", quiet)
	}
}
