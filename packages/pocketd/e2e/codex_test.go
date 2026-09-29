package e2e

import (
	"encoding/json"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/codex/codextest"
	"pocketd/internal/ops"
)

const codexHistory = `{"thread":{"name":"Codex task","turns":[{"id":"t1","status":"completed","durationMs":3,"items":[
 {"type":"userMessage","id":"u1","content":[{"type":"text","text":"first"}]},
 {"type":"agentMessage","id":"a1","text":"hi"}]}]}}`

func TestPocketSpawnedCodexThreadReachesPhone(t *testing.T) {
	h := Start(t)
	codexHome := filepath.Join(h.Home, "codex")
	h.Env = append(h.Env, "CODEX_HOME="+codexHome)
	srv := codextest.Start(t, filepath.Join(codexHome, "app-server-control", "app-server-control.sock"), func(method string, _ json.RawMessage) (any, string) {
		if method == "thread/resume" {
			return json.RawMessage(codexHistory), ""
		}
		return map[string]any{}, ""
	})
	phone := h.Phone()
	id := h.Spawn("codex")
	h.WaitScreen(id, "fake codex ready")
	if screen := h.Screen(id); strings.Contains(screen, "--remote") {
		t.Fatalf("codex spawned with --remote:\n%s", screen)
	}
	a := phone.WaitFor("the codex's agent", func(m Message) bool { return m.Type == "agent.update" && m.Agent.TerminalID == id }).Agent

	phone.Send(map[string]any{"type": "agent.prompt", "id": "p1", "agentId": a.ID, "text": "again"})
	phone.WaitFor("ack", func(m Message) bool { return m.Type == "ack" && m.ID == "p1" })
	h.WaitScreen(id, "typed: again")
	srv.Broadcast("thread/status/changed", `{"threadId":"th9","status":{"type":"active","activeFlags":[]}}`)
	phone.WaitAll("the thread", streamOf(a.ID, "user", "first"), streamOf(a.ID, "assistant", "hi"), func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.ID == a.ID && m.Agent.Title == "Codex task" && m.Agent.ProviderSessionID == "th9" && m.Agent.Status == "working"
	})

	srv.Push("item/commandExecution/requestApproval", 5, `{"threadId":"th9","turnId":"t2","itemId":"c1","startedAtMs":1,"command":"ls"}`)
	srv.Broadcast("thread/status/changed", `{"threadId":"th9","status":{"type":"active","activeFlags":["waitingOnApproval"]}}`)
	var req Message
	phone.WaitAll("the approval", func(m Message) bool {
		if m.Type == "permission.request" {
			req = m
		}
		return m.Type == "permission.request"
	}, statusOf(a.ID, "needsYou"))
	if req.Request.AgentID != a.ID || req.Request.Detail.Command != "ls" {
		t.Fatalf("request: %s", req.Raw)
	}
	phone.Send(map[string]any{"type": "permission.resolve", "id": "r1", "requestId": req.Request.RequestID, "decision": "allow"})
	if got := srv.Reply("5"); got != `{"decision":"accept"}` {
		t.Fatalf("reply: %s", got)
	}

	srv.Broadcast("thread/status/changed", `{"threadId":"th9","status":{"type":"idle"}}`)
	phone.WaitStatus(a.ID, "done")
	h.Ops().Send(ops.Msg{Op: "close", ID: id})
	phone.WaitStatus(a.ID, "closed")
}

func TestCodexTypedInAShellGoesWorkingThenDone(t *testing.T) {
	h := Start(t)
	codexHome := filepath.Join(h.Home, "codex")
	srv := codextest.Start(t, filepath.Join(codexHome, "app-server-control", "app-server-control.sock"), func(method string, _ json.RawMessage) (any, string) {
		if method == "thread/resume" {
			return json.RawMessage(codexHistory), ""
		}
		return map[string]any{}, ""
	})
	phone := h.Phone()
	c := h.Ops()
	c.Send(ops.Msg{Op: "spawn", Cmd: "sh", Cwd: h.Home, Env: []string{"PATH=" + filepath.Join(binDir, "fake") + ":/bin:/usr/bin", "PS1=ready$ ", "CODEX_HOME=" + codexHome}})
	m, err := c.Recv()
	if err != nil || m.Ev != "spawned" {
		t.Fatalf("spawn: %+v %v", m, err)
	}
	id := m.ID
	h.WaitScreen(id, "ready$")
	c.Send(ops.Msg{Op: "input", ID: id, Data: []byte("codex\r")})
	a := phone.WaitFor("the codex's agent", func(m Message) bool { return m.Type == "agent.update" && m.Agent.TerminalID == id }).Agent
	c.Send(ops.Msg{Op: "input", ID: id, Data: []byte("fix the tests")})
	c.Send(ops.Msg{Op: "input", ID: id, Data: []byte("\r")})
	h.WaitScreen(id, "typed: fix the tests")
	srv.Broadcast("thread/started", `{"thread":{"id":"th1","parentThreadId":null,"threadSource":"user","ephemeral":false}}`)
	srv.Broadcast("thread/status/changed", `{"threadId":"th1","status":{"type":"active","activeFlags":[]}}`)
	phone.WaitFor("working on th1", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.ID == a.ID && m.Agent.ProviderSessionID == "th1" && m.Agent.Status == "working"
	})
	srv.Broadcast("thread/status/changed", `{"threadId":"th1","status":{"type":"idle"}}`)
	phone.WaitStatus(a.ID, "done")
}
