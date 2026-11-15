//go:build unix

package e2e

import (
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func TestAServedPocketdTakesFactoryVerbs(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapFactory)
	o.Send(map[string]any{"type": "factory.config.get", "id": "g1", "project": "/elsewhere"})
	if e := o.WaitFor("the answer to g1", func(m Message) bool { return m.ID == "g1" }); e.Type != "error" || e.Code != proto.CodeUnknownProject {
		t.Fatalf("got %s", e.Raw)
	}
}

// fakeGh puts the fake gh of internal/github first on the path of pocketd and of its login shell: signed in, with
// an open pull request for every folder. Pass it after launchReady, which writes the rc files this appends to.
func fakeGh(h *Harness) {
	bin, err := filepath.Abs(filepath.Join("..", "internal", "github", "testdata", "fakegh"))
	if err != nil {
		h.t.Fatal(err)
	}
	dir := filepath.Join(h.Home, "gh")
	os.MkdirAll(dir, 0o755)
	for name, body := range map[string]string{
		"version":      "gh version 2.81.0 (2025-10-01)\n",
		"auth":         "  ✓ Logged in to github.com account e2e (keyring)\n  - Active account: true\n",
		"view.json":    `{"isDraft":true,"number":7,"state":"OPEN","title":"Review","url":"https://github.com/acme/app/pull/7","mergeable":"MERGEABLE","reviewDecision":"","statusCheckRollup":[]}`,
		"threads.json": `{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[]}}}}}`,
	} {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644); err != nil {
			h.t.Fatal(err)
		}
	}
	for i, kv := range h.Env {
		if rest, ok := strings.CutPrefix(kv, "PATH="); ok {
			h.Env[i] = "PATH=" + bin + ":" + rest
		}
	}
	h.Env = append(h.Env, "FAKEGH="+dir)
	sh := fmt.Sprintf("export PATH='%s':$PATH\nexport FAKEGH='%s'\n", bin, dir)
	fish := fmt.Sprintf("set -gx PATH '%s' $PATH\nset -gx FAKEGH '%s'\n", bin, dir)
	os.MkdirAll(filepath.Join(h.Home, ".config", "fish"), 0o755)
	for name, body := range map[string]string{".zshenv": sh, ".bash_profile": sh, ".profile": sh, ".config/fish/config.fish": fish} {
		f, err := os.OpenFile(filepath.Join(h.Home, name), os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
		if err != nil {
			h.t.Fatal(err)
		}
		f.WriteString(body)
		f.Close()
	}
}

// factoryOn turns the Factory on for the harness's Repo with the verb the desktop sends.
func factoryOn(h *Harness, o *Phone, change func(*proto.FactoryConfig)) {
	h.t.Helper()
	cfg := proto.DefaultFactoryConfig()
	cfg.Enabled = true
	change(&cfg)
	o.Send(map[string]any{"type": "factory.config.save", "id": "on", "project": h.Repo, "config": cfg})
	if m := o.WaitFor("the saved Factory settings", func(m Message) bool { return m.ID == "on" }); m.Type != "factory.config" || !m.Config.Enabled {
		h.t.Fatalf("factory.config.save: %s", m.Raw)
	}
}

func factoryDetail(o *Phone, id, ticket string) proto.FactoryTicket {
	o.t.Helper()
	o.Send(map[string]any{"type": "factory.ticket", "id": id, "ticketId": ticket})
	m := o.WaitFor("the ticket's detail", func(m Message) bool { return m.ID == id })
	if m.Type != "factory.ticket" {
		o.t.Fatalf("factory.ticket: %s", m.Raw)
	}
	return m.Ticket
}

func activityOf(d proto.FactoryTicket) []string {
	var out []string
	for _, ev := range d.Activity {
		out = append(out, ev.Actor+":"+ev.Kind+"@"+ev.Station)
	}
	return out
}

