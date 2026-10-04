package proto

import (
	"encoding/json"
	"errors"
	"math"
	"slices"
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
	AgentIDs        []string
	Pinned          bool
	Spec            *LaunchSpec
	Key             string
	Value           string
	Caps            []string
	Protocol        *Range
	Code            string
	Name            string
	Platform        string
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
	stringList := func(key string, dst *[]string) bool {
		var items []*string
		if !get(key, &items) || slices.Contains(items, nil) {
			return false
		}
		*dst = make([]string, len(items))
		for n, item := range items {
			(*dst)[n] = *item
		}
		return true
	}
	optionalList := func(key string, dst *[]string) bool {
		_, has := fields[key]
		return !has || stringList(key, dst)
	}
	optionalRange := func(key string, dst **Range) bool {
		if _, has := fields[key]; !has {
			return true
		}
		var r struct{ Min, Max *float64 }
		if !get(key, &r) || r.Min == nil || r.Max == nil || *r.Min != math.Trunc(*r.Min) || *r.Max != math.Trunc(*r.Max) {
			return false
		}
		*dst = &Range{int(*r.Min), int(*r.Max)}
		return true
	}
	ok := get("type", &m.Type) && get("id", &m.ID)
	switch {
	case !ok:
	case m.Type == "hello":
		ok = optionalString("token", &m.Token) && get("clientId", &m.ClientID) && get("protocolVersion", &m.ProtocolVersion) &&
			optionalList("caps", &m.Caps) && optionalRange("protocol", &m.Protocol)
	case m.Type == "pair":
		ok = get("code", &m.Code) && get("name", &m.Name) && get("platform", &m.Platform) &&
			(m.Platform == "ios" || m.Platform == "android") && optionalRange("protocol", &m.Protocol)
	case m.Type == "agent.list", m.Type == "pair.begin", m.Type == "project.list", m.Type == "agent.providers":
	case m.Type == "agent.prompt":
		ok = get("agentId", &m.AgentID) && get("text", &m.Text)
	case m.Type == "agent.interrupt", m.Type == "agent.compact", m.Type == "agent.close":
		ok = get("agentId", &m.AgentID)
	case m.Type == "agent.pin":
		ok = get("agentId", &m.AgentID) && get("pinned", &m.Pinned)
	case m.Type == "agent.view", m.Type == "agent.seen":
		ok = stringList("agentIds", &m.AgentIDs)
	case m.Type == "agent.timeline":
		ok = get("agentId", &m.AgentID) && optional("sinceSeq", &m.SinceSeq) && optional("limit", &m.Limit) &&
			(m.Limit == nil || *m.Limit >= 1 && *m.Limit <= 500)
	case m.Type == "permission.resolve":
		ok = get("requestId", &m.RequestID) && get("decision", &m.Decision) && (m.Decision == "allow" || m.Decision == "deny") &&
			optionalString("option", &m.Option) && optionalString("message", &m.Message)
	case m.Type == "agent.create":
		ok = get("requestId", &m.RequestID) && m.RequestID != "" && len(m.RequestID) <= 64 && decodeSpec(fields["spec"], &m.Spec)
	case m.Type == "config.set":
		ok = get("key", &m.Key) && m.Key == "phone.maxAccess" && get("value", &m.Value) && slices.Contains(PhoneAccesses, m.Value)
	default:
		ok = false
	}
	if !ok {
		return m, ErrMalformed
	}
	return m, nil
}

type HelloOK struct {
	Type            string     `json:"type"`
	ID              string     `json:"id"`
	ServerID        string     `json:"serverId"`
	Hostname        string     `json:"hostname"`
	ProtocolVersion int        `json:"protocolVersion"`
	Caps            []string   `json:"caps"`
	Protocol        Range      `json:"protocol"`
	Scopes          []string   `json:"scopes,omitempty"`
	Host            *HostState `json:"host,omitempty"`
}

// NewHelloOK answers with the negotiated version and caps (see Negotiate) and this server's range.
func NewHelloOK(id, hostname string, version int, caps []string) HelloOK {
	return HelloOK{"hello.ok", id, hostname, hostname, version, caps, Range{MinVersion, MaxVersion}, nil, nil}
}

type PairOK struct {
	Type     string `json:"type"`
	ID       string `json:"id"`
	DeviceID string `json:"deviceId"`
	Token    string `json:"token"`
}

func NewPairOK(id, deviceID, token string) PairOK {
	return PairOK{"pair.ok", id, deviceID, token}
}

// PairOffer answers the owner's pair.begin with the code a phone redeems.
type PairOffer struct {
	Type      string `json:"type"`
	ID        string `json:"id"`
	URL       string `json:"url"`
	Code      string `json:"code"`
	ExpiresAt int64  `json:"expiresAt"`
}

func NewPairOffer(id, url, code string, expiresAt int64) PairOffer {
	return PairOffer{"pair.offer", id, url, code, expiresAt}
}

// PairDone tells the conn that began pairing which phone redeemed its code.
type PairDone struct {
	Type     string `json:"type"`
	DeviceID string `json:"deviceId"`
	Name     string `json:"name"`
}

func NewPairDone(deviceID, name string) PairDone { return PairDone{"pair.done", deviceID, name} }

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

type Worktree struct {
	Name   string `json:"name"`
	Path   string `json:"path"`
	Branch string `json:"branch"`
	IsMain bool   `json:"isMain"`
}

type Project struct {
	Path      string     `json:"path"`
	Name      string     `json:"name"`
	Worktrees []Worktree `json:"worktrees"`
}

type ProjectList struct {
	Type     string    `json:"type"`
	ID       string    `json:"id,omitempty"`
	Projects []Project `json:"projects"`
}

func NewProjectList(id string, projects []Project) ProjectList {
	if projects == nil {
		projects = []Project{}
	}
	return ProjectList{"project.list", id, projects}
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
	Code    string `json:"code,omitempty"`
	Detail  string `json:"detail,omitempty"`
}

func NewError(id, message string) Error { return Error{Type: "error", ID: id, Message: message} }

// NewErrorCode is an error the client acts on by code, not by message.
func NewErrorCode(id, code, message string) Error {
	return Error{Type: "error", ID: id, Message: message, Code: code}
}

func NewCodedError(id, code, message, detail string) Error {
	return Error{Type: "error", ID: id, Message: message, Code: code, Detail: detail}
}

type HostState struct {
	Tailnet      bool `json:"tailnet"`
	KeepingAwake bool `json:"keepingAwake"`
}

type HostChanged struct {
	Type string    `json:"type"`
	Host HostState `json:"host"`
}

func NewHostChanged(h HostState) HostChanged { return HostChanged{"host.changed", h} }
