//go:build unix

package e2e

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func TestAHintForAnUnknownTicketIsRefusedByItsCode(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapBoard, proto.CapTeam, proto.CapFactory)
	o.Send(map[string]any{"type": "factory.hint", "id": "hint", "ticketId": "nope", "text": "try again"})
	if m := o.WaitFor("the reply", replyTo("hint")); m.Type != "error" || m.Code != "unknown_ticket" {
		t.Fatalf("got %s", m.Raw)
	}
}

// withOrigin gives the harness's Repo a bare remote named origin.
func withOrigin(h *Harness) {
	origin := filepath.Join(h.Home, "origin.git")
	for _, args := range [][]string{{"init", "-q", "--bare", origin}, {"-C", h.Repo, "remote", "add", "origin", origin}} {
		if out, err := exec.Command("git", args...).CombinedOutput(); err != nil {
			h.t.Fatalf("git %v: %s", args, out)
		}
	}
}

// ghFolder is the folder the fake gh of fakeGh answers from.
type ghFolder struct {
	t   *testing.T
	dir string
}

func (g *ghFolder) put(name, content string) {
	g.t.Helper()
	if err := os.WriteFile(filepath.Join(g.dir, name), []byte(content), 0o644); err != nil {
		g.t.Fatal(err)
	}
}

func (g *ghFolder) called(prefix string) bool {
	raw, _ := os.ReadFile(filepath.Join(g.dir, "calls"))
	for _, line := range strings.Split(strings.TrimSpace(string(raw)), "\n") {
		if _, args, _ := strings.Cut(line, " "); strings.HasPrefix(args, prefix) {
			return true
		}
	}
	return false
}

// factoryAtChecks turns the Factory on with cfg, starts a Ticket whose lead plans teamPlan, approves the plan, and
// returns once the Ticket is at Checks. The fake gh has no PR until pocketd opens #57, whose one check passes on the
// commit of the checkout.
func factoryAtChecks(t *testing.T, cfg proto.FactoryConfig) (*Harness, *Phone, *ghFolder, proto.Ticket) {
	t.Helper()
	h := Start(t, launchReady(""), gitIdentity, fakeGh, withOrigin)
	o := h.Owner(proto.CapBoard, proto.CapTeam, proto.CapFactory, proto.CapGitHub)
	gh := &ghFolder{t: t, dir: filepath.Join(h.Home, "gh")}
	gh.put("nopr", "")
	gh.put("created", "https://github.com/acme/app/pull/57\n")
	gh.put("view.json", `{"isDraft":true,"number":57,"state":"OPEN","title":"Review","url":"https://github.com/acme/app/pull/57","mergeable":"MERGEABLE","headRefOid":"@HEAD@","baseRefName":"main","statusCheckRollup":[{"__typename":"CheckRun","name":"ci","status":"COMPLETED","conclusion":"SUCCESS"}]}`)
	factoryOn(h, o, func(c *proto.FactoryConfig) { *c = cfg; c.Enabled = true })
	saved := o.Board("save", "s1", map[string]any{"ticket": ticketDraft(h, teamPlan(t)), "start": true})
	if saved.Type != "ack" {
		t.Fatalf("board.save: %s", saved.Raw)
	}
	gate := o.WaitTicket("the plan gate", func(x proto.Ticket) bool { return x.Gate == proto.GatePlan })
	o.Send(map[string]any{"type": "team.start", "id": "start", "lead": gate.AgentID, "tasks": []map[string]any{
		{"id": "t1", "agent": "claude", "allow": []string{"make test"}},
		{"id": "t2", "agent": "claude", "allow": []string{}},
	}})
	if m := o.WaitFor("team.start's reply", replyTo("start")); m.Type != "ack" {
		t.Fatalf("team.start: %s", m.Raw)
	}
	ticket := o.WaitTicket("Checks", func(x proto.Ticket) bool { return x.ID == gate.ID && x.Station == proto.StationChecks })
	return h, o, gh, ticket
}

