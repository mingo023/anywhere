package proto

import (
	"encoding/json"
	"errors"
	"math"
	"slices"
	"strings"
)

type ClientMessage struct {
	Type            string
	ID              string
	Token           string
	OwnerToken      string
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
	Answers         map[string]string
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
	Path            string
	Title           string
	Automation      *Automation
	AutomationID    string
	Enabled         bool
	LocalID         string
	Project         string
	Model           string
	Effort          string
	Access          string
	Action          string
	Server          string
	// BeforeSeq pages agent.timeline backwards; never set with SinceSeq.
	BeforeSeq *float64

	// The phone verbs' fields, in protocol.md. Count fields are 0 when absent.
	TerminalID       string
	Data             []byte // decoded from base64
	Cols, Rows       int
	Cwd              string
	Base, Branch     string
	Number           int // a PR's
	Setup            *bool
	DeleteBranch     bool
	Tree             string
	Trees            []string
	Provider         string
	Account          string
	Cursor           int
	At               *DiffAt
	Context          *int
	IgnoreWhitespace bool
	MaxLines         int
	All              bool
	Skip             int
	Sha              string
	Paths            []string
	Staged           bool
	Amend            bool
	Lease            bool
	Body             string
	Draft            bool
	Method           string
	Thread           string
	Resolved         bool
	Dir              string
	Query            string
	Max              int
	UploadID         string
	Index            int
	Last             bool
	Env              string
	Alert, Start     *PushHandle
	Handle           *PushHandle
	Prefs            *PushPrefs
	Frequent         bool
	ActivityID       string
	Ended            bool
}

// McpActions are what agent.mcp may do to a server before it lists them.
var McpActions = []string{"auth", "reconnect", "enable"}

var ErrMalformed = errors.New("Malformed message")

type fields map[string]json.RawMessage

func (f fields) has(key string) bool {
	_, ok := f[key]
	return ok
}

func (f fields) get(key string, dst any) bool {
	v, ok := f[key]
	return ok && string(v) != "null" && json.Unmarshal(v, dst) == nil
}

func (f fields) optional(key string, dst **float64) bool {
	if !f.has(key) {
		return true
	}
	*dst = new(float64)
	return f.get(key, *dst)
}

func (f fields) optionalString(key string, dst *string) bool {
	return !f.has(key) || f.get(key, dst)
}

func (f fields) stringList(key string, dst *[]string) bool {
	var items []*string
	if !f.get(key, &items) || slices.Contains(items, nil) {
		return false
	}
	*dst = make([]string, len(items))
	for n, item := range items {
		(*dst)[n] = *item
	}
	return true
}

func (f fields) optionalList(key string, dst *[]string) bool {
	return !f.has(key) || f.stringList(key, dst)
}

func (f fields) optionalAnswers(key string, dst *map[string]string) bool {
	return !f.has(key) || f.get(key, dst)
}

func (f fields) optionalRange(key string, dst **Range) bool {
	if !f.has(key) {
		return true
	}
	var r struct{ Min, Max *float64 }
	if !f.get(key, &r) || r.Min == nil || r.Max == nil || *r.Min != math.Trunc(*r.Min) || *r.Max != math.Trunc(*r.Max) {
		return false
	}
	*dst = &Range{int(*r.Min), int(*r.Max)}
	return true
}

