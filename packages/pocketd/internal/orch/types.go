package orch

import (
	"encoding/json"
	"errors"
)

const (
	CmdMessageSend     = "message.send"
	CmdTurnInterrupt   = "turn.interrupt"
	CmdTurnSendNow     = "turn.send_now"
	CmdSessionStop     = "session.stop"
	CmdSessionClose    = "session.close"
	CmdSessionCreate   = "session.create"
	CmdSessionStart    = "session.start"
	CmdSessionLaunched = "session.launched"
	CmdSessionRename   = "session.rename"
	CmdSessionRetitle  = "session.retitle"
	CmdSessionConfig   = "session.configure"
	CmdSessionSettings = "session.settings"
	CmdSessionOrganize = "session.organize"
	CmdQueueEdit       = "queue.edit"
	CmdQueueCancel     = "queue.cancel"
	CmdQueueCancelRefs = "queue.cancel_refs"
	CmdQueueReorder    = "queue.reorder"
	CmdQueueSendNow    = "queue.send_now"
	CmdQueueDrain      = "queue.drain"
	CmdQueueSent       = "queue.sent"
	CmdQueueUnsent     = "queue.unsent"
	CmdRequestOpen     = "request.open"
	CmdRequestClose    = "request.close"
	CmdRequestAnswer   = "request.answer"
	CmdDelegate        = "delegation.request"
	CmdDelegateStarted = "delegation.started"
	CmdDelegateFailed  = "delegation.launch_failed"
	CmdDelegateSettle  = "delegation.settle"
	CmdDelegateCancel  = "delegation.cancel"
	CmdDelegateWake    = "delegation.wake_policy"
	CmdDelegatePump    = "delegation.pump"
	CmdDelegateWaited  = "delegation.waited"
	CmdWakeDeliver     = "wake.deliver"
	CmdParentIdle      = "parent.idle"
	CmdTransferFork    = "transfer.fork"
	CmdTransferStarted = "transfer.started"
	CmdTransferMerge   = "transfer.merge_back"
	CmdBackgroundHold  = "background.hold"
	CmdWorkflowStop    = "workflow.stop"
)

const (
	EffLaunch     = "agent.launch"
	EffCreate     = "agent.create"
	EffTurnStart  = "turn.start"
	EffInterrupt  = "turn.interrupt"
	EffSendNow    = "turn.send_now"
	EffAnswer     = "request.answer"
	EffClose      = "agent.close"
	EffRename     = "session.rename"
	EffNaming     = "naming.run"
	EffConfigure  = "session.configure"
	EffSettings   = "session.settings"
	EffPin        = "session.pin"
	EffMarkUnread = "session.mark_unread"
	EffDelivery   = "delivery.record"
	EffHold       = "background.hold"
	EffStopTask   = "stop_task"
	EffForkLaunch = "fork.launch"
	// EffFollow is a command a decision set off, kept with it so Recover runs it if pocketd died first; it is
	// never claimed.
	EffFollow = "command.follow"
)

const (
	CodeInvalid       = "invalid_request"
	CodeNotFound      = "not_found"
	CodeForbidden     = "forbidden"
	CodeConflict      = "conflict"
	CodeUnsupported   = "unsupported"
	CodeDepthExceeded = "depth_exceeded"
	CodeUnavailable   = "unavailable"
)

// Failure is a refused command; Code is one of the codes above.
type Failure struct{ Code, Message string }

func (f *Failure) Error() string { return f.Message }

func refuse(code, message string) error { return &Failure{Code: code, Message: message} }

// AsFailure is err's Failure, or an invalid_request one.
func AsFailure(err error) *Failure {
	var f *Failure
	if errors.As(err, &f) {
		return f
	}
	return &Failure{Code: CodeInvalid, Message: err.Error()}
}

// Command is one intent. ID makes it idempotent: the same ID commits once.
type Command struct {
	ID      string          `json:"id"`
	Kind    string          `json:"kind"`
	Session string          `json:"session"`
	Payload json.RawMessage `json:"payload,omitempty"`
}

func NewCommand(id, kind, session string, payload any) Command {
	b, _ := json.Marshal(payload)
	return Command{ID: id, Kind: kind, Session: session, Payload: b}
}

// Effect is one side effect the worker runs for a session's lane. Key is the
// idempotency key the agent sees.
type Effect struct {
	ID      int64           `json:"id"`
	Lane    string          `json:"lane"`
	Kind    string          `json:"kind"`
	Payload json.RawMessage `json:"payload"`
	Command string          `json:"command"`
	// Then is the command the engine runs with the effect's result once it is done; Else once it failed.
	Then string `json:"then,omitempty"`
	Else string `json:"else,omitempty"`
}

