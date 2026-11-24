package wsserver

import (
	"reflect"
	"testing"

	"pocketd/internal/peer"
	"pocketd/internal/registry"
)

const helloWithRegistry = `{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3,"caps":["registry.v2"]}`

func withRegistry(t *testing.T) (func(*Server), string) {
	dir, folder := t.TempDir(), t.TempDir()
	return func(s *Server) { s.Registry = registry.Open(dir) }, folder
}

func (p *phone) helloAndList(hello string) map[string]any {
	p.t.Helper()
	p.send(hello)
	for _, want := range []string{"hello.ok", "agent.list", "registry.list"} {
		if m := p.recv(); m["type"] != want {
			p.t.Fatalf("want %s: %v", want, m)
		}
	}
	return nil
}

func (p *phone) ackAndList(id string) map[string]any {
	p.t.Helper()
	got := map[string]map[string]any{}
	for range 2 {
		m := p.recv()
		got[m["type"].(string)] = m
	}
	if got["ack"]["id"] != id || got["registry.list"] == nil {
		p.t.Fatalf("%v", got)
	}
	return got["registry.list"]
}

func TestARegistryClientGetsTheListOnHelloThenEachChange(t *testing.T) {
	opt, folder := withRegistry(t)
	_, _, p := local(t, peer.OwnerOf(1), opt)
	p.send(helloWithRegistry)
	for _, want := range []string{"hello.ok", "agent.list"} {
		if m := p.recv(); m["type"] != want {
			t.Fatalf("%v", m)
		}
	}
	if m := p.recv(); m["type"] != "registry.list" || !reflect.DeepEqual(m["projects"], []any{}) || !reflect.DeepEqual(m["tracked"], []any{}) {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"project.add","id":"a","path":` + q(folder) + `,"repo":{"name":"App"}}`)
	l := p.ackAndList("a")
	row := l["projects"].([]any)[0].(map[string]any)
	if row["path"] != folder || len(row["id"].(string)) != 16 || row["repo"].(map[string]any)["name"] != "App" {
		t.Fatalf("%v", l)
	}
	p.send(`{"type":"worktree.track","id":"t","path":"/wt/a","tracked":true}`)
	if l := p.ackAndList("t"); !reflect.DeepEqual(l["tracked"], []any{"/wt/a"}) {
		t.Fatalf("%v", l)
	}
	p.send(`{"type":"settings.shared","id":"s","settings":{"git":{"remote":"fork","commit_all":null,"force_with_lease":true},"access":"edits","worktree_root":"/wt","external_worktrees":true}}`)
	if l := p.ackAndList("s"); l["settings"].(map[string]any)["worktree_root"] != "/wt" {
		t.Fatalf("%v", l)
	}
	p.send(`{"type":"project.set","id":"u","path":` + q(folder) + `,"repo":{"name":"Renamed","hidden":["codex"]}}`)
	l = p.ackAndList("u")
	if r := l["projects"].([]any)[0].(map[string]any); r["id"] != row["id"] || r["repo"].(map[string]any)["name"] != "Renamed" {
		t.Fatalf("%v", l)
	}
	p.send(`{"type":"project.remove","id":"r","path":` + q(folder) + `}`)
	if l := p.ackAndList("r"); !reflect.DeepEqual(l["projects"], []any{}) {
		t.Fatalf("%v", l)
	}
}

func TestARefusedRegistryChangeAnswersAnErrorThenTheListAsItStands(t *testing.T) {
	opt, _ := withRegistry(t)
	_, _, p := local(t, peer.OwnerOf(1), opt)
	p.helloAndList(helloWithRegistry)
	p.send(`{"type":"project.set","id":"u","path":"/nowhere","repo":{}}`)
	if m := p.recv(); m["type"] != "error" || m["code"] != "unknown_project" || m["id"] != "u" {
		t.Fatalf("%v", m)
	}
	if m := p.recv(); m["type"] != "registry.list" {
		t.Fatalf("%v", m)
	}
	p.send(`{"type":"project.add","id":"a","path":"relative"}`)
	if m := p.recv(); m["type"] != "error" || m["id"] != "a" {
		t.Fatalf("%v", m)
	}
	if m := p.recv(); m["type"] != "registry.list" {
		t.Fatalf("%v", m)
	}
}

func TestOnlyTheOwnerChangesTheRegistry(t *testing.T) {
	opt, folder := withRegistry(t)
	_, _, p := local(t, phoneDevice, opt)
	p.helloAndList(helloWithRegistry)
	p.send(`{"type":"project.add","id":"a","path":` + q(folder) + `}`)
	if m := p.recv(); m["type"] != "error" || m["code"] != "scope_denied" {
		t.Fatalf("%v", m)
	}
}

func TestAClientWithoutTheRegistryCapGetsNoListAndCannotChangeIt(t *testing.T) {
	opt, folder := withRegistry(t)
	_, _, p := local(t, peer.OwnerOf(1), opt)
	p.hello()
	p.send(`{"type":"project.add","id":"a","path":` + q(folder) + `}`)
	if m := p.recv(); m["type"] != "error" || m["id"] != "a" {
		t.Fatalf("%v", m)
	}
}

func TestTheDesktopNeedsTheRegistryCapButAPhoneWithoutItIsServed(t *testing.T) {
	opt, _ := withRegistry(t)
	_, _, desktop := local(t, peer.OwnerOf(1), opt)
	desktop.send(`{"type":"hello","id":"h","clientId":"desktop","protocolVersion":3}`)
	if m := desktop.recv(); m["code"] != "client_too_old" {
		t.Fatalf("%v", m)
	}
	if ce := desktop.closedWith(); ce.Code != 4426 || ce.Reason != "client_too_old" {
		t.Fatalf("%v", ce)
	}
	_, _, ios := local(t, peer.OwnerOf(1), opt)
	ios.send(`{"type":"hello","id":"h","clientId":"ios","protocolVersion":3}`)
	if m := ios.recv(); m["type"] != "hello.ok" {
		t.Fatalf("%v", m)
	}
	_, _, current := local(t, peer.OwnerOf(1), opt)
	current.helloAndList(`{"type":"hello","id":"h","clientId":"desktop","protocolVersion":3,"caps":["registry.v2"]}`)
}
