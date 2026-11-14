//go:build unix

package wsserver

import (
	"fmt"
	"testing"

	"pocketd/internal/factory"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
)

// factoryFixture is ghFixture's project with a Factory config store that
// every peer of the fixture shares.
type factoryFixture struct {
	*ghFixture
	configs *factory.Configs
}

func newFactoryFixture(t *testing.T) *factoryFixture {
	return &factoryFixture{newGhFixture(t), factory.OpenConfigs(t.TempDir())}
}

func (f *factoryFixture) peer(who peer.Principal, caps ...string) *phone {
	_, _, p := local(f.t, who, f.server, func(s *Server) {
		s.Projects = func() []proto.Project { return []proto.Project{{Path: f.project}} }
		s.FactoryConfigs = f.configs
	})
	helloWith(p, caps...)
	return p
}

func (f *factoryFixture) get() string {
	return fmt.Sprintf(`{"type":"factory.config.get","id":"g","project":%q}`, f.project)
}

// outOfReach is each way GitHub can be out of reach, and what pocketd says about it.
var outOfReach = map[string]struct {
	cut     func(f *factoryFixture)
	problem string
}{
	"no gh":      {func(f *factoryFixture) { f.withGh.Store(false) }, "Install GitHub CLI (gh) to use the Factory"},
	"signed out": {func(f *factoryFixture) { f.put("auth.fail", "") }, "Run gh auth login to use the Factory"},
	"no remote": {func(f *factoryFixture) {
		f.put("view.err", "none of the git remotes configured for this repository point to a known GitHub host. To tell gh about a new GitHub host, please use `gh auth login`\n")
	}, "This project has no GitHub remote"},
}

func TestTheFactoryCapIsServedOnlyWithAConfigStore(t *testing.T) {
	hello := `{"type":"hello","id":"h","clientId":"c","protocolVersion":3,"caps":["factory.v1"]}`
	_, _, without := local(t, peer.OwnerOf(1))
	without.send(hello)
	if m := without.recv(); m["type"] != "hello.ok" || fmt.Sprint(m["caps"]) != "[]" {
		t.Fatalf("%v", m)
	}
	_, _, with := local(t, peer.OwnerOf(1), func(s *Server) { s.FactoryConfigs = factory.OpenConfigs(t.TempDir()) })
	with.send(hello)
	if m := with.recv(); m["type"] != "hello.ok" || fmt.Sprint(m["caps"]) != "[factory.v1]" {
		t.Fatalf("%v", m)
	}
}

func TestFactoryConfigGetAnswersTheDefaultsWhenNothingIsSaved(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.OwnerOf(1), proto.CapFactory)
	p.send(f.get())
	m := p.recv()
	cfg, _ := m["config"].(map[string]any)
	if m["type"] != "factory.config" || m["id"] != "g" || m["project"] != f.project || m["problem"] != nil ||
		cfg["enabled"] != false || cfg["spec"] != true || cfg["lead"] != "claude-opus" || cfg["fix"] != 2.0 || fmt.Sprint(cfg["checks"]) != "[]" {
		t.Fatalf("%v", m)
	}
}

func TestFactoryConfigGetSaysWhyTheFactoryCannotBeTurnedOn(t *testing.T) {
	for name, c := range outOfReach {
		f := newFactoryFixture(t)
		c.cut(f)
		p := f.peer(peer.OwnerOf(1), proto.CapFactory)
		p.send(f.get())
		if m := p.recv(); m["type"] != "factory.config" || m["problem"] != c.problem {
			t.Errorf("%s: %v", name, m)
		}
	}
}

func TestAFactoryVerbWithoutTheCapSaysTheFactoryIsOff(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.OwnerOf(1))
	p.send(f.get())
	if m := p.recv(); m["type"] != "error" || m["code"] != proto.CodeFactoryOff || m["message"] != "The Factory is off" {
		t.Fatalf("%v", m)
	}
}

func TestAFactoryVerbForAFolderThatIsNoProjectIsRefused(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.OwnerOf(1), proto.CapFactory)
	p.send(`{"type":"factory.config.get","id":"g","project":"/elsewhere"}`)
	if m := p.recv(); m["type"] != "error" || m["code"] != proto.CodeUnknownProject {
		t.Fatalf("%v", m)
	}
}

func TestAnAgentInATerminalReadsTheFactoryConfig(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.Principal{Kind: peer.PTY, Terminal: "t1", Scopes: peer.PTYScopes}, proto.CapFactory)
	p.send(f.get())
	if m := p.recv(); m["type"] != "factory.config" || m["project"] != f.project {
		t.Fatalf("%v", m)
	}
}

func (f *factoryFixture) save(enabled bool, slots int) string {
	return fmt.Sprintf(`{"type":"factory.config.save","id":"s","project":%q,"config":{"enabled":%v,"spec":true,"reviewer":true,"lead":"claude-opus","workers":"claude-sonnet","review":"codex","slots":%d,"fix":2,"checks":["go test ./..."]}}`, f.project, enabled, slots)
}

