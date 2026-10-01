package launch

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

const trusted = `{"projects":{"/a":{"hasTrustDialogAccepted":true},"/b":{"hasTrustDialogAccepted":false}}}`

func TestTrustWalkStopsAtTheGitToplevel(t *testing.T) {
	if Trusted([]byte(trusted), "/a/b/c", "", "/a/b") {
		t.Fatal("walked past the toplevel")
	}
	if !Trusted([]byte(trusted), "/a/b/c", "", "") {
		t.Fatal("outside git the walk goes up to /")
	}
	if !Trusted([]byte(trusted), "/x/y", "/a", "/x/y") {
		t.Fatal("the canonical root is checked first")
	}
}

func TestAnUntrustedFolderIsNotTrusted(t *testing.T) {
	for _, cwd := range []string{"/b", "/c"} {
		if Trusted([]byte(trusted), cwd, "", cwd) {
			t.Errorf("%s: trusted", cwd)
		}
	}
	if Trusted(nil, "/a", "", "") {
		t.Fatal("no config trusts everything")
	}
}

func git(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir, "-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"}, args...)...)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v: %s", args, out)
	}
}

func TestALinkedWorktreeOfATrustedRepoIsTrusted(t *testing.T) {
	root, _ := filepath.EvalSymlinks(t.TempDir())
	repo, wt, cfg := filepath.Join(root, "repo"), filepath.Join(root, "wt"), filepath.Join(root, "claude")
	os.MkdirAll(repo, 0o755)
	os.MkdirAll(cfg, 0o755)
	git(t, repo, "init", "-q", "-b", "main")
	git(t, repo, "commit", "-q", "--allow-empty", "-m", "init")
	git(t, repo, "worktree", "add", "-q", "-b", "fix", wt)
	env := []string{"CLAUDE_CONFIG_DIR=" + cfg}
	if trustedAt(env, wt) {
		t.Fatal("trusted with no config")
	}
	os.WriteFile(filepath.Join(cfg, ".claude.json"), []byte(`{"projects":{"`+repo+`":{"hasTrustDialogAccepted":true}}}`), 0o600)
	if !trustedAt(env, wt) {
		t.Fatal("a Worktree of a trusted repo is untrusted")
	}
}

func TestSandboxedIsTrusted(t *testing.T) {
	if !trustedAt([]string{"CLAUDE_CODE_SANDBOXED=1", "HOME=" + t.TempDir()}, t.TempDir()) {
		t.Fatal("sandboxed claude trusts every folder")
	}
}
