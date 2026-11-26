package daemon

import (
	"path/filepath"
	"slices"
	"testing"

	"pocketd/internal/terminal"
)

func TestALoginSpawnRunsTheLoginShellInHome(t *testing.T) {
	root := t.TempDir()
	home, srv := filepath.Join(root, "home", "me"), filepath.Join(root, "srv", "x")
	for cwd, want := range map[string]string{"": home, "~": home, "~/app": filepath.Join(home, "app"), "app": filepath.Join(home, "app"), srv: srv} {
		s := loginSpec(terminal.Spec{Cwd: cwd, Env: []string{"A=1"}}, "/bin/bash", home, nil)
		if s.Cmd != "/bin/bash" || !slices.Equal(s.Args, []string{"-l"}) || s.Cwd != want || s.Env != nil {
			t.Fatalf("cwd %q: %+v", cwd, s)
		}
	}
}

func TestALoginSpawnWithArgvDropsBackToTheShell(t *testing.T) {
	s := loginSpec(terminal.Spec{}, "/bin/zsh", "/h", []string{"claude", "--resume"})
	want := []string{"-l", "-c", `"$@"; exec '/bin/zsh' -l`, "/bin/zsh", "claude", "--resume"}
	if !slices.Equal(s.Args, want) {
		t.Fatalf("%q", s.Args)
	}
	s = loginSpec(terminal.Spec{}, "/opt/homebrew/bin/fish", "/h", []string{"claude"})
	want = []string{"-l", "-c", `$argv; exec '/opt/homebrew/bin/fish' -l`, "claude"}
	if !slices.Equal(s.Args, want) {
		t.Fatalf("fish: %q", s.Args)
	}
}
