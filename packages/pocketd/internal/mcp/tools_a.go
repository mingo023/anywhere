package mcp

import (
	"cmp"
	"context"
	"encoding/json"
	"fmt"
	"slices"
	"strconv"
	"strings"
	"sync"
	"time"
	"unicode/utf8"

	"pocketd/internal/orch"
	"pocketd/internal/proto"
)

// MaxWait bounds every wait a tool makes.
const MaxWait = 45 * time.Second

// waitPoll is how often a wait looks again; a var so tests needn't wait.
var waitPoll = 200 * time.Millisecond

// RunningNext tells a parent not to poll a task whose completion wakes it.
const RunningNext = "Completion wakes this session. Don't wait on it or poll it. Continue other work, and end your turn when none is left."

func allTools() []Tool {
	return slices.Concat(groupA(), groupB(), groupC())
}

func fail(code, message string) error { return &orch.Failure{Code: code, Message: message} }

// commandID scopes a client's request id to the calling session.
func commandID(c *Call, tool, clientID string) string {
	if clientID == "" {
		return ""
	}
	return "mcp:" + c.Caller.ID + ":" + tool + ":" + clientID
}

func waitFor(ctx context.Context, timeoutMs int64, done func() bool) bool {
	wait := MaxWait
	if timeoutMs > 0 {
		wait = min(wait, time.Duration(timeoutMs)*time.Millisecond)
	}
	deadline := time.Now().Add(wait)
	for {
		if done() {
			return true
		}
		if time.Now().After(deadline) {
			return false
		}
		select {
		case <-ctx.Done():
			return false
		case <-time.After(waitPoll):
		}
	}
}

// session is id's live summary, or its stored row once closed.
func session(c *Call, id string) (proto.AgentSummary, *orch.Session, error) {
	if id == "" {
		id = c.Caller.ID
	}
	sum, live := c.Host.Summary(id)
	row, _ := c.Host.Engine().Store().Session(id)
	if !live && row == nil {
		return sum, nil, fail(orch.CodeNotFound, "Unknown session "+id)
	}
	if !live {
		sum = proto.AgentSummary{ID: row.ID, Title: row.Title, Provider: row.Agent, Access: row.Access, Status: "closed", Cwd: row.Cwd, Project: row.Project, Branch: row.Branch, Model: row.Model}
	}
	return sum, row, nil
}

// ceiling refuses to act on a session that may do more than the caller.
func ceiling(c *Call, target proto.AgentSummary) error {
	if target.ID != c.Caller.ID && orch.Broader(target.Access, c.Caller.Access) {
		return fail(orch.CodeForbidden, fmt.Sprintf("That session runs with %s access, broader than this session's %s", target.Access, c.Caller.Access))
	}
	return nil
}

// descends says whether id is the caller or a session under it.
func descends(c *Call, id string) bool {
	store := c.Host.Engine().Store()
	for range 8 {
		if id == c.Caller.ID {
			return true
		}
		row, _ := store.Session(id)
		if row == nil || row.ParentID == "" {
			return false
		}
		id = row.ParentID
	}
	return false
}

type taskView struct {
	TaskID             string   `json:"taskId"`
	ChildThreadID      string   `json:"childThreadId"`
	Link               string   `json:"link,omitempty"`
	Title              string   `json:"title"`
	Agent              string   `json:"agent"`
	Model              string   `json:"model,omitempty"`
	Status             string   `json:"status"`
	WorkState          string   `json:"workState"`
	Summary            *string  `json:"summary"`
	Error              string   `json:"error,omitempty"`
	Access             string   `json:"access"`
	SteppedDown        string   `json:"accessSteppedDownTo,omitempty"`
	Isolation          string   `json:"isolation"`
	EnvironmentID      string   `json:"environmentId,omitempty"`
	HasPendingQuestion bool     `json:"hasPendingQuestion"`
	WaitTimedOut       bool     `json:"waitTimedOut"`
	LatestReply        string   `json:"latestReply,omitempty"`
	Next               string   `json:"next,omitempty"`
	Warnings           []string `json:"warnings,omitempty"`
}

func viewTask(t orch.Delegation) taskView {
	status := t.Status
	if status == orch.TaskWorking {
		status = "running"
	}
	work := "working"
	if t.Terminal() {
		work = "result_available"
	}
	v := taskView{TaskID: t.TaskID, ChildThreadID: t.ChildID, Title: t.Title, Agent: t.Agent, Model: t.Model, Status: status, WorkState: work, Error: t.Error,
		Access: t.Access, SteppedDown: t.SteppedDown, Isolation: t.Isolation, EnvironmentID: t.Environment, HasPendingQuestion: t.PendingQuestion != ""}
	if t.ChildID != "" {
		v.Link = Link(t.ChildID)
	}
	if t.Terminal() {
		s := t.Summary
		v.Summary = &s
	} else if t.Mode == "async" {
		v.Next = RunningNext
	}
	return v
}

func task(c *Call, id string) (orch.Delegation, error) {
	ts, err := c.Host.Engine().Store().Delegations(`task_id = ?`, id)
	if err != nil {
		return orch.Delegation{}, err
	}
	if len(ts) == 0 {
		return orch.Delegation{}, fail(orch.CodeNotFound, "Unknown task "+id)
	}
	return ts[0], nil
}

type target struct {
	Agent       string `json:"agent,omitempty"`
	Model       string `json:"model,omitempty"`
	Effort      string `json:"effort,omitempty"`
	Environment string `json:"environmentId,omitempty"`
	Project     string `json:"projectId,omitempty"`
}

