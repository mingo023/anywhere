package e2e

import "testing"

func TestPromptFromOpsReachesCLI(t *testing.T) {
	h := Start(t)
	id := h.SpawnReady("claude")
	h.Prompt(id, "hello")
	h.WaitScreen(id, "echo: hello")
}
