package providers

import "testing"

// Clients decode efforts as a list, so a nil one (sent as null) would drop the whole agent.providers frame.
func TestEveryProviderSendsItsEffortsAsAList(t *testing.T) {
	for _, p := range All {
		if p.Efforts == nil {
			t.Errorf("%s has nil Efforts", p.ID)
		}
	}
}

func TestGetFindsOnlyAListedProvider(t *testing.T) {
	if Get("claude") != &Claude || Get("codex") != &Codex || Get("gemini") != nil {
		t.Fatal("Get doesn't match All")
	}
}