func effect(lane, kind string, payload any) Effect {
	b, _ := json.Marshal(payload)
	return Effect{Lane: lane, Kind: kind, Payload: b}
}

type Event struct {
	Session string
	Kind    string
	Payload any
}

// Session is the projection of one session the rules read.
type Session struct {
	ID, Project, Worktree, Cwd, Branch, Agent, Access, Model, Effort string
	Status, Title, Origin, ParentID, Relationship, CreatedBy         string
	Conversation, Terminal                                           string
	Depth                                                            int
	Chat, Pinned, Failed, Archived                                   bool
	ClosedAt, CreatedAt, UpdatedAt                                   int64
}

func (s Session) Busy() bool { return s.Status == "working" || s.Status == "needsYou" }

const (
	TaskQueued    = "queued"
	TaskStarting  = "starting"
	TaskWorking   = "working"
	TaskCompleted = "completed"
	TaskFailed    = "failed"
	TaskCancelled = "cancelled"
)

const (
	DeliverNone      = "none"
	DeliverWaiting   = "waiting" // a wait-mode parent reads it through its wait
	DeliverPending   = "pending" // a wake is due
	DeliverQueued    = "queued"  // in a wake entry not yet sent
	DeliverDelivered = "delivered"
	DeliverReturned  = "returned" // a wait returned it
	DeliverDisposed  = "disposed" // a Stop or cancel dropped the wake for good
)

type Delegation struct {
	TaskID          string `json:"taskId"`
	ParentID        string `json:"parentId"`
	ParentTurn      int64  `json:"parentTurn,omitempty"`
	ChildID         string `json:"childId,omitempty"`
	Title           string `json:"title"`
	Role            string `json:"role,omitempty"`
	Agent           string `json:"agent"`
	Model           string `json:"model,omitempty"`
	Effort          string `json:"effort,omitempty"`
	Brief           string `json:"brief"`
	Isolation       string `json:"isolation"`
	Mode            string `json:"mode"`
	Status          string `json:"status"`
	WorkState       string `json:"workState,omitempty"`
	Summary         string `json:"summary,omitempty"`
	Error           string `json:"error,omitempty"`
	Access          string `json:"access"`
	SteppedDown     string `json:"steppedDown,omitempty"`
	CompletionWake  string `json:"completionWake"`
	Delivery        string `json:"delivery"`
	PendingQuestion string `json:"pendingQuestion,omitempty"`
	ClientRequestID string `json:"clientRequestId,omitempty"`
	Environment     string `json:"environmentId,omitempty"`
	HostProject     string `json:"hostProject,omitempty"`
	CreatedAt       int64  `json:"createdAt"`
	SettledAt       int64  `json:"settledAt,omitempty"`
}

func (d Delegation) Terminal() bool {
	return d.Status == TaskCompleted || d.Status == TaskFailed || d.Status == TaskCancelled
}

func (d Delegation) Running() bool { return d.Status == TaskStarting || d.Status == TaskWorking }

const (
	OriginUser     = "user"
	OriginWake     = "wake"
	OriginQuestion = "question"
	OriginContext  = "context"

	EntryPending   = "pending"
	EntrySending   = "sending"
	EntrySent      = "sent"
	EntryCancelled = "cancelled"
	EntryHeld      = "held"
)

type QueueEntry struct {
	ID        string   `json:"id"`
	Session   string   `json:"agentId"`
	Text      string   `json:"text"`
	Origin    string   `json:"origin"`
	Ord       float64  `json:"order"`
	State     string   `json:"state"`
	Refs      []string `json:"refs,omitempty"`
	CreatedAt int64    `json:"createdAt"`
}

type Request struct {
	ID        string          `json:"id"`
	Session   string          `json:"agentId"`
	Kind      string          `json:"kind"`
	Payload   json.RawMessage `json:"payload"`
	State     string          `json:"state"`
	CreatedAt int64           `json:"createdAt"`
}

type Transfer struct {
	ID          string `json:"id"`
	Source      string `json:"sourceId"`
	Target      string `json:"targetId,omitempty"`
	Kind        string `json:"kind"`
	SourcePoint int64  `json:"sourcePoint,omitempty"`
	State       string `json:"state"`
	CreatedAt   int64  `json:"createdAt"`
}

// Limits bound delegation, as Settings sets them.
type Limits struct {
	Depth      int `json:"depth"`
	PerSession int `json:"perSession"`
	PerMac     int `json:"perMac"`
}

var DefaultLimits = Limits{Depth: 3, PerSession: 6, PerMac: 12}

// accessRank orders access from narrowest to broadest; settings is ask.
var accessRank = map[string]int{"settings": 0, "ask": 0, "edits": 1, "auto": 2, "full": 3}

// Broader says whether access a lets more happen without asking than b.
func Broader(a, b string) bool { return accessRank[a] > accessRank[b] }
