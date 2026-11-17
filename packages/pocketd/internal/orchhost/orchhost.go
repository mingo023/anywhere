// Package orchhost joins the orchestrator to the rest of pocketd: it runs the
// engine's effects on agents, the launcher and the broker, answers the MCP
// tools' questions, and hands chat agents Anywhere's MCP server.
package orchhost

import (
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"pocketd/internal/acp"
	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/catalog"
	"pocketd/internal/hosts"
	"pocketd/internal/launch"
	"pocketd/internal/mcp"
	"pocketd/internal/orch"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
	"pocketd/internal/registry"
	"pocketd/internal/shellenv"
	"pocketd/internal/timeline"
	"pocketd/internal/worktree"
)

// Host is the orchestrator's view of pocketd.
type Host struct {
	Registry *agent.Registry
	Broker   *broker.Broker
	Launcher *launch.Launcher
	// Projects is desktop.json, where a session's cwd finds its Project's path.
	Projects *registry.Registry
	Tokens   *mcp.Tokens
	// StateDir holds state/mcp, the Claude MCP config files.
	StateDir string
	// URL is the MCP endpoint, http://127.0.0.1:<port>/mcp.
	URL string
	// Planning says whether the agent in a terminal is in plan mode.
	Planning func(terminal string) bool
	// LoginEnv is the user's shell environment, for git.
	LoginEnv func() []string
	Sched    mcp.Schedules
	// Hosts runs the sessions on linked hosts; nil links none.
	Hosts *hosts.Manager
	Envs  mcp.Environments

	engine *orch.Engine
	models sync.Map // kind -> func() []string
}

func (h *Host) SetEngine(e *orch.Engine) { h.engine = e }

func (h *Host) Engine() *orch.Engine { return h.engine }

func AgentInfo(kind string) orch.AgentInfo {
	p := providers.Get(kind)
	if p == nil {
		return orch.AgentInfo{}
	}
	steer := p.Chat.SendWhileWorking && (kind == "claude" || kind == "codex")
	if p.NeverAsks {
		return orch.AgentInfo{Known: true, NeverAsks: true, Access: []string{"full"}, CanSteer: steer}
	}
	return orch.AgentInfo{Known: true, Access: proto.Accesses, CanSteer: steer}
}

// Parent says whether kind's chats get the tools: Claude, Codex and HTTP-MCP ACP agents.
func Parent(kind string) bool { return kind != "pi" && kind != "omp" }

func (h *Host) agent(id string) (*agent.Agent, error) {
	a, err := h.Registry.Get(id)
	if err != nil {
		return nil, &orch.Failure{Code: orch.CodeNotFound, Message: "That session isn't running"}
	}
	return a, nil
}

