package proto

import (
	"encoding/json"
	"slices"
	"testing"
)

func TestTheServerOffersTheTerminalCap(t *testing.T) {
	_, caps, code := Negotiate(Range{Min: 3, Max: 3}, []string{CapTerminal}, ServerCaps)
	if code != "" || !slices.Equal(caps, []string{CapTerminal}) {
		t.Fatalf("caps %v, code %q", caps, code)
	}
}

func TestACreatingPushForAPhonesTerminalHasNoID(t *testing.T) {
	raw, _ := json.Marshal(NewAgentCreating("", "r1", "t1", "/w", false))
	if string(raw) != `{"type":"agent.creating","requestId":"r1","terminalId":"t1","cwd":"/w","setup":false}` {
		t.Fatal(string(raw))
	}
}
