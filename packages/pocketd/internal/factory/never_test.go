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

// A miss breaks "no agent merges"; a false alarm only turns an auto-allow into an Ask. So each case where a
// read could be a write is on the list.
func TestTheNeverAllowListCatchesTheWaysAroundIt(t *testing.T) {
	for _, c := range []struct {
		command string
		never   bool
	}{
		{"gh pr -R o/r merge 5", true},
		{"gh pr --repo x merge 5", true},
		{"gh pr --repo=x merge 5", true},
		{"gh --repo x pr merge 5", true},
		{"gh pr merge", true},
		{"gh pr {merge,view} 5", true},
		{`"gh" 'pr' "merge" 5`, true},
		{`g"h" pr merge 5`, true},
		{`\gh pr merge 5`, true},
		{"gh pr -R o/r review 5 --approve", true},
		{"gh pr --repo x create --fill", true},
		{"$GH pr merge 5", true},
		{"gh api -X PUT repos/o/r/pulls/5/merge", true},
		{"gh api --method PUT /repos/o/r/pulls/5/merge -f merge_method=squash", true},
		{"gh api repos/o/r/pulls/5/reviews -f event=APPROVE", true},
		{"gh api repos/o/r/pulls/5/reviews/9/events -f event=APPROVE", true},
		{"gh api repos/o/r/merges -f base=main -f head=x", true},
		{"gh api repos/o/r/pulls -f title=x -f head=x -f base=main", true},
		{"gh api -X POST repos/o/r/pulls", true},
		{"gh api -X PATCH repos/o/r/git/refs/heads/main -f sha=abc", true},
		{`gh api graphql -f query='mutation { mergePullRequest(input:{pullRequestId:"x"}) { clientMutationId } }'`, true},
		{`gh api graphql -f query='mutation { addPullRequestReview(input:{pullRequestId:"x",event:APPROVE}) { clientMutationId } }'`, true},
		{`gh api graphql -f query='mutation { enablePullRequestAutoMerge(input:{pullRequestId:"x"}) { clientMutationId } }'`, true},
		{`sh -c "gh pr merge 5"`, true},
		{`bash -c 'gh pr merge 5'`, true},
		{`bash -lc "cd x && gh pr merge 5"`, true},
		{`sh -c "sh -c 'gh pr merge 5'"`, true},
		{"env FOO=1 gh pr merge", true},
		{"env -i FOO=1 gh pr merge", true},
		{"sudo -u bob gh pr merge 5", true},
		{"xargs gh pr merge", true},
		{"echo 5 | xargs -n1 gh pr merge", true},
		{"time gh pr merge 5", true},
		{"nohup gh pr merge 5 &", true},
		{"command gh pr merge 5", true},
		{"eval gh pr merge 5", true},
		{"cd x && gh pr merge 5", true},
		{"cd x;gh pr merge 5", true},
		{"true || gh pr merge 5", true},
		{"echo $(gh pr merge 5)", true},
		{"echo `gh pr merge 5`", true},
		{"(gh pr merge 5)", true},
		{"{ gh pr merge 5; }", true},
		{"if true; then gh pr merge 5; fi", true},
		{"git status\ngh pr merge 5", true},
		{"env git push --force", true},
		{`sh -c "git push origin main"`, true},
		{"sudo rm -rf /var/x", true},

		// A read of GitHub, or a word that only looks like a verb, stays allowed.
		{"gh pr view 5", false},
		{"gh pr view 5 --json state,mergeable", false},
		{"gh pr checks 5", false},
		{"gh pr -R o/r view 5", false},
		{"gh pr diff 5", false},
		{"gh pr list", false},
		{"gh api repos/o/r/pulls/5", false},
		{"gh api repos/o/r/pulls/5/comments", false},
		{"gh api repos/o/r/pulls", false},
		{"gh api repos/o/r/git/refs/heads/feature", false},
		{"gh api graphql -f query='query { viewer { login } }'", false},
		{"git log --grep merge", false},
		{"git log --grep='gh pr merge'", false},
		{"git status", false},
		{"env FOO=1 make test", false},
		{`sh -c "make test"`, false},
		{"sudo make install", false},
		{"xargs rm -f", false},

		// Left to the Ask: a search for the word is taken for the verb.
		{"gh pr list --search merge", true},
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
