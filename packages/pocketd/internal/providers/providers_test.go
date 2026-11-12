package providers

import (
	"slices"
	"testing"
)

// Clients decode efforts as a list, so a nil one (sent as null) would drop the whole agent.providers frame.
func TestEveryProviderSendsItsEffortsAsAList(t *testing.T) {
	for _, p := range All {
		if p.Efforts == nil {
			t.Errorf("%s has nil Efforts", p.ID)
		}
	}
}

func TestGetFindsOnlyAListedProvider(t *testing.T) {
	if Get("claude") != &Claude || Get("codex") != &Codex || Get("antigravity") != &Antigravity || Get("gemini") != nil {
		t.Fatal("Get doesn't match All")
	}
}

func TestProvidersAreListedInTheOrderClientsShowThem(t *testing.T) {
	var ids []string
	for _, p := range All {
		ids = append(ids, p.ID)
	}
	want := []string{"claude", "codex", "cursor", "grok", "opencode", "pi", "omp", "fx", "hermes", "antigravity"}
	if !slices.Equal(ids, want) {
		t.Fatalf("got %v", ids)
	}
}

func TestEveryProviderHasANameAndATransportPocketdDrives(t *testing.T) {
	for _, p := range All {
		if !slices.Contains([]string{StreamJSON, AppServer, ACP, PiRPC}, p.Transport) || p.Name == "" {
			t.Errorf("%s: name %q, transport %q", p.ID, p.Name, p.Transport)
		}
	}
}
