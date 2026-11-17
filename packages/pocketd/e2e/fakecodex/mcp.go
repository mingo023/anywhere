package main

import (
	"encoding/json"
	"fmt"
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
	a.mcp, a.mcpNames, a.mcpApproved = nil, nil, nil
	for _, k := range keys {
		var s struct {
			URL      string            `json:"url"`
			Headers  map[string]string `json:"http_headers"`
			Enabled  []string          `json:"enabled_tools"`
			Disabled []string          `json:"disabled_tools"`
			Approval string            `json:"default_tools_approval_mode"`
		}
		raw, _ := json.Marshal(config[k])
		json.Unmarshal(raw, &s)
		a.mcp = append(a.mcp, fakemcp.Server{URL: s.URL, Headers: s.Headers, Enabled: s.Enabled, Disabled: s.Disabled})
		a.mcpNames = append(a.mcpNames, strings.TrimPrefix(k, "mcp_servers."))
		a.mcpApproved = append(a.mcpApproved, s.Approval == "approve")
	}
}

// callMCP runs line as fakemcp.Run does. Like codex at approval_policy
// on-request, it first asks to run a tool its server doesn't approve.
func (a *appServer) callMCP(t *liveTurn, line string) string {
	a.mu.Lock()
	servers, names, approved, approval := a.mcp, a.mcpNames, a.mcpApproved, a.approval
	a.mu.Unlock()
	tool, args, _ := strings.Cut(strings.TrimSpace(line), " ")
	for i, s := range servers {
		if tool == "tools/list" || approval != "on-request" || !s.Offers(tool) {
			continue
		}
		if !approved[i] {
			if action := a.elicit(t, names[i], tool, args); action != "accept" {
				return "mcp call " + action
			}
		}
		break
	}
	return fakemcp.Run(servers, line)
}

// elicit asks before calling tool on server as codex-cli 0.163 does, and
// gives the action the reply took.
func (a *appServer) elicit(t *liveTurn, server, tool, args string) string {
	if args == "" {
		args = "{}"
	}
	a.status("active", "waitingOnApproval")
	r := a.request("mcpServer/elicitation/request", map[string]any{"threadId": a.thread.ID, "turnId": t.id, "serverName": server,
		"mode": "form", "message": fmt.Sprintf("Allow the %s MCP server to run tool %q?", server, tool),
		"requestedSchema": map[string]any{"type": "object", "properties": map[string]any{}},
		"_meta":           map[string]any{"codex_approval_kind": "mcp_tool_call", "persist": []string{"session", "always"}, "tool_name": tool, "tool_params": json.RawMessage(args)}})
	a.status("active")
	action, _ := r["action"].(string)
	return action
}
