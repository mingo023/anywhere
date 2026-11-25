package proto

// CapDelivery: agent.prompt is acked once pocketd takes it, before the agent
// does; agent.delivery then says what came of it. Its id is the receipt, so
// the same id sent again is the same prompt, delivered once. A client sends it
// only when its prompt ids never repeat, across launches too.
const CapDelivery = "delivery.v1"

// Delivery states.
const (
	DeliveryStarted = "started"
	DeliveryQueued  = "queued"
	DeliveryFailed  = "failed"
)

// AgentDelivery is what came of an acked agent.prompt: its turn started, it
// waits in the session's queue, or the agent didn't take it, and why.
type AgentDelivery struct {
	Type    string `json:"type"`
	ID      string `json:"id"`
	AgentID string `json:"agentId"`
	State   string `json:"state"`
	Error   string `json:"error,omitempty"`
}

func NewAgentDelivery(id, agentID, state, err string) AgentDelivery {
	return AgentDelivery{Type: "agent.delivery", ID: id, AgentID: agentID, State: state, Error: err}
}

// Recap is what a session's last turn did: the prompt the user last sent, how
// many files it read and edited, and the last thing the agent said. Sub-agents'
// work isn't counted.
type Recap struct {
	Asked  string `json:"asked,omitempty"`
	Read   int    `json:"read,omitempty"`
	Edited int    `json:"edited,omitempty"`
	Said   string `json:"said,omitempty"`
}