// Run is orch.Effector.
func (h *Host) Run(ctx context.Context, e orch.Effect) (map[string]any, error) {
	switch e.Kind {
	case orch.EffLaunch, orch.EffForkLaunch:
		var p orch.LaunchPayload
		json.Unmarshal(e.Payload, &p)
		if p.Env != "" {
			return h.launchOn(ctx, e, p)
		}
		return h.launch(e, p)
	}
	// Naming has a lane of its own so it doesn't wait behind the session's turns.
	e.Lane = strings.TrimPrefix(e.Lane, "naming:")
	if hosts.Tagged(e.Lane) {
		return nil, h.forward(ctx, e)
	}
	a, err := h.agent(e.Lane)
	if err != nil && e.Kind != orch.EffAnswer {
		return nil, err
	}
	switch e.Kind {
	case orch.EffTurnStart:
		var p orch.TurnPayload
		json.Unmarshal(e.Payload, &p)
		return nil, prompt(ctx, a, p)
	case orch.EffInterrupt:
		return nil, a.Driver().Interrupt()
	case orch.EffAnswer:
		var p orch.RequestPayload
		json.Unmarshal(e.Payload, &p)
		if !h.Broker.Resolve(p.RequestID, broker.Answer{Decision: p.Decision, Option: p.Option, Message: p.Message, Answers: p.Answers}) {
			return nil, &orch.Failure{Code: orch.CodeNotFound, Message: "That question is no longer open"}
		}
		return nil, nil
	case orch.EffClose:
		a.Driver().Close()
		return nil, nil
	case orch.EffRename:
		var p orch.TitlePayload
		json.Unmarshal(e.Payload, &p)
		a.Rename(p.Title)
		return nil, nil
	case orch.EffNaming:
		return nil, h.Launcher.Retitle(a)
	case orch.EffConfigure:
		var p orch.ConfigPayload
		json.Unmarshal(e.Payload, &p)
		if p.Model != "" || p.Effort != "" {
			if err := launch.ValidSwitch(a.Provider(), p.Model, p.Effort); err != nil {
				return nil, &orch.Failure{Code: orch.CodeInvalid, Message: err.Error()}
			}
			if err := a.Driver().Settings(p.Model, p.Effort); err != nil {
				return nil, err
			}
		}
		if len(p.Options) > 0 {
			o, ok := a.Driver().(agent.Optioner)
			if !ok {
				return nil, &orch.Failure{Code: orch.CodeUnsupported, Message: "This agent takes no model options"}
			}
			return nil, o.SetOptions(p.Options)
		}
		return nil, nil
	case orch.EffPin:
		var p struct {
			Pinned bool `json:"pinned"`
		}
		json.Unmarshal(e.Payload, &p)
		a.SetPinned(p.Pinned)
		return nil, nil
	case orch.EffMarkUnread:
		a.MarkUnread()
		return nil, nil
	case orch.EffDelivery:
		var p orch.DeliveryPayload
		json.Unmarshal(e.Payload, &p)
		a.Record(timeline.Event{Kind: "delivery", Delivery: &proto.Delivery{TaskIDs: p.TaskIDs, ChildIDs: p.ChildIDs, Titles: p.Titles, Agents: p.Agents, Outcome: p.Outcome, Question: p.Question}})
		return nil, nil
	case orch.EffHold:
		var p orch.HoldPayload
		json.Unmarshal(e.Payload, &p)
		a.Hold(p.TaskIDs, p.Waiting, orch.HoldCap*time.Millisecond)
		return nil, nil
	case orch.EffStopTask:
		var p orch.StopTaskPayload
		json.Unmarshal(e.Payload, &p)
		s, ok := a.Driver().(agent.TaskStopper)
		if !ok {
			return nil, &orch.Failure{Code: orch.CodeUnsupported, Message: "Only a Claude chat stops a workflow"}
		}
		return nil, s.StopTask(p.TaskID)
	}
	return nil, fmt.Errorf("unknown effect %s", e.Kind)
}

// prompt starts a turn with p. A wake or notice to a session still resuming
// from a restart waits until it has, since nobody sends it again; the user's
// own prompt is refused at once, as before.
func prompt(ctx context.Context, a *agent.Agent, p orch.TurnPayload) error {
	for {
		err := a.Driver().Prompt(p.Text)
		if !errors.Is(err, agent.ErrResuming) || p.Origin == orch.OriginUser {
			return err
		}
		select {
		case <-ctx.Done():
			return err
		case <-time.After(200 * time.Millisecond):
		}
	}
}

// forward runs an effect on a host's session there: the four a parent's tools raise; the rest are refused.
func (h *Host) forward(ctx context.Context, e orch.Effect) error {
	host, theirs := hosts.Untag(e.Lane)
	var msg map[string]any
	switch e.Kind {
	case orch.EffTurnStart:
		var p orch.TurnPayload
		json.Unmarshal(e.Payload, &p)
		msg = map[string]any{"type": "agent.prompt", "agentId": theirs, "text": p.Text}
	case orch.EffInterrupt:
		msg = map[string]any{"type": "agent.interrupt", "agentId": theirs}
	case orch.EffAnswer:
		var p orch.RequestPayload
		json.Unmarshal(e.Payload, &p)
		_, request := hosts.Untag(p.RequestID)
		msg = map[string]any{"type": "permission.resolve", "requestId": request, "decision": cmp.Or(p.Decision, "allow"), "answers": p.Answers}
	case orch.EffClose:
		msg = map[string]any{"type": "agent.close", "agentId": theirs}
	default:
		return &orch.Failure{Code: orch.CodeUnsupported, Message: fmt.Sprintf("That session runs on %s; open it there", host)}
	}
	if h.Hosts == nil {
		return &orch.Failure{Code: orch.CodeUnavailable, Message: "Linked hosts are off"}
	}
	return h.Hosts.Send(ctx, host, msg)
}

