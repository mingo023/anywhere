package e2e

import (
	"encoding/json"
	"path/filepath"
	"sync/atomic"
	"testing"

	"pocketd/internal/codex/codextest"
	"pocketd/internal/ops"
)

const codexHistory = `{"thread":{"name":"Codex task","turns":[{"id":"t1","status":"completed","durationMs":3,"items":[
 {"type":"userMessage","id":"u1","content":[{"type":"text","text":"first"}]},
 {"type":"agentMessage","id":"a1","text":"hi"}]}]}}`

func TestCodexThreadReachesPhone(t *testing.T) {
	h := Start(t)
	codexHome := filepath.Join(h.Home, "codex")
	h.Env = append(h.Env, "CODEX_HOME="+codexHome)
	var lists atomic.Int32
	srv := codextest.Start(t, filepath.Join(codexHome, "app-server-control", "app-server-control.sock"), func(method string, _ json.RawMessage) (any, string) {
		switch method {
		case "thread/loaded/list":
			if lists.Add(1) == 1 {
				return map[string]any{"data": []string{"old"}}, ""
			}
			return map[string]any{"data": []string{"old", "th9"}}, ""
		case "thread/resume":
			return json.RawMessage(codexHistory), ""
		}
		return map[string]any{}, ""
	})
	phone := h.Phone()
	id := h.Spawn("codex")
	h.WaitScreen(id, "fake codex ready --remote unix://")

	phone.WaitStream("th9", "user", "first")
	phone.WaitStream("th9", "assistant", "hi")
	phone.WaitFor("title", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.ID == "th9" && m.Agent.Title == "Codex task"
	})

	phone.Send(map[string]any{"type": "agent.prompt", "id": "p1", "agentId": "th9", "text": "again"})
	phone.WaitFor("ack", func(m Message) bool { return m.Type == "ack" && m.ID == "p1" })
	if got := string(srv.Next("turn/start").Params); got != `{"input":[{"text":"again","text_elements":[],"type":"text"}],"threadId":"th9"}` {
		t.Fatalf("turn/start: %s", got)
	}

	srv.Push("item/commandExecution/requestApproval", 5, `{"threadId":"th9","turnId":"t2","itemId":"c1","startedAtMs":1,"command":"ls"}`)
	req := phone.WaitFor("permission.request", func(m Message) bool { return m.Type == "permission.request" })
	if req.Request.AgentID != "th9" || req.Request.Detail.Command != "ls" {
		t.Fatalf("request: %s", req.Raw)
	}
	phone.Send(map[string]any{"type": "permission.resolve", "id": "r1", "requestId": req.Request.RequestID, "decision": "allow"})
	if got := srv.Reply("5"); got != `{"decision":"accept"}` {
		t.Fatalf("reply: %s", got)
	}

	srv.Push("turn/completed", nil, `{"threadId":"th9","turn":{"id":"t2","items":[],"status":"completed","durationMs":9}}`)
	phone.WaitFor("result", func(m Message) bool { return m.Type == "agent.stream" && m.Item.Kind == "result" && m.Item.OK })
	phone.WaitStatus("th9", "idle")

	h.Ops().Send(ops.Msg{Op: "close", ID: id})
	phone.WaitStatus("th9", "closed")
}
