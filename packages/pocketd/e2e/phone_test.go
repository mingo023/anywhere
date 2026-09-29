package e2e

import (
	"context"
	"encoding/json"
	"fmt"
	"slices"
	"testing"
	"time"

	"github.com/coder/websocket"
)

// Phone speaks the phone protocol to pocketd the way packages/app does.
type Phone struct {
	t  *testing.T
	ws *websocket.Conn
}

type Message struct {
	Type    string `json:"type"`
	ID      string `json:"id"`
	Message string `json:"message"`
	AgentID string `json:"agentId"`
	Agent   struct {
		ID                string `json:"id"`
		TerminalID        string `json:"terminalId"`
		ProviderSessionID string `json:"providerSessionId"`
		Status            string `json:"status"`
		Title             string `json:"title"`
		Attached          bool   `json:"attached"`
	} `json:"agent"`
	Agents []struct {
		ID string `json:"id"`
	} `json:"agents"`
	Item struct {
		Kind string `json:"kind"`
		Text string `json:"text"`
		OK   bool   `json:"ok"`
		Call struct {
			Status string `json:"status"`
			Output string `json:"output"`
		} `json:"call"`
	} `json:"item"`
	Request struct {
		RequestID string `json:"requestId"`
		AgentID   string `json:"agentId"`
		ToolName  string `json:"toolName"`
		Detail    struct {
			Command string `json:"command"`
		} `json:"detail"`
	} `json:"request"`
	RequestID string `json:"requestId"`
	Decision  string `json:"decision"`
	Raw       string `json:"-"`
}

func (h *Harness) Phone() *Phone {
	h.t.Helper()
	var ws *websocket.Conn
	h.eventually("phone port", func() bool {
		var err error
		ws, _, err = websocket.Dial(context.Background(), fmt.Sprintf("ws://127.0.0.1:%d", h.Port), nil)
		return err == nil
	})
	h.t.Cleanup(func() { ws.CloseNow() })
	p := &Phone{t: h.t, ws: ws}
	p.Send(map[string]any{"type": "hello", "id": "hello", "token": h.Token, "clientId": "e2e", "protocolVersion": 3})
	p.WaitFor("hello.ok", func(m Message) bool { return m.Type == "hello.ok" })
	return p
}

func (p *Phone) Send(msg map[string]any) {
	p.t.Helper()
	raw, _ := json.Marshal(msg)
	if err := p.ws.Write(context.Background(), websocket.MessageText, raw); err != nil {
		p.t.Fatal(err)
	}
}

// WaitFor reads messages until ok matches one, failing after 10s.
func (p *Phone) WaitFor(what string, ok func(Message) bool) Message {
	p.t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	for {
		_, raw, err := p.ws.Read(ctx)
		if err != nil {
			p.t.Fatalf("waiting for %s: %v", what, err)
		}
		var m Message
		json.Unmarshal(raw, &m)
		m.Raw = string(raw)
		if ok(m) {
			return m
		}
	}
}

// WaitAll reads messages until each of oks has matched one, in any order.
func (p *Phone) WaitAll(what string, oks ...func(Message) bool) {
	p.t.Helper()
	p.WaitFor(what, func(m Message) bool {
		oks = slices.DeleteFunc(oks, func(ok func(Message) bool) bool { return ok(m) })
		return len(oks) == 0
	})
}

func streamOf(agentID, kind, text string) func(Message) bool {
	return func(m Message) bool {
		return m.Type == "agent.stream" && m.AgentID == agentID && m.Item.Kind == kind && m.Item.Text == text
	}
}

func statusOf(agentID, status string) func(Message) bool {
	return func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.ID == agentID && m.Agent.Status == status
	}
}

func (p *Phone) WaitStream(agentID, kind, text string) Message {
	p.t.Helper()
	return p.WaitFor(fmt.Sprintf("%s %q", kind, text), streamOf(agentID, kind, text))
}

func (p *Phone) WaitStatus(agentID, status string) {
	p.t.Helper()
	p.WaitFor("status "+status, statusOf(agentID, status))
}