// launchOn starts a session on the linked host p.Env, in its project p.HostProject.
func (h *Host) launchOn(ctx context.Context, e orch.Effect, p orch.LaunchPayload) (map[string]any, error) {
	if h.Hosts == nil {
		return nil, &orch.Failure{Code: orch.CodeUnavailable, Message: "Linked hosts are off"}
	}
	spec := proto.LaunchSpec{Provider: p.Agent, Model: p.Model, Effort: p.Effort, Access: p.Access, Plan: h.planning(p.Parent), Prompt: firstPrompt(p.Context, p.Prompt), Project: p.HostProject, Chat: p.Chat}
	switch p.Checkout {
	case "worktree":
		// The host's default branch: the parent's HEAD may not exist there.
		spec.Checkout = proto.Checkout{New: &proto.NewWorktree{Name: cmp.Or(launch.Slug(cmp.Or(p.Title, p.Prompt)), "task"), AutoName: true}}
	case "existing_worktree":
		spec.Checkout = proto.Checkout{Worktree: p.Worktree}
	default:
		spec.Checkout = proto.Checkout{Worktree: p.HostProject}
	}
	child, err := h.Hosts.Launch(ctx, p.Env, spec, cmp.Or(p.TaskID, p.TransferID, fmt.Sprint(e.ID)))
	if err != nil {
		return nil, &orch.Failure{Code: orch.CodeUnavailable, Message: err.Error()}
	}
	return map[string]any{"childId": child, "targetId": child}, nil
}

func (h *Host) launch(e orch.Effect, p orch.LaunchPayload) (map[string]any, error) {
	project, ok := projectPath(h.Projects.Load(), p.Project, p.Cwd)
	if !ok {
		return nil, &orch.Failure{Code: orch.CodeInvalid, Message: "Anywhere starts sessions in a project; this one is in none"}
	}
	spec := proto.LaunchSpec{Provider: p.Agent, Model: p.Model, Effort: p.Effort, Access: p.Access, Plan: h.planning(p.Parent), Prompt: p.Prompt, Project: project, Chat: p.Chat && providers.Get(p.Agent) != nil}
	spec.Prompt = firstPrompt(p.Context, p.Prompt)
	if p.Fork != "" {
		src, err := h.agent(p.Fork)
		if err != nil {
			return nil, err
		}
		spec.Fork = src.Summary().ProviderSessionID
	}
	switch p.Checkout {
	case "worktree":
		name := launch.Slug(cmp.Or(p.Title, p.Prompt))
		if name == "" {
			name = "task"
		}
		base := h.head(p.Cwd)
		spec.Checkout = proto.Checkout{New: &proto.NewWorktree{Name: name, Base: base, AutoName: true}}
	case "existing_worktree":
		spec.Checkout = proto.Checkout{Worktree: p.Worktree}
	case "root":
		spec.Checkout = proto.Checkout{Worktree: project}
	default:
		spec.Checkout = proto.Checkout{Worktree: cmp.Or(h.treeOf(p.Cwd), project)}
	}
	key := cmp.Or(p.TaskID, p.TransferID, fmt.Sprint(e.ID))
	r := h.Launcher.Create(launch.Who{Owner: true, Key: "orch"}, key, spec, func(string, string) {}, func(launch.Creating) {})
	if r.Err != nil {
		return nil, &orch.Failure{Code: orch.CodeUnavailable, Message: cmp.Or(r.Err.Message, r.Err.Code)}
	}
	if a, err := h.Registry.Get(r.AgentID); err == nil {
		a.SetLineage(p.Parent, p.Rel, p.CreatedBy)
		if p.TaskID != "" {
			a.SetDelegation(&proto.DelegationRef{TaskID: p.TaskID, Status: orch.TaskWorking})
		}
	}
	return map[string]any{"childId": r.AgentID, "targetId": r.AgentID}, nil
}

// firstPrompt is what a session started from a context block is first told: the block, then the prompt.
func firstPrompt(context, prompt string) string {
	if context == "" {
		return prompt
	}
	return context + "\n\n" + cmp.Or(prompt, "Continue from this context. Wait for the next instruction before changing anything.")
}

// planning says whether a child of parent starts in plan mode: the parent's own.
func (h *Host) planning(parent string) bool {
	if parent == "" || h.Planning == nil {
		return false
	}
	a, err := h.Registry.Get(parent)
	return err == nil && h.Planning(a.Summary().TerminalID)
}

