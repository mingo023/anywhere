package proto

// CapNames gates worktree.names, naming.failed and worktree.rename.
const CapNames = "names.v1"

// WorktreeNames is every Worktree's display name by path, sent whole on hello and on each change.
type WorktreeNames struct {
	Type  string            `json:"type"`
	Names map[string]string `json:"names"`
}

func NewWorktreeNames(names map[string]string) WorktreeNames {
	if names == nil {
		names = map[string]string{}
	}
	return WorktreeNames{"worktree.names", names}
}

// NamingFailed: no attempt named the agent's session from its prompt.
type NamingFailed struct {
	Type    string `json:"type"`
	AgentID string `json:"agentId"`
}

func NewNamingFailed(agentID string) NamingFailed { return NamingFailed{"naming.failed", agentID} }
