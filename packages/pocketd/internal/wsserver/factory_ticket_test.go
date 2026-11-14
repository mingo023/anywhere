package wsserver

import (
	"fmt"
	"testing"
	"time"

	"pocketd/internal/board"
	"pocketd/internal/factory"
	"pocketd/internal/launch"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
)

// factoryWorld is an engine with one Ticket, and the option that serves it.
func factoryWorld(t *testing.T) (*factory.Engine, proto.Ticket, func(*Server)) {
	t.Helper()
	home := t.TempDir()
	e := &factory.Engine{Home: home, Board: board.Open(home), Configs: factory.OpenConfigs(home), Now: time.Now}
	e.Launch = func(string, string, proto.LaunchSpec, func(string)) launch.Result {
		return launch.Result{Err: &launch.Failure{Code: "x", Message: "No such agent"}}
	}
	x, err := e.Board.Save(proto.Ticket{Title: "Queue sends", Prompt: "Keep sends in a queue", Provider: "claude", Folder: "/app"}, false, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	return e, x, func(s *Server) { s.Factory, s.FactoryConfigs = e, e.Configs }
}

func TestFactoryTicketAnswersWithTheTicketsActivity(t *testing.T) {
	e, x, serve := factoryWorld(t)
	e.Start("req12345678", x, func(string) {})
	_, _, p := local(t, peer.OwnerOf(1), serve)
	helloWith(p, proto.CapFactory)
	p.send(fmt.Sprintf(`{"type":"factory.ticket","id":"d","ticketId":%q}`, x.ID))
	m := p.recv()
	for m["type"] == "factory.update" {
		m = p.recv()
	}
	detail, _ := m["ticket"].(map[string]any)
	acts, _ := detail["activity"].([]any)
	if m["type"] != "factory.ticket" || m["id"] != "d" || detail["ticketId"] != x.ID || len(acts) != 2 {
		t.Fatalf("%v", m)
	}
	if last, _ := acts[1].(map[string]any); last["kind"] != "failed" || last["text"] != "No such agent" || last["station"] != proto.StationSpec {
		t.Fatalf("%v", acts[1])
	}
}

func TestFactoryTicketRefusesATicketTheBoardDoesNotHold(t *testing.T) {
	_, _, serve := factoryWorld(t)
	_, _, p := local(t, peer.OwnerOf(1), serve)
	helloWith(p, proto.CapFactory)
	p.send(`{"type":"factory.ticket","id":"d","ticketId":"nope"}`)
	if m := p.recv(); m["type"] != "error" || m["id"] != "d" || m["code"] != proto.CodeUnknownTicket {
		t.Fatalf("%v", m)
	}
}

func TestFactoryTicketIsOffWithoutAnEngineOrTheCap(t *testing.T) {
	e, x, serve := factoryWorld(t)
	ask := fmt.Sprintf(`{"type":"factory.ticket","id":"d","ticketId":%q}`, x.ID)
	_, _, noEngine := local(t, peer.OwnerOf(1), func(s *Server) { s.FactoryConfigs = e.Configs })
	helloWith(noEngine, proto.CapFactory)
	_, _, noCap := local(t, peer.OwnerOf(1), serve)
	helloWith(noCap)
	for name, p := range map[string]*phone{"no engine": noEngine, "no cap": noCap} {
		p.send(ask)
		if m := p.recv(); m["type"] != "error" || m["code"] != proto.CodeFactoryOff || m["message"] != "The Factory is off" {
			t.Errorf("%s: %v", name, m)
		}
	}
}

func TestOnlyAnOwnerWithTheCapHearsThatAFactoryTicketChanged(t *testing.T) {
	e, x, serve := factoryWorld(t)
	_, _, owner := local(t, peer.OwnerOf(1), serve)
	helloWith(owner, proto.CapFactory)
	_, _, plain := local(t, peer.OwnerOf(1), serve)
	helloWith(plain)
	_, _, agent := local(t, peer.Principal{Kind: peer.PTY, Terminal: "t1", Scopes: peer.PTYScopes}, serve)
	helloWith(agent, proto.CapFactory)

	e.Start("req12345678", x, func(string) {})
	if m := owner.recv(); m["type"] != "factory.update" || m["ticketId"] != x.ID {
		t.Fatalf("the owner got %v", m)
	}
	// The next frame each of the others gets is the answer to its own request, so no update came first.
	ask := fmt.Sprintf(`{"type":"factory.ticket","id":"d","ticketId":%q}`, x.ID)
	plain.send(ask)
	if m := plain.recv(); m["code"] != proto.CodeFactoryOff {
		t.Fatalf("an owner without the cap got %v", m)
	}
	agent.send(ask)
	if m := agent.recv(); m["code"] != "scope_denied" {
		t.Fatalf("a terminal agent got %v", m)
	}
}