// factoryAtPR is factoryAtChecks, returned once the Ticket passed Checks.
func factoryAtPR(t *testing.T, cfg proto.FactoryConfig) (*Harness, *Phone, *ghFolder, proto.Ticket) {
	t.Helper()
	h, o, gh, ticket := factoryAtChecks(t, cfg)
	ticket = o.WaitTicket("PR review", func(x proto.Ticket) bool { return x.ID == ticket.ID && x.Station == proto.StationPR })
	return h, o, gh, ticket
}

func TestAFailingCheckIsRepairedByTheLeadOnTheFirstAttempt(t *testing.T) {
	cfg := proto.DefaultFactoryConfig()
	cfg.Fix, cfg.Checks = 2, []string{`test -f fixed.txt || { echo "write fixed.txt ok"; exit 1; }`}
	_, o, gh, done := factoryAtPR(t, cfg)
	if done.Fix != 1 || done.PR != 57 || done.Status != proto.TicketWorking || done.Hint {
		t.Fatalf("got %+v", done)
	}
	if raw, _ := os.ReadFile(filepath.Join(done.Worktree, "fixed.txt")); string(raw) != "ok\n" {
		t.Fatalf("fixed.txt: %q", raw)
	}
	for _, f := range []string{"a.txt", "b.txt"} {
		if _, err := os.Stat(filepath.Join(done.Worktree, f)); err != nil {
			t.Fatalf("the worker branches were not merged: %v", err)
		}
	}
	if msg := gitOut(t, done.Worktree, "log", "-1", "--format=%s"); msg != "Fix the failed checks" {
		t.Fatalf("the last commit is %q", msg)
	}
	if !gh.called("pr create --title Review --body-file - --draft") {
		t.Fatal("no draft PR was opened")
	}
	d := factoryDetail(o, "d1", done.ID)
	if len(d.Checks) != 1 || d.Checks[0].Exit != 0 || d.Checks[0].Attempt != 1 {
		t.Fatalf("checks %+v", d.Checks)
	}
	got := strings.Join(activityOf(d), " ")
	for _, want := range []string{"anywhere:moved@checks", "anywhere:check@checks", "anywhere:fix@checks", "lead:fix@checks", "anywhere:moved@pr"} {
		if !strings.Contains(got, want) {
			t.Fatalf("activity %s lacks %s", got, want)
		}
	}
}

func TestAfterTheLastAttemptAHintGrantsOneMoreAndItsFixPasses(t *testing.T) {
	cfg := proto.DefaultFactoryConfig()
	cfg.Fix, cfg.Checks = 2, []string{`test -f hinted.txt || { echo "write tried.txt $$"; exit 1; }`}
	_, o, _, ticket := factoryAtChecks(t, cfg)
	failed := o.WaitTicket("the failure", func(x proto.Ticket) bool { return x.ID == ticket.ID && x.Status == proto.TicketFailed })
	if failed.Station != proto.StationChecks || failed.Fix != 2 || !strings.HasSuffix(failed.Why, " · Auto-fix 2 of 2 used") {
		t.Fatalf("got %+v", failed)
	}
	o.Send(map[string]any{"type": "factory.hint", "id": "hint", "ticketId": ticket.ID, "text": "write hinted.txt ok"})
	if m := o.WaitFor("the hint's reply", replyTo("hint")); m.Type != "ack" {
		t.Fatalf("factory.hint: %s", m.Raw)
	}
	done := o.WaitTicket("PR review", func(x proto.Ticket) bool { return x.ID == ticket.ID && x.Station == proto.StationPR })
	if done.Fix != 2 || done.Hint || done.Status != proto.TicketWorking {
		t.Fatalf("got %+v", done)
	}
	if raw, _ := os.ReadFile(filepath.Join(done.Worktree, "hinted.txt")); string(raw) != "ok\n" {
		t.Fatalf("hinted.txt: %q", raw)
	}
	if got := strings.Join(activityOf(factoryDetail(o, "d1", ticket.ID)), " "); !strings.Contains(got, "you:hint@checks") {
		t.Fatalf("activity %s", got)
	}
}
