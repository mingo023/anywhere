package wsserver

import (
	"context"
	"reflect"
	"strings"
	"testing"
	"time"

	"pocketd/internal/broker"
	"pocketd/internal/orch"
	"pocketd/internal/orchhost"
	"pocketd/internal/proto"
)

func TestAgentListGivesTheStoresOpenSessionsAsTheRegistryHasThemLive(t *testing.T) {
	stored := []orch.Session{
		{ID: "a0", Agent: "codex", Title: "Kept", Status: "done", CreatedAt: 1},
		{ID: "a1", Agent: "claude", Title: "Stale", Status: "working", CreatedAt: 2},
	}
	reg, _, p := setup(t, func(s *Server) {
		s.OpenSessions = func() ([]orch.Session, error) { return stored, nil }
	})
	a1, _ := reg.Get("a1")
	a1.SetTitle("Live")
	p.send(`{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3}`)
	p.recv()
	m := p.recv()
	if m["type"] != "agent.list" {
		t.Fatalf("%v", m)
	}
	var got []string
	for _, a := range m["agents"].([]any) {
		a := a.(map[string]any)
		got = append(got, a["id"].(string)+" "+a["title"].(string)+" "+a["status"].(string))
	}
	if want := []string{"a0 Kept done", "a1 Live idle"}; len(got) != 2 || got[0] != want[0] || got[1] != want[1] {
		t.Fatalf("listed %q, want %q", got, want)
	}
}

// withEngine runs the verbs the orchestrator owns through its engine, over
// the server's registry and broker, as serve wires them.
func withEngine(t *testing.T) func(*Server) {
	return func(s *Server) {
		store, err := orch.Open(t.TempDir())
		if err != nil {
			t.Fatal(err)
		}
		host := &orchhost.Host{Registry: s.Agents, Broker: s.Broker}
		e := orch.NewEngine(store, host)
		host.SetEngine(e)
		e.Live = func(id string) (orch.Session, bool) {
			a, err := s.Agents.Get(id)
			if err != nil {
				return orch.Session{}, false
			}
			sum := a.Summary()
			return orch.Session{ID: sum.ID, Agent: sum.Provider, Access: sum.Access, Status: sum.Status, Terminal: sum.TerminalID, CreatedAt: sum.CreatedAt}, true
		}
		s.Broker.OnAsk = func(req proto.PermissionRequest) { e.AskOpened(req.AgentID, req.RequestID, "permission", nil) }
		s.Broker.OnClose = func(req proto.PermissionRequest) { e.AskClosed(req.AgentID, req.RequestID) }
		t.Cleanup(func() {
			e.Close()
			store.Close()
		})
		s.Orch = e
	}
}

// eachWay runs f on a server that drives agents directly and on one that
// runs the orchestrator's engine: a verb answers the same either way.
func eachWay(t *testing.T, f func(t *testing.T, opts ...func(*Server))) {
	t.Run("direct", func(t *testing.T) { f(t) })
	t.Run("engine", func(t *testing.T) { f(t, withEngine(t)) })
}

// ranAsCommand fails t unless a server with an engine ran an effect of kind.
func ranAsCommand(t *testing.T, s *Server, kind string) {
	t.Helper()
	if s.Orch == nil {
		return
	}
	var n int
	s.Orch.Store().DB().QueryRow(`SELECT COUNT(*) FROM outbox WHERE kind = ?`, kind).Scan(&n)
	if n == 0 {
		t.Fatalf("no %s effect: the verb didn't go through the engine", kind)
	}
}

func TestAgentCloseClosesTheAgentEachTimeAndRefusesAnUnknownOne(t *testing.T) {
	eachWay(t, func(t *testing.T, opts ...func(*Server)) {
		_, d, p := setup(t, opts...)
		p.hello()
		for range 2 {
			p.send(`{"type":"agent.close","id":"c","agentId":"a1"}`)
			if m := p.recv(); m["type"] != "ack" || m["id"] != "c" {
				t.Fatalf("%v", m)
			}
		}
		if d.closes != 2 {
			t.Fatalf("closed %d times", d.closes)
		}
		p.send(`{"type":"agent.close","id":"z","agentId":"zz"}`)
		if m := p.recv(); m["message"] != "Unknown agent: zz" || m["id"] != "z" {
			t.Fatalf("%v", m)
		}
	})
}

func TestAgentPinPinsAndUnpinsAndRefusesAnUnknownAgent(t *testing.T) {
	eachWay(t, func(t *testing.T, opts ...func(*Server)) {
		reg, _, p := setup(t, opts...)
		p.hello()
		a, _ := reg.Get("a1")
		for _, pinned := range []bool{true, true, false} {
			if pinned {
				p.send(`{"type":"agent.pin","id":"p","agentId":"a1","pinned":true}`)
			} else {
				p.send(`{"type":"agent.pin","id":"p","agentId":"a1","pinned":false}`)
			}
			for m := p.recv(); m["type"] != "ack"; m = p.recv() {
				if m["type"] != "agent.update" {
					t.Fatalf("%v", m)
				}
			}
			if a.Summary().Pinned != pinned {
				t.Fatalf("pinned %t, want %t", a.Summary().Pinned, pinned)
			}
		}
		p.send(`{"type":"agent.pin","id":"z","agentId":"zz","pinned":true}`)
		m := p.recv()
		for m["type"] == "agent.update" {
			m = p.recv()
		}
		if m["message"] != "Unknown agent: zz" || m["id"] != "z" {
			t.Fatalf("%v", m)
		}
	})
}

