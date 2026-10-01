package daemon

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/agent"
	"pocketd/internal/hub"
	"pocketd/internal/registry"
	"pocketd/internal/terminal"
)

func gitIn(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	cmd.Env = append(os.Environ(), "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_NOSYSTEM=1", "GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@t", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@t")
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
}

// pocketProject registers a repo named "Pocket" with d and returns its linked
// Worktree calm-otter, at its real path as git reports it.
func pocketProject(t *testing.T, d *Daemon) string {
	dir, _ := filepath.EvalSymlinks(t.TempDir())
	p, linked := filepath.Join(dir, "pocket"), filepath.Join(dir, "calm-otter")
	os.Mkdir(p, 0o755)
	gitIn(t, p, "init", "-q", "-b", "main")
	gitIn(t, p, "commit", "-q", "--allow-empty", "-m", "init")
	gitIn(t, p, "worktree", "add", "-q", "-b", "calm-otter", linked)
	os.Mkdir(filepath.Join(linked, "sub"), 0o755)
	raw, _ := json.Marshal(registry.File{Projects: []string{p}, Repos: map[string]registry.Repo{p: {Name: "Pocket"}}})
	path := filepath.Join(dir, "desktop.json")
	os.WriteFile(path, raw, 0o600)
	d.Registry = registry.New(path)
	return linked
}

func claudeAt(t *testing.T, d *Daemon, cwd, origin string) *terminal.Terminal {
	t.Helper()
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "claude"), Cwd: cwd, Env: []string{"PATH=/bin:/usr/bin"}, Origin: origin})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	return term
}

func TestAnAgentInALinkedWorktreeIsPlacedInItsProject(t *testing.T) {
	d := newDaemon(t)
	linked := pocketProject(t, d)
	a := waitAgent(t, d, claudeAt(t, d, filepath.Join(linked, "sub"), ""))
	if a.Project != "Pocket" || a.Worktree != "calm-otter" || a.Branch != "calm-otter" || a.MainWorktree || a.Origin != "desktop" {
		t.Fatalf("%+v", a)
	}
}

func TestAnAgentOutsideEveryProjectHasNoPlace(t *testing.T) {
	d := newDaemon(t)
	pocketProject(t, d)
	a := waitAgent(t, d, claudeAt(t, d, t.TempDir(), ""))
	if a.Project != "" || a.Worktree != "" || a.Branch != "" || a.MainWorktree {
		t.Fatalf("%+v", a)
	}
}

func TestTheBranchIsReadAgainWhenATurnEnds(t *testing.T) {
	d := newDaemon(t)
	linked := pocketProject(t, d)
	term := claudeAt(t, d, linked, "")
	waitAgent(t, d, term)
	gitIn(t, linked, "checkout", "-q", "-b", "renamed")
	hookFrom(d, d.presentIn(term.Info().ID), `{"hook_event_name":"Stop"}`)
	if a, _ := agentIn(d, term); a.Branch != "renamed" {
		t.Fatalf("%+v", a)
	}
}

func TestTheTerminalsOriginGoesToItsFirstAgent(t *testing.T) {
	d := newDaemon(t)
	if a := waitAgent(t, d, claudeAt(t, d, t.TempDir(), "phone")); a.Origin != "phone" {
		t.Fatalf("%+v", a)
	}
}

func TestClaudeReportsTheContextItAnsweredFrom(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	path := transcript(t, "hi")
	hookFrom(d, pr, sessionStart("s1", path))
	appendLine(t, path, `{"type":"assistant","message":{"model":"claude-opus-5-5","content":[],"usage":{"input_tokens":3,"cache_creation_input_tokens":200,"cache_read_input_tokens":40000,"output_tokens":900}}}`)
	eventually(t, "tokens", func() bool {
		s := pr.a.Summary()
		return s.TokensUsed == 40203 && s.ContextWindow == 1_000_000
	})
}

func TestAResumedTranscriptReportsOnlyItsLastContext(t *testing.T) {
	d := newDaemon(t)
	h := hub.New()
	d.Agents = agent.NewRegistry(h)
	_, pr := claudeIn(t, d)
	path := transcript(t, "hi")
	for _, used := range []int{1000, 2000, 3000} {
		appendLine(t, path, fmt.Sprintf(`{"type":"assistant","message":{"model":"claude-opus-5-5","content":[],"usage":{"input_tokens":%d}}}`, used))
	}
	msgs, stop := h.Subscribe()
	hookFrom(d, pr, sessionStart("s1", path))
	eventually(t, "tokens", func() bool { return pr.a.Summary().TokensUsed == 3000 })
	stop()
	var published []int64
	for raw := range msgs {
		var m struct {
			Type  string
			Agent struct{ TokensUsed int64 }
		}
		json.Unmarshal(raw, &m)
		if m.Type == "agent.update" && m.Agent.TokensUsed != 0 && (len(published) == 0 || published[len(published)-1] != m.Agent.TokensUsed) {
			published = append(published, m.Agent.TokensUsed)
		}
	}
	if fmt.Sprint(published) != "[3000]" {
		t.Fatalf("published %v", published)
	}
}

func TestA1MSessionKeepsItsWindowThoughTheTranscriptDropsTheSuffix(t *testing.T) {
	d := newDaemon(t)
	_, pr := claudeIn(t, d)
	path := transcript(t, "hi")
	hookFrom(d, pr, strings.Replace(sessionStart("s1", path), "claude-opus-5-5", "claude-sonnet-4-5[1m]", 1))
	appendLine(t, path, `{"type":"assistant","message":{"model":"claude-sonnet-4-5-20250929","content":[],"usage":{"input_tokens":5000}}}`)
	eventually(t, "1M window", func() bool {
		s := pr.a.Summary()
		return s.TokensUsed == 5000 && s.ContextWindow == 1_000_000
	})
}
