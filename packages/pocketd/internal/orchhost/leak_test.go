package orchhost

import (
	"bytes"
	"encoding/json"
	"log"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/agent"
	"pocketd/internal/handoff"
	"pocketd/internal/hub"
	"pocketd/internal/mcp"
	"pocketd/internal/orch"
)

func TestNoTokenReachesHandoffJsonOrAnyLogLine(t *testing.T) {
	dir := t.TempDir()
	store, err := orch.OpenPath(filepath.Join(dir, "orchestrator.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	reg := agent.NewRegistry(hub.New())
	reg.Add("s1", "/r", "claude", nil).SetTerminal("t1")
	h := &Host{Registry: reg, Tokens: mcp.NewTokens(store), StateDir: dir, URL: "http://127.0.0.1:1/mcp"}

	var logged bytes.Buffer
	log.SetOutput(&logged)
	t.Cleanup(func() { log.SetOutput(os.Stderr) })

	flags := h.ClaudeFlags("t1", "claude")
	var cfg struct {
		McpServers map[string]struct {
			Headers map[string]string `json:"headers"`
		} `json:"mcpServers"`
	}
	raw, err := os.ReadFile(filepath.Join(dir, "mcp", "t1.json"))
	if err != nil {
		t.Fatal(err)
	}
	json.Unmarshal(raw, &cfg)
	claude, _ := strings.CutPrefix(cfg.McpServers["anywhere"].Headers["Authorization"], "Bearer ")
	codex, _ := json.Marshal(h.CodexConfig("t2"))
	acp, _ := json.Marshal(h.ACPServers("t3"))
	h.Closed("s1")

	path := filepath.Join(dir, "handoff.json")
	argv := append([]string{"claude", "--output-format", "stream-json"}, flags...)
	if err := handoff.Write(path, handoff.File{Format: handoff.Format, Terminals: []handoff.Terminal{{Cmd: argv[0], Args: argv[1:]}}}); err != nil {
		t.Fatal(err)
	}
	handed, _ := os.ReadFile(path)

	tokens := []string{claude}
	for _, blob := range [][]byte{codex, acp} {
		_, after, _ := strings.Cut(string(blob), "Bearer ")
		tokens = append(tokens, after[:len("anyw_")+64])
	}
	for _, token := range tokens {
		if !strings.HasPrefix(token, "anyw_") {
			t.Fatalf("no token to look for in %q", token)
		}
		if bytes.Contains(handed, []byte(token)) {
			t.Errorf("a token is in handoff.json")
		}
		if strings.Contains(logged.String(), token) {
			t.Errorf("a token is in the log:\n%s", logged.String())
		}
	}
	if got, want := strings.TrimSpace(logged.String()), "mcp: t1 launches with MCP config"; !strings.HasSuffix(got, want) || strings.Count(got, "\n") != 0 {
		t.Errorf("log = %q, want the one line %q after its timestamp", got, want)
	}
}
