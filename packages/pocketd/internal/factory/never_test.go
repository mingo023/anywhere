package factory

import (
	"testing"

	"pocketd/internal/proto"
)

func TestTheNeverAllowListCatchesEachRuleAndNothingBesideIt(t *testing.T) {
	for _, c := range []struct {
		command string
		never   bool
	}{
		{"gh pr merge 7 --squash", true},
		{"gh pr review 7 --approve", true},
		{"gh pr create --fill", true},
		{"gh -R owner/repo pr merge 7", true},
		{"/opt/homebrew/bin/gh pr merge 7", true},
		{"GH_TOKEN=x gh pr merge 7", true},
		{"cd sub && gh pr merge 7", true},
		{"make test; gh pr create", true},
		{"/bin/zsh -lc 'gh pr merge 7'", true},
		{"gh pr view 7", false},
		{"gh pr checks 7 --watch", false},
		{"gh run list", false},
		{"echo gh pr merge", false},

		{"git push --force", true},
		{"git push --force-with-lease origin team-abc-t1", true},
		{"git push -f origin team-abc-t1", true},
		{"git push -uf origin team-abc-t1", true},
		{"git push origin +team-abc-t1", true},
		{"git push --all", true},
		{"git push --mirror origin", true},
		{"git push origin main", true},
		{"git push origin HEAD:main", true},
		{"git push origin team-abc-t1:refs/heads/main", true},
		{"git -C sub push origin main", true},
		{"git push origin team-abc-t1", false},
		{"git push -u origin team-abc-t1", false},
		{"git push origin maintenance", false},
		{"git status", false},
		{"git commit -m 'push --force later'", false},

		{"rm -rf /tmp/build", true},
		{"rm -rf ~/Library", true},
		{"rm -rf $HOME/x", true},
		{"rm -rf ../other", true},
		{"rm -fr sub/../../other", true},
		{"rm -r -f /var/x", true},
		{"rm --recursive --force /var/x", true},
		{"rm -rf build", false},
		{"rm -rf ./node_modules sub/dist", false},
		{"rm -r /tmp/build", false},
		{"rm -f /tmp/file", false},
		{"rm notes..txt", false},
	} {
		if got := NeverAllowed(c.command, "main"); got != c.never {
			t.Errorf("NeverAllowed(%q) = %t, want %t", c.command, got, c.never)
		}
	}
}

func TestAPushToTheBaseBranchIsReadFromTheTicketsBase(t *testing.T) {
	if NeverAllowed("git push origin main", "develop") || !NeverAllowed("git push origin develop", "develop") {
		t.Fatal("the base branch was not taken from the ticket")
	}
	if NeverAllowed("git push origin main", "") {
		t.Fatal("with no base, a plain push was refused")
	}
}

func TestAnAllowlistPrefixOnTheListIsRefused(t *testing.T) {
	if prefix, refused := RefusedPrefix([]string{"make test", "gh pr merge", "git push --force"}); !refused || prefix != "gh pr merge" {
		t.Fatalf("%q %t", prefix, refused)
	}
	if prefix, refused := RefusedPrefix([]string{"gh", "git push", "rm -rf build", "make test"}); refused {
		t.Fatalf("%q was refused", prefix)
	}
}

func TestASessionBelongsToTheTicketItLeadsOrWorksFor(t *testing.T) {
	tickets := []proto.Ticket{
		{ID: "x1", AgentID: "lead-1", State: proto.TicketLive, Station: proto.StationImplement},
		{ID: "x2", AgentID: "lead-2", State: proto.TicketLive},
		{ID: "x3", AgentID: "lead-3", State: proto.TicketClosed, Station: proto.StationImplement},
	}
	teams := []proto.Team{
		{Lead: "lead-1", State: "running", Tasks: []proto.TeamTask{{ID: "t1", Worker: "w1", State: "running"}}},
		{Lead: "lead-2", State: "running", Tasks: []proto.TeamTask{{ID: "t1", Worker: "w2", State: "running"}}},
	}
	for agent, want := range map[string]string{"lead-1": "x1", "w1": "x1", "lead-2": "", "w2": "", "lead-3": "", "stranger": "", "": ""} {
		got, ok := Owner(agent, tickets, teams)
		if got.ID != want || ok != (want != "") {
			t.Errorf("%q: %q %t, want %q", agent, got.ID, ok, want)
		}
	}
}

func TestAFactoryLeadsMergeCommandIsNeverAutoAllowed(t *testing.T) {
	w := newWorld(t)
	w.gated()
	ask := func(agent, command string) proto.PermissionRequest {
		return proto.PermissionRequest{AgentID: agent, ToolName: "Bash", Detail: proto.ToolDetail{Kind: "shell", Command: command}}
	}
	if !w.e.Never(ask(leadID, "gh pr merge 7")) || !w.e.Never(ask(leadID, "git push origin main")) || !w.e.Never(ask(leadID, "git push origin master")) {
		t.Fatal("a never-allow command of the lead passed")
	}
	if w.e.Never(ask(leadID, "make test")) || w.e.Never(ask("stranger", "gh pr merge 7")) || w.e.Never(proto.PermissionRequest{AgentID: leadID}) {
		t.Fatal("a command off the list, or of a session outside the Factory, was held")
	}
	if !w.e.Runs(leadID) || w.e.Runs("stranger") {
		t.Fatal("Runs")
	}
	var off *Engine
	if off.Never(ask(leadID, "gh pr merge 7")) || off.Runs(leadID) {
		t.Fatal("a nil engine held a command")
	}
}
