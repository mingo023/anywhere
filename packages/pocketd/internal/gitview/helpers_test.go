package gitview

import (
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"pocketd/internal/proto"
)

// isolated keeps the user's git config, hooks and signing out of the tests.
func isolated() []string {
	return append(os.Environ(), "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_NOSYSTEM=1",
		"GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@t", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@t")
}

func service() *Service { return &Service{Env: isolated, Every: 20 * time.Millisecond} }

func sh(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	cmd.Env = isolated()
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
	return strings.TrimSpace(string(out))
}

func write(t *testing.T, dir, path, text string) {
	t.Helper()
	full := filepath.Join(dir, path)
	if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(full, []byte(text), 0o644); err != nil {
		t.Fatal(err)
	}
}

// scratch is a repo on main with one commit of a.
func scratch(t *testing.T) string {
	t.Helper()
	dir, err := filepath.EvalSymlinks(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	repo := filepath.Join(dir, "repo")
	os.Mkdir(repo, 0o755)
	sh(t, repo, "init", "-q", "-b", "main")
	write(t, repo, "a", "a\n")
	sh(t, repo, "add", ".")
	sh(t, repo, "commit", "-qm", "init")
	return repo
}

func file(t *testing.T, s *Service, tree, path string) (proto.GitFile, bool) {
	t.Helper()
	st, err := s.Status(tree, 0, 0)
	if err != nil {
		t.Fatal(err)
	}
	i := slices.IndexFunc(st.Files, func(f proto.GitFile) bool { return f.Path == path })
	if i < 0 {
		return proto.GitFile{}, false
	}
	return st.Files[i], true
}
