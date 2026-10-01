// Package daemon wires PTY terminals to the agents the phone sees.
package daemon

import (
	"context"
	"encoding/json"
	"errors"
	"log"
	"os"
	"os/exec"
	"sync"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/ops"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/shellenv"
	"pocketd/internal/terminal"
	"pocketd/internal/timeline"
)

type Daemon struct {
	Terminals *terminal.Manager
	Agents    *agent.Registry
	Broker    *broker.Broker
	Home      string // the Claude plugin lives in Home/plugin
	Exe       string // absolute path of this binary, for the hook command
	Sock      string
	Plugin    string
	// Capture reads the user's login-shell environment. Nil keeps pocketd's own.
	Capture func() shellenv.Result

	mu       sync.Mutex
	env      *shellenv.Result
	present  map[string]*presence          // by terminal id
	watchers map[string]context.CancelFunc // by app-server socket
	watch    sync.Mutex                    // one observe at a time: the poller and hooks both run it
}

// Spawn starts m in a terminal. Without m.Env it gets the login-shell
// environment, read again once if the command isn't on its PATH, so a tool
// installed after pocketd started is found.
func (d *Daemon) Spawn(m ops.Msg) (*terminal.Terminal, error) {
	t, err := d.spawn(m)
	if errors.Is(err, exec.ErrNotFound) && m.Env == nil && d.Capture != nil {
		d.Recapture()
		t, err = d.spawn(m)
	}
	return t, err
}

func (d *Daemon) spawn(m ops.Msg) (*terminal.Terminal, error) {
	spec := terminal.Spec{ID: terminal.NewID(), Cmd: m.Cmd, Args: m.Args, Cwd: m.Cwd, Env: m.Env, Cols: m.Cols, Rows: m.Rows}
	if spec.Env == nil {
		spec.Env = d.LoginEnv()
	}
	spec.Env = d.Env(spec.Env, spec.ID)
	return d.Terminals.Spawn(spec)
}

func (d *Daemon) Recapture() {
	r := d.Capture()
	if r.Err != "" {
		log.Printf("login shell env: %s", r)
	}
	d.mu.Lock()
	d.env = &r
	d.mu.Unlock()
}

// LoginEnv is pocketd's own environment until the first capture lands.
func (d *Daemon) LoginEnv() []string {
	d.mu.Lock()
	defer d.mu.Unlock()
	if d.env == nil {
		return os.Environ()
	}
	return d.env.Env
}

// ShellEnv describes the last capture for pocketd status.
func (d *Daemon) ShellEnv() string {
	d.mu.Lock()
	defer d.mu.Unlock()
	if d.env == nil {
		return "pending"
	}
	return d.env.String()
}

// dismissAnswered closes the phone's card when a tool it is asking about
// finishes anyway: the desktop dialog answered, and Claude ignores the hook.
// The hook payload carries no tool_use_id, so identical parallel calls share
// a key and the broker dismisses the oldest.
func (d *Daemon) dismissAnswered(agentID string, keys map[string]string, e timeline.Event) {
	switch e.Kind {
	case "tool_start":
		keys[e.ToolUseID] = permissionKey(agentID, e.Name, e.Input)
	case "tool_end":
		decision := "allow"
		if !e.OK {
			decision = "deny"
		}
		d.Broker.Dismiss(keys[e.ToolUseID], decision)
		delete(keys, e.ToolUseID)
	}
}

// permissionKey matches a hook payload to its tool_use line. Re-encoding
// sorts object keys, so both sides agree on the input's bytes.
func permissionKey(agentID, tool string, input json.RawMessage) string {
	var v any
	json.Unmarshal(input, &v)
	canonical, _ := json.Marshal(v)
	return agentID + "\x00" + tool + "\x00" + string(canonical)
}

type hookInput struct {
	Event                 string            `json:"hook_event_name"`
	SessionID             string            `json:"session_id"`
	TranscriptPath        string            `json:"transcript_path"`
	Cwd                   string            `json:"cwd"`
	Model                 string            `json:"model"`
	Source                string            `json:"source"`
	ToolName              string            `json:"tool_name"`
	ToolInput             json.RawMessage   `json:"tool_input"`
	PermissionMode        string            `json:"permission_mode"`
	PermissionSuggestions []json.RawMessage `json:"permission_suggestions"`
}

