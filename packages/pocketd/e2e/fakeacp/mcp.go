package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"time"

	"pocketd/e2e/fakemcp"
)

// useMCP keeps the servers session/new or session/load gave, as an agent
// that takes MCP over HTTP does, and records them in $HOME/.acp-fake/mcp.json.
func (a *agent) useMCP(servers []any) {
	if len(servers) == 0 {
		return
	}
	b, _ := json.Marshal(servers)
	os.MkdirAll(dir(), 0o700)
	os.WriteFile(filepath.Join(dir(), "mcp.json"), b, 0o600)
	a.mu.Lock()
	defer a.mu.Unlock()
	a.mcp = nil
	for _, e := range servers {
		var s struct {
			URL     string `json:"url"`
			Headers []struct {
				Name  string `json:"name"`
				Value string `json:"value"`
			} `json:"headers"`
		}
		raw, _ := json.Marshal(e)
		json.Unmarshal(raw, &s)
		server := fakemcp.Server{URL: s.URL, Headers: map[string]string{}}
		for _, h := range s.Headers {
			server.Headers[h.Name] = h.Value
		}
		a.mcp = append(a.mcp, server)
	}
}

// script runs the lines of an "mcp" prompt and gives their results, one per
// line: "mcp <tool> <json>" calls a tool, "await <name>" waits for
// $HOME/.acp-fake/<name> to exist. It gives false when cancelled meanwhile.
func (a *agent) script(text string, stop <-chan struct{}) (string, bool) {
	a.mu.Lock()
	servers := a.mcp
	a.mu.Unlock()
	var out []string
	for _, line := range strings.Split(text, "\n") {
		if name, ok := strings.CutPrefix(line, "await "); ok {
			for {
				if _, err := os.Stat(filepath.Join(dir(), name)); err == nil {
					break
				}
				select {
				case <-stop:
					return "", false
				case <-time.After(20 * time.Millisecond):
				}
			}
			continue
		}
		out = append(out, fakemcp.Run(servers, strings.TrimPrefix(line, "mcp ")))
	}
	return strings.Join(out, "\n"), true
}
