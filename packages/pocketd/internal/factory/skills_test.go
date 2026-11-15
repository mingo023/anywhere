package factory

import (
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"pocketd/internal/worktree"
)

// gitRepo is a temp repository with one commit, and an identity for the commits a test makes.
func gitRepo(t *testing.T) string {
	t.Helper()
	for _, kv := range []string{"GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@x", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@x"} {
		k, v, _ := strings.Cut(kv, "=")
		t.Setenv(k, v)
	}
	dir, _ := filepath.EvalSymlinks(t.TempDir())
	gitIn(t, dir, "init", "-q", "-b", "main")
	gitIn(t, dir, "commit", "-q", "--allow-empty", "-m", "init")
	return dir
}

func gitIn(t *testing.T, dir string, args ...string) string {
	t.Helper()
	out, err := exec.Command("git", append([]string{"-C", dir}, args...)...).CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
	return strings.TrimSpace(string(out))
}

func TestAProfilesSkillsAreCopiedWhereItsAgentLooksForThem(t *testing.T) {
	for provider, root := range map[string]string{"claude": ".claude/skills", "codex": ".agents/skills"} {
		dir := gitRepo(t)
		copied, skipped, err := CopySkills(goBackend(), provider, dir)
		if err != nil || !slices.Equal(copied, []string{"review", "test"}) || len(skipped) != 0 {
			t.Fatalf("%s: copied %q, skipped %q, %v", provider, copied, skipped, err)
		}
		text, _ := os.ReadFile(filepath.Join(dir, root, "review", "SKILL.md"))
		logo, _ := os.ReadFile(filepath.Join(dir, root, "review", "logo.png"))
		if string(text) != "# Review\n" || !slices.Equal(logo, []byte{0xff, 0xd8, 0x00}) {
			t.Fatalf("%s: %q %v", provider, text, logo)
		}
	}
}

func TestASkillTheCheckoutAlreadyHasIsLeftAloneAndReported(t *testing.T) {
	dir := gitRepo(t)
	own := filepath.Join(dir, ".claude", "skills", "review")
	os.MkdirAll(own, 0o755)
	os.WriteFile(filepath.Join(own, "SKILL.md"), []byte("the repository's own\n"), 0o644)
	copied, skipped, err := CopySkills(goBackend(), "claude", dir)
	if err != nil || !slices.Equal(copied, []string{"test"}) || !slices.Equal(skipped, []string{"review"}) {
		t.Fatalf("copied %q, skipped %q, %v", copied, skipped, err)
	}
	if text, _ := os.ReadFile(filepath.Join(own, "SKILL.md")); string(text) != "the repository's own\n" {
		t.Fatalf("the repository's skill was overwritten: %q", text)
	}
	if _, err := os.Stat(filepath.Join(own, "logo.png")); err == nil {
		t.Fatal("a file was added to the repository's skill")
	}
}

func TestCopiedSkillsStayOutOfGitStatusAndOutOfAHandbackCommit(t *testing.T) {
	dir := gitRepo(t)
	if _, _, err := CopySkills(goBackend(), "claude", dir); err != nil {
		t.Fatal(err)
	}
	if out := gitIn(t, dir, "status", "--porcelain"); out != "" {
		t.Fatalf("git lists %q", out)
	}
	os.WriteFile(filepath.Join(dir, "a.txt"), []byte("one\n"), 0o644)
	if committed, err := worktree.Commit(dir, "team t1: Write a"); err != nil || !committed {
		t.Fatalf("%t %v", committed, err)
	}
	if files := gitIn(t, dir, "show", "--name-only", "--format=", "HEAD"); files != "a.txt" {
		t.Fatalf("the handback commit holds %q", files)
	}
}

func TestAProfileWithoutSkillsCopiesNothingAndWritesNoExclude(t *testing.T) {
	dir := gitRepo(t)
	copied, skipped, err := CopySkills(BuiltIns()[0], "claude", dir)
	if err != nil || len(copied)+len(skipped) != 0 {
		t.Fatalf("%q %q %v", copied, skipped, err)
	}
	if _, err := os.Stat(filepath.Join(dir, ".claude")); err == nil {
		t.Fatal("a folder was made for no skill")
	}
}
