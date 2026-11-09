//go:build unix

package e2e

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

// gitIdentity lets pocketd commit and merge with no git config.
func gitIdentity(h *Harness) {
	h.Env = append(h.Env, "GIT_AUTHOR_NAME=e2e", "GIT_AUTHOR_EMAIL=e2e@x", "GIT_COMMITTER_NAME=e2e", "GIT_COMMITTER_EMAIL=e2e@x")
}

func gitOut(t *testing.T, dir string, args ...string) string {
	t.Helper()
	out, err := exec.Command("git", append([]string{"-C", dir}, args...)...).CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
	return strings.TrimSpace(string(out))
}

func taskIn(lead, id, state string) func(Message) bool {
	return func(m Message) bool {
		if m.Type != "team.update" || m.Team.Lead != lead {
			return false
		}
		for _, task := range m.Team.Tasks {
			if task.ID == id {
				return task.State == state
			}
		}
		return false
	}
}

func replyTo(id string) func(Message) bool {
	return func(m Message) bool { return m.ID == id && (m.Type == "ack" || m.Type == "error") }
}

// teamPlan is the lead's team prompt. Fake claude echoes it, so its block is
// the lead's plan: t1 writes a.txt and runs `make test`, which its allowlist
// lets through; t2 starts from t1's work and writes b.txt.
func teamPlan(t *testing.T) string {
	t.Helper()
	plan, err := json.Marshal(map[string]any{"tasks": []map[string]any{
		{"id": "t1", "title": "Write a", "agent": "claude", "files": []string{"a.txt"}, "after": []string{}, "allow": []string{"make test"},
			"instructions": "write a.txt one\nrun make test\nReady to merge"},
		{"id": "t2", "title": "Write b", "agent": "claude", "files": []string{"b.txt"}, "after": []string{"t1"}, "allow": []string{},
			"instructions": "write b.txt two\nReady to merge"},
	}})
	if err != nil {
		t.Fatal(err)
	}
	return "<anywhere_team_plan>\n" + string(plan) + "\n</anywhere_team_plan>"
}

// reviewedTeam has a chat lead plan teamPlan, starts it, and returns the team
// once both workers handed back and the lead said it is ready to merge.
func reviewedTeam(t *testing.T) (h *Harness, o *Phone, lead string, team proto.Team) {
	t.Helper()
	h = Start(t, launchReady(""), gitIdentity)
	o = h.Owner("names.v1")
	create(o, "lead", inRepo(h, map[string]any{"chat": true}))
	created := reply(o, "agent.created", "error")
	if created.Type != "agent.created" {
		t.Fatalf("got %s", created.Raw)
	}
	lead = created.AgentID
	h.WaitScreen(created.TerminalID, "fake claude ready")
	o.Send(map[string]any{"type": "agent.prompt", "id": "plan", "agentId": lead, "text": teamPlan(t), "team": true})
	o.WaitFor("the proposed plan", teamOf(lead, "proposed"))
	o.Send(map[string]any{"type": "team.start", "id": "start", "lead": lead, "tasks": []map[string]any{
		{"id": "t1", "agent": "claude", "allow": []string{"make test"}},
		{"id": "t2", "agent": "claude", "allow": []string{}},
	}})
	if m := o.WaitFor("team.start's reply", replyTo("start")); m.Type != "ack" {
		t.Fatalf("team.start: %s", m.Raw)
	}
	asked, noticed, named := false, false, false
	t1Tree := filepath.Join(filepath.Dir(h.Repo), "wt", "team-"+lead[:6]+"-t1")
	o.WaitFor("t1's handback", func(m Message) bool {
		asked = asked || m.Type == "permission.request"
		noticed = noticed || m.Type == "agent.stream" && m.Item.Kind == "notice" && m.Item.Text == "Allowed by the team plan: make test"
		named = named || m.Type == "worktree.names" && m.Names[t1Tree] == "Write a"
		return taskIn(lead, "t1", "done")(m)
	})
	if asked || !noticed || !named {
		t.Fatalf("make test: asked the user %t, noted in the timeline %t; t1's worktree named for its task %t", asked, noticed, named)
	}
	o.WaitFor("the review", teamOf(lead, "review"))
	team = o.WaitFor("the lead's verdict", func(m Message) bool { return teamOf(lead, "review")(m) && m.Team.Ready }).Team
	return h, o, lead, team
}

