package proto

import (
	"encoding/json"
	"slices"
)

const CapLaunch = "launch.v1"

const MaxPrompt = 64 << 10

var (
	Accesses      = []string{"ask", "edits", "auto", "full"}
	PhoneAccesses = []string{"ask", "edits", "auto"}
)

type LaunchSpec struct {
	Project  string   `json:"project"`
	Checkout Checkout `json:"checkout"`
	Provider string   `json:"provider"`
	Model    string   `json:"model,omitempty"`
	Effort   string   `json:"effort,omitempty"`
	Access   string   `json:"access"`
	Plan     bool     `json:"plan"`
	Prompt   string   `json:"prompt,omitempty"`
}

type Checkout struct {
	Worktree string       `json:"worktree,omitempty"`
	New      *NewWorktree `json:"new,omitempty"`
}

type NewWorktree struct {
	Name  string `json:"name"`
	Base  string `json:"base,omitempty"`
	Copy  *bool  `json:"copy,omitempty"`
	Setup *bool  `json:"setup,omitempty"`
}

type AgentCreating struct {
	Type       string `json:"type"`
	ID         string `json:"id"`
	RequestID  string `json:"requestId"`
	TerminalID string `json:"terminalId"`
	Cwd        string `json:"cwd"`
	Setup      bool   `json:"setup"`
}

func NewAgentCreating(id, requestID, terminalID, cwd string, setup bool) AgentCreating {
	return AgentCreating{"agent.creating", id, requestID, terminalID, cwd, setup}
}

type AgentCreated struct {
	Type       string `json:"type"`
	ID         string `json:"id"`
	RequestID  string `json:"requestId"`
	AgentID    string `json:"agentId"`
	TerminalID string `json:"terminalId"`
}

func NewAgentCreated(id, requestID, agentID, terminalID string) AgentCreated {
	return AgentCreated{"agent.created", id, requestID, agentID, terminalID}
}

type ProviderInfo struct {
	ID        string   `json:"id"`
	Available bool     `json:"available"`
	Efforts   []string `json:"efforts"`
	Plan      bool     `json:"plan"`
}

type AgentProviders struct {
	Type           string         `json:"type"`
	ID             string         `json:"id"`
	Providers      []ProviderInfo `json:"providers"`
	MaxAccess      string         `json:"maxAccess"`
	PhoneMaxAccess string         `json:"phoneMaxAccess"`
}

func NewAgentProviders(id string, p []ProviderInfo, maxAccess, phoneMaxAccess string) AgentProviders {
	return AgentProviders{"agent.providers", id, p, maxAccess, phoneMaxAccess}
}

// strictObject reads a JSON object whose keys are all in allowed, matched
// exactly, and whose values are never null. encoding/json alone matches keys
// case-insensitively and zero-fills nulls; the TS schema rejects both.
func strictObject(raw json.RawMessage, allowed ...string) (map[string]json.RawMessage, bool) {
	var fields map[string]json.RawMessage
	if json.Unmarshal(raw, &fields) != nil || fields == nil {
		return nil, false
	}
	for k, v := range fields {
		if !slices.Contains(allowed, k) || string(v) == "null" {
			return nil, false
		}
	}
	return fields, true
}

func decodeSpec(raw json.RawMessage, dst **LaunchSpec) bool {
	spec, ok := strictObject(raw, "project", "checkout", "provider", "model", "effort", "access", "plan", "prompt")
	if !ok {
		return false
	}
	for _, k := range []string{"project", "checkout", "provider", "access", "plan"} {
		if _, has := spec[k]; !has {
			return false
		}
	}
	checkout, ok := strictObject(spec["checkout"], "worktree", "new")
	if !ok {
		return false
	}
	if n, has := checkout["new"]; has {
		if _, ok := strictObject(n, "name", "base", "copy", "setup"); !ok {
			return false
		}
	}
	var s LaunchSpec
	if json.Unmarshal(raw, &s) != nil {
		return false
	}
	c := s.Checkout
	if (c.Worktree == "") == (c.New == nil) || c.New != nil && c.New.Name == "" {
		return false
	}
	if s.Provider != "claude" && s.Provider != "codex" || !slices.Contains(Accesses, s.Access) || len(s.Prompt) > MaxPrompt {
		return false
	}
	*dst = &s
	return true
}