const targetSchema = `{"type":"object","properties":{"agent":{"type":"string","description":"Agent id from orchestrator_capabilities. Default: your own agent."},"model":{"type":"string"},"effort":{"type":"string"},` +
	`"environmentId":{"type":"string","description":"A linked host's id, to run there. delegate_task and anywhere_thread_launch only."},"projectId":{"type":"string","description":"With environmentId: the host's project, by path there or by name. Default: the one named like yours."}}}`

// onHost is the path of the project on t's host a session runs in: t.Project, else the one named like the caller's.
func onHost(ctx context.Context, c *Call, t target) (string, error) {
	envs, err := environments(c)
	if err != nil {
		return "", err
	}
	list := envs.List()
	i := slices.IndexFunc(list, func(e Environment) bool { return e.ID == t.Environment })
	if i < 0 {
		return "", fail(orch.CodeNotFound, "No linked host "+t.Environment)
	}
	env := list[i]
	if env.State != "online" {
		return "", fail(orch.CodeUnavailable, fmt.Sprintf("%s isn't online (%s): %s", env.Label, env.State, cmp.Or(env.Error, "it's connecting")))
	}
	projects, err := envs.Projects(ctx, env.ID)
	if err != nil {
		return "", fail(orch.CodeUnavailable, err.Error())
	}
	want := cmp.Or(t.Project, c.Caller.Project)
	var names []string
	for _, p := range projects {
		if p.Path == want || p.Name == want {
			return p.Path, nil
		}
		names = append(names, fmt.Sprintf("%s (%s)", p.Name, p.Path))
	}
	return "", fail(orch.CodeInvalid, fmt.Sprintf("%s has no project %q; name one in projectId: %s", env.Label, want, strings.Join(names, ", ")))
}

func groupA() []Tool {
	return []Tool{
		{Name: "orchestrator_capabilities", Group: "A",
			Description: "What Anywhere can run: agents with their models and access, who can delegate, the limits, and this session. Call before delegate_task.",
			Schema:      `{"type":"object","properties":{"search":{"type":"string","description":"Filter models by this text."}}}`,
			Run:         capabilities},
		{Name: "delegate_task", Group: "A",
			Description: "Start a child session on any agent with a brief, in a new worktree by default. mode 'async' (default) wakes you with its report when it finishes; 'wait' returns the result if it lands within 45 s. The child runs with your access.",
			Schema: `{"type":"object","required":["task"],"properties":{"task":{"type":"string","description":"The brief: the child's only prompt."},"target":` + targetSchema + `,"title":{"type":"string"},"role":{"type":"string","enum":["implementation","research","review","design","test","general"]},` +
				`"isolation":{"type":"string","enum":["worktree","shared"],"description":"'worktree' (default): a new worktree from your HEAD commit; uncommitted edits stay out. 'shared': your checkout."},` +
				`"mode":{"type":"string","enum":["async","wait"]},"completionWake":{"type":"string","enum":["settled_only","always"],"description":"'always' also wakes you when the task is cancelled."},"timeoutMs":{"type":"number","description":"wait only; at most 45000."},"clientRequestId":{"type":"string"}}}`,
			Run: delegateTask},
		{Name: "task_status", Group: "A",
			Description: "Read delegated tasks: status, result summary when settled, the child's latest reply when it said more since, and whether one is blocked on a question. A message to a settled child opens a follow-up task, with its own id and result.",
			Schema:      `{"type":"object","properties":{"taskId":{"type":"string"},"taskIds":{"type":"array","items":{"type":"string"}}}}`,
			Run:         taskStatus},
		{Name: "task_cancel", Group: "A",
			Description: "Cancel a delegated task and every task under it, deepest first.",
			Schema:      `{"type":"object","required":["taskId"],"properties":{"taskId":{"type":"string"},"reason":{"type":"string"},"clientRequestId":{"type":"string"}}}`,
			Run:         taskCancel},
		{Name: "create_threads", Group: "A",
			Description: "Start 1 to 20 independent sessions in your checkout, each with its own prompt. They aren't your children and don't wake you.",
			Schema:      `{"type":"object","required":["threads"],"properties":{"threads":{"type":"array","minItems":1,"maxItems":20,"items":{"type":"object","properties":{"prompt":{"type":"string"},"title":{"type":"string"},"target":` + targetSchema + `}}},"clientRequestId":{"type":"string"}}}`,
			Run:         createThreads},
		{Name: "anywhere_thread_launch", Group: "A",
			Description: "Start one session in a project: in its root, a new worktree, or an existing worktree.",
			Schema:      `{"type":"object","properties":{"prompt":{"type":"string"},"title":{"type":"string"},"target":` + targetSchema + `,"checkout":{"type":"object","properties":{"strategy":{"type":"string","enum":["root","worktree","existing_worktree"]},"worktree":{"type":"string"},"project":{"type":"string"}}},"scratch":{"type":"boolean"},"clientRequestId":{"type":"string"}}}`,
			Run:         threadLaunch},
		{Name: "anywhere_thread_list", Group: "A",
			Description: "List sessions in your project (or all), newest first.",
			Schema:      `{"type":"object","properties":{"allProjects":{"type":"boolean"},"statuses":{"type":"array","items":{"type":"string"}},"titleContains":{"type":"string"},"includeSubagents":{"type":"boolean"},"includeClosed":{"type":"boolean"},"cursor":{"type":"integer"},"limit":{"type":"integer","maximum":100}}}`,
			Run:         threadList},
		{Name: "anywhere_thread_search", Group: "A",
			Description: "Search sessions' conversations, titles, branch names and pull requests. Delegated children are left out.",
			Schema:      `{"type":"object","required":["query"],"properties":{"query":{"type":"string"},"allProjects":{"type":"boolean"},"limit":{"type":"integer","maximum":50}}}`,
			Run:         threadSearch},
		{Name: "anywhere_thread_read", Group: "A",
			Description: "Read a session's conversation (view 'messages') or its tool activity (view 'activity'), about 30k characters at a time, by position.",
			Schema:      `{"type":"object","properties":{"threadId":{"type":"string"},"view":{"type":"string","enum":["messages","activity"]},"afterPosition":{"type":"integer"},"beforePosition":{"type":"integer"},"limit":{"type":"integer","maximum":100},"maxCharsPerItem":{"type":"integer","maximum":20000}}}`,
			Run:         threadRead},
		{Name: "anywhere_thread_send", Group: "A",
			Description: "Send a message to a session. mode 'auto' queues it while the session works; 'queue' always queues; 'steer' sends it now.",
			Schema:      `{"type":"object","required":["threadId","message"],"properties":{"threadId":{"type":"string"},"message":{"type":"string"},"mode":{"type":"string","enum":["auto","queue","steer"]},"clientRequestId":{"type":"string"}}}`,
			Run:         threadSend},
		{Name: "anywhere_thread_wait", Group: "A",
			Description: "Wait up to 45 s for a session's turn to end or to need someone. Returns waitTimedOut when it didn't.",
			Schema:      `{"type":"object","required":["threadId"],"properties":{"threadId":{"type":"string"},"timeoutMs":{"type":"number"}}}`,
			Run:         threadWait},
		{Name: "anywhere_thread_interrupt", Group: "A",
			Description: "Stop a session's current turn.",
			Schema:      `{"type":"object","required":["threadId"],"properties":{"threadId":{"type":"string"}}}`,
			Run:         threadInterrupt},
		{Name: "anywhere_thread_configure", Group: "A",
			Description: "Read or set a session's model, effort and model options. A session stays one agent: fork it to change agents.",
			Schema:      `{"type":"object","properties":{"threadId":{"type":"string"},"agent":{"type":"string"},"model":{"type":"string"},"effort":{"type":"string"},"options":{"type":"object","additionalProperties":{"type":"string"}}}}`,
			Run:         threadConfigure},
		{Name: "anywhere_pending_request", Group: "A",
			Description: "List the open questions of a session under you, or answer one. Permission asks are the user's.",
			Schema:      `{"type":"object","properties":{"threadId":{"type":"string"},"taskId":{"type":"string"},"requestId":{"type":"string"},"answers":{"type":"object","additionalProperties":{"type":"string"},"description":"Answer per question text."}}}`,
			Run:         pendingRequest},
		{Name: "anywhere_queue", Group: "A",
			Description: "List, edit, cancel, reorder or send now the messages queued for a session.",
			Schema:      `{"type":"object","properties":{"threadId":{"type":"string"},"action":{"type":"string","enum":["list","edit","cancel","reorder","send_now"]},"entryId":{"type":"string"},"text":{"type":"string"},"before":{"type":"string"}}}`,
			Run:         queueTool},
	}
}