// projectPath is the registered Project a launch goes to: the one holding
// cwd, else the one named, as sessions name theirs.
func projectPath(f registry.File, name, cwd string) (string, bool) {
	if cwd != "" {
		return worktree.Project(f, cwd)
	}
	for _, p := range f.Projects {
		if p == name || f.Name(p) == name {
			return p, true
		}
	}
	return "", false
}

// treeOf is the checkout holding cwd; "" when none does.
func (h *Host) treeOf(cwd string) string {
	l, _ := worktree.Locate(cwd)
	return l.Root
}

// head is cwd's HEAD commit, which a child's worktree starts from.
func (h *Host) head(cwd string) string {
	if cwd == "" {
		return ""
	}
	out, err := exec.Command("git", "-C", cwd, "rev-parse", "HEAD").Output()
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(out))
}

func (h *Host) TaskOf(session, toolUseID string) string {
	a, err := h.Registry.Get(session)
	if err != nil {
		return ""
	}
	return a.Timeline.TaskOf(toolUseID)
}

// Rehold gives each agent an upgrade handed down the background tasks it
// declared it waits on: the exec didn't stop the agent, so they run on. A
// cold start, whose agents ended with the pocketd before and run none of
// them, drops them instead.
func (h *Host) Rehold(handedDown bool) {
	if !handedDown {
		if err := h.engine.Store().Unhold(); err != nil {
			log.Printf("orch: held: %v", err)
		}
		return
	}
	tasks, until, err := h.engine.Store().Held()
	if err != nil {
		log.Printf("orch: held: %v", err)
		return
	}
	for session, ids := range tasks {
		left := time.Until(time.UnixMilli(until[session]))
		if a, err := h.Registry.Get(session); err == nil && left > 0 {
			a.Hold(ids, true, left)
		}
	}
}

func (h *Host) InBackground(session, id string) bool {
	a, err := h.Registry.Get(session)
	return err == nil && a.Timeline.InBackground(id)
}

// SessionIn is mcp.Host: the live session in terminal.
func (h *Host) SessionIn(terminal string) (proto.AgentSummary, bool) {
	for _, s := range h.Registry.List() {
		if s.TerminalID == terminal && s.Status != "closed" {
			return s, true
		}
	}
	return proto.AgentSummary{}, false
}

func (h *Host) Summary(id string) (proto.AgentSummary, bool) {
	if hosts.Tagged(id) && h.Hosts != nil {
		return h.Hosts.Summary(id)
	}
	a, err := h.Registry.Get(id)
	if err != nil {
		return proto.AgentSummary{}, false
	}
	return a.Summary(), true
}

func (h *Host) Items(id string) []proto.Item {
	if a, err := h.Registry.Get(id); err == nil {
		items, _ := a.Timeline.Page(0, 1<<20)
		return items
	}
	items, _ := h.engine.Store().Items(id)
	return items
}

func (h *Host) Agents() []mcp.AgentKind {
	var out []mcp.AgentKind
	for _, p := range h.Launcher.Providers() {
		spec := providers.Get(p.ID)
		info := AgentInfo(p.ID)
		out = append(out, mcp.AgentKind{ID: p.ID, Name: spec.Name, Installed: p.Available, Efforts: p.Efforts, Access: info.Access, NeverAsks: p.NeverAsks, Parent: Parent(p.ID)})
	}
	return out
}

// Models is mcp.Host: what an ACP agent's last session opened with, else what
// kind's CLI lists, asked once per run.
func (h *Host) Models(kind string) []string {
	if m := acp.Models(kind); m != nil {
		return m
	}
	list, _ := h.models.LoadOrStore(kind, sync.OnceValue(func() []string { return h.listModels(kind) }))
	return list.(func() []string)()
}

func (h *Host) listModels(kind string) []string {
	exe := h.Launcher.Paths()[kind]
	argv := catalog.Command(kind, exe)
	if exe == "" || argv == nil {
		return catalog.Backup(kind)
	}
	if !filepath.IsAbs(exe) {
		argv = launch.InShell(shellenv.LoginShell(), argv)
	}
	models, err := catalog.List(context.Background(), kind, argv, h.LoginEnv())
	if len(models) == 0 {
		log.Printf("models: %s listed none: %v", kind, err)
		return catalog.Backup(kind)
	}
	return models
}