func slotsOf(m map[string]any) any {
	cfg, _ := m["config"].(map[string]any)
	return cfg["slots"]
}

func TestFactoryConfigSaveKeepsTheConfigAndAnswersIt(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.OwnerOf(1), proto.CapFactory)
	p.send(f.save(true, 2))
	m := p.recv()
	cfg, _ := m["config"].(map[string]any)
	if m["type"] != "factory.config" || m["id"] != "s" || m["problem"] != nil || cfg["enabled"] != true || cfg["slots"] != 2.0 || fmt.Sprint(cfg["checks"]) != "[go test ./...]" {
		t.Fatalf("%v", m)
	}
	if got := f.configs.Get(f.project); !got.Enabled || f.configs.Limit(f.project) != 2 {
		t.Fatalf("kept %+v", got)
	}
}

func TestTheFactoryCannotBeTurnedOnWhileGitHubIsOutOfReach(t *testing.T) {
	for name, c := range outOfReach {
		f := newFactoryFixture(t)
		c.cut(f)
		p := f.peer(peer.OwnerOf(1), proto.CapFactory)
		p.send(f.save(true, 0))
		if m := p.recv(); m["type"] != "error" || m["id"] != "s" || m["code"] != proto.CodeFactoryUnavailable || m["message"] != c.problem {
			t.Errorf("%s: %v", name, m)
		}
		if f.configs.Get(f.project).Enabled {
			t.Errorf("%s: the Factory was turned on", name)
		}
	}
}

func TestASaveWithTheFactoryOffIsKeptAndSaysWhyItCannotBeTurnedOn(t *testing.T) {
	f := newFactoryFixture(t)
	f.withGh.Store(false)
	p := f.peer(peer.OwnerOf(1), proto.CapFactory)
	p.send(f.save(false, 3))
	if m := p.recv(); m["type"] != "factory.config" || m["problem"] != "Install GitHub CLI (gh) to use the Factory" || slotsOf(m) != 3.0 {
		t.Fatalf("%v", m)
	}
	if got := f.configs.Get(f.project); got.Enabled || got.Slots != 3 {
		t.Fatalf("kept %+v", got)
	}
}

func TestASaveThatLeavesTheFactoryOnDoesNotAskGhAgain(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.OwnerOf(1), proto.CapFactory)
	p.send(f.save(true, 1))
	if m := p.recv(); m["type"] != "factory.config" {
		t.Fatalf("%v", m)
	}
	f.withGh.Store(false)
	p.send(f.save(true, 2))
	if m := p.recv(); m["type"] != "factory.config" || m["problem"] != nil || slotsOf(m) != 2.0 {
		t.Fatalf("%v", m)
	}
	p.send(f.save(false, 2))
	if m := p.recv(); m["type"] != "factory.config" || m["problem"] != "Install GitHub CLI (gh) to use the Factory" {
		t.Fatalf("turning it off: %v", m)
	}
}

func TestAFactoryConfigOutOfRangeIsRefusedAsInvalid(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.OwnerOf(1), proto.CapFactory)
	p.send(f.save(false, proto.MaxBoardLimit+1))
	if m := p.recv(); m["type"] != "error" || m["code"] != proto.CodeInvalidConfig || m["message"] != "These Factory settings aren't valid" {
		t.Fatalf("%v", m)
	}
	if got := f.configs.Get(f.project); got.Slots != 0 {
		t.Fatalf("kept %+v", got)
	}
}

func TestSavesSentBackToBackAreKeptInTheOrderSent(t *testing.T) {
	f := newFactoryFixture(t)
	f.put("sleep", "0.1")
	p := f.peer(peer.OwnerOf(1), proto.CapFactory)
	p.send(f.save(false, 1))
	p.send(f.save(false, 2))
	if first, second := p.recv(), p.recv(); slotsOf(first) != 1.0 || slotsOf(second) != 2.0 {
		t.Fatalf("answered %v then %v", first, second)
	}
	if got := f.configs.Get(f.project); got.Slots != 2 {
		t.Fatalf("kept %+v", got)
	}
}

func TestAnAgentInATerminalCannotSaveTheFactoryConfig(t *testing.T) {
	f := newFactoryFixture(t)
	p := f.peer(peer.Principal{Kind: peer.PTY, Terminal: "t1", Scopes: peer.PTYScopes}, proto.CapFactory)
	p.send(f.save(false, 1))
	if m := p.recv(); m["type"] != "error" || m["code"] != "scope_denied" {
		t.Fatalf("%v", m)
	}
	if got := f.configs.Get(f.project); got.Slots != 0 {
		t.Fatalf("kept %+v", got)
	}
}
