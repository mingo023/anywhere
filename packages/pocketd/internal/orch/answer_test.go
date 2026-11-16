package orch

import "testing"

func TestTheUsersAnswerGoesToTheAgentWhetherOrNotTheStoreHasTheAsk(t *testing.T) {
	asks := map[string]*Request{
		"a permission ask": {ID: "r1", Session: "s1", Kind: "permission", State: "open"},
		"an unknown ask":   nil,
	}
	for name, r := range asks {
		d, err := decide(State{Session: &Session{ID: "s1"}, Request: r}, NewCommand("", CmdRequestAnswer, "s1", RequestPayload{RequestID: "r1", Decision: "allow", User: true}))
		if err != nil || len(d.Effects) != 1 || d.Effects[0].Kind != EffAnswer || d.Effects[0].Lane != "s1" {
			t.Fatalf("%s: %+v, %v", name, d, err)
		}
	}
	if _, err := decide(State{Session: &Session{ID: "s1"}, Request: asks["a permission ask"]}, NewCommand("", CmdRequestAnswer, "s1", RequestPayload{RequestID: "r1", Decision: "allow"})); AsFailure(err).Code != CodeForbidden {
		t.Fatalf("an agent answered a permission ask: %v", err)
	}
}