func (h *Host) Dirty(cwd string) bool {
	if cwd == "" {
		return false
	}
	out, err := exec.Command("git", "-C", cwd, "status", "--porcelain", "--untracked-files=no").Output()
	return err == nil && len(strings.TrimSpace(string(out))) > 0
}

func (h *Host) OutsideGit(cwd string) bool { return outsideGit(h.Projects.Load(), cwd) }

func outsideGit(f registry.File, cwd string) bool {
	p, ok := worktree.Project(f, cwd)
	if !ok {
		return false
	}
	_, repo := worktree.Locate(p)
	return !repo
}

func (h *Host) Schedules() mcp.Schedules { return h.Sched }

func (h *Host) Environments() mcp.Environments { return h.Envs }

func (h *Host) token(terminal, provider string) (string, error) {
	return h.Tokens.Issue(terminal, provider)
}

// ClaudeFlags writes state/mcp/<terminal>.json (0600) with a fresh token and
// gives the flags that load it. The token stays out of argv, which ps shows
// and handoff.json keeps.
func (h *Host) ClaudeFlags(terminal, provider string) []string {
	if provider != "claude" || h.URL == "" {
		return nil
	}
	token, err := h.token(terminal, provider)
	if err != nil {
		log.Printf("mcp: token for %s: %v", terminal, err)
		return nil
	}
	dir := filepath.Join(h.StateDir, "mcp")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		log.Printf("mcp: %v", err)
		return nil
	}
	path := filepath.Join(dir, terminal+".json")
	body, _ := json.Marshal(map[string]any{"mcpServers": map[string]any{"anywhere": map[string]any{
		"type": "http", "url": h.URL, "headers": map[string]string{"Authorization": "Bearer " + token}}}})
	if err := writePrivate(path, body); err != nil {
		log.Printf("mcp: config for %s: %v", terminal, err)
		return nil
	}
	log.Printf("mcp: %s launches with MCP config", terminal)
	return []string{"--mcp-config=" + path, "--allowedTools=mcp__anywhere__*"}
}

func writePrivate(path string, body []byte) error {
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, body, 0o600); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

// CodexConfig is the thread config that gives a Codex chat both servers:
// anywhere without the blocking tools, and anywhere-direct with only them.
func (h *Host) CodexConfig(terminal string) map[string]any {
	if h.URL == "" {
		return nil
	}
	token, err := h.token(terminal, "codex")
	if err != nil {
		log.Printf("mcp: token for %s: %v", terminal, err)
		return nil
	}
	server := func(url string) map[string]any {
		return map[string]any{"url": url, "tool_timeout_sec": 60, "http_headers": map[string]string{"Authorization": "Bearer " + token}}
	}
	main := server(h.URL)
	main["disabled_tools"] = mcp.DirectOnly
	direct := server(h.URL + "?direct=1")
	direct["enabled_tools"] = mcp.DirectOnly
	direct["omit_tools_from"] = []string{"code_mode", "deferred"}
	return map[string]any{"mcp_servers.anywhere": main, "mcp_servers.anywhere-direct": direct}
}

// ACPServers is the mcpServers entry of an ACP agent that takes HTTP MCP.
func (h *Host) ACPServers(terminal string) []any {
	if h.URL == "" {
		return []any{}
	}
	token, err := h.token(terminal, "acp")
	if err != nil {
		return []any{}
	}
	return []any{map[string]any{"type": "http", "name": "anywhere", "url": h.URL, "headers": []map[string]string{{"name": "Authorization", "value": "Bearer " + token}}}}
}

// Closed revokes the tokens of a session that closed and removes its Claude config.
func (h *Host) Closed(session string) {
	a, err := h.Registry.Get(session)
	terminal := ""
	if err == nil {
		terminal = a.Summary().TerminalID
	} else if row, _ := h.engine.Store().Session(session); row != nil {
		terminal = row.Terminal
	}
	if terminal == "" {
		return
	}
	if err := h.Tokens.Revoke(terminal); err != nil {
		log.Printf("mcp: revoke %s: %v", terminal, err)
	}
	if err := os.Remove(filepath.Join(h.StateDir, "mcp", terminal+".json")); err != nil && !errors.Is(err, os.ErrNotExist) {
		log.Printf("mcp: %v", err)
	}
}
