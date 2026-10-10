package wsserver

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
	"time"

	"pocketd/internal/locals"
	"pocketd/internal/peer"
	"pocketd/internal/registry"
	"pocketd/internal/terminal"
)

const helloWithLocals = `{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3,"caps":["locals.v1"]}`

func withLocals(t *testing.T, terms *terminal.Manager) func(*Server) {
	dir := t.TempDir()
	path := filepath.Join(dir, "desktop.json")
	if err := os.WriteFile(path, []byte(`{"projects":["/p"],"repos":{}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	return func(s *Server) {
		s.Locals, s.Registry, s.Terminals = locals.Open(dir), registry.New(path), terms
	}
}

func TestALocalsClientGetsEveryLocalThenEachChange(t *testing.T) {
	_, _, p := local(t, peer.OwnerOf(1), withLocals(t, terminal.NewManager()))
	p.send(helloWithLocals)
	for _, want := range []string{"hello.ok", "agent.list"} {
		if m := p.recv(); m["type"] != want {
			t.Fatalf("%v", m)
		}
	}
	if m := p.recv(); m["type"] != "local.list" || !reflect.DeepEqual(m["locals"], []any{}) {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"local.create","id":"c","localId":"l1","project":"/p","name":" Local 2 "}`)
	got := map[string]map[string]any{}
	for range 2 {
		m := p.recv()
		got[m["type"].(string)] = m
	}
	want := []any{map[string]any{"id": "l1", "project": "/p", "name": "Local 2"}}
	if got["ack"]["id"] != "c" || !reflect.DeepEqual(got["local.list"]["locals"], want) {
		t.Fatalf("%v", got)
	}
	p.send(`{"type":"local.create","id":"x","localId":"l2","project":"/elsewhere","name":"Local 3"}`)
	if m := p.recv(); m["type"] != "error" || m["code"] != "unknown_project" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"local.rename","id":"r","localId":"gone","title":"x"}`)
	if m := p.recv(); m["type"] != "error" || m["code"] != "unknown_local" {
		t.Fatalf("%v", m)
	}
}

func TestDeletingALocalClosesItsTerminalsOnly(t *testing.T) {
	terms := terminal.NewManager()
	t.Cleanup(func() { terms.CloseAll(0) })
	_, _, p := local(t, peer.OwnerOf(1), withLocals(t, terms))
	in, err := terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}, Local: "l1"})
	if err != nil {
		t.Fatal(err)
	}
	out, err := terms.Spawn(terminal.Spec{Cmd: "sleep", Args: []string{"30"}})
	if err != nil {
		t.Fatal(err)
	}
	p.send(helloWithLocals)
	for range 3 {
		p.recv()
	}
	p.send(`{"type":"local.create","id":"c","localId":"l1","project":"/p","name":"Local 2"}`)
	p.recv()
	p.recv()
	p.send(`{"type":"local.delete","id":"d","localId":"l1"}`)
	got := map[string]map[string]any{}
	for range 2 {
		m := p.recv()
		got[m["type"].(string)] = m
	}
	if got["ack"]["id"] != "d" || !reflect.DeepEqual(got["local.list"]["locals"], []any{}) {
		t.Fatalf("%v", got)
	}
	select {
	case <-in.Done():
	case <-time.After(5 * time.Second):
		t.Fatal("the Local's terminal is still open")
	}
	select {
	case <-out.Done():
		t.Fatal("a terminal outside the Local closed")
	default:
	}
}

func TestAClientWithoutTheLocalsCapCannotUseThem(t *testing.T) {
	_, _, p := local(t, peer.OwnerOf(1), withLocals(t, terminal.NewManager()))
	p.hello()
	p.send(`{"type":"local.create","id":"c","localId":"l1","project":"/p","name":"Local 2"}`)
	if m := p.recv(); m["type"] != "error" || m["id"] != "c" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"agent.create","id":"a","requestId":"r","spec":{"project":"/p","checkout":{"local":"l1"},"provider":"claude","model":"opus","effort":"high","access":"edits","plan":false,"prompt":"x"}}`)
	if m := p.recv(); m["type"] != "error" || m["id"] != "a" {
		t.Fatalf("%v", m)
	}
}