// DecodeClient accepts what packages/protocol's ClientMessage accepts, except
// a limit below 1.
func DecodeClient(raw []byte) (ClientMessage, error) {
	var m ClientMessage
	var f fields
	if json.Unmarshal(raw, &f) != nil {
		return m, ErrMalformed
	}
	get, optional, optionalString, stringList, optionalList, optionalAnswers, optionalRange :=
		f.get, f.optional, f.optionalString, f.stringList, f.optionalList, f.optionalAnswers, f.optionalRange
	ok := get("type", &m.Type) && get("id", &m.ID)
	switch {
	case !ok:
	case m.Type == "hello":
		ok = optionalString("token", &m.Token) && optionalString("ownerToken", &m.OwnerToken) && get("clientId", &m.ClientID) && get("protocolVersion", &m.ProtocolVersion) &&
			optionalList("caps", &m.Caps) && optionalRange("protocol", &m.Protocol)
	case m.Type == "pair":
		ok = get("code", &m.Code) && get("name", &m.Name) && get("platform", &m.Platform) &&
			(m.Platform == "ios" || m.Platform == "android") && optionalRange("protocol", &m.Protocol)
	case m.Type == "agent.list", m.Type == "pair.begin", m.Type == "project.list", m.Type == "agent.providers":
	case m.Type == "agent.prompt":
		ok = get("agentId", &m.AgentID) && get("text", &m.Text)
	case m.Type == "agent.btw":
		ok = get("agentId", &m.AgentID) && get("text", &m.Text) && strings.TrimSpace(m.Text) != ""
	case m.Type == "agent.interrupt", m.Type == "agent.send_now", m.Type == "agent.compact", m.Type == "agent.close", m.Type == "agent.commands":
		ok = get("agentId", &m.AgentID)
	case m.Type == "agent.mcp":
		ok = get("agentId", &m.AgentID) && optionalString("action", &m.Action) && optionalString("server", &m.Server) &&
			(m.Action == "" && m.Server == "" || slices.Contains(McpActions, m.Action) && m.Server != "")
	case m.Type == "agent.settings":
		ok = get("agentId", &m.AgentID) && optionalString("model", &m.Model) && optionalString("effort", &m.Effort) && optionalString("access", &m.Access) &&
			(m.Access == "" || m.Access != "settings" && slices.Contains(Accesses, m.Access)) && (m.Model != "" || m.Effort != "" || m.Access != "")
	case m.Type == "agent.pin":
		ok = get("agentId", &m.AgentID) && get("pinned", &m.Pinned)
	case m.Type == "agent.view", m.Type == "agent.seen":
		ok = stringList("agentIds", &m.AgentIDs)
	case m.Type == "agent.timeline":
		ok = get("agentId", &m.AgentID) && optional("sinceSeq", &m.SinceSeq) && optional("beforeSeq", &m.BeforeSeq) && optional("limit", &m.Limit) &&
			(m.Limit == nil || *m.Limit >= 1 && *m.Limit <= 500) && (m.SinceSeq == nil || m.BeforeSeq == nil)
	case m.Type == "permission.resolve":
		ok = get("requestId", &m.RequestID) && get("decision", &m.Decision) && (m.Decision == "allow" || m.Decision == "deny") &&
			optionalString("option", &m.Option) && optionalString("message", &m.Message) && optionalAnswers("answers", &m.Answers)
	case m.Type == "agent.create":
		ok = get("requestId", &m.RequestID) && m.RequestID != "" && len(m.RequestID) <= 64 && decodeSpec(f["spec"], &m.Spec)
	case m.Type == "config.set":
		ok = get("key", &m.Key) && get("value", &m.Value) && validConfig(m.Key, m.Value)
	case m.Type == "worktree.rename":
		ok = get("path", &m.Path) && m.Path != "" && get("title", &m.Title)
	case m.Type == "local.create":
		ok = get("localId", &m.LocalID) && ValidLocalID(m.LocalID) && get("project", &m.Project) && m.Project != "" && get("name", &m.Name)
	case m.Type == "local.rename":
		ok = get("localId", &m.LocalID) && ValidLocalID(m.LocalID) && get("title", &m.Title)
	case m.Type == "local.delete":
		ok = get("localId", &m.LocalID) && ValidLocalID(m.LocalID)
	case m.Type == "automation.save":
		ok = decodeAutomation(f["automation"], &m.Automation)
	case m.Type == "automation.enable":
		ok = get("automationId", &m.AutomationID) && get("enabled", &m.Enabled)
	case m.Type == "automation.delete", m.Type == "automation.run":
		ok = get("automationId", &m.AutomationID)
	default:
		decode := phoneVerb(m.Type)
		ok = decode != nil && decode(f, &m)
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
	// Version is the pocketd build (the release tag, or a VCS stamp), not the protocol version.
	Version string `json:"version,omitempty"`
}

// NewHelloOK answers with the negotiated version and caps (see Negotiate) and this server's range.
func NewHelloOK(id, hostname string, version int, caps []string) HelloOK {
	return HelloOK{"hello.ok", id, hostname, hostname, version, caps, Range{MinVersion, MaxVersion}, nil, nil, ""}
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

// Command is a slash command an agent's chat takes.
type Command struct {
	Name         string `json:"name"`
	Description  string `json:"description,omitempty"`
	ArgumentHint string `json:"argumentHint,omitempty"`
}

type AgentCommands struct {
	Type     string    `json:"type"`
	ID       string    `json:"id"`
	AgentID  string    `json:"agentId"`
	Commands []Command `json:"commands"`
}

func NewAgentCommands(id, agentID string, commands []Command) AgentCommands {
	if commands == nil {
		commands = []Command{}
	}
	return AgentCommands{"agent.commands", id, agentID, commands}
}

// AgentBtw answers a side question, which leaves the conversation as it was.
type AgentBtw struct {
	Type    string `json:"type"`
	ID      string `json:"id"`
	AgentID string `json:"agentId"`
	Answer  string `json:"answer"`
}

func NewAgentBtw(id, agentID, answer string) AgentBtw {
	return AgentBtw{"agent.btw", id, agentID, answer}
}

// McpServer is an MCP server the agent's session has, and how its connection stands:
// connected, pending, needsAuth, failed or disabled.
type McpServer struct {
	Name   string `json:"name"`
	Status string `json:"status"`
	Scope  string `json:"scope,omitempty"`
}

// AgentMcp lists the servers after any action the request asked for; a sign-in the user finishes in a browser
// carries its URL.
type AgentMcp struct {
	Type    string      `json:"type"`
	ID      string      `json:"id"`
	AgentID string      `json:"agentId"`
	Servers []McpServer `json:"servers"`
	AuthURL string      `json:"authUrl,omitempty"`
}

func NewAgentMcp(id, agentID string, servers []McpServer, authURL string) AgentMcp {
	if servers == nil {
		servers = []McpServer{}
	}
	return AgentMcp{"agent.mcp", id, agentID, servers, authURL}
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
	// UpgradeFailed is the version pocketd last failed to upgrade to.
	UpgradeFailed string `json:"upgradeFailed,omitempty"`
}

type HostChanged struct {
	Type string    `json:"type"`
	Host HostState `json:"host"`
}

func NewHostChanged(h HostState) HostChanged { return HostChanged{"host.changed", h} }
