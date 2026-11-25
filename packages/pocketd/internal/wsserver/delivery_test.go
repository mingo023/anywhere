package wsserver

import (
	"errors"
	"strings"
	"testing"
	"time"

	"pocketd/internal/orch"
	"pocketd/internal/proto"
)

const prompt = `{"type":"agent.prompt","id":"p1","agentId":"a2","text":"SECRET-WORDS"}`

func TestADeliveryPromptIsAckedBeforeTheAgentTakesItAndThenStarted(t *testing.T) {
	gate, opt := withGate()
	p, _ := onSocket(t, withEngine(t), opt)
	p.helloAs("c", proto.CapDelivery)
	in := p.feed()
	p.send(prompt)
	if m := in.get(); m["type"] != "ack" || m["id"] != "p1" {
		t.Fatalf("%v", m)
	}
	in.quiet(200 * time.Millisecond)
	close(gate.gate)
	if m := in.get(); m["type"] != "agent.delivery" || m["id"] != "p1" || m["agentId"] != "a2" || m["state"] != "started" {
		t.Fatalf("%v", m)
	}
}

func TestADeliveryPromptTheAgentRefusesIsReportedFailedWithItsReason(t *testing.T) {
	gate, opt := withGate()
	gate.err = errors.New("The agent is gone")
	close(gate.gate)
	p, _ := onSocket(t, withEngine(t), opt)
	p.helloAs("c", proto.CapDelivery)
	p.send(prompt)
	if m := p.msg(); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	if m := p.msg(); m["type"] != "agent.delivery" || m["state"] != "failed" || m["error"] != "The agent is gone" {
		t.Fatalf("%v", m)
	}
}

func TestADeliveryPromptRefusedByTheEngineGetsTheUsualErrorAndNoAck(t *testing.T) {
	_, opt := withGate()
	p, _ := onSocket(t, withEngine(t), opt)
	p.helloAs("c", proto.CapDelivery)
	p.send(`{"type":"agent.prompt","id":"p1","agentId":"a2","text":"  "}`)
	if m := p.msg(); m["type"] != "error" || m["id"] != "p1" || m["message"] != "A message can't be empty" {
		t.Fatalf("%v", m)
	}
	p.feed().quiet(100 * time.Millisecond)
}

func TestADeliveryPromptToABusySessionIsReportedQueued(t *testing.T) {
	gate, opt := withGate()
	close(gate.gate)
	p, _ := onSocket(t, withEngine(t), opt)
	p.helloAs("c", proto.CapDelivery)
	p.send(`{"type":"agent.prompt","id":"p0","agentId":"a2","text":"first"}`)
	if m := p.msg(); m["type"] != "ack" {
		t.Fatalf("%v", m)
	}
	if m := p.msg(); m["state"] != "started" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"agent.prompt","id":"p1","agentId":"a2","text":"second","queue":true}`)
	if m := p.msg(); m["type"] != "ack" || m["id"] != "p1" {
		t.Fatalf("%v", m)
	}
	if m := p.msg(); m["type"] != "agent.delivery" || m["id"] != "p1" || m["state"] != "queued" {
		t.Fatalf("%v", m)
	}
}

func TestAPromptSentAgainUnderTheSameIdAfterAReconnectReachesTheAgentOnce(t *testing.T) {
	gate, opt := withGate()
	close(gate.gate)
	p, redial := onSocket(t, withEngine(t), opt)
	p.helloAs("c", proto.CapDelivery)
	p.send(prompt)
	for _, want := range []string{"ack", "agent.delivery"} {
		if m := p.msg(); m["type"] != want {
			t.Fatalf("%v", m)
		}
	}
	redial()
	p.helloAs("c", proto.CapDelivery)
	p.send(prompt)
	if m := p.msg(); m["type"] != "ack" || m["id"] != "p1" {
		t.Fatalf("%v", m)
	}
	if m := p.msg(); m["type"] != "agent.delivery" || m["id"] != "p1" || m["state"] != "started" {
		t.Fatalf("%v", m)
	}
	if got := gate.prompts(); len(got) != 1 || got[0] != "SECRET-WORDS" {
		t.Fatalf("the agent got %q", got)
	}
}

func TestAPromptSentAgainAfterItFailedIsReportedFailedAgain(t *testing.T) {
	gate, opt := withGate()
	gate.err = errors.New("boom")
	close(gate.gate)
	p, redial := onSocket(t, withEngine(t), opt)
	p.helloAs("c", proto.CapDelivery)
	p.send(prompt)
	p.msg()
	p.msg()
	redial()
	p.helloAs("c", proto.CapDelivery)
	p.send(prompt)
	p.msg()
	if m := p.msg(); m["state"] != "failed" || m["error"] != "boom" {
		t.Fatalf("%v", m)
	}
}

func TestAPromptIdFromAnotherClientIsAnotherPrompt(t *testing.T) {
	gate, opt := withGate()
	close(gate.gate)
	p, redial := onSocket(t, withEngine(t), opt)
	p.helloAs("one", proto.CapDelivery)
	p.send(prompt)
	p.msg()
	p.msg()
	redial()
	p.helloAs("two", proto.CapDelivery)
	p.send(prompt)
	p.msg()
	p.msg()
	if got := gate.prompts(); len(got) != 2 {
		t.Fatalf("the agent got %q", got)
	}
}

func TestAClientWithoutDeliveryIsAckedOnlyOnceTheAgentTakesItAndHearsNoDelivery(t *testing.T) {
	gate, opt := withGate()
	p, _ := onSocket(t, withEngine(t), opt)
	p.helloAs("c")
	in := p.feed()
	p.send(prompt)
	in.quiet(200 * time.Millisecond)
	close(gate.gate)
	if m := in.get(); m["type"] != "ack" || m["id"] != "p1" {
		t.Fatalf("%v", m)
	}
	in.quiet(200 * time.Millisecond)
}

func TestARefusedPromptIsLoggedWithItsCodeReasonAndSessionButNotItsText(t *testing.T) {
	out := logged(t)
	var s *Server
	_, opt := withGate()
	p, _ := onSocket(t, withEngine(t), opt, func(srv *Server) { s = srv })
	p.helloAs("c", proto.CapDelivery)
	if err := s.Orch.Store().Tx(func(tx *orch.Tx) error {
		_, err := tx.Exec(`INSERT INTO sessions (id, closed_at) VALUES ('a2', 1)`)
		return err
	}); err != nil {
		t.Fatal(err)
	}
	p.send(prompt)
	if m := p.msg(); m["type"] != "error" {
		t.Fatalf("%v", m)
	}
	line := waitLog(t, out, "prompt refused: session=a2 code=conflict reason=This session is closed")
	if strings.Contains(line, "SECRET-WORDS") {
		t.Fatal(line)
	}
}

func TestAgentListTellsTheRecapOfAStoredSessionThatIsNotRunning(t *testing.T) {
	stored := []orch.Session{{ID: "a0", Agent: "codex", Status: "idle", CreatedAt: 1, Recap: proto.Recap{Asked: "fix it", Read: 2, Said: "Fixed."}}}
	_, _, p := setup(t, func(s *Server) {
		s.OpenSessions = func() ([]orch.Session, error) { return stored, nil }
	})
	p.send(`{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3}`)
	p.recv()
	m := p.recv()
	recap, _ := m["agents"].([]any)[0].(map[string]any)["recap"].(map[string]any)
	if recap["asked"] != "fix it" || recap["read"] != float64(2) || recap["said"] != "Fixed." {
		t.Fatalf("%v", m)
	}
}
