package proto

// CapOrchestrator: agent.delegations, delegation.update, queue verbs and
// delivery timeline items.
const CapOrchestrator = "orchestrator.v1"

// DelegationRef is a child session's delegation, on its AgentSummary.
type DelegationRef struct {
	TaskID    string `json:"taskId"`
	Status    string `json:"status"`
	WorkState string `json:"workState,omitempty"`
}

// Delegation is one session a chat delegated to, as its panel shows it.
type Delegation struct {
	TaskID    string `json:"taskId"`
	ParentID  string `json:"parentId"`
	ChildID   string `json:"childId,omitempty"`
	Title     string `json:"title"`
	Agent     string `json:"agent"`
	Model     string `json:"model,omitempty"`
	Status    string `json:"status"` // queued starting working completed failed cancelled
	Summary   string `json:"summary,omitempty"`
	Error     string `json:"error,omitempty"`
	Question  string `json:"question,omitempty"`
	CreatedAt int64  `json:"createdAt"`
	SettledAt int64  `json:"settledAt,omitempty"`
}

type AgentDelegations struct {
	Type    string       `json:"type"`
	ID      string       `json:"id"`
	AgentID string       `json:"agentId"`
	Items   []Delegation `json:"items"`
}

func NewAgentDelegations(id, agentID string, items []Delegation) AgentDelegations {
	if items == nil {
		items = []Delegation{}
	}
	return AgentDelegations{"agent.delegations", id, agentID, items}
}

type DelegationUpdate struct {
	Type       string     `json:"type"`
	AgentID    string     `json:"agentId"`
	Delegation Delegation `json:"delegation"`
}

func NewDelegationUpdate(agentID string, d Delegation) DelegationUpdate {
	return DelegationUpdate{"delegation.update", agentID, d}
}

// Delivery is a timeline item: what pocketd told the parent about its children.
type Delivery struct {
	TaskIDs  []string `json:"taskIds"`
	ChildIDs []string `json:"childIds"`
	Titles   []string `json:"titles"`
	Agents   []string `json:"agents"`
	Outcome  string   `json:"outcome"` // done failed asks
	Question string   `json:"question,omitempty"`
}

// QueueEntry is a message waiting for its session's turn to end.
type QueueEntry struct {
	ID     string  `json:"id"`
	Text   string  `json:"text"`
	Origin string  `json:"origin"` // user wake question context
	State  string  `json:"state"`  // pending sending held
	Order  float64 `json:"order"`
}

type QueueList struct {
	Type    string       `json:"type"`
	ID      string       `json:"id,omitempty"`
	AgentID string       `json:"agentId"`
	Entries []QueueEntry `json:"entries"`
}

func NewQueueList(id, agentID string, entries []QueueEntry) QueueList {
	if entries == nil {
		entries = []QueueEntry{}
	}
	return QueueList{"queue.list", id, agentID, entries}
}

func NewQueueUpdate(agentID string, entries []QueueEntry) QueueList {
	q := NewQueueList("", agentID, entries)
	q.Type = "queue.update"
	return q
}

// orchestratorVerb reads agent.delegations and the queue verbs.
func orchestratorVerb(f fields, m *ClientMessage) (handled, ok bool) {
	switch m.Type {
	case "agent.delegations", "queue.list":
		return true, f.get("agentId", &m.AgentID)
	case "queue.cancel", "queue.send_now":
		return true, f.get("agentId", &m.AgentID) && f.get("entryId", &m.EntryID) && m.EntryID != ""
	case "queue.edit":
		return true, f.get("agentId", &m.AgentID) && f.get("entryId", &m.EntryID) && m.EntryID != "" && f.get("text", &m.Text) && m.Text != ""
	case "queue.reorder":
		return true, f.get("agentId", &m.AgentID) && f.get("entryId", &m.EntryID) && m.EntryID != "" && f.optionalString("before", &m.Before)
	}
	return false, false
}