func TestATeamsWorkersHandBackInOrderAndTheLeadReviews(t *testing.T) {
	h, _, lead, team := reviewedTeam(t)
	b1, b2 := "team-"+lead[:6]+"-t1", "team-"+lead[:6]+"-t2"
	t1, t2 := team.Tasks[0], team.Tasks[1]
	trees := filepath.Join(filepath.Dir(h.Repo), "wt")
	if t1.Branch != b1 || t2.Branch != b2 || t1.Worktree != filepath.Join(trees, b1) || t2.Worktree != filepath.Join(trees, b2) || t1.Worker == "" || t2.Worker == "" {
		t.Fatalf("t1 %+v\nt2 %+v", t1, t2)
	}
	if !strings.Contains(t1.Stat, "a.txt | 1 +") || !strings.Contains(t2.Stat, "b.txt | 1 +") || strings.Contains(t2.Stat, "a.txt") {
		t.Fatalf("stats:\n%s\n%s", t1.Stat, t2.Stat)
	}
	if !strings.HasPrefix(t1.Summary, `echo: <anywhere_team_assignment task="t1">`) {
		t.Fatalf("summary %q", t1.Summary)
	}
	if got := gitOut(t, h.Repo, "log", "-1", "--format=%s", b1); got != "team t1: Write a" {
		t.Fatalf("t1's commit %q", got)
	}
	if got := gitOut(t, h.Repo, "show", b2+":a.txt"); got != "one" {
		t.Fatalf("t2 didn't start from t1's work: %q", got)
	}
}

func TestATeamNotesWhatTheUserToldAWorkerAndMergesIntoTheLeadsCheckout(t *testing.T) {
	h, o, lead, team := reviewedTeam(t)
	o.Prompt(team.Tasks[0].Worker, "check again")
	o.WaitAll("the note and the second handback",
		streamOf(lead, "user", `<anywhere_team_note task="t1">The user told worker t1: "check again"</anywhere_team_note>`),
		func(m Message) bool {
			return taskIn(lead, "t1", "done")(m) && m.Team.Tasks[0].Summary == "echo: check again"
		},
	)
	o.Send(map[string]any{"type": "team.merge", "id": "merge", "lead": lead})
	if m := o.WaitFor("team.merge's reply", replyTo("merge")); m.Type != "ack" {
		t.Fatalf("team.merge: %s", m.Raw)
	}
	merged := o.WaitFor("the merge", teamOf(lead, "merged")).Team
	for _, task := range merged.Tasks {
		if task.State != "merged" {
			t.Fatalf("%s is %s", task.ID, task.State)
		}
	}
	for name, want := range map[string]string{"a.txt": "one\n", "b.txt": "two\n"} {
		if got, err := os.ReadFile(filepath.Join(h.Repo, name)); err != nil || string(got) != want {
			t.Fatalf("%s: %q, %v", name, got, err)
		}
	}
	b1, b2 := "team-"+lead[:6]+"-t1", "team-"+lead[:6]+"-t2"
	if got := gitOut(t, h.Repo, "log", "--merges", "--format=%s"); got != "Merge "+b2+"\nMerge "+b1 {
		t.Fatalf("merges %q", got)
	}
	if got := gitOut(t, h.Repo, "worktree", "list", "--porcelain"); strings.Count(got, "worktree ") != 1 {
		t.Fatalf("worktrees left:\n%s", got)
	}
	if got := gitOut(t, h.Repo, "branch", "--list", "team-*", "--format=%(refname:short)"); got != b1+"\n"+b2 {
		t.Fatalf("branches %q", got)
	}
}
