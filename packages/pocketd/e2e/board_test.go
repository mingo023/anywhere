//go:build unix

package e2e

import (
	"os"
	"path/filepath"
	"testing"

	"pocketd/internal/proto"
)

func (p *Phone) Board(verb, id string, fields map[string]any) Message {
	p.t.Helper()
	msg := map[string]any{"type": "board." + verb, "id": id}
	for k, v := range fields {
		msg[k] = v
	}
	p.Send(msg)
	return p.WaitFor("the answer to "+id, func(m Message) bool { return m.ID == id && (m.Type == "ack" || m.Type == "error") })
}

func (p *Phone) WaitTicket(what string, ok func(proto.Ticket) bool) proto.Ticket {
	p.t.Helper()
	var found proto.Ticket
	p.WaitFor(what, func(m Message) bool {
		if m.Type != "board" {
			return false
		}
		for _, t := range m.Tickets {
			if ok(t) {
				found = t
				return true
			}
		}
		return false
	})
	return found
}

func ticketDraft(h *Harness, prompt string) map[string]any {
	return map[string]any{"title": "Review", "prompt": prompt, "provider": "claude", "folder": h.Repo}
}

func TestAStartedTicketRunsInItsOwnWorktreeAndReachesDone(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapBoard)
	if a := o.Board("save", "s1", map[string]any{"ticket": ticketDraft(h, "review the diff"), "start": true}); a.Type != "ack" {
		t.Fatalf("got %s", a.Raw)
	}
	done := o.WaitTicket("a finished ticket", func(x proto.Ticket) bool { return x.Status == proto.TicketDone || x.Status == proto.TicketIdle })
	home, _ := filepath.EvalSymlinks(h.Home)
	if done.State != proto.TicketLive || done.AgentID == "" || filepath.Dir(done.Worktree) != filepath.Join(home, "wt") {
		t.Fatalf("got %+v", done)
	}
	if _, err := os.Stat(filepath.Join(done.Worktree, ".git")); err != nil {
		t.Fatalf("no worktree at %s: %v", done.Worktree, err)
	}
	h.WaitScreen(done.TerminalID, "echo: review the diff")
}

func TestOverTheLimitATicketWaitsInQueued(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapBoard)
	if a := o.Board("limit", "l1", map[string]any{"limit": 1}); a.Type != "ack" {
		t.Fatalf("got %s", a.Raw)
	}
	o.Board("save", "s1", map[string]any{"ticket": ticketDraft(h, "run ls"), "start": true})
	first := o.WaitTicket("the first asking", func(x proto.Ticket) bool { return x.Status == proto.TicketNeedsYou })
	o.Board("save", "s2", map[string]any{"ticket": ticketDraft(h, "hello"), "start": true})
	second := o.WaitTicket("the second waiting", func(x proto.Ticket) bool { return x.ID != first.ID && x.State == proto.TicketQueued })

	if a := o.Board("close", "c1", map[string]any{"ticketId": first.ID}); a.Type != "ack" {
		t.Fatalf("got %s", a.Raw)
	}
	o.WaitTicket("the second started", func(x proto.Ticket) bool { return x.ID == second.ID && x.State == proto.TicketLive })
}

func TestALiveTicketCannotBeDeleted(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapBoard)
	o.Board("save", "s1", map[string]any{"ticket": ticketDraft(h, "run ls"), "start": true})
	x := o.WaitTicket("the ticket live", func(x proto.Ticket) bool { return x.State == proto.TicketLive })
	if e := o.Board("delete", "d1", map[string]any{"ticketId": x.ID}); e.Type != "error" || e.Code != proto.CodeTicketBusy {
		t.Fatalf("got %s", e.Raw)
	}
}

func TestABoardVerbWithoutTheCapIsMalformed(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner()
	if e := o.Board("limit", "l1", map[string]any{"limit": 2}); e.Type != "error" {
		t.Fatalf("got %s", e.Raw)
	}
}
