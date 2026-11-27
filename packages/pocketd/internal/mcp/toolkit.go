package mcp

import (
	"context"
	"encoding/json"
	"errors"
	"slices"
	"sort"
	"strings"
	"sync"

	"pocketd/internal/orch"
	"pocketd/internal/proto"
)

// AgentKind is one agent pocketd can run, as the tools describe it.
type AgentKind struct {
	ID        string   `json:"id"`
	Name      string   `json:"name"`
	Installed bool     `json:"installed"`
	Models    []string `json:"models,omitempty"`
	Efforts   []string `json:"efforts,omitempty"`
	Access    []string `json:"access,omitempty"`
	NeverAsks bool     `json:"alwaysFull,omitempty"`
	Parent    bool     `json:"canDelegate"`
}

// Host is the rest of pocketd as the tools need it.
type Host interface {
	Engine() *orch.Engine
	// SessionIn is the live session in terminal.
	SessionIn(terminal string) (proto.AgentSummary, bool)
	Summary(id string) (proto.AgentSummary, bool)
	// Items is a session's timeline: live while it runs, the store's once closed.
	Items(id string) []proto.Item
	Agents() []AgentKind
	// Models is the models an agent kind offers; asking may run its CLI.
	Models(kind string) []string
	// Efforts is the reasoning efforts an agent kind's CLI lists with its models.
	Efforts(kind string) []string
	// Dirty says whether cwd has uncommitted changes.
	Dirty(cwd string) bool
	// OutsideGit says whether the Project holding cwd isn't a git repository.
	OutsideGit(cwd string) bool
	// InBackground says whether id names a call of session's still running in the background.
	InBackground(session, id string) bool
	TaskOf(session, toolUseID string) string
	Schedules() Schedules
	// Environments is the linked hosts; nil when there are none to link.
	Environments() Environments
	// PR is the pull request of the branch checked out at tree, as gh last told, asking gh within ctx for a
	// tree it never told of; nil when it told none.
	PR(ctx context.Context, tree string) *proto.PR
}

type Environment struct {
	ID, Label, State, Error string
}

type Environments interface {
	List() []Environment
	Projects(ctx context.Context, id string) ([]proto.Project, error)
	AddProject(ctx context.Context, id, path string) error
	Remove(id string) error
	Card(session string, l proto.HostLink) error
}

// Schedules are Automations, as the schedule tools see them.
type Schedules interface {
	List() []proto.Automation
	Save(a proto.Automation) (proto.Automation, error)
	Delete(id string) error
	RunNow(id string) (proto.Run, error)
	Get(id string) (proto.Automation, bool)
}

// Call is one tool call: who made it and with what.
type Call struct {
	Host   Host
	Grant  Grant
	Caller proto.AgentSummary
	Args   json.RawMessage
}

func (c *Call) args(v any) error {
	if len(c.Args) == 0 || string(c.Args) == "null" {
		return nil
	}
	if err := json.Unmarshal(c.Args, v); err != nil {
		return &orch.Failure{Code: orch.CodeInvalid, Message: "Bad arguments: " + err.Error()}
	}
	return nil
}

type Tool struct {
	Name        string
	Group       string
	Description string
	Schema      string // JSON Schema of the arguments
	Run         func(ctx context.Context, c *Call) (any, error)
}

// Direct tools are listed from the start; tool_search loads the others.
var Direct = []string{"orchestrator_capabilities", "delegate_task", "task_status", "tool_search"}

// DirectOnly are the blocking tools on Codex's second server.
var DirectOnly = []string{"anywhere_thread_wait", "delegate_task"}

// Toolkit holds the tools and which ones each session has loaded.
type Toolkit struct {
	Host  Host
	tools map[string]Tool

	mu     sync.Mutex
	loaded map[string]map[string]bool // by terminal
}

func NewToolkit(h Host) *Toolkit {
	k := &Toolkit{Host: h, tools: map[string]Tool{}, loaded: map[string]map[string]bool{}}
	for _, t := range allTools() {
		k.tools[t.Name] = t
	}
	// Call answers tool_search itself; this entry only lists it.
	k.tools["tool_search"] = Tool{Name: "tool_search", Group: "A",
		Description: "Find Anywhere's other orchestrator tools by what you want to do (send to a thread, read a thread, schedules, queue, fork). The tools found are listed for this session from then on.",
		Schema:      `{"type":"object","required":["query"],"properties":{"query":{"type":"string"}}}`}
	return k
}

