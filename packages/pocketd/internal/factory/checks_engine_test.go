//go:build unix

package factory

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/board"
	"pocketd/internal/github"
	"pocketd/internal/hub"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/team"
)

// rig is an engine with one live Ticket at Checks, in a worktree with a remote, and the fake gh of internal/github.
type rig struct {
	t                         *testing.T
	e                         *Engine
	teams                     *team.Store
	home, project, tree, fake string
	ticket                    string
	mail                      chan string
}

func newRig(t *testing.T, cfg proto.FactoryConfig) *rig {
	t.Helper()
	for k, v := range map[string]string{"GIT_CONFIG_GLOBAL": "/dev/null", "GIT_AUTHOR_NAME": "T", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "T", "GIT_COMMITTER_EMAIL": "t@t"} {
		t.Setenv(k, v)
	}
	root, _ := filepath.EvalSymlinks(t.TempDir())
	r := &rig{t: t, home: filepath.Join(root, "home"), project: filepath.Join(root, "app"), tree: filepath.Join(root, "app-fix"), fake: filepath.Join(root, "gh"), mail: make(chan string, 16)}
	for _, d := range []string{r.home, r.project, r.fake} {
		os.MkdirAll(d, 0o755)
	}
	gitIn(t, r.project, "init", "-q", "-b", "main")
	gitIn(t, r.project, "commit", "-q", "--allow-empty", "-m", "first")
	gitIn(t, root, "init", "-q", "--bare", "origin.git")
	gitIn(t, r.project, "remote", "add", "origin", filepath.Join(root, "origin.git"))
	gitIn(t, r.project, "worktree", "add", "-q", "-b", "fix", r.tree)
	gitIn(t, r.tree, "commit", "-q", "--allow-empty", "-m", "Fix it")
	reg := filepath.Join(root, "desktop.json")
	os.WriteFile(reg, []byte(fmt.Sprintf(`{"projects":[%q]}`, r.project)), 0o600)

	r.put("version", "gh version 2.81.0 (2025-10-01)\n")
	r.put("auth", "github.com\n  ✓ Logged in to github.com account t (keyring)\n  - Active account: true\n")
	r.put("nopr", "")
	r.put("created", "https://github.com/acme/app/pull/57\n")
	r.put("threads.json", `{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[]}}}}}`)
	r.view(passedCI)
	bin, _ := filepath.Abs(filepath.Join("..", "github", "testdata", "fakegh"))
	env := func(string) []string {
		return []string{"PATH=" + bin + ":/usr/bin:/bin", "FAKEGH=" + r.fake, "HOME=" + r.fake, "GIT_CONFIG_GLOBAL=/dev/null", "GH_TOKEN=tok"}
	}

	store := board.Open(r.home)
	saved, err := store.Save(proto.Ticket{Title: "Fix it", Prompt: "Fix it.", Provider: "claude", Folder: r.project}, false, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	r.ticket = saved.ID
	store.Update(saved.ID, func(x *proto.Ticket) {
		x.State, x.Status, x.Station, x.AgentID, x.Worktree = proto.TicketLive, proto.TicketWorking, proto.StationChecks, leadID, r.tree
	})
	configs := OpenConfigs(r.home)
	full := proto.DefaultFactoryConfig()
	full.Enabled, full.Fix, full.Checks = true, cfg.Fix, cfg.Checks
	if err := configs.Save(r.project, full); err != nil {
		t.Fatal(err)
	}
	h := hub.New()
	agents := agent.NewRegistry(h)
	agents.Add(leadID, r.tree, "claude", &chat{})
	r.teams, err = team.OpenStore(r.home)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { r.teams.Close() })
	r.e = &Engine{
		Home: r.home, Board: store, Configs: configs, Agents: agents, Now: time.Now, Poll: time.Millisecond, Profiles: OpenProfiles(r.home),
		Teams:  &team.Teams{Store: r.teams, Agents: agents, Hub: h, Poll: time.Millisecond},
		Mail:   func(_, text string) error { r.mail <- text; return nil },
		GitHub: github.New(env, registry.New(reg)),
		Env:    env,
	}
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	go r.e.Run(ctx)
	return r
}

func (r *rig) put(name, content string) {
	r.t.Helper()
	if err := os.WriteFile(filepath.Join(r.fake, name), []byte(content), 0o644); err != nil {
		r.t.Fatal(err)
	}
}

const passedCI = `[{"__typename":"CheckRun","name":"ci","status":"COMPLETED","conclusion":"SUCCESS"}]`

