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
		{"echo gh pr merge", true},

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

		// A read of GitHub, or a word that only looks like a verb, stays allowed. A quoted word or a graphql call,
		// which can hold a mutation, goes to the Ask.
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
		{"gh api graphql -f query='query { viewer { login } }'", true},
		{"git log --grep merge", false},
		{"git log --grep='gh pr merge'", true},
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

// A command that names gh or git is let through only as one plain call of a safe subcommand. A miss breaks the
// rule, so every shell trick that builds, hides or chains the call is on the list, and so is each subcommand that
// can run another command or write a ref.
func TestAGhOrGitCommandIsAutoAllowedOnlyAsOnePlainCallOfASafeSubcommand(t *testing.T) {
	for _, c := range []struct {
		command string
		never   bool
	}{
		{"gh pr $'merge' 5", true},
		{"gh $'pr' merge", true},
		{"gh pr merge${IFS}5", true},
		{"$GH pr merge 5", true},
		{"X=gh; $X pr merge 5", true},
		{"g${E}h pr merge 5", true},
		{"GH pr view 5", true},
		{"gh alias set p pr", true},
		{"gh p merge 5", true},
		{"gh pr view 5; gh pr merge 5", true},
		{"gh pr view 5 && gh pr merge 5", true},
		{"gh pr view 5 | sh", true},
		{"gh pr view 5 > out.txt", true},
		{"gh pr view *", true},
		{"gh extension exec x", true},
		{"gh pr ready 5", true},
		{"gh pr edit 5 --base main", true},
		{"gh pr close 5", true},
		{"gh issue create", true},
		{"gh release create v1", true},
		{"gh repo delete o/r", true},
		{"gh run rerun 5", true},
		{"gh auth status", true},
		{"/opt/homebrew/bin/gh pr view 5", true},
		{"./gh pr view 5", true},

		{"git -c alias.m='!gh pr merge' m", true},
		{"git config alias.p '!gh pr merge'", true},
		{"git p", true},
		{"git rebase -x 'gh pr merge' main", true},
		{"git rebase main", true},
		{"git submodule foreach 'gh pr merge'", true},
		{"git bisect run gh pr merge", true},
		{"git merge team-abc-t1", true},
		{"git pull", true},
		{"git remote set-url origin x", true},
		{"git reset --hard origin/main", true},
		{"git cherry-pick abc", true},
		{"git tag v1", true},
		{"git worktree add ../x", true},
		{"git fetch --upload-pack=gh origin", true},
		{"git fetch ext::sh", true},
		{"git --git-dir .git push origin HEAD:main", true},
		{"git --git-dir=.git status", true},
		{"git --work-tree=. status", true},
		{"git --exec-path=/tmp status", true},
		{"git -C sub status", true},
		{"git status | sh", true},
		{"git commit -m wip --exec x", true},
		{"git commit -cx", true},
		{"git log -1 --format=%b --output=out.txt", true},
		{"git log -1 --output out.txt", true},
		{"git diff --out=x", true},
		{"git show --ext-diff HEAD", true},
		{"git fetch --upl=./x .", true},
		{"git fetch --upload=x .", true},
		{"git push --rec=x origin team-abc-t1", true},
		{"git status --git=.", true},
		{"git branch --del x", true},
		{"git branch --mov a b", true},
		{"git branch --forc x main", true},
		{"git branch --set-upstream-to=origin/main", true},
		{"git branch --unset-upstream", true},
		{"git branch -vd x", true},
		{"git log -x", true},
		{"git status alias", true},
		{"git push origin HEAD:heads/main", true},
		{"git push origin HEAD:main", true},
		{"git push origin HEAD", true},
		{"git push origin", true},
		{"git push", true},
		{"git push origin main", true},
		{"git push origin heads/main", true},
		{"git push origin refs/heads/main", true},
		{"git push origin team-abc-t1 main", true},
		{"git push origin :main", true},
		{"git push --delete origin team-abc-t1", true},
		{"git push -d origin team-abc-t1", true},
		{"git push --tags origin", true},
		{"git push --receive-pack=gh origin team-abc-t1", true},
		{"git push https://x/y.git team-abc-t1", true},
		{"git -c remote.origin.push=HEAD:main push origin", true},
		{"git branch -D team-abc-t1", true},
		{"git branch -M main", true},
		{"git branch --delete x", true},
		{"git branch -u origin/main", true},
		{"git commit -m \"$(gh pr merge 5)\"", true},
		{"git commit -m \"`gh pr merge 5`\"", true},
		{"git commit -m 'a'; gh pr merge 5", true},
		{"git commit -m 'a\\'; gh pr merge 5; echo '", true},
		{"git commit -m \"a\\\"; gh pr merge 5; echo \\\"\"", true},
		{"git commit -m \"wip!\"", true},
		{"git log --grep 'x'", true},
		{"git diff @{u}", true},
		{"git add *.go", true},
		{"git add ~/x", true},
		{"git status\ngh pr merge 5", true},

		{"gh api repos/o/r/contents/x -X PUT -f branch=main -f message=x -f content=eA==", true},
		{"gh api repos/o/r/contents/x --method PUT", true},
		{"gh api repos/o/r/contents/x -XPUT", true},
		{"gh api repos/o/r/contents/x --method=PUT", true},
		{"gh api repos/o/r/issues -f title=x", true},
		{"gh api repos/o/r/issues -F title=x", true},
		{"gh api repos/o/r/issues -ftitle=x", true},
		{"gh api repos/o/r/issues --field title=x", true},
		{"gh api repos/o/r/issues --raw-field title=x", true},
		{"gh api repos/o/r/issues --input q.json", true},
		{"gh api repos/o/r/issues -H x-http-method-override:PUT", true},
		{"gh api graphql -F query=@q.graphql", true},
		{"gh api graphql", true},
		{"gh api /graphql", true},
		{"gh api https://api.github.com/graphql", true},
		{"gh api graphql -f query=createCommitOnBranch", true},
		{"gh api graphql -f query=mergeBranch", true},
		{"gh api repos/o/r/pulls/5 -X DELETE", true},
		{"gh api repos/o/r/pulls/5 -X", true},
		{"gh api repos/o/r/pulls/5/merge", true},

		{"echo 'gh pr merge' | sh", true},
		{"echo gh pr merge | sh", true},
		{"find . -exec gh pr merge 5 \\;", true},
		{"find . -exec git push origin HEAD:main \\;", true},
		{"xargs -n1 gh pr merge", true},
		{"env gh pr merge 5", true},
		{"env git status", true},
		{"sh -c 'git status; gh pr merge 5'", true},
		{"/bin/zsh -lc 'gh pr merge 5'", true},
		{"cd sub && git status", true},
		{"nohup git status", true},

		{"git status", false},
		{"git status --short --branch", false},
		{"git diff", false},
		{"git diff --stat", false},
		{"git diff --cached origin/main...HEAD", false},
		{"git diff HEAD~1 -- src/lib.rs", false},
		{"git log --oneline -5", false},
		{"git log --format=%h origin/main..HEAD", false},
		{"git log --grep merge", false},
		{"git show HEAD:README.md", false},
		{"git rev-parse --abbrev-ref HEAD", false},
		{"git ls-files src", false},
		{"git branch", false},
		{"git branch -a", false},
		{"git branch --show-current", false},
		{"git branch -vv", false},
		{"git branch --list --contains abc", false},
		{"git branch --sort=-committerdate", false},
		{"git log --pretty=format:%h --oneline", false},
		{"git diff --output-indicator-new=+", true},
		{"git branch team-abc-t2 main", false},
		{"git add .", false},
		{"git add src/lib.rs src/main.rs", false},
		{"git commit -m wip", false},
		{"git commit -am wip", false},
		{"git commit --amend --no-edit", false},
		{"git commit -m 'fix: gh pr merge; rm -rf /'", false},
		{"git commit -m \"wip: a | b && c\"", false},
		{"git commit -am 'a' -m 'b'", false},
		{"git switch team-abc-t1", false},
		{"git checkout team-abc-t1", false},
		{"git checkout -b team-abc-t2", false},
		{"git restore --staged src", false},
		{"git fetch origin", false},
		{"git fetch origin main", false},
		{"git stash", false},
		{"git stash pop", false},
		{"git merge-base HEAD origin/main", false},
		{"git push origin team-abc-t1", false},
		{"git push -u origin team-abc-t1", false},
		{"git push --set-upstream origin feature/team-abc-t1", false},
		{"gh pr view 5", false},
		{"gh -R o/r pr view 5", false},
		{"gh pr -R o/r view 5", false},
		{"gh pr view 5 --repo o/r", false},
		{"gh pr view 5 --json state,mergeable", false},
		{"gh pr view --web", false},
		{"gh pr checks 5", false},
		{"gh pr checks 5 --watch", false},
		{"gh pr diff 5", false},
		{"gh pr list", false},
		{"gh pr status", false},
		{"gh run list", false},
		{"gh run view 9 --log-failed", false},
		{"gh run watch 9", false},
		{"gh issue view 3", false},
		{"gh issue list", false},
		{"gh repo view", false},
		{"gh api repos/o/r/pulls/5", false},
		{"gh api repos/o/r/pulls/5/comments", false},
		{"gh api repos/o/r/pulls", false},
		{"gh api -X GET repos/o/r/pulls/5", false},
		{"gh api repos/o/r/pulls/5 --method GET", false},
		{"gh api repos/o/r/pulls/5 --method=GET", false},
		{"gh api repos/o/r/pulls/5 -XGET --paginate", false},
		{"gh api repos/o/r/pulls/5 -q .state", false},
		{"/bin/zsh -lc 'git status'", false},
		{"/bin/zsh -lc 'gh pr view 5'", false},
		{"bash -c 'git commit -m \"wip\"'", false},
		{"make test", false},
		{"cargo test --workspace", false},
		{"grep -rn github src", false},
		{"ls .gitignore", false},
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
