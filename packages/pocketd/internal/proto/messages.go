package proto

import (
	"encoding/json"
	"errors"
)

type ClientMessage struct {
	Type            string
	ID              string
	Token           string
	ClientID        string
	ProtocolVersion float64
	AgentID         string
	Text            string
	SinceSeq        *float64
	Limit           *float64
	RequestID       string
	Decision        string
	Option          string
	Message         string
}

var ErrMalformed = errors.New("Malformed message")

// DecodeClient accepts what packages/protocol's ClientMessage accepts, except
// a limit below 1.
func DecodeClient(raw []byte) (ClientMessage, error) {
	var m ClientMessage
	var fields map[string]json.RawMessage
	if json.Unmarshal(raw, &fields) != nil {
		return m, ErrMalformed
	}
	get := func(key string, dst any) bool {
		v, ok := fields[key]
		return ok && string(v) != "null" && json.Unmarshal(v, dst) == nil
	}
	optional := func(key string, dst **float64) bool {
		if _, ok := fields[key]; !ok {
			return true
		}
		*dst = new(float64)
		return get(key, *dst)
	}
	optionalString := func(key string, dst *string) bool {
		_, has := fields[key]
		return !has || get(key, dst)
	}
	ok := get("type", &m.Type) && get("id", &m.ID)
	switch {
	case !ok:
	case m.Type == "hello":
		ok = get("token", &m.Token) && get("clientId", &m.ClientID) && get("protocolVersion", &m.ProtocolVersion)
	case m.Type == "agent.list":
	case m.Type == "agent.prompt":
		ok = get("agentId", &m.AgentID) && get("text", &m.Text)
	case m.Type == "agent.interrupt", m.Type == "agent.compact", m.Type == "agent.close":
		ok = get("agentId", &m.AgentID)
	case m.Type == "agent.timeline":
		ok = get("agentId", &m.AgentID) && optional("sinceSeq", &m.SinceSeq) && optional("limit", &m.Limit) &&
			(m.Limit == nil || *m.Limit >= 1 && *m.Limit <= 500)
	case m.Type == "permission.resolve":
		ok = get("requestId", &m.RequestID) && get("decision", &m.Decision) && (m.Decision == "allow" || m.Decision == "deny") &&
			optionalString("option", &m.Option) && optionalString("message", &m.Message)
	default:
		ok = false
	}
	if !ok {
		return m, ErrMalformed
	}
	return m, nil
}

type HelloOK struct {
	Type            string `json:"type"`
	ID              string `json:"id"`
	ServerID        string `json:"serverId"`
	Hostname        string `json:"hostname"`
	ProtocolVersion int    `json:"protocolVersion"`
}

func NewHelloOK(id, hostname string) HelloOK {
	return HelloOK{"hello.ok", id, hostname, hostname, Version}
}

type AgentList struct {
	Type   string         `json:"type"`
	ID     string         `json:"id,omitempty"`
	Agents []AgentSummary `json:"agents"`
}

func NewAgentList(id string, agents []AgentSummary) AgentList {
	if agents == nil {
		agents = []AgentSummary{}
	}
	return AgentList{"agent.list", id, agents}
}

type AgentUpdate struct {
	Type  string       `json:"type"`
	Agent AgentSummary `json:"agent"`
}

func NewAgentUpdate(a AgentSummary) AgentUpdate { return AgentUpdate{"agent.update", a} }

type AgentStream struct {
	Type    string `json:"type"`
	AgentID string `json:"agentId"`
	Epoch   int64  `json:"epoch"`
	Item    Item   `json:"item"`
}

func NewAgentStream(agentID string, epoch int64, item Item) AgentStream {
	return AgentStream{"agent.stream", agentID, epoch, item}
}

type AgentTimeline struct {
	Type     string `json:"type"`
	ID       string `json:"id"`
	AgentID  string `json:"agentId"`
	Epoch    int64  `json:"epoch"`
	Items    []Item `json:"items"`
	HasOlder bool   `json:"hasOlder"`
	MaxSeq   int64  `json:"maxSeq"`
}

func NewAgentTimeline(id, agentID string, epoch int64, items []Item, hasOlder bool, maxSeq int64) AgentTimeline {
	if items == nil {
		items = []Item{}
	}
	return AgentTimeline{"agent.timeline", id, agentID, epoch, items, hasOlder, maxSeq}
}

type PermissionRequested struct {
	Type    string            `json:"type"`
	Request PermissionRequest `json:"request"`
}

func NewPermissionRequest(r PermissionRequest) PermissionRequested {
	return PermissionRequested{"permission.request", r}
}

type PermissionResolved struct {
	Type      string `json:"type"`
	RequestID string `json:"requestId"`
	Decision  string `json:"decision"`
}

func NewPermissionResolved(requestID, decision string) PermissionResolved {
	return PermissionResolved{"permission.resolved", requestID, decision}
}

type Ack struct {
	Type string `json:"type"`
	ID   string `json:"id"`
}

func NewAck(id string) Ack { return Ack{"ack", id} }

type Error struct {
	Type    string `json:"type"`
	ID      string `json:"id,omitempty"`
	Message string `json:"message"`
}

func NewError(id, message string) Error { return Error{"error", id, message} }
