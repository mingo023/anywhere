package mcp

import (
	"cmp"
	"context"
	"fmt"
	"slices"
	"strings"
	"time"

	"pocketd/internal/orch"
	"pocketd/internal/proto"
)

// contextBudget bounds the conversation a fork or merge_back carries.
const contextBudget = 24_000

// ContextBlock is a conversation as plain text for another agent: newest
// first until the budget, then put back in order, tool calls as one line.
func ContextBlock(title string, items []proto.Item, after int64) string {
	var lines []string
	used := 0
	for i := len(items) - 1; i >= 0; i-- {
		it := items[i]
		if it.ParentToolUseID != "" || it.Seq <= after {
			continue
		}
		var line string
		switch it.Kind {
		case "user":
			line = "User: " + clip(it.Text, 4000)
		case "assistant":
			line = "Assistant: " + clip(it.Text, 4000)
		case "tool":
			if it.Call != nil {
				d := it.Call.Detail
				line = "Tool " + it.Call.Name + ": " + clip(strings.TrimSpace(d.Command+" "+d.Path+" "+d.Query), 200)
			}
		}
		if line == "" {
			continue
		}
		if used+len(line) > contextBudget {
			lines = append(lines, "[earlier conversation left out]")
			break
		}
		used += len(line)
		lines = append(lines, line)
	}
	slices.Reverse(lines)
	return fmt.Sprintf("<context source=%q>\n%s\n</context>", title, strings.Join(lines, "\n\n"))
}

func groupB() []Tool {
	return []Tool{
		{Name: "anywhere_thread_transfer", Group: "B",
			Description: "Move conversation context between sessions. 'fork' copies a session into a new one, on any agent; 'merge_back' sends a summary of this session since the split into a related session; 'status' lists transfers. Never touches git.",
			Schema: `{"type":"object","required":["action"],"properties":{"action":{"type":"string","enum":["fork","merge_back","status"]},"threadId":{"type":"string"},"targetThreadId":{"type":"string"},"target":` + targetSchema +
				`,"isolation":{"type":"string","enum":["shared","worktree"]},"title":{"type":"string"},"prompt":{"type":"string"},"sourcePoint":{"type":"integer"}}}`,
			Run: threadTransfer},
		{Name: "anywhere_thread_update", Group: "B",
			Description: "Rename a session, or have Anywhere name it again from its first prompt and latest reply.",
			Schema:      `{"type":"object","properties":{"threadId":{"type":"string"},"title":{"type":"string"},"regenerateTitle":{"type":"boolean"},"clientRequestId":{"type":"string"}}}`,
			Run:         threadUpdate},
		{Name: "schedule_task", Group: "B",
			Description: "Create or update a schedule: an Automation that runs a prompt on a schedule or a local webhook. Bound to a session, it posts there. enabled false pauses it.",
			Schema: `{"type":"object","required":["prompt"],"properties":{"id":{"type":"string"},"name":{"type":"string"},"prompt":{"type":"string"},"agent":{"type":"string"},"schedule":{"type":"object","description":"{kind: hourly|daily|weekdays|weekly|interval, ...} as the Automations page uses"},` +
				`"bindToThisSession":{"type":"boolean"},"webhook":{"type":"boolean"},"webhookSecret":{"type":"string"},"enabled":{"type":"boolean"}}}`,
			Run: scheduleTask},
		{Name: "list_scheduled_tasks", Group: "B",
			Description: "List schedules (Automations), with who made each.",
			Schema:      `{"type":"object","properties":{"mine":{"type":"boolean"}}}`,
			Run:         listSchedules},
		{Name: "delete_scheduled_task", Group: "B",
			Description: "Delete a schedule.",
			Schema:      `{"type":"object","required":["id"],"properties":{"id":{"type":"string"}}}`,
			Run:         deleteSchedule},
		{Name: "run_scheduled_task_now", Group: "B",
			Description: "Run a schedule now; returns once its run has started.",
			Schema:      `{"type":"object","required":["id"],"properties":{"id":{"type":"string"}}}`,
			Run:         runScheduleNow},
	}
}