func TestAFactoryTicketStopsAtThePlanGateAndStartsItsWorkersWhenYouApprove(t *testing.T) {
	h := Start(t, launchReady(""), gitIdentity, fakeGh)
	o := h.Owner(proto.CapBoard, proto.CapFactory)
	factoryOn(h, o, func(*proto.FactoryConfig) {})
	if a := o.Board("save", "s1", map[string]any{"ticket": ticketDraft(h, teamPlan(t)), "start": true}); a.Type != "ack" {
		t.Fatalf("got %s", a.Raw)
	}

	gate := o.WaitTicket("the plan gate", func(x proto.Ticket) bool { return x.Gate == proto.GatePlan })
	if gate.Station != proto.StationPlan || gate.Status != proto.TicketNeedsYou || gate.AgentID == "" || gate.Worktree == "" {
		t.Fatalf("at the gate: %+v", gate)
	}
	d := factoryDetail(o, "d1", gate.ID)
	if !strings.Contains(d.Spec, "## Acceptance criteria\n- [ ] One checkable statement per line") || !slices.Equal(activityOf(d), []string{"anywhere:moved@spec", "anywhere:moved@plan", "anywhere:gate@plan"}) {
		t.Fatalf("spec %q, activity %v", d.Spec, activityOf(d))
	}

	o.Send(map[string]any{"type": "team.start", "id": "start", "lead": gate.AgentID, "tasks": []map[string]any{
		{"id": "t1", "agent": "claude", "allow": []string{"make test"}},
		{"id": "t2", "agent": "claude", "allow": []string{}},
	}})
	// The engine can write the Ticket before the ack arrives, so the three are awaited together.
	o.WaitAll("team.start's ack, the ticket at Implement, and the update that says so",
		func(m Message) bool {
			if m.ID == "start" && m.Type != "ack" {
				t.Fatalf("team.start: %s", m.Raw)
			}
			return m.ID == "start"
		},
		func(m Message) bool {
			return m.Type == "board" && slices.ContainsFunc(m.Tickets, func(x proto.Ticket) bool {
				return x.ID == gate.ID && x.Station == proto.StationImplement && x.Gate == "" && x.Status == proto.TicketWorking
			})
		},
		func(m Message) bool { return m.Type == "factory.update" && m.TicketID == gate.ID },
	)
	d = factoryDetail(o, "d2", gate.ID)
	if acts := activityOf(d); !slices.Contains(acts, "you:approved@plan") || acts[len(acts)-1] != "anywhere:moved@implement" {
		t.Fatalf("activity %v", acts)
	}
}

func TestAProjectLimitOfOneStartsTheNextTicketOnlyWhenTheFirstWaitsOnYou(t *testing.T) {
	h := Start(t, launchReady(""), gitIdentity, fakeGh)
	o := h.Owner(proto.CapBoard, proto.CapFactory)
	factoryOn(h, o, func(c *proto.FactoryConfig) { c.Slots = 1 })
	first := o.Board("save", "s1", map[string]any{"ticket": ticketDraft(h, teamPlan(t)), "start": true})
	second := o.Board("save", "s2", map[string]any{"ticket": ticketDraft(h, teamPlan(t)), "start": true})
	if first.Type != "ack" || second.Type != "ack" {
		t.Fatalf("got %s and %s", first.Raw, second.Raw)
	}
	find := func(m Message, id string) proto.Ticket {
		i := slices.IndexFunc(m.Tickets, func(x proto.Ticket) bool { return x.ID == id })
		if i < 0 {
			return proto.Ticket{}
		}
		return m.Tickets[i]
	}
	var early []string
	o.WaitFor("the second ticket's start", func(m Message) bool {
		if m.Type != "board" {
			return false
		}
		a, b := find(m, first.TicketID), find(m, second.TicketID)
		if b.State != proto.TicketLive {
			return false
		}
		if a.Gate != proto.GatePlan {
			early = append(early, fmt.Sprintf("the first was %s at %s", a.Status, a.Station))
		}
		return true
	})
	if len(early) > 0 {
		t.Fatalf("the second ticket started while the first held the project's slot: %v", early)
	}
}
