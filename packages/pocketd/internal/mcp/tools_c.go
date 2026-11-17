package mcp

import (
	"context"
	"encoding/json"
	"time"

	"pocketd/internal/orch"
)

func groupC() []Tool {
	return []Tool{
		{Name: "anywhere_background_wait", Group: "C",
			Description: "Keep this session Working after its turn ends while background tasks you started run (their ids). They release on their end notification or after 6 hours. waiting false clears them. Claude and OpenCode only.",
			Schema:      `{"type":"object","properties":{"taskIds":{"type":"array","items":{"type":"string"}},"waiting":{"type":"boolean"}}}`,
			Run:         backgroundWait},
		{Name: "anywhere_workflow_stop", Group: "C",
			Description: "Stop a Claude background workflow by the taskId anywhere_thread_read's activity view gives its call. Claude only.",
			Schema:      `{"type":"object","required":["taskId"],"properties":{"threadId":{"type":"string"},"taskId":{"type":"string"}}}`,
			Run:         workflowStop},
		{Name: "anywhere_thread_organize", Group: "C",
			Description: "Pin, unpin, mark unread (back to Done) or archive (close; history is kept 15 days) a session.",
			Schema:      `{"type":"object","required":["action"],"properties":{"threadId":{"type":"string"},"action":{"type":"string","enum":["pin","unpin","mark_unread","archive","snooze","settle"]}}}`,
			Run:         threadOrganize},
	}
}

func backgroundWait(ctx context.Context, c *Call) (any, error) {
	var p struct {
		TaskIDs []string `json:"taskIds"`
		Waiting *bool    `json:"waiting"`
	}
	if err := c.args(&p); err != nil {
		return nil, err
	}
	waiting := p.Waiting == nil || *p.Waiting
	if waiting && len(p.TaskIDs) == 0 {
		return nil, fail(orch.CodeInvalid, "Name the background task ids to wait on, or set waiting false")
	}
	matched, unmatched := []string{}, []string{}
	for _, id := range p.TaskIDs {
		if c.Host.InBackground(c.Caller.ID, id) {
			matched = append(matched, id)
		} else {
			unmatched = append(unmatched, id)
		}
	}
	if waiting && len(matched) == 0 {
		return map[string]any{"matched": matched, "unmatched": unmatched}, nil
	}
	r, err := c.Host.Engine().DispatchWait(orch.NewCommand("", orch.CmdBackgroundHold, c.Caller.ID, orch.HoldPayload{TaskIDs: matched, Waiting: waiting}), 10*time.Second)
	if err != nil {
		return nil, err
	}
	var out map[string]any
	json.Unmarshal(r.Result, &out)
	out["unmatched"] = unmatched
	return out, nil
}

func workflowStop(ctx context.Context, c *Call) (any, error) {
	var p struct {
		ThreadID string `json:"threadId"`
		TaskID   string `json:"taskId"`
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
	if _, err := c.Host.Engine().DispatchWait(orch.NewCommand("", orch.CmdWorkflowStop, sum.ID, orch.StopTaskPayload{TaskID: p.TaskID}), 15*time.Second); err != nil {
		return nil, err
	}
	return map[string]any{"threadId": sum.ID, "link": Link(sum.ID), "taskId": p.TaskID, "status": "stop_requested"}, nil
}

func threadOrganize(ctx context.Context, c *Call) (any, error) {
	var p struct {
		ThreadID string `json:"threadId"`
		Action   string `json:"action"`
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
	if _, err := c.Host.Engine().DispatchWait(orch.NewCommand("", orch.CmdSessionOrganize, sum.ID, orch.OrganizePayload{Action: p.Action}), 10*time.Second); err != nil {
		return nil, err
	}
	return map[string]any{"threadId": sum.ID, "action": p.Action, "link": Link(sum.ID)}, nil
}
