package proto

import "encoding/json"

// TaskPick is a task kept at team.start, with the agent, model and allowlist
// the user left it. A task the plan has but the picks lack is removed.
type TaskPick struct {
	ID    string   `json:"id"`
	Agent string   `json:"agent"`
	Model string   `json:"model,omitempty"`
	Allow []string `json:"allow,omitempty"`
}

// decodePicks takes team.start's tasks: each an id and an agent, and maybe a
// model and an allowlist.
func decodePicks(raw json.RawMessage, dst *[]TaskPick) bool {
	var items []json.RawMessage
	if json.Unmarshal(raw, &items) != nil || items == nil {
		return false
	}
	picks := make([]TaskPick, len(items))
	for i, item := range items {
		f, ok := strictObject(item, "id", "agent", "model", "allow")
		p := &picks[i]
		if !ok || !fields(f).get("id", &p.ID) || !fields(f).get("agent", &p.Agent) ||
			!fields(f).optionalString("model", &p.Model) || !fields(f).optionalList("allow", &p.Allow) {
			return false
		}
	}
	*dst = picks
	return true
}
