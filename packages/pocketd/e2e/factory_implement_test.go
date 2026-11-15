//go:build unix

package e2e

import (
	"encoding/json"
	"slices"
	"strconv"
	"strings"
	"testing"

	"pocketd/internal/factory"
	"pocketd/internal/proto"
)

// factoryPreset turns the Factory on for the harness's repository before pocketd starts, with no Spec station.
func factoryPreset(h *Harness) {
	cfg := proto.DefaultFactoryConfig()
	cfg.Enabled, cfg.Spec, cfg.Reviewer = true, false, false
	if err := factory.OpenConfigs(h.Home).Save(h.Repo, cfg); err != nil {
		h.t.Fatal(err)
	}
}

// mergingPlan is a lead's plan whose t1 tries to merge a pull request. Fake claude echoes the plan request, so
// this block is the lead's plan.
func mergingPlan(t *testing.T) string {
	t.Helper()
	plan, err := json.Marshal(map[string]any{"tasks": []map[string]any{
		{"id": "t1", "title": "Write a", "agent": "claude", "files": []string{"a.txt"}, "after": []string{}, "allow": []string{"gh"},
			"instructions": "write a.txt one\nrun gh pr merge 7"},
		{"id": "t2", "title": "Write b", "agent": "claude", "files": []string{"b.txt"}, "after": []string{"t1"}, "allow": []string{},
			"instructions": "write b.txt two"},
	}})
	if err != nil {
		t.Fatal(err)
	}
	return "<anywhere_team_plan>\n" + string(plan) + "\n</anywhere_team_plan>"
}

func TestAWorkersMergeCommandReachesYouAsAnAskEvenWhenItsAllowlistHoldsGh(t *testing.T) {
	h := Start(t, launchReady(""), gitIdentity, factoryPreset)
	o := h.Owner("board.v1", "team.v1", "names.v1", "factory.v1")
	draft := ticketDraft(h, mergingPlan(t))
	if a := o.Board("save", "s1", map[string]any{"ticket": draft, "start": true}); a.Type != "ack" {
		t.Fatalf("got %s", a.Raw)
	}
	x := o.WaitTicket("the plan gate", func(t proto.Ticket) bool { return t.Gate == proto.GatePlan })
	lead := x.AgentID

	o.Send(map[string]any{"type": "agent.list", "id": "al"})
	list := o.WaitFor("the agent list", func(m Message) bool { return m.Type == "agent.list" && m.ID == "al" })
	var v struct {
		Agents []struct{ ID, Title string } `json:"agents"`
	}
	json.Unmarshal([]byte(list.Raw), &v)
	i := slices.IndexFunc(v.Agents, func(a struct{ ID, Title string }) bool { return a.ID == lead })
	if suffix := "-" + strconv.FormatInt(x.Number, 10) + " · Review"; i < 0 || !strings.HasSuffix(v.Agents[i].Title, suffix) {
		t.Fatalf("the lead's session isn't named for its ticket: %s", list.Raw)
	}

	start := func(id string, allow []string) Message {
		o.Send(map[string]any{"type": "team.start", "id": id, "lead": lead, "tasks": []map[string]any{
			{"id": "t1", "agent": "claude", "allow": allow},
			{"id": "t2", "agent": "claude", "allow": []string{}},
		}})
		return o.WaitFor("the reply to "+id, replyTo(id))
	}
	if m := start("refused", []string{"gh pr merge"}); m.Type != "error" || m.Message != `Task t1 can't allow "gh pr merge": only you can run it` {
		t.Fatalf("an allowlist with a never-allow prefix: %s", m.Raw)
	}
	if m := start("start", []string{"gh"}); m.Type != "ack" {
		t.Fatalf("team.start: %s", m.Raw)
	}

	var req Message
	gated := false
	o.WaitAll("the ask and the ask gate",
		func(m Message) bool {
			if asked(m) {
				req = m
			}
			return req.Type != ""
		},
		func(m Message) bool {
			for _, t := range m.Tickets {
				gated = gated || m.Type == "board" && t.ID == x.ID && t.Gate == proto.GateAsk && t.Status == proto.TicketNeedsYou && t.Station == proto.StationImplement
			}
			return gated
		})
	if req.Request.Detail.Command != "gh pr merge 7" {
		t.Fatalf("asked for %q", req.Request.Detail.Command)
	}
	resolve(o, req, map[string]any{"decision": "allow"})
	o.WaitTicket("the gate lifted", func(t proto.Ticket) bool { return t.ID == x.ID && t.Gate == "" && t.Station == proto.StationImplement })
	o.WaitFor("t1's handback", taskIn(lead, "t1", "done"))

	o.Send(map[string]any{"type": "factory.ticket", "id": "d1", "ticketId": x.ID})
	detail := o.WaitFor("the detail", func(m Message) bool { return m.ID == "d1" })
	var d struct {
		Ticket proto.FactoryTicket `json:"ticket"`
	}
	json.Unmarshal([]byte(detail.Raw), &d)
	var got []string
	for _, ev := range d.Ticket.Activity {
		got = append(got, ev.Actor+":"+ev.Kind+":"+ev.Text)
	}
	for _, want := range []string{"worker:t1:asked:gh pr merge 7", "you:allowed:gh pr merge 7"} {
		if !slices.Contains(got, want) {
			t.Fatalf("no %q in the activity: %q", want, got)
		}
	}
	if !slices.ContainsFunc(got, func(s string) bool { return strings.HasPrefix(s, "worker:t1:handback:") }) || slices.Contains(got, "worker:t1:autoAllowed:gh pr merge 7") {
		t.Fatalf("activity %q", got)
	}
}
