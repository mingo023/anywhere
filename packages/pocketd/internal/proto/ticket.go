package proto

import (
	"encoding/json"
	"slices"
	"strings"
	"unicode/utf8"

	"pocketd/internal/providers"
)

// CapBoard: board.* verbs and the board snapshot.
const CapBoard = "board.v1"

const (
	CodeInvalidTicket = "invalid_ticket"
	CodeUnknownTicket = "unknown_ticket"
	CodeTicketBusy    = "ticket_busy"
)

const (
	MinBoardLimit     = 1
	MaxBoardLimit     = 8
	DefaultBoardLimit = 4
	MaxTicketTitle    = 200
	MaxTicketPriority = 4
)

const (
	TicketTodo   = "todo"
	TicketQueued = "queued"
	TicketLive   = "live"
	TicketClosed = "closed"
)

const (
	TicketStarting = "starting"
	TicketWorking  = "working"
	TicketNeedsYou = "needsYou"
	TicketDone     = "done"
	TicketFailed   = "failed"
	TicketStopped  = "stopped"
	TicketIdle     = "idle"
)

const (
	ClosedByUser  = "closed"
	ClosedMerged  = "merged"
	ClosedDeleted = "deleted"
)

// Ticket is a piece of work on the Board. State says where it is in its life;
// Status, while it is live, what its session is doing.
type Ticket struct {
	ID       string `json:"id"`
	Title    string `json:"title"`
	Prompt   string `json:"prompt"`
	Provider string `json:"provider"`
	Folder   string `json:"folder"`
	// Base is the branch its Worktree starts from; empty is the project's default.
	Base  string `json:"base,omitempty"`
	State string `json:"state"`
	// Order places a todo or queued ticket in its column; queued ones start lowest first.
	Order      int64  `json:"order"`
	Status     string `json:"status,omitempty"`
	Why        string `json:"why,omitempty"`
	AgentID    string `json:"agentId,omitempty"`
	TerminalID string `json:"terminalId,omitempty"`
	Worktree   string `json:"worktree,omitempty"`
	CreatedAt  int64  `json:"createdAt"`
	ChangedAt  int64  `json:"changedAt"`
	ClosedAt   int64  `json:"closedAt,omitempty"`
	ClosedWhy  string `json:"closedWhy,omitempty"`
	// Relaunch: Start again was asked, so the next pump launches it whatever the limit.
	Relaunch bool `json:"relaunch,omitempty"`
	// Number is its short ID on the Board, unique among the tickets.
	Number int64 `json:"number,omitempty"`
	// Priority is 1 (urgent) to 4 (low); 0 is none. It never moves a ticket in line.
	Priority int      `json:"priority,omitempty"`
	Labels   []string `json:"labels,omitempty"`
}

// Valid is what a client's board.save must satisfy.
func (t Ticket) Valid() bool {
	title := utf8.RuneCountInString(strings.TrimSpace(t.Title))
	return title >= 1 && title <= MaxTicketTitle && strings.TrimSpace(t.Prompt) != "" && len(t.Prompt) <= MaxPrompt &&
		providers.Get(t.Provider) != nil && t.Folder != "" && !strings.HasPrefix(t.Base, "-") &&
		t.Priority >= 0 && t.Priority <= MaxTicketPriority && validLabels(t.Labels)
}

// TicketLabels are the only labels a ticket can carry, so every one has a row in the desktop's picker.
var TicketLabels = []string{"Bug", "Feature", "Docs", "Infra"}

func validLabels(labels []string) bool {
	for i, l := range labels {
		if !slices.Contains(TicketLabels, l) || slices.Contains(labels[:i], l) {
			return false
		}
	}
	return true
}

// HoldsSlot is whether t counts against the Board's limit.
func (t Ticket) HoldsSlot() bool {
	return t.State == TicketLive && slices.Contains([]string{TicketStarting, TicketWorking, TicketNeedsYou}, t.Status)
}

// Board is the full snapshot; it is never a delta.
type Board struct {
	Type    string   `json:"type"`
	Tickets []Ticket `json:"tickets"`
	Limit   int      `json:"limit"`
}

func NewBoard(tickets []Ticket, limit int) Board {
	if tickets == nil {
		tickets = []Ticket{}
	}
	return Board{"board", tickets, limit}
}

func decodeTicket(raw json.RawMessage, dst **Ticket) bool {
	f, ok := strictObject(raw, "id", "title", "prompt", "provider", "folder", "base", "priority", "labels")
	if !ok {
		return false
	}
	for _, k := range []string{"title", "prompt", "provider", "folder"} {
		if _, has := f[k]; !has {
			return false
		}
	}
	var t Ticket
	if json.Unmarshal(raw, &t) != nil || !t.Valid() {
		return false
	}
	*dst = &t
	return true
}
