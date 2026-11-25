package wsserver

import (
	"database/sql"
	"fmt"
	"path/filepath"
	"testing"
	"time"

	"pocketd/internal/automation"
	"pocketd/internal/board"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/statedb/dbtest"
)

func TestAStoresRefusalOfAFolderInNoProjectAnswersTheUnknownProjectCode(t *testing.T) {
	for name, e := range map[string]proto.Error{
		"board":       boardError("b", fmt.Errorf("save: %w", registry.ErrUnknownProject)),
		"automations": automationError("a", fmt.Errorf("save: %w", registry.ErrUnknownProject)),
	} {
		if e.Code != proto.CodeUnknownProject {
			t.Errorf("%s answered %q", name, e.Code)
		}
	}
}

const helloWithBoardAndAutomations = `{"type":"hello","id":"h","token":"tok","clientId":"c","protocolVersion":3,"caps":["board.v1","automations.v1"]}`

// answer is the reply to the request id, after whatever else the server sent.
func (p *phone) answer(id string) map[string]any {
	p.t.Helper()
	for {
		if m := p.recv(); m["id"] == id && m["type"] != "hello.ok" {
			return m
		}
	}
}

func TestSavingATicketOrAnAutomationInNoProjectAnswersTheUnknownProjectCodeAndSavesNothing(t *testing.T) {
	var db *sql.DB
	var tickets *board.Store
	var autos *automation.Store
	project := t.TempDir()
	_, _, p := local(t, peer.OwnerOf(1), func(s *Server) {
		db = dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db"))
		reg, err := registry.Open(db)
		if err != nil {
			t.Fatal(err)
		}
		if err := reg.AddProject(project, nil); err != nil {
			t.Fatal(err)
		}
		if tickets, err = board.Open(db, reg.Load); err != nil {
			t.Fatal(err)
		}
		if autos, err = automation.Open(db, reg.Load); err != nil {
			t.Fatal(err)
		}
		s.Board, s.Automations, s.Registry = tickets, autos, reg
		s.Scheduler = &automation.Scheduler{Store: autos, Now: time.Now}
		s.Projects = func() []proto.Project { return []proto.Project{{Path: project}} }
	})
	p.send(helloWithBoardAndAutomations)
	elsewhere := t.TempDir()
	ticket := `"provider":"claude","title":"t","prompt":"p","folder":`
	auto := `"provider":"claude","name":"a","prompt":"p","enabled":true,"schedule":{"kind":"interval","everyMin":60},"folder":`
	for id, raw := range map[string]string{
		"b1": `{"type":"board.save","id":"b1","ticket":{` + ticket + `"` + elsewhere + `"}}`,
		"a1": `{"type":"automation.save","id":"a1","automation":{` + auto + `"` + elsewhere + `"}}`,
	} {
		p.send(raw)
		if m := p.answer(id); m["type"] != "error" || m["code"] != "unknown_project" {
			t.Errorf("%s: %v", id, m)
		}
	}
	p.send(`{"type":"board.save","id":"b2","ticket":{` + ticket + `"` + project + `"}}`)
	p.send(`{"type":"automation.save","id":"a2","automation":{` + auto + `"` + project + `"}}`)
	for _, id := range []string{"b2", "a2"} {
		if m := p.answer(id); m["type"] != "ack" {
			t.Errorf("%s: %v", id, m)
		}
	}
	ts, _ := tickets.Snapshot()
	as, _ := autos.Snapshot()
	if len(ts) != 1 || len(as) != 1 {
		t.Fatalf("saved %+v %+v", ts, as)
	}
}