type hookDecision struct {
	Behavior           string            `json:"behavior"`
	UpdatedPermissions []json.RawMessage `json:"updatedPermissions,omitempty"`
	Message            string            `json:"message,omitempty"`
	Interrupt          bool              `json:"interrupt,omitempty"`
}

var (
	errHookForged     = &peer.Refusal{Code: proto.CodeHookForged, Message: "hook sender is not a claude in this Terminal"}
	errNotOwnTerminal = &peer.Refusal{Code: proto.CodeNotOwnTerminal, Message: "hooks are accepted only for the caller's own Terminal"}
)

// Hook takes one hook call for terminal m.ID from p, which must be a PTY peer
// of that terminal whose nearest claude is the agent there. Only a
// PermissionRequest gets a reply; nil lets Claude go on as if there were no hook.
func (d *Daemon) Hook(ctx context.Context, p peer.Principal, m ops.Msg) ([]byte, error) {
	if p.Kind != peer.PTY {
		return nil, errHookForged
	}
	if p.Terminal != m.ID {
		return nil, errNotOwnTerminal
	}
	var in hookInput
	if json.Unmarshal(m.Data, &in) != nil {
		return nil, nil
	}
	chain, _ := peer.Ancestors(p.Pid)
	pr := d.claudeAt(m.ID, NearestClaude(chain))
	if pr == nil {
		return nil, errHookForged
	}
	switch in.Event {
	case "SessionStart":
		d.sessionStart(pr, in)
	case "UserPromptSubmit":
		pr.working("")
	case "PostToolUse", "PostToolUseFailure", "PermissionDenied":
		pr.working(permissionKey(pr.a.ID(), in.ToolName, in.ToolInput))
	case "PreToolUse", "Notification":
		pr.a.NeedsYou()
	case "PermissionRequest":
		return d.permission(ctx, pr, in), nil
	case "Stop":
		pr.a.TurnEnded(false)
	case "StopFailure":
		pr.a.TurnEnded(true)
	case "PreCompact":
		pr.a.SetCompacting()
	}
	return nil, nil
}

// permission asks the phone until pr ends; nil lets Claude's dialog decide.
func (d *Daemon) permission(ctx context.Context, pr *presence, in hookInput) []byte {
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	defer context.AfterFunc(pr.ctx, cancel)()
	ag := pr.a
	req := proto.PermissionRequest{AgentID: ag.ID(), ToolName: in.ToolName, Detail: timeline.Detail(in.ToolName, in.ToolInput), Options: in.options(), Feedback: true}
	key := permissionKey(ag.ID(), in.ToolName, in.ToolInput)
	pr.mu.Lock()
	pr.asks[key]++
	ag.NeedsYou()
	pr.mu.Unlock()
	a := d.Broker.Ask(ctx, req, key)
	pr.mu.Lock()
	if pr.asks[key]--; pr.asks[key] == 0 {
		delete(pr.asks, key)
	}
	pr.mu.Unlock()
	var decision hookDecision
	switch a.Decision {
	case "":
		return nil
	case "allow":
		decision = hookDecision{Behavior: "allow", UpdatedPermissions: in.updates(a.Option)}
		pr.working("")
	case "deny":
		decision = hookDecision{Behavior: "deny", Message: "Denied from phone", Interrupt: true}
		ag.Clear()
		if a.Message != "" {
			// Claude distrusts a hook's deny message as tool output, so feedback
			// goes in as the next prompt, like the desktop's "What should Claude do instead?".
			go promptAfterTurn(ag, ag.Summary().MaxSeq, a.Message)
		}
	}
	out, _ := json.Marshal(map[string]any{"hookSpecificOutput": map[string]any{"hookEventName": "PermissionRequest", "decision": decision}})
	return out
}

var FeedbackWait = 30 * time.Second

// promptAfterTurn types text once the interrupted turn has ended past
// seq; typing earlier would land in Claude's still-open permission dialog.
func promptAfterTurn(a *agent.Agent, seq int64, text string) {
	for deadline := time.Now().Add(FeedbackWait); time.Now().Before(deadline); time.Sleep(100 * time.Millisecond) {
		if s := a.Summary(); (s.Status == "idle" || s.Status == "done") && s.MaxSeq > seq {
			a.Driver().Prompt(text)
			return
		}
	}
}