type listed struct {
	Name        string          `json:"name"`
	Description string          `json:"description"`
	InputSchema json.RawMessage `json:"inputSchema"`
}

// List is what terminal's session sees in tools/list.
func (k *Toolkit) List(terminal string, direct bool) []listed {
	names := slices.Clone(Direct)
	if direct {
		names = slices.Clone(DirectOnly)
	} else {
		k.mu.Lock()
		for n := range k.loaded[terminal] {
			if !slices.Contains(names, n) {
				names = append(names, n)
			}
		}
		k.mu.Unlock()
	}
	sort.Strings(names)
	out := []listed{}
	for _, n := range names {
		if t, ok := k.tools[n]; ok {
			out = append(out, listed{t.Name, t.Description, json.RawMessage(t.Schema)})
		}
	}
	return out
}

// Call runs a tool and says whether the session's list changed.
func (k *Toolkit) Call(ctx context.Context, g Grant, name string, args json.RawMessage) (map[string]any, bool) {
	caller, ok := k.Host.SessionIn(g.Terminal)
	if !ok {
		return failure(&orch.Failure{Code: orch.CodeUnavailable, Message: "This session isn't running in Anywhere anymore"}), false
	}
	if name == "tool_search" {
		var p struct {
			Query string `json:"query"`
		}
		json.Unmarshal(args, &p)
		found := k.search(p.Query)
		k.mu.Lock()
		if k.loaded[g.Terminal] == nil {
			k.loaded[g.Terminal] = map[string]bool{}
		}
		changed := false
		for _, t := range found {
			if !k.loaded[g.Terminal][t.Name] {
				k.loaded[g.Terminal][t.Name], changed = true, true
			}
		}
		k.mu.Unlock()
		var res []map[string]string
		for _, t := range found {
			res = append(res, map[string]string{"name": t.Name, "description": t.Description})
		}
		return success(map[string]any{"tools": res, "note": "These tools are now listed for this session; call them directly."}), changed
	}
	t, ok := k.tools[name]
	if !ok {
		return failure(&orch.Failure{Code: orch.CodeNotFound, Message: "Unknown tool " + name}), false
	}
	out, err := t.Run(ctx, &Call{Host: k.Host, Grant: g, Caller: caller, Args: args})
	if err != nil {
		return failure(err), false
	}
	return success(out), false
}

func (k *Toolkit) Forget(terminal string) {
	k.mu.Lock()
	delete(k.loaded, terminal)
	k.mu.Unlock()
}

// search scores every tool against the query's terms, best first, at most 10.
func (k *Toolkit) search(query string) []Tool {
	terms := strings.Fields(strings.ToLower(strings.NewReplacer("_", " ", "-", " ").Replace(query)))
	type scored struct {
		t Tool
		n int
	}
	var all []scored
	for _, t := range k.tools {
		if slices.Contains(Direct, t.Name) {
			continue
		}
		name := strings.ToLower(strings.ReplaceAll(t.Name, "_", " "))
		desc := strings.ToLower(t.Description)
		n := 0
		for _, term := range terms {
			if strings.Contains(name, term) {
				n += 3
			}
			if strings.Contains(desc, term) {
				n++
			}
		}
		if len(terms) == 0 {
			n = 1
		}
		if n > 0 {
			all = append(all, scored{t, n})
		}
	}
	sort.Slice(all, func(i, j int) bool {
		if all[i].n != all[j].n {
			return all[i].n > all[j].n
		}
		return all[i].t.Name < all[j].t.Name
	})
	var out []Tool
	for i, s := range all {
		if i == 10 {
			break
		}
		out = append(out, s.t)
	}
	return out
}

func success(v any) map[string]any {
	b, _ := json.Marshal(v)
	res := map[string]any{"content": []map[string]any{{"type": "text", "text": string(b)}}, "isError": false}
	var obj map[string]any
	if json.Unmarshal(b, &obj) == nil {
		res["structuredContent"] = obj
	}
	return res
}

func failure(err error) map[string]any {
	f := orch.AsFailure(err)
	var o *orch.Failure
	if !errors.As(err, &o) {
		f = &orch.Failure{Code: orch.CodeInvalid, Message: err.Error()}
	}
	body := map[string]any{"error": map[string]string{"code": f.Code, "message": f.Message}}
	b, _ := json.Marshal(body)
	return map[string]any{"content": []map[string]any{{"type": "text", "text": string(b)}}, "structuredContent": body, "isError": true}
}

// Link is how a result points at a session; the desktop opens it.
func Link(id string) string { return "anywhere://session/" + id }
