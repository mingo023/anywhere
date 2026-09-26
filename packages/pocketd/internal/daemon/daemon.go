// Package daemon wires PTY sessions to the agents the phone sees.
package daemon

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/claude"
	"pocketd/internal/ops"
	"pocketd/internal/proto"
	"pocketd/internal/session"
	"pocketd/internal/timeline"
)

type Daemon struct {
	Sessions *session.Manager
	Agents   *agent.Registry
	Broker   *broker.Broker
	Home     string // settings files live in Home/run
	Exe      string // absolute path of this binary, for the hook command
	Sock     string

	codexLocks   sync.Map
	codexThreads sync.Map
}

func (d *Daemon) Spawn(m ops.Msg) (*session.Session, error) {
	spec := session.Spec{ID: session.NewID(), Cmd: m.Cmd, Args: m.Args, Cwd: m.Cwd, Env: m.Env, Cols: m.Cols, Rows: m.Rows}
	if spec.Env == nil {
		spec.Env = os.Environ()
	}
	switch filepath.Base(m.Cmd) {
	case "claude":
		return d.spawnClaude(spec)
	case "codex":
		return d.spawnCodex(spec)
	}
	return d.Sessions.Spawn(spec)
}

func (d *Daemon) spawnClaude(spec session.Spec) (*session.Session, error) {
	settings, err := d.writeSettings(spec.ID)
	if err != nil {
		return nil, err
	}
	spec.Args = append([]string{"--session-id", spec.ID, "--settings", settings}, spec.Args...)
	spec.Env = append(spec.Env, "POCKETD_SOCK="+d.Sock)
	s, err := d.Sessions.Spawn(spec)
	if err != nil {
		os.Remove(settings)
		return nil, err
	}
	drv := &claudeDriver{s: s}
	d.track(s, spec.ID, spec.Cwd, "claude", func(a *agent.Agent) agent.Driver { drv.a = a; return drv }, func(ctx context.Context, a *agent.Agent) {
		defer os.Remove(settings)
		keys := map[string]string{}
		claude.Tail(ctx, claude.Glob(spec.Env, spec.ID), func(line []byte) {
			events, title := claude.Map(line)
			if title != "" {
				a.SetTitle(title)
			}
			if model := claude.Model(line); model != "" {
				a.SetModel(model)
			}
			for _, e := range events {
				d.dismissAnswered(a.ID(), keys, e)
				a.Apply(e)
			}
		})
	})
	return s, nil
}

// track shows s to the phone as agent id while follow runs. follow's ctx
// ends with the session; the agent goes when follow returns.
func (d *Daemon) track(s *session.Session, id, cwd, provider string, newDriver func(*agent.Agent) agent.Driver, follow func(context.Context, *agent.Agent)) {
	a := d.Agents.AddFunc(id, cwd, provider, newDriver)
	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		<-s.Done()
		cancel()
	}()
	go func() {
		follow(ctx, a)
		cancel()
		d.Broker.DenyAll(id)
		d.Agents.Remove(id)
	}()
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

func (d *Daemon) writeSettings(id string) (string, error) {
	hook := map[string]any{"type": "command", "command": fmt.Sprintf("%q hook", d.Exe), "timeout": 610}
	settings := map[string]any{"hooks": map[string]any{"PermissionRequest": []any{map[string]any{"hooks": []any{hook}}}}}
	raw, _ := json.Marshal(settings)
	dir := filepath.Join(d.Home, "run")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return "", err
	}
	path := filepath.Join(dir, id+".settings.json")
	return path, os.WriteFile(path, raw, 0o600)
}

type hookInput struct {
	SessionID             string            `json:"session_id"`
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

// Hook answers one PermissionRequest hook call; nil lets Claude's dialog decide.
func (d *Daemon) Hook(ctx context.Context, payload []byte) []byte {
	var in hookInput
	if json.Unmarshal(payload, &in) != nil {
		return nil
	}
	ag, err := d.Agents.Get(in.SessionID)
	if err != nil {
		return nil
	}
	req := proto.PermissionRequest{AgentID: in.SessionID, ToolName: in.ToolName, Detail: timeline.Detail(in.ToolName, in.ToolInput), Options: in.options(), Feedback: true}
	a := d.Broker.Ask(ctx, req, permissionKey(in.SessionID, in.ToolName, in.ToolInput))
	var decision hookDecision
	switch a.Decision {
	case "":
		return nil
	case "allow":
		decision = hookDecision{Behavior: "allow", UpdatedPermissions: in.updates(a.Option)}
	case "deny":
		decision = hookDecision{Behavior: "deny", Message: "Denied from phone", Interrupt: true}
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
		if s := a.Summary(); s.Status == "idle" && s.MaxSeq > seq {
			a.Driver().Prompt(text)
			return
		}
	}
}

type claudeDriver struct {
	s *session.Session
	a *agent.Agent
}

func (c *claudeDriver) Prompt(text string) error { return c.s.Prompt(text) }
func (c *claudeDriver) Interrupt() error         { return c.s.Write([]byte{0x1b}) }
func (c *claudeDriver) Close()                   { c.s.Close() }

func (c *claudeDriver) Compact() error {
	c.a.SetCompacting()
	return c.s.Prompt("/compact")
}