// view sets what gh pr view reports for the open PR: its checks, on the commit of the checkout.
func (r *rig) view(rollup string) {
	r.put("view.json", `{"isDraft":true,"number":57,"state":"OPEN","title":"Fix it","url":"https://github.com/acme/app/pull/57","headRefOid":"@HEAD@","baseRefName":"main","mergeable":"MERGEABLE","statusCheckRollup":`+rollup+`}`)
}

func (r *rig) called(prefix string) bool {
	raw, _ := os.ReadFile(filepath.Join(r.fake, "calls"))
	for _, c := range strings.Split(strings.TrimSpace(string(raw)), "\n") {
		if _, args, _ := strings.Cut(c, " "); strings.HasPrefix(args, prefix) {
			return true
		}
	}
	return false
}

func (r *rig) get() proto.Ticket {
	tickets, _ := r.e.Board.Snapshot()
	for _, x := range tickets {
		if x.ID == r.ticket {
			return x
		}
	}
	r.t.Fatal("the ticket is gone")
	return proto.Ticket{}
}

func (r *rig) until(what string, ok func(proto.Ticket) bool) proto.Ticket {
	r.t.Helper()
	for end := time.Now().Add(15 * time.Second); time.Now().Before(end); time.Sleep(10 * time.Millisecond) {
		if x := r.get(); ok(x) {
			return x
		}
	}
	r.t.Fatalf("timed out waiting for %s: %+v", what, r.get())
	return proto.Ticket{}
}

func TestATicketAtChecksGetsADraftPRWithItsSpecAndItsKey(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Enabled: true, Fix: 2})
	d := dir(r.home, r.ticket)
	os.MkdirAll(d, 0o700)
	os.WriteFile(filepath.Join(d, "spec.md"), []byte("## Brief\nAdd the flag.\n\n## Acceptance criteria\n- [ ] The flag exists\n\n## Risks\n- None\n"), 0o600)
	r.e.Recover()
	got := r.until("the draft PR", func(x proto.Ticket) bool { return x.PR == 57 })
	if !r.called("pr create --title Fix it --body-file - --draft") {
		t.Fatal("gh pr create was not called for a draft with the ticket's title")
	}
	body, _ := os.ReadFile(filepath.Join(r.fake, "body"))
	if want := fmt.Sprintf("## Brief\nAdd the flag.\n\n## Acceptance criteria\n- [ ] The flag exists\n\nTicket APP-%d", got.Number); string(body) != want {
		t.Fatalf("body\n%s\nwant\n%s", body, want)
	}
	if remote, local := gitIn(t, filepath.Join(filepath.Dir(r.tree), "origin.git"), "rev-parse", "fix"), gitIn(t, r.tree, "rev-parse", "HEAD"); remote != local {
		t.Fatalf("the remote has %s, the checkout has %s", remote, local)
	}
	events, _ := ReadActivity(r.home, r.ticket)
	if !slices.ContainsFunc(events, func(ev proto.FactoryEvent) bool {
		return ev.Kind == "check" && ev.Actor == "anywhere" && ev.Text == "Opened draft PR #57"
	}) {
		t.Fatalf("activity %+v", events)
	}
}

func TestAMergeConflictFailsTheTicketWithTheFilesAndUsesNoAttempt(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Fix: 2})
	tm := proto.Team{Lead: leadID, State: "conflict", Conflict: []string{"a.rs", "b.rs"}, Tasks: make([]proto.TeamTask, 2)}
	if err := r.teams.Put(tm); err != nil {
		t.Fatal(err)
	}
	r.e.TeamChanged(tm)
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Station != proto.StationChecks || got.Why != "Conflict in a.rs, b.rs" || got.Fix != 0 || got.PR != 0 {
		t.Fatalf("got %+v", got)
	}
	if r.called("pr create") {
		t.Fatal("a PR was opened for a branch that did not merge")
	}
}

func TestAPushThatFailsFailsTheTicketWithoutAnAttempt(t *testing.T) {
	r := newRig(t, proto.FactoryConfig{Enabled: true, Fix: 2})
	gitIn(t, r.project, "remote", "set-url", "origin", filepath.Join(r.home, "nowhere.git"))
	r.e.Recover()
	got := r.until("the failure", func(x proto.Ticket) bool { return x.Status == proto.TicketFailed })
	if got.Station != proto.StationChecks || got.Fix != 0 || !strings.HasPrefix(got.Why, "Couldn't open the pull request") {
		t.Fatalf("got %+v", got)
	}
}