func capabilities(ctx context.Context, c *Call) (any, error) {
	var p struct {
		Search string `json:"search"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	agents := c.Host.Agents()
	var wg sync.WaitGroup
	for i := range agents {
		wg.Go(func() {
			agents[i].Models = c.Host.Models(agents[i].ID)
			if len(agents[i].Efforts) == 0 {
				agents[i].Efforts = c.Host.Efforts(agents[i].ID)
			}
		})
	}
	wg.Wait()
	for i, a := range agents {
		if p.Search != "" {
			a.Models = slices.DeleteFunc(slices.Clone(a.Models), func(m string) bool { return !strings.Contains(strings.ToLower(m), strings.ToLower(p.Search)) })
		} else if len(a.Models) > 8 {
			a.Models = a.Models[:8]
		}
		agents[i] = a
	}
	row, _ := c.Host.Engine().Store().Session(c.Caller.ID)
	depth := 0
	if row != nil {
		depth = row.Depth
	}
	limits := orch.DefaultLimits
	if l := c.Host.Engine().Limits; l != nil {
		limits = l()
	}
	return map[string]any{
		"self":   map[string]any{"threadId": c.Caller.ID, "agent": c.Caller.Provider, "access": c.Caller.Access, "depth": depth, "link": Link(c.Caller.ID)},
		"agents": agents, "limits": limits,
		"notes": []string{"A child runs with your access; an agent without it steps down to a narrower one.", "Waits return within 45 s.", "tool_search loads the other tools."},
	}, nil
}

func delegateTask(ctx context.Context, c *Call) (any, error) {
	var p struct {
		Task            string `json:"task"`
		Target          target `json:"target"`
		Title           string `json:"title"`
		Role            string `json:"role"`
		Isolation       string `json:"isolation"`
		Mode            string `json:"mode"`
		CompletionWake  string `json:"completionWake"`
		TimeoutMs       int64  `json:"timeoutMs"`
		ClientRequestID string `json:"clientRequestId"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	if strings.TrimSpace(p.Task) == "" {
		return nil, fail(orch.CodeInvalid, "task is the child's brief and can't be empty")
	}
	if utf8.RuneCountInString(p.Task) > 100_000 {
		return nil, fail(orch.CodeInvalid, "task is longer than 100,000 characters")
	}
	agent := p.Target.Agent
	if agent == "" {
		agent = c.Caller.Provider
	}
	var hostProject string
	if p.Target.Environment != "" {
		var err error
		if hostProject, err = onHost(ctx, c, p.Target); err != nil {
			return nil, err
		}
	}
	e := c.Host.Engine()
	r, err := e.Dispatch(orch.NewCommand(commandID(c, "delegate_task", p.ClientRequestID), orch.CmdDelegate, c.Caller.ID, orch.DelegatePayload{
		Agent: agent, Model: p.Target.Model, Effort: p.Target.Effort, Title: p.Title, Brief: p.Task, Role: p.Role, Isolation: p.Isolation, Mode: p.Mode, CompletionWake: p.CompletionWake, ClientRequestID: p.ClientRequestID,
		Environment: p.Target.Environment, HostProject: hostProject, OutsideGit: c.Host.OutsideGit(c.Caller.Cwd)}))
	if err != nil {
		return nil, err
	}
	var t orch.Delegation
	json.Unmarshal(r.Result, &t)
	var warnings []string
	switch {
	case t.Isolation == "worktree" && t.Environment != "":
		warnings = append(warnings, t.Environment+"'s worktree starts from its own default branch; your local commits aren't there.")
	case t.Isolation == "worktree" && c.Host.Dirty(c.Caller.Cwd):
		warnings = append(warnings, "Your checkout has uncommitted changes; the child's worktree starts from your HEAD commit without them.")
	}
	if t.SteppedDown != "" {
		warnings = append(warnings, fmt.Sprintf("%s doesn't offer %s access, so the child runs with %s.", agent, c.Caller.Access, t.SteppedDown))
	}
	if t.Status == orch.TaskQueued {
		warnings = append(warnings, "The concurrency limit is reached; the task starts when a running one settles.")
	}
	if t.Mode != "wait" {
		v := viewTask(t)
		v.Warnings = warnings
		return v, nil
	}
	var latest orch.Delegation
	done := waitFor(ctx, p.TimeoutMs, func() bool {
		latest, _ = task(c, t.TaskID)
		return latest.Terminal() || latest.PendingQuestion != ""
	})
	v := viewTask(latest)
	v.Warnings = append(warnings, unreachable(c, latest)...)
	v.WaitTimedOut = !done
	if latest.Terminal() {
		e.Dispatch(orch.NewCommand("", orch.CmdDelegateWaited, c.Caller.ID, orch.TaskPayload{TaskID: t.TaskID}))
	} else {
		v.Next = RunningNext
	}
	return v, nil
}

// unreachable warns that a running task's host isn't online, which a task there waits through.
func unreachable(c *Call, t orch.Delegation) []string {
	envs := c.Host.Environments()
	if t.Environment == "" || t.Terminal() || envs == nil {
		return nil
	}
	for _, e := range envs.List() {
		if e.ID == t.Environment && e.State != "online" {
			return []string{"Host " + cmp.Or(e.Label, e.ID) + " is unreachable; the task keeps waiting."}
		}
	}
	return nil
}

func taskStatus(ctx context.Context, c *Call) (any, error) {
	var p struct {
		TaskID  string   `json:"taskId"`
		TaskIDs []string `json:"taskIds"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	ids := p.TaskIDs
	if p.TaskID != "" {
		ids = append(ids, p.TaskID)
	}
	if len(ids) == 0 {
		ts, err := c.Host.Engine().Store().Delegations(`parent_id = ? ORDER BY created_at DESC LIMIT 20`, c.Caller.ID)
		if err != nil {
			return nil, err
		}
		for _, t := range ts {
			ids = append(ids, t.TaskID)
		}
	}
	var out []taskView
	var read []string
	for _, id := range ids {
		t, err := task(c, id)
		if err != nil {
			return nil, err
		}
		if !descends(c, t.ParentID) {
			return nil, fail(orch.CodeForbidden, "Task "+id+" wasn't delegated from this session or one under it")
		}
		v := viewTask(t)
		v.Warnings = unreachable(c, t)
		if t.Terminal() && t.ChildID != "" {
			store := c.Host.Engine().Store()
			// A follow-up's words are its own task's; only the child's newest task reads them.
			newest, _ := store.Delegations(`child_id = ? ORDER BY created_at DESC, rowid DESC LIMIT 1`, t.ChildID)
			if reply := store.LatestReply(t.ChildID); len(newest) == 1 && newest[0].TaskID == t.TaskID && reply != t.Summary {
				v.LatestReply = reply
			}
		}
		if t.Terminal() && t.ParentID == c.Caller.ID && (t.Delivery == orch.DeliverPending || t.Delivery == orch.DeliverQueued || t.Delivery == orch.DeliverWaiting) {
			read = append(read, t.TaskID)
		}
		out = append(out, v)
	}
	// Only a call that returns the results has them read.
	for _, id := range read {
		c.Host.Engine().Dispatch(orch.NewCommand("", orch.CmdDelegateWaited, c.Caller.ID, orch.TaskPayload{TaskID: id}))
	}
	return map[string]any{"tasks": out}, nil
}

func taskCancel(ctx context.Context, c *Call) (any, error) {
	var p struct {
		TaskID          string `json:"taskId"`
		ClientRequestID string `json:"clientRequestId"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	t, err := task(c, p.TaskID)
	if err != nil {
		return nil, err
	}
	if !descends(c, t.ParentID) {
		return nil, fail(orch.CodeForbidden, "That task wasn't delegated from this session or one under it")
	}
	out := map[string]any{"taskId": t.TaskID, "status": t.Status}
	if t.ChildID != "" {
		out["threadId"], out["link"] = t.ChildID, Link(t.ChildID)
	}
	if t.Terminal() {
		return out, nil
	}
	if _, err := c.Host.Engine().Dispatch(orch.NewCommand(commandID(c, "task_cancel", p.ClientRequestID), orch.CmdDelegateCancel, t.ParentID, orch.TaskPayload{TaskID: t.TaskID})); err != nil {
		return nil, err
	}
	out["status"] = "cancel_requested"
	return out, nil
}

// launched waits for a session.create to start its session, within the call's budget.
func launched(c *Call, r orch.Receipt, deadline time.Time) (string, string) {
	if len(r.Effects) == 0 {
		return "", "starting"
	}
	e := c.Host.Engine()
	e.Wait(r.Effects, max(time.Until(deadline), 0))
	res, state, last := e.Result(r.Effects[0])
	switch state {
	case "done":
		id, _ := res["childId"].(string)
		return id, "started"
	case "failed", "retired":
		return "", "failed: " + last
	}
	return "", "starting"
}

func launchPayload(c *Call, prompt, title string, t target) (orch.LaunchPayload, error) {
	agent := t.Agent
	if agent == "" {
		agent = c.Caller.Provider
	}
	known := slices.ContainsFunc(c.Host.Agents(), func(a AgentKind) bool { return a.ID == agent })
	if !known {
		return orch.LaunchPayload{}, fail(orch.CodeInvalid, "Unknown agent "+agent)
	}
	return orch.LaunchPayload{Agent: agent, Model: t.Model, Effort: t.Effort, Access: c.Caller.Access, Prompt: prompt, Title: title, Project: c.Caller.Project,
		Cwd: c.Caller.Cwd, Checkout: "shared", CreatedBy: "agent", Chat: true}, nil
}

func createThreads(ctx context.Context, c *Call) (any, error) {
	var p struct {
		Threads []struct {
			Prompt string `json:"prompt"`
			Title  string `json:"title"`
			Target target `json:"target"`
		} `json:"threads"`
		ClientRequestID string `json:"clientRequestId"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	if len(p.Threads) < 1 || len(p.Threads) > 20 {
		return nil, fail(orch.CodeInvalid, "threads takes 1 to 20 sessions")
	}
	payloads := make([]orch.LaunchPayload, len(p.Threads))
	for i, t := range p.Threads {
		if t.Target.Environment != "" {
			return nil, fail(orch.CodeUnsupported, "create_threads runs on this Mac; use anywhere_thread_launch for a linked host")
		}
		lp, err := launchPayload(c, t.Prompt, t.Title, t.Target)
		if err != nil {
			return nil, err
		}
		payloads[i] = lp
	}
	deadline := time.Now().Add(MaxWait)
	receipts := make([]orch.Receipt, len(payloads))
	refused := make([]error, len(payloads))
	for i, lp := range payloads {
		id := commandID(c, "create_threads", p.ClientRequestID)
		if id != "" {
			id = fmt.Sprintf("%s:%d", id, i)
		}
		receipts[i], refused[i] = c.Host.Engine().Dispatch(orch.NewCommand(id, orch.CmdSessionCreate, c.Caller.ID, lp))
	}
	if !slices.Contains(refused, nil) {
		return nil, refused[0]
	}
	var out []map[string]any
	for i, r := range receipts {
		item := map[string]any{"title": p.Threads[i].Title}
		if refused[i] != nil {
			item["status"] = "failed: " + refused[i].Error()
			out = append(out, item)
			continue
		}
		id, status := launched(c, r, deadline)
		item["status"] = status
		if id != "" {
			item["threadId"], item["link"] = id, Link(id)
		}
		addAccess(item, r)
		out = append(out, item)
	}
	return map[string]any{"threads": out}, nil
}

func threadLaunch(ctx context.Context, c *Call) (any, error) {
	var p struct {
		Prompt   string `json:"prompt"`
		Title    string `json:"title"`
		Target   target `json:"target"`
		Checkout struct {
			Strategy string `json:"strategy"`
			Worktree string `json:"worktree"`
			Project  string `json:"project"`
		} `json:"checkout"`
		Scratch         bool   `json:"scratch"`
		ClientRequestID string `json:"clientRequestId"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	lp, err := launchPayload(c, p.Prompt, p.Title, p.Target)
	if err != nil {
		return nil, err
	}
	if p.Checkout.Project != "" {
		lp.Project, lp.Cwd = p.Checkout.Project, ""
	}
	switch p.Checkout.Strategy {
	case "", "root":
		lp.Checkout = "root"
	case "worktree":
		lp.Checkout = "worktree"
	case "existing_worktree":
		if p.Checkout.Worktree == "" {
			return nil, fail(orch.CodeInvalid, "existing_worktree needs checkout.worktree")
		}
		lp.Checkout, lp.Worktree = "existing_worktree", p.Checkout.Worktree
	default:
		return nil, fail(orch.CodeInvalid, "checkout.strategy is root, worktree or existing_worktree")
	}
	if p.Scratch {
		return nil, fail(orch.CodeUnsupported, "A scratch session starts only from the desktop for now")
	}
	if p.Target.Environment != "" {
		if lp.HostProject, err = onHost(ctx, c, p.Target); err != nil {
			return nil, err
		}
		lp.Env = p.Target.Environment
	} else if lp.Project == "" {
		return nil, fail(orch.CodeInvalid, "This session isn't in a project; name checkout.project")
	}
	r, err := c.Host.Engine().Dispatch(orch.NewCommand(commandID(c, "anywhere_thread_launch", p.ClientRequestID), orch.CmdSessionCreate, c.Caller.ID, lp))
	if err != nil {
		return nil, err
	}
	id, status := launched(c, r, time.Now().Add(MaxWait))
	out := map[string]any{"status": status}
	if id != "" {
		out["threadId"], out["link"] = id, Link(id)
	}
	addAccess(out, r)
	return out, nil
}

// addAccess says which access a launch runs with, when the agent lacked the caller's.
func addAccess(out map[string]any, r orch.Receipt) {
	var res struct{ Access, SteppedDown string }
	json.Unmarshal(r.Result, &res)
	if res.SteppedDown != "" {
		out["access"], out["steppedDown"] = res.Access, res.SteppedDown
	}
}

type threadItem struct {
	ThreadID     string `json:"threadId"`
	Title        string `json:"title"`
	Link         string `json:"link"`
	Agent        string `json:"agent"`
	Model        string `json:"model,omitempty"`
	Status       string `json:"status"`
	Access       string `json:"access,omitempty"`
	Project      string `json:"project,omitempty"`
	Branch       string `json:"branch,omitempty"`
	Worktree     string `json:"worktreePath,omitempty"`
	Pinned       bool   `json:"pinned,omitempty"`
	Parent       string `json:"parentThreadId,omitempty"`
	Relationship string `json:"relationshipToParent,omitempty"`
	CreatedBy    string `json:"createdBy,omitempty"`
	UpdatedAt    int64  `json:"updatedAt"`
}

func itemOf(s orch.Session) threadItem {
	status := s.Status
	if s.ClosedAt > 0 {
		status = "closed"
	}
	return threadItem{ThreadID: s.ID, Title: s.Title, Link: Link(s.ID), Agent: s.Agent, Model: s.Model, Status: status, Access: s.Access, Project: s.Project, Branch: s.Branch,
		Worktree: s.Worktree, Pinned: s.Pinned, Parent: s.ParentID, Relationship: s.Relationship, CreatedBy: s.CreatedBy, UpdatedAt: s.UpdatedAt}
}

func threadList(ctx context.Context, c *Call) (any, error) {
	var p struct {
		AllProjects      bool     `json:"allProjects"`
		Statuses         []string `json:"statuses"`
		TitleContains    string   `json:"titleContains"`
		IncludeSubagents bool     `json:"includeSubagents"`
		IncludeClosed    bool     `json:"includeClosed"`
		Cursor           int      `json:"cursor"`
		Limit            int      `json:"limit"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	if p.Cursor < 0 {
		return nil, fail(orch.CodeInvalid, "cursor can't be negative")
	}
	limit := min(max(p.Limit, 1), 100)
	if p.Limit == 0 {
		limit = 30
	}
	where, args := []string{"agent != ''"}, []any{}
	if !p.IncludeClosed && !slices.Contains(p.Statuses, "closed") {
		where = append(where, "closed_at = 0")
	}
	if !p.AllProjects {
		where, args = append(where, "project = ?"), append(args, c.Caller.Project)
	}
	if !p.IncludeSubagents {
		where = append(where, "relationship != 'subagent'")
	}
	if p.TitleContains != "" {
		where, args = append(where, "title LIKE ?"), append(args, "%"+p.TitleContains+"%")
	}
	rows, err := c.Host.Engine().Store().Sessions(strings.Join(where, " AND ")+" ORDER BY updated_at DESC", args...)
	if err != nil {
		return nil, err
	}
	var out []threadItem
	for _, s := range rows {
		it := itemOf(s)
		if len(p.Statuses) > 0 && !slices.Contains(p.Statuses, it.Status) {
			continue
		}
		out = append(out, it)
	}
	var next any
	start := min(p.Cursor, len(out))
	end := min(start+limit, len(out))
	if end < len(out) {
		next = end
	}
	return map[string]any{"currentThreadId": c.Caller.ID, "project": c.Caller.Project, "threads": out[start:end], "nextCursor": next}, nil
}

func threadSearch(ctx context.Context, c *Call) (any, error) {
	var p struct {
		Query       string `json:"query"`
		AllProjects bool   `json:"allProjects"`
		Limit       int    `json:"limit"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	if strings.TrimSpace(p.Query) == "" {
		return nil, fail(orch.CodeInvalid, "query can't be empty")
	}
	limit := 20
	if p.Limit > 0 {
		limit = min(p.Limit, 50)
	}
	where, args := "relationship != 'subagent'", []any{}
	if !p.AllProjects {
		where, args = where+" AND project = ?", append(args, c.Caller.Project)
	}
	hits, err := c.Host.Engine().Store().Search(p.Query, limit, where, args...)
	if err != nil {
		return nil, err
	}
	if len(hits) < limit {
		trees, err := c.Host.Engine().Store().Sessions(where+" AND worktree != '' ORDER BY updated_at DESC", args...)
		if err != nil {
			return nil, err
		}
		// gh is asked for the trees it never told of, as no client watches them, within prWait for the whole search.
		prCtx, cancel := context.WithTimeout(ctx, prWait)
		defer cancel()
		for _, s := range trees {
			if len(hits) >= limit {
				break
			}
			pr := c.Host.PR(prCtx, s.Worktree)
			if pr != nil && prMatches(pr, p.Query) && !slices.ContainsFunc(hits, func(h orch.Hit) bool { return h.Session.ID == s.ID }) {
				hits = append(hits, orch.Hit{Session: s, Snippet: fmt.Sprintf("PR #%d %s", pr.Number, pr.Title)})
			}
		}
	}
	var out []map[string]any
	for _, h := range hits {
		out = append(out, map[string]any{"thread": itemOf(h.Session), "snippet": h.Snippet, "position": h.Seq})
	}
	return map[string]any{"results": out}, nil
}

// prWait bounds how long one search waits on gh for pull requests it never told of.
const prWait = 5 * time.Second

// prMatches says whether query names pr: its number, with or without #, its URL, or words of its title.
func prMatches(pr *proto.PR, query string) bool {
	q := strings.TrimSpace(query)
	return strings.TrimPrefix(q, "#") == strconv.Itoa(pr.Number) || q == pr.URL || strings.Contains(strings.ToLower(pr.Title), strings.ToLower(q))
}

const readBudget = 30_000

func threadRead(ctx context.Context, c *Call) (any, error) {
	var p struct {
		ThreadID        string `json:"threadId"`
		View            string `json:"view"`
		AfterPosition   *int64 `json:"afterPosition"`
		BeforePosition  *int64 `json:"beforePosition"`
		Limit           int    `json:"limit"`
		MaxCharsPerItem int    `json:"maxCharsPerItem"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	sum, _, err := session(c, p.ThreadID)
	if err != nil {
		return nil, err
	}
	limit := 50
	if p.Limit > 0 {
		limit = min(p.Limit, 100)
	}
	per := 4000
	if p.MaxCharsPerItem > 0 {
		per = min(p.MaxCharsPerItem, 20_000)
	}
	activity := p.View == "activity"
	var picked []map[string]any
	items := c.Host.Items(sum.ID)
	if p.BeforePosition != nil {
		slices.Reverse(items)
	}
	used := 0
	for _, it := range items {
		if it.ParentToolUseID != "" {
			continue
		}
		if p.AfterPosition != nil && it.Seq <= *p.AfterPosition || p.BeforePosition != nil && it.Seq >= *p.BeforePosition {
			continue
		}
		var m map[string]any
		switch {
		case activity && it.Kind == "tool" && it.Call != nil:
			m = map[string]any{"position": it.Seq, "kind": "tool", "tool": it.Call.Name, "toolUseId": it.Call.ToolUseID, "status": it.Call.Status, "detail": it.Call.Detail}
			if id := c.Host.TaskOf(sum.ID, it.Call.ToolUseID); id != "" {
				m["taskId"] = id
			}
			if it.Call.Output != nil {
				m["output"] = clip(*it.Call.Output, per)
			}
		case !activity && (it.Kind == "user" || it.Kind == "assistant" || it.Kind == "plan"):
			m = map[string]any{"position": it.Seq, "role": it.Kind, "text": clip(it.Text, per)}
		case !activity && it.Kind == "result" && it.Error != "":
			m = map[string]any{"position": it.Seq, "role": "error", "text": it.Error}
		default:
			continue
		}
		b, _ := json.Marshal(m)
		if used+len(b) > readBudget && len(picked) > 0 || len(picked) == limit {
			break
		}
		used += len(b)
		picked = append(picked, m)
	}
	if p.BeforePosition != nil {
		slices.Reverse(picked)
	}
	return map[string]any{"thread": map[string]any{"threadId": sum.ID, "title": sum.Title, "status": sum.Status, "agent": sum.Provider, "link": Link(sum.ID)}, "view": cmp.Or(p.View, "messages"), "items": picked}, nil
}

func clip(s string, n int) string {
	if r := []rune(s); len(r) > n {
		return string(r[:n]) + "…"
	}
	return s
}

func threadSend(ctx context.Context, c *Call) (any, error) {
	var p struct {
		ThreadID        string `json:"threadId"`
		Message         string `json:"message"`
		Mode            string `json:"mode"`
		ClientRequestID string `json:"clientRequestId"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	sum, _, err := session(c, p.ThreadID)
	if err != nil {
		return nil, err
	}
	if err := ceiling(c, sum); err != nil {
		return nil, err
	}
	r, err := c.Host.Engine().Dispatch(orch.NewCommand(commandID(c, "anywhere_thread_send", p.ClientRequestID), orch.CmdMessageSend, sum.ID, orch.SendPayload{Text: p.Message, Mode: p.Mode, Origin: orch.OriginUser}))
	if err != nil {
		return nil, err
	}
	var res map[string]any
	json.Unmarshal(r.Result, &res)
	res["threadId"], res["link"] = sum.ID, Link(sum.ID)
	return res, nil
}

func threadWait(ctx context.Context, c *Call) (any, error) {
	var p struct {
		ThreadID  string `json:"threadId"`
		TimeoutMs int64  `json:"timeoutMs"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	sum, _, err := session(c, p.ThreadID)
	if err != nil {
		return nil, err
	}
	if sum.ID == c.Caller.ID {
		return nil, fail(orch.CodeInvalid, "A session can't wait on itself")
	}
	var now proto.AgentSummary
	done := waitFor(ctx, p.TimeoutMs, func() bool {
		var ok bool
		now, ok = c.Host.Summary(sum.ID)
		return !ok || now.Status != "working"
	})
	if now.ID == "" {
		now = sum
		now.Status = "closed"
	}
	out := map[string]any{"threadId": sum.ID, "status": now.Status, "waitTimedOut": !done, "link": Link(sum.ID)}
	if done {
		out["latestReply"] = c.Host.Engine().Store().LatestReply(sum.ID)
	}
	return out, nil
}

func threadInterrupt(ctx context.Context, c *Call) (any, error) {
	var p struct {
		ThreadID string `json:"threadId"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	sum, _, err := session(c, p.ThreadID)
	if err != nil {
		return nil, err
	}
	if err := ceiling(c, sum); err != nil {
		return nil, err
	}
	if _, err := c.Host.Engine().DispatchWait(orch.Command{Kind: orch.CmdTurnInterrupt, Session: sum.ID}, 10*time.Second); err != nil {
		return nil, err
	}
	return map[string]any{"threadId": sum.ID, "link": Link(sum.ID), "status": "interrupt_requested"}, nil
}

func threadConfigure(ctx context.Context, c *Call) (any, error) {
	var p struct {
		ThreadID string            `json:"threadId"`
		Agent    string            `json:"agent"`
		Model    string            `json:"model"`
		Effort   string            `json:"effort"`
		Options  map[string]string `json:"options"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	sum, _, err := session(c, p.ThreadID)
	if err != nil {
		return nil, err
	}
	current := map[string]any{"threadId": sum.ID, "link": Link(sum.ID), "agent": sum.Provider, "model": sum.Model, "effort": sum.Effort, "options": sum.Options, "access": sum.Access}
	if p.Agent == "" && p.Model == "" && p.Effort == "" && len(p.Options) == 0 {
		return current, nil
	}
	if err := ceiling(c, sum); err != nil {
		return nil, err
	}
	if _, err := c.Host.Engine().DispatchWait(orch.NewCommand("", orch.CmdSessionConfig, sum.ID, orch.ConfigPayload{Agent: p.Agent, Model: p.Model, Effort: p.Effort, Options: p.Options}), 15*time.Second); err != nil {
		return nil, err
	}
	now, _ := c.Host.Summary(sum.ID)
	return map[string]any{"threadId": sum.ID, "link": Link(sum.ID), "agent": now.Provider, "model": now.Model, "effort": now.Effort, "options": now.Options}, nil
}

func pendingRequest(ctx context.Context, c *Call) (any, error) {
	var p struct {
		ThreadID  string            `json:"threadId"`
		TaskID    string            `json:"taskId"`
		RequestID string            `json:"requestId"`
		Answers   map[string]string `json:"answers"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	id := p.ThreadID
	if p.TaskID != "" {
		t, err := task(c, p.TaskID)
		if err != nil {
			return nil, err
		}
		id = t.ChildID
	}
	if id == "" {
		return nil, fail(orch.CodeInvalid, "Name threadId or taskId")
	}
	if id == c.Caller.ID || !descends(c, id) {
		return nil, fail(orch.CodeForbidden, "Only questions of sessions under this one can be read or answered here")
	}
	store := c.Host.Engine().Store()
	if p.RequestID == "" {
		open, err := store.Requests(id, "open")
		if err != nil {
			return nil, err
		}
		var qs []map[string]any
		for _, r := range open {
			if r.Kind == "question" {
				qs = append(qs, map[string]any{"requestId": r.ID, "question": json.RawMessage(r.Payload), "createdAt": r.CreatedAt})
			}
		}
		return map[string]any{"threadId": id, "link": Link(id), "questions": qs}, nil
	}
	if len(p.Answers) == 0 {
		return nil, fail(orch.CodeInvalid, "answers can't be empty")
	}
	if _, err := c.Host.Engine().DispatchWait(orch.NewCommand("", orch.CmdRequestAnswer, id, orch.RequestPayload{RequestID: p.RequestID, Decision: "allow", Answers: p.Answers}), 10*time.Second); err != nil {
		return nil, err
	}
	return map[string]any{"threadId": id, "link": Link(id), "requestId": p.RequestID, "status": "answered"}, nil
}

func queueTool(ctx context.Context, c *Call) (any, error) {
	var p struct {
		ThreadID string `json:"threadId"`
		Action   string `json:"action"`
		EntryID  string `json:"entryId"`
		Text     string `json:"text"`
		Before   string `json:"before"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	sum, _, err := session(c, p.ThreadID)
	if err != nil {
		return nil, err
	}
	kind := map[string]string{"edit": orch.CmdQueueEdit, "cancel": orch.CmdQueueCancel, "reorder": orch.CmdQueueReorder, "send_now": orch.CmdQueueSendNow}[p.Action]
	if p.Action != "" && p.Action != "list" {
		if kind == "" {
			return nil, fail(orch.CodeInvalid, "action is list, edit, cancel, reorder or send_now")
		}
		if err := ceiling(c, sum); err != nil {
			return nil, err
		}
		if _, err := c.Host.Engine().Dispatch(orch.NewCommand("", kind, sum.ID, orch.EntryPayload{EntryID: p.EntryID, Text: p.Text, Before: p.Before})); err != nil {
			return nil, err
		}
	}
	entries, err := c.Host.Engine().Store().Queue(sum.ID, true)
	if err != nil {
		return nil, err
	}
	if entries == nil {
		entries = []orch.QueueEntry{}
	}
	return map[string]any{"threadId": sum.ID, "link": Link(sum.ID), "entries": entries}, nil
}
