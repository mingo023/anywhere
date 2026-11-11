package acp

import (
	"testing"

	"pocketd/internal/providers"
)

func TestAnACPAgentOffersCompactOnlyWhenItsFlavorCanAskForIt(t *testing.T) {
	for _, p := range providers.All {
		if p.Transport != providers.ACP {
			continue
		}
		f := flavors[p.ID]
		if can := f.compactMethod != "" || f.compactPrompt != ""; p.Chat.Compact != can {
			t.Errorf("%s offers Compact %v, but its flavor can ask for it: %v", p.ID, p.Chat.Compact, can)
		}
	}
}