func threadTransfer(ctx context.Context, c *Call) (any, error) {
	var p struct {
		Action      string `json:"action"`
		ThreadID    string `json:"threadId"`
		TargetID    string `json:"targetThreadId"`
		Target      target `json:"target"`
		Isolation   string `json:"isolation"`
		Title       string `json:"title"`
		Prompt      string `json:"prompt"`
		SourcePoint int64  `json:"sourcePoint"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	src, _, err := session(c, p.ThreadID)
	if err != nil {
		return nil, err
	}
	e := c.Host.Engine()
	switch p.Action {
	case "status":
		ts, err := e.Store().Transfers(src.ID)
		if err != nil {
			return nil, err
		}
		if ts == nil {
			ts = []orch.Transfer{}
		}
		return map[string]any{"threadId": src.ID, "transfers": ts}, nil
	case "fork":
		if err := ceiling(c, src); err != nil {
			return nil, err
		}
		items := c.Host.Items(src.ID)
		if p.SourcePoint > 0 {
			items = slices.DeleteFunc(slices.Clone(items), func(it proto.Item) bool { return it.Seq > p.SourcePoint })
		}
		agent := cmp.Or(p.Target.Agent, src.Provider)
		native := agent == src.Provider && p.SourcePoint == 0 && (agent == "claude" || agent == "codex")
		r, err := e.Dispatch(orch.NewCommand("", orch.CmdTransferFork, src.ID, orch.TransferPayload{Agent: agent, Model: p.Target.Model, Isolation: p.Isolation, Title: p.Title,
			Prompt: p.Prompt, SourcePoint: p.SourcePoint, Context: ContextBlock(src.Title, items, 0), Native: native}))
		if err != nil {
			return nil, err
		}
		id, status := launched(c, r, time.Now().Add(MaxWait))
		out := map[string]any{"status": status, "sourceThreadId": src.ID, "native": native}
		if id != "" {
			out["threadId"], out["link"] = id, Link(id)
		}
		return out, nil
	case "merge_back":
		if p.TargetID == "" {
			return nil, fail(orch.CodeInvalid, "merge_back needs targetThreadId")
		}
		dst, row, err := session(c, p.TargetID)
		if err != nil {
			return nil, err
		}
		if err := ceiling(c, dst); err != nil {
			return nil, err
		}
		items := c.Host.Items(src.ID)
		if row != nil && row.ParentID == src.ID {
			// The source is the parent: only what it said after the target split off.
			items = slices.DeleteFunc(slices.Clone(items), func(it proto.Item) bool { return it.Ts < row.CreatedAt })
		}
		block := ContextBlock(src.Title, items, 0)
		text := "Context merged back from session " + src.ID + " (" + src.Title + "), since it split from this one:\n\n" + block
		r, err := e.Dispatch(orch.NewCommand("", orch.CmdTransferMerge, src.ID, orch.TransferPayload{TargetID: dst.ID, Context: text}))
		if err != nil {
			return nil, err
		}
		return map[string]any{"result": r.Result, "targetThreadId": dst.ID, "link": Link(dst.ID)}, nil
	}
	return nil, fail(orch.CodeInvalid, "action is fork, merge_back or status")
}

func threadUpdate(ctx context.Context, c *Call) (any, error) {
	var p struct {
		ThreadID        string `json:"threadId"`
		Title           string `json:"title"`
		Regenerate      bool   `json:"regenerateTitle"`
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
	e := c.Host.Engine()
	switch {
	case p.Regenerate:
		r, err := e.Dispatch(orch.NewCommand(commandID(c, "anywhere_thread_update", p.ClientRequestID), orch.CmdSessionRetitle, sum.ID, nil))
		if err != nil {
			return nil, err
		}
		e.Wait(r.Effects, 30*time.Second)
	case strings.TrimSpace(p.Title) != "":
		if _, err := e.DispatchWait(orch.NewCommand(commandID(c, "anywhere_thread_update", p.ClientRequestID), orch.CmdSessionRename, sum.ID, orch.TitlePayload{Title: p.Title}), 10*time.Second); err != nil {
			return nil, err
		}
	default:
		return nil, fail(orch.CodeInvalid, "Give title or regenerateTitle")
	}
	now, _ := c.Host.Summary(sum.ID)
	return map[string]any{"threadId": sum.ID, "title": now.Title, "link": Link(sum.ID)}, nil
}

func schedules(c *Call) (Schedules, error) {
	s := c.Host.Schedules()
	if s == nil {
		return nil, fail(orch.CodeUnavailable, "Automations are off")
	}
	return s, nil
}

type scheduleView struct {
	proto.Automation
	Webhook string `json:"webhookUrl,omitempty"`
}

func viewSchedule(a proto.Automation) scheduleView {
	v := scheduleView{Automation: a, Webhook: WebhookURL(a)}
	v.WebhookSecret = ""
	return v
}

// schedule is the Automation id names, if the caller may change it: one
// running with no broader access than the caller's own.
func schedule(c *Call, s Schedules, id string) (proto.Automation, error) {
	a, ok := s.Get(id)
	if !ok {
		return a, fail(orch.CodeNotFound, "Unknown schedule "+id)
	}
	if orch.Broader(a.Access, c.Caller.Access) {
		return a, fail(orch.CodeForbidden, "That schedule runs with broader access than this session")
	}
	return a, nil
}

func scheduleTask(ctx context.Context, c *Call) (any, error) {
	var p struct {
		ID            string          `json:"id"`
		Name          string          `json:"name"`
		Prompt        string          `json:"prompt"`
		Agent         string          `json:"agent"`
		Schedule      *proto.Schedule `json:"schedule"`
		Bind          bool            `json:"bindToThisSession"`
		Webhook       bool            `json:"webhook"`
		WebhookSecret string          `json:"webhookSecret"`
		Enabled       *bool           `json:"enabled"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	s, err := schedules(c)
	if err != nil {
		return nil, err
	}
	a := proto.Automation{Enabled: true, Access: c.Caller.Access, CreatedBy: c.Caller.ID}
	if p.ID != "" {
		if a, err = schedule(c, s, p.ID); err != nil {
			return nil, err
		}
	}
	a.Name = cmp.Or(p.Name, cmp.Or(a.Name, clip(firstLine(p.Prompt), 60)))
	a.Prompt = cmp.Or(p.Prompt, a.Prompt)
	a.Provider = cmp.Or(p.Agent, cmp.Or(a.Provider, c.Caller.Provider))
	a.Folder = cmp.Or(a.Folder, cmp.Or(c.Caller.Worktree, c.Caller.Cwd))
	kinds := c.Host.Agents()
	i := slices.IndexFunc(kinds, func(k AgentKind) bool { return k.ID == a.Provider })
	if i < 0 {
		return nil, fail(orch.CodeInvalid, "Unknown agent "+a.Provider)
	}
	if a.Access, _, err = orch.Access(a.Access, orch.AgentInfo{Known: true, Access: kinds[i].Access, NeverAsks: kinds[i].NeverAsks}); err != nil {
		return nil, err
	}
	if p.Schedule != nil {
		a.Schedule = *p.Schedule
	}
	if p.Bind {
		a.Bound = c.Caller.ID
	}
	if p.Webhook {
		a.Webhook = true
		a.WebhookSecret = p.WebhookSecret
	}
	if p.Enabled != nil {
		a.Enabled = *p.Enabled
	}
	saved, err := s.Save(a)
	if err != nil {
		return nil, err
	}
	return viewSchedule(saved), nil
}

// HookBase is the webhook address prefix; serve sets it once the listener is up.
var HookBase string

func WebhookURL(a proto.Automation) string {
	if !a.Webhook || HookBase == "" {
		return ""
	}
	return HookBase + "/hooks/" + a.ID
}

func firstLine(s string) string {
	line, _, _ := strings.Cut(strings.TrimSpace(s), "\n")
	return line
}

func listSchedules(ctx context.Context, c *Call) (any, error) {
	var p struct {
		Mine bool `json:"mine"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	s, err := schedules(c)
	if err != nil {
		return nil, err
	}
	out := []scheduleView{}
	for _, a := range s.List() {
		if p.Mine && a.CreatedBy != c.Caller.ID {
			continue
		}
		out = append(out, viewSchedule(a))
	}
	return map[string]any{"schedules": out}, nil
}

func deleteSchedule(ctx context.Context, c *Call) (any, error) {
	var p struct {
		ID string `json:"id"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	s, err := schedules(c)
	if err != nil {
		return nil, err
	}
	if _, err := schedule(c, s, p.ID); err != nil {
		return nil, err
	}
	if err := s.Delete(p.ID); err != nil {
		return nil, fail(orch.CodeNotFound, err.Error())
	}
	return map[string]any{"id": p.ID, "deleted": true}, nil
}

func runScheduleNow(ctx context.Context, c *Call) (any, error) {
	var p struct {
		ID string `json:"id"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	s, err := schedules(c)
	if err != nil {
		return nil, err
	}
	if _, err := schedule(c, s, p.ID); err != nil {
		return nil, err
	}
	run, err := s.RunNow(p.ID)
	if err != nil {
		return nil, fail(orch.CodeConflict, err.Error())
	}
	return run, nil
}
