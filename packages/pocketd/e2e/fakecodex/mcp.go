package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"pocketd/e2e/fakemcp"
)

// useMCP keeps the servers a thread/start or thread/resume config gave, as
// codex does, and records the config in $HOME/.codex-fake/mcp.json.
func (a *appServer) useMCP(config map[string]any) {
	if config == nil {
		return
	}
	b, _ := json.Marshal(config)
	os.MkdirAll(threadsDir(), 0o700)
	os.WriteFile(filepath.Join(threadsDir(), "mcp.json"), b, 0o600)
	var keys []string
	for k := range config {
		if strings.HasPrefix(k, "mcp_servers.") {
			keys = append(keys, k)
		}
	}
	sort.Strings(keys)
	a.mu.Lock()
	defer a.mu.Unlock()
	a.mcp = nil
	for _, k := range keys {
		var s struct {
			URL      string            `json:"url"`
			Headers  map[string]string `json:"http_headers"`
			Enabled  []string          `json:"enabled_tools"`
			Disabled []string          `json:"disabled_tools"`
		}
		raw, _ := json.Marshal(config[k])
		json.Unmarshal(raw, &s)
		a.mcp = append(a.mcp, fakemcp.Server{URL: s.URL, Headers: s.Headers, Enabled: s.Enabled, Disabled: s.Disabled})
	}
}

func (a *appServer) callMCP(line string) string {
	a.mu.Lock()
	servers := a.mcp
	a.mu.Unlock()
	return fakemcp.Run(servers, line)
}