func TestPermissionResolveAnswersTheAskOnceAndRefusesItAfter(t *testing.T) {
	eachWay(t, func(t *testing.T, opts ...func(*Server)) {
		var b *broker.Broker
		_, _, p := setup(t, append(opts, func(s *Server) { b = s.Broker })...)
		p.hello()
		ctx, cancel := context.WithCancel(context.Background())
		t.Cleanup(cancel)
		answered := make(chan broker.Answer, 1)
		go func() {
			answered <- b.Ask(ctx, proto.PermissionRequest{AgentID: "a1", ToolName: "Bash", Detail: proto.ToolDetail{Kind: "shell", Command: "ls"}}, "k")
		}()
		var id string
		for m := p.recv(); ; m = p.recv() {
			if req, ok := m["request"].(map[string]any); ok && m["type"] == "permission.request" {
				id = req["requestId"].(string)
				break
			}
		}
		resolve := `{"type":"permission.resolve","id":"r","requestId":"` + id + `","decision":"allow"}`
		p.send(resolve)
		for m := p.recv(); m["type"] != "ack"; m = p.recv() {
			if m["type"] == "error" {
				t.Fatalf("%v", m)
			}
		}
		select {
		case a := <-answered:
			if a.Decision != "allow" {
				t.Fatalf("answered %+v", a)
			}
		case <-time.After(5 * time.Second):
			t.Fatal("the ask was never answered")
		}
		p.send(resolve)
		for m := p.recv(); ; m = p.recv() {
			if m["type"] == "ack" {
				t.Fatalf("answered twice: %v", m)
			}
			if m["type"] == "error" {
				if m["message"] != "Permission request is no longer open" || m["id"] != "r" {
					t.Fatalf("%v", m)
				}
				break
			}
		}
	})
}

type nowDriver struct {
	fakeDriver
	sends int
}

func (d *nowDriver) SendNow() error { d.sends++; return nil }

func TestAgentSendNowHasTheAgentSendWhatItHoldsOrSaysItCant(t *testing.T) {
	eachWay(t, func(t *testing.T, opts ...func(*Server)) {
		var s *Server
		reg, _, p := setup(t, append(opts, func(srv *Server) { s = srv })...)
		p.hello()
		d := &nowDriver{}
		reg.Add("a2", "/w", "claude", d)
		p.send(`{"type":"agent.send_now","id":"n","agentId":"a2"}`)
		m := p.recv()
		for m["type"] == "agent.update" {
			m = p.recv()
		}
		if m["type"] != "ack" || m["id"] != "n" {
			t.Fatalf("%v", m)
		}
		if d.sends != 1 {
			t.Fatalf("sent now %d times", d.sends)
		}
		ranAsCommand(t, s, orch.EffSendNow)
		p.send(`{"type":"agent.send_now","id":"x","agentId":"a1"}`)
		if m := p.recv(); m["message"] != "This agent takes a message only after its current step" || m["id"] != "x" {
			t.Fatalf("%v", m)
		}
	})
}

func TestAgentSettingsSwitchesTheModelAndAccessAndRefusesWhatTheAgentCant(t *testing.T) {
	eachWay(t, func(t *testing.T, opts ...func(*Server)) {
		var s *Server
		_, d, p := setup(t, append(opts, func(srv *Server) { s = srv })...)
		p.hello()
		p.send(`{"type":"agent.settings","id":"s","agentId":"a1","model":"opus","effort":"high","access":"full"}`)
		if m := p.recv(); m["type"] != "ack" || m["id"] != "s" {
			t.Fatalf("%v", m)
		}
		if !reflect.DeepEqual(d.settings, [][2]string{{"opus", "high"}}) || !reflect.DeepEqual(d.accesses, []string{"full"}) {
			t.Fatalf("settings %v, accesses %v", d.settings, d.accesses)
		}
		ranAsCommand(t, s, orch.EffSettings)
		p.send(`{"type":"agent.settings","id":"o","agentId":"a1","options":{"fast":"true"},"access":"ask"}`)
		if m := p.recv(); m["message"] != "This agent takes its model's settings only as it starts" || m["id"] != "o" {
			t.Fatalf("%v", m)
		}
		p.send(`{"type":"agent.settings","id":"b","agentId":"a1","model":"-bad"}`)
		if m := p.recv(); m["id"] != "b" || !strings.HasPrefix(m["message"].(string), "Model names use") {
			t.Fatalf("%v", m)
		}
		if len(d.accesses) != 1 || len(d.settings) != 1 {
			t.Fatalf("a refused switch reached the driver: settings %v, accesses %v", d.settings, d.accesses)
		}
	})
}
