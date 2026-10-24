//go:build unix

package wsserver

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sync/atomic"
	"testing"

	"pocketd/internal/github"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
)

// ghFixture is a project with a worktree on branch fix where the desktop puts
// them, and a fake gh (internal/github/testdata/fakegh) answering from the
// files in dir.
type ghFixture struct {
	t                  *testing.T
	dir, project, tree string
	reg                *registry.Registry
	withGh             atomic.Bool
}

func newGhFixture(t *testing.T) *ghFixture {
	root, _ := filepath.EvalSymlinks(t.TempDir())
	f := &ghFixture{t: t, dir: filepath.Join(root, "gh"), project: filepath.Join(root, "app"), tree: filepath.Join(root, "wt", "app", "fix")}
	f.withGh.Store(true)
	os.Mkdir(f.dir, 0o755)
	os.Mkdir(f.project, 0o755)
	for _, args := range [][]string{{"init", "-q"}, {"commit", "-q", "--allow-empty", "-m", "first"}, {"worktree", "add", "-q", "-b", "fix", f.tree}} {
		cmd := exec.Command("git", append([]string{"-C", f.project, "-c", "user.name=T", "-c", "user.email=t@t"}, args...)...)
		cmd.Env = append(os.Environ(), "GIT_CONFIG_GLOBAL=/dev/null")
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	path := filepath.Join(root, "desktop.json")
	os.WriteFile(path, []byte(fmt.Sprintf(`{"projects":[%q],"worktree":{"root":%q}}`, f.project, filepath.Join(root, "wt"))), 0o600)
	f.reg = registry.New(path)
	f.put("version", "gh version 2.81.0 (2025-10-01)\n")
	f.put("auth", "  ✓ Logged in to github.com account mingo023 (keyring)\n  - Active account: true\n")
	f.put("view.json", `{"isDraft":false,"number":42,"state":"OPEN","title":"Fix the flaky snapshot","url":"https://github.com/acme/app/pull/42","mergeable":"MERGEABLE","reviewDecision":"APPROVED","statusCheckRollup":[{"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"SUCCESS"}]}`)
	f.put("threads.json", `{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[]}}}}}`)
	return f
}

func (f *ghFixture) put(name, content string) {
	if err := os.WriteFile(filepath.Join(f.dir, name), []byte(content), 0o644); err != nil {
		f.t.Fatal(err)
	}
}

func (f *ghFixture) server(s *Server) {
	bin, _ := filepath.Abs(filepath.Join("..", "github", "testdata", "fakegh"))
	env := func(string) []string {
		path := "/usr/bin:/bin"
		if f.withGh.Load() {
			path = bin + ":" + path
		}
		return []string{"PATH=" + path, "FAKEGH=" + f.dir, "HOME=" + f.dir}
	}
	s.Registry, s.GitHub = f.reg, github.New(env, f.reg)
}

func (f *ghFixture) phone(caps ...string) *phone {
	_, _, p := local(f.t, peer.OwnerOf(1), f.server)
	helloWith(p, caps...)
	return p
}

func TestAPhoneGetsATreesPRAsGhReportsIt(t *testing.T) {
	f := newGhFixture(t)
	p := f.phone(proto.CapGitHub)
	p.send(fmt.Sprintf(`{"type":"gh.pr","id":"p","tree":%q}`, f.tree))
	m := p.recv()
	pr, _ := m["pr"].(map[string]any)
	if m["type"] != "gh.pr" || m["id"] != "p" || m["tree"] != f.tree || pr["number"] != 42.0 || pr["gate"] != "ready" {
		t.Fatalf("%v", m)
	}
}

func TestAPathOutsideEveryProjectIsNotATree(t *testing.T) {
	f := newGhFixture(t)
	p := f.phone(proto.CapGitHub)
	for _, tree := range []string{"/etc", f.tree + "/../../x", "app-fix", f.tree + "/gone"} {
		p.send(fmt.Sprintf(`{"type":"gh.pr","id":"p","tree":%q}`, tree))
		if m := p.recv(); m["code"] != proto.CodeNotATree {
			t.Errorf("%s: %v", tree, m)
		}
	}
	p.send(`{"type":"gh.prs","id":"s","project":"/elsewhere"}`)
	if m := p.recv(); m["code"] != proto.CodeUnknownProject {
		t.Errorf("%v", m)
	}
}

func TestGhStatusSaysWhetherGhIsThereAndSignedIn(t *testing.T) {
	f := newGhFixture(t)
	p := f.phone(proto.CapGitHub)
	p.send(`{"type":"gh.status","id":"s"}`)
	if m := p.recv(); m["state"] != "ready" || m["login"] != "mingo023" {
		t.Fatalf("%v", m)
	}
	f.withGh.Store(false)
	p.send(fmt.Sprintf(`{"type":"gh.status","id":"s","tree":%q}`, f.tree))
	if m := p.recv(); m["state"] != "missing" || m["login"] != nil {
		t.Fatalf("%v", m)
	}
	p.send(fmt.Sprintf(`{"type":"gh.pr.ready","id":"r","tree":%q,"number":42}`, f.tree))
	if m := p.recv(); m["code"] != proto.CodeGhMissing || m["id"] != "r" {
		t.Fatalf("%v", m)
	}
}

func TestAMergeIsAckedAndAFailedOneSaysWhatGhSaid(t *testing.T) {
	f := newGhFixture(t)
	p := f.phone(proto.CapGitHub)
	p.send(fmt.Sprintf(`{"type":"gh.pr.merge","id":"m","tree":%q,"number":42,"method":"squash"}`, f.tree))
	if m := p.recv(); m["type"] != "ack" || m["id"] != "m" {
		t.Fatalf("%v", m)
	}
	f.put("fail", "Pull request acme/app#42 is not mergeable: the base branch policy prohibits the merge.\n")
	p.send(fmt.Sprintf(`{"type":"gh.thread.resolve","id":"r","tree":%q,"thread":"PRRT_a","resolved":true}`, f.tree))
	if m := p.recv(); m["code"] != proto.CodeGhFailed || m["message"] != "Pull request acme/app#42 is not mergeable: the base branch policy prohibits the merge." {
		t.Fatalf("%v", m)
	}
}

func TestAWatchedTreeIsToldWhenItsPRChanges(t *testing.T) {
	servingPhoneCaps(t)
	f := newGhFixture(t)
	p := f.phone(proto.CapGit, proto.CapGitHub)
	p.send(fmt.Sprintf(`{"type":"git.watch","id":"w","trees":[%q]}`, f.tree))
	got := map[string]map[string]any{}
	for range 2 {
		m := p.recv()
		got[m["type"].(string)] = m
	}
	if got["gh.changed"]["tree"] != f.tree {
		t.Fatalf("%v", got)
	}
	p.send(fmt.Sprintf(`{"type":"gh.prs","id":"s","project":%q}`, f.project))
	m := p.recv()
	items, _ := m["items"].([]any)
	if m["type"] != "gh.prs" || len(items) != 1 || items[0].(map[string]any)["tree"] != f.tree {
		t.Fatalf("%v", m)
	}
}
