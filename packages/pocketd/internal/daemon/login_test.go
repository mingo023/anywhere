package daemon

import (
	"slices"
	"testing"

	"pocketd/internal/terminal"
)

func TestALoginSpawnRunsTheLoginShellInHome(t *testing.T) {
	for cwd, want := range map[string]string{"": "/home/me", "~": "/home/me", "~/app": "/home/me/app", "app": "/home/me/app", "/srv/x": "/srv/x"} {
		s := loginSpec(terminal.Spec{Cwd: cwd, Env: []string{"A=1"}}, "/bin/bash", "/home/me", nil)
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
