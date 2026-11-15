//go:build unix

package wsserver

import (
	"fmt"
	"os"
	"path/filepath"
	"syscall"
	"testing"
	"time"

	"pocketd/internal/board"
	"pocketd/internal/factory"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
)

func metricsPeer(t *testing.T, who peer.Principal, engine bool, caps ...string) (*phone, string) {
	t.Helper()
	f := newGhFixture(t)
	home := t.TempDir()
	e := &factory.Engine{Home: home, Board: board.Open(home), Configs: factory.OpenConfigs(home), Now: time.Now}
	_, _, p := local(t, who, f.server, func(s *Server) {
		s.Projects = func() []proto.Project { return []proto.Project{{Path: f.project}} }
		s.FactoryConfigs = e.Configs
		if engine {
			s.Factory = e
		}
	})
	helloWith(p, caps...)
	return p, f.project
}

func TestFactoryMetricsAnswersWithZeroesForAProjectWithNoActivity(t *testing.T) {
	p, project := metricsPeer(t, peer.OwnerOf(1), true, proto.CapFactory)
	p.send(fmt.Sprintf(`{"type":"factory.metrics","id":"m","project":%q}`, project))
	m := p.recv()
	metrics, _ := m["metrics"].(map[string]any)
	if m["type"] != "factory.metrics" || m["id"] != "m" || m["project"] != project || metrics["days"] != float64(14) || metrics["mergedPerDay"] != float64(0) {
		t.Fatalf("%v", m)
	}
}

func TestFactoryMetricsRefusesAnUnknownProjectAndIsOffWithoutAnEngineOrTheCap(t *testing.T) {
	p, _ := metricsPeer(t, peer.OwnerOf(1), true, proto.CapFactory)
	p.send(`{"type":"factory.metrics","id":"m","project":"/nowhere"}`)
	if m := p.recv(); m["type"] != "error" || m["code"] != proto.CodeUnknownProject {
		t.Fatalf("%v", m)
	}
	for name, off := range map[string]*phone{"no engine": first(metricsPeer(t, peer.OwnerOf(1), false, proto.CapFactory)), "no cap": first(metricsPeer(t, peer.OwnerOf(1), true))} {
		off.send(`{"type":"factory.metrics","id":"m","project":"/app"}`)
		if m := off.recv(); m["type"] != "error" || m["code"] != proto.CodeFactoryOff {
			t.Errorf("%s: %v", name, m)
		}
	}
}

func first(p *phone, _ string) *phone { return p }

// A log that can't be read yet stands for the seconds that a project's logs can take to read.
func TestFactoryMetricsDoesNotHoldTheReadLoopWhileItReadsTheLogs(t *testing.T) {
	f := newGhFixture(t)
	home := t.TempDir()
	store := board.Open(home)
	e := &factory.Engine{Home: home, Board: store, Configs: factory.OpenConfigs(home), Now: time.Now}
	saved, err := store.Save(proto.Ticket{Title: "Slow", Prompt: "Slow.", Provider: "claude", Folder: f.project}, false, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	logDir := filepath.Join(home, "state", "factory", saved.ID)
	if err := os.MkdirAll(logDir, 0o700); err != nil {
		t.Fatal(err)
	}
	log := filepath.Join(logDir, "activity.jsonl")
	if err := syscall.Mkfifo(log, 0o600); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if w, err := os.OpenFile(log, os.O_WRONLY, 0); err == nil {
			w.Close()
		}
	})
	_, _, p := local(t, peer.OwnerOf(1), f.server, func(s *Server) {
		s.Projects = func() []proto.Project { return []proto.Project{{Path: f.project}} }
		s.FactoryConfigs = e.Configs
		s.Factory = e
	})
	helloWith(p, proto.CapFactory)
	p.send(fmt.Sprintf(`{"type":"factory.metrics","id":"slow","project":%q}`, f.project))
	p.send(`{"type":"factory.metrics","id":"next","project":"/nowhere"}`)
	if m := p.recv(); m["id"] != "next" || m["type"] != "error" {
		t.Fatalf("%v", m)
	}
}
