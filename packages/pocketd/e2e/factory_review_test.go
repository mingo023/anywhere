//go:build unix

package e2e

import (
	"os"
	"path/filepath"
	"slices"
	"testing"

	"pocketd/internal/proto"
)

func reviewed() proto.FactoryConfig {
	cfg := proto.DefaultFactoryConfig()
	cfg.Enabled, cfg.Review = true, "claude-sonnet"
	return cfg
}

func TestAReviewerThatTriesToWriteIsRefusedAndItsFindingsAreSaved(t *testing.T) {
	h, o, gh, ticket := factoryAtPR(t, reviewed())
	var d proto.FactoryTicket
	h.eventually("the reviewer's findings", func() bool {
		d = factoryDetail(o, "review", ticket.ID)
		return len(d.Findings) > 0
	})
	if f := d.Findings[0]; f.Severity != "critical" || f.File != "path/from/repo/root" || f.Line != 12 {
		t.Fatalf("got %+v", d.Findings)
	}
	said := func(actor, kind, text string) bool {
		return slices.ContainsFunc(d.Activity, func(ev proto.FactoryEvent) bool {
			return ev.Actor == actor && ev.Kind == kind && (text == "" || ev.Text == text)
		})
	}
	if !said("reviewer", "denied", "Refused to write "+filepath.Join(ticket.Worktree, "reviewer-wrote.txt")) || !said("reviewer", "denied", "Refused: gh pr merge 57 --squash") || !said("reviewer", "finding", "") {
		t.Fatalf("activity %+v", d.Activity)
	}
	if said("reviewer", "denied", "Refused: git diff main") || said("reviewer", "asked", "") {
		t.Fatalf("a read-only command was refused or shown to you: %+v", d.Activity)
	}
	if gh.called("pr merge") {
		t.Fatal("the reviewer merged")
	}
	if _, err := os.Stat(filepath.Join(ticket.Worktree, "reviewer-wrote.txt")); !os.IsNotExist(err) {
		t.Fatalf("the reviewer wrote a file: %v", err)
	}
}

func TestAReviewedTicketWaitsAtThePRGateAndClosesWhenYouMerge(t *testing.T) {
	h, o, gh, ticket := factoryAtPR(t, reviewed())
	gate := o.WaitTicket("the PR review gate", func(x proto.Ticket) bool { return x.ID == ticket.ID && x.Gate == proto.GatePR })
	if gate.Station != proto.StationPR || gate.Status != proto.TicketNeedsYou || gate.PR == 0 {
		t.Fatalf("got %+v", gate)
	}
	if d := factoryDetail(o, "gate", ticket.ID); len(d.Findings) != 1 || d.Findings[0].Severity != "critical" {
		t.Fatalf("findings %+v", d.Findings)
	}
	h.eventually("gh pr ready", func() bool { return gh.called("pr ready") })
	if gh.called("pr review") || gh.called("pr merge") {
		t.Fatal("the pipeline reviewed or merged on GitHub")
	}

	// What GitHub says once the merge is made.
	gh.put("view.json", `{"isDraft":false,"number":57,"state":"MERGED","title":"Review","url":"https://github.com/acme/app/pull/57","mergeable":"MERGEABLE","headRefOid":"@HEAD@","baseRefName":"main","statusCheckRollup":[]}`)
	o.Send(map[string]any{"type": "gh.pr.merge", "id": "m1", "tree": gate.Worktree, "number": gate.PR, "method": "squash"})
	if m := o.WaitFor("the merge's reply", replyTo("m1")); m.Type != "ack" {
		t.Fatalf("gh.pr.merge: %s", m.Raw)
	}
	closed := o.WaitTicket("the merged ticket", func(x proto.Ticket) bool { return x.ID == ticket.ID && x.State == proto.TicketClosed })
	if closed.ClosedWhy != proto.ClosedMerged || closed.Station != proto.StationMerged || closed.Gate != "" {
		t.Fatalf("got %+v", closed)
	}
	h.eventually("the merge in the activity", func() bool {
		d := factoryDetail(o, "after", ticket.ID)
		last := d.Activity[len(d.Activity)-1]
		return last.Kind == "merged" && last.Actor == "you" && last.Text == "Merged"
	})
	if _, err := os.Stat(filepath.Join(closed.Worktree, ".git")); err != nil {
		t.Fatalf("the merge removed the worktree: %v", err)
	}
}
