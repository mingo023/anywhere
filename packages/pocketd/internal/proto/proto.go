// Package proto mirrors packages/protocol (TypeScript). The golden files in
// testdata are the contract: packages/protocol decodes them too.
package proto

import (
	"encoding/json"
	"errors"
)

const (
	MinVersion       = 3
	MaxVersion       = 3
	ToolOutputLimit  = 64 * 1024
	DiffPreviewLines = 24
	DiffLineChars    = 160
)

// AgentSummary.Restore says how an Agent came back after a pocketd restart.
const (
	RestoreResumed       = "resumed"
	RestoreInterrupted   = "interrupted"
	RestoreAccessLowered = "access_lowered"
	RestoreFailed        = "failed"
	CapRestore           = "restore.v1"
	CodeAgentResuming    = "agent_resuming"
)

type DiffLine struct {
	Number int    `json:"number,omitempty"`
	Kind   string `json:"kind"`
	Text   string `json:"text"`
}

type FileDiff struct {
	Lines       []DiffLine `json:"lines"`
	Additions   int        `json:"additions"`
	Deletions   int        `json:"deletions"`
	ContentOnly bool       `json:"contentOnly,omitempty"`
}

// ToolDetail is a union keyed by Kind; MarshalJSON writes only that kind's fields.
type ToolDetail struct {
	Kind        string
	Command     string
	Description string
	Path        string
	Diff        *FileDiff
	Query       string
	Name        string
	Input       json.RawMessage
}

func (d ToolDetail) MarshalJSON() ([]byte, error) {
	switch d.Kind {
	case "shell":
		return json.Marshal(struct {
			Kind        string `json:"kind"`
			Command     string `json:"command"`
			Description string `json:"description,omitempty"`
		}{d.Kind, d.Command, d.Description})
	case "read":
		return json.Marshal(struct {
			Kind string `json:"kind"`
			Path string `json:"path"`
		}{d.Kind, d.Path})
	case "edit", "write":
		return json.Marshal(struct {
			Kind string    `json:"kind"`
			Path string    `json:"path"`
			Diff *FileDiff `json:"diff,omitempty"`
		}{d.Kind, d.Path, d.Diff})
	case "search":
		return json.Marshal(struct {
			Kind  string `json:"kind"`
			Query string `json:"query"`
			Path  string `json:"path,omitempty"`
		}{d.Kind, d.Query, d.Path})
	case "task":
		return json.Marshal(struct {
			Kind        string `json:"kind"`
			Description string `json:"description"`
		}{d.Kind, d.Description})
	case "other":
		input := d.Input
		if len(input) == 0 {
			input = json.RawMessage("null")
		}
		return json.Marshal(struct {
			Kind  string          `json:"kind"`
			Name  string          `json:"name"`
			Input json.RawMessage `json:"input"`
		}{d.Kind, d.Name, input})
	}
	return nil, errors.New("proto: unknown tool detail kind " + d.Kind)
}

type ToolCall struct {
	ToolUseID  string     `json:"toolUseId"`
	Name       string     `json:"name"`
	Detail     ToolDetail `json:"detail"`
	Status     string     `json:"status"`
	Output     *string    `json:"output,omitempty"`
	DurationMs *int64     `json:"durationMs,omitempty"`
}

type TaskItem struct {
	Text   string `json:"text"`
	Status string `json:"status"`
}

type TurnUsage struct {
	InputTokens     int `json:"inputTokens"`
	OutputTokens    int `json:"outputTokens"`
	CacheReadTokens int `json:"cacheReadTokens,omitempty"`
}

// Item is a TimelineItem, a union keyed by Kind; MarshalJSON writes only
// that kind's fields next to id, seq and ts.
type Item struct {
	ID         string
	Seq        int64
	Ts         int64
	Kind       string
	Text       string
	Call       *ToolCall
	Tasks      []TaskItem
	Trigger    string
	OK         bool
	DurationMs int64
	Error      string
	Usage      *TurnUsage
}

func (it Item) MarshalJSON() ([]byte, error) {
	type head struct {
		Kind string `json:"kind"`
		ID   string `json:"id"`
		Seq  int64  `json:"seq"`
		Ts   int64  `json:"ts"`
	}
	h := head{it.Kind, it.ID, it.Seq, it.Ts}
	switch it.Kind {
	case "user", "assistant", "thinking", "plan":
		return json.Marshal(struct {
			head
			Text string `json:"text"`
		}{h, it.Text})
	case "tool":
		return json.Marshal(struct {
			head
			Call *ToolCall `json:"call"`
		}{h, it.Call})
	case "tasks":
		items := it.Tasks
		if items == nil {
			items = []TaskItem{}
		}
		return json.Marshal(struct {
			head
			Items []TaskItem `json:"items"`
		}{h, items})
	case "compact":
		return json.Marshal(struct {
			head
			Trigger string `json:"trigger"`
		}{h, it.Trigger})
	case "result":
		return json.Marshal(struct {
			head
			OK         bool       `json:"ok"`
			DurationMs int64      `json:"durationMs"`
			Error      string     `json:"error,omitempty"`
			Usage      *TurnUsage `json:"usage,omitempty"`
		}{h, it.OK, it.DurationMs, it.Error, it.Usage})
	}
	return nil, errors.New("proto: unknown item kind " + it.Kind)
}

type AgentSummary struct {
	ID                string `json:"id"`
	TerminalID        string `json:"terminalId"`
	Title             string `json:"title"`
	Cwd               string `json:"cwd"`
	Provider          string `json:"provider"`
	Model             string `json:"model,omitempty"`
	Effort            string `json:"effort,omitempty"`
	Status            string `json:"status"`
	Failed            bool   `json:"failed,omitempty"`
	Attached          bool   `json:"attached"`
	Restore           string `json:"restore,omitempty"`
	Compacting        bool   `json:"compacting,omitempty"`
	Pinned            bool   `json:"pinned,omitempty"`
	Epoch             int64  `json:"epoch"`
	MaxSeq            int64  `json:"maxSeq"`
	ProviderSessionID string `json:"providerSessionId,omitempty"`
	CreatedAt         int64  `json:"createdAt"`
	UpdatedAt         int64  `json:"updatedAt"`
	Project           string `json:"project,omitempty"`
	Worktree          string `json:"worktree,omitempty"`
	MainWorktree      bool   `json:"mainWorktree,omitempty"`
	Branch            string `json:"branch,omitempty"`
	TokensUsed        int64  `json:"tokensUsed,omitempty"`
	ContextWindow     int64  `json:"contextWindow,omitempty"`
	Origin            string `json:"origin"`
	Activity          string `json:"activity,omitempty"`
}

type PermissionRequest struct {
	RequestID string             `json:"requestId"`
	AgentID   string             `json:"agentId"`
	ToolName  string             `json:"toolName"`
	Detail    ToolDetail         `json:"detail"`
	Options   []PermissionOption `json:"options,omitempty"`
	// Feedback is set when a deny can carry a message back to the agent.
	Feedback bool `json:"feedback,omitempty"`
}

// PermissionOption is an allow variant offered beside plain allow and deny.
type PermissionOption struct {
	ID    string `json:"id"`
	Label string `json:"label"`
}
