//go:build unix

package shellenv

import (
	"os"
	"path/filepath"
	"testing"
)

func TestTheFirstShellThatRunsWins(t *testing.T) {
	dir := t.TempDir()
	notExecutable, shell := filepath.Join(dir, "zsh"), filepath.Join(dir, "bash")
	if os.WriteFile(notExecutable, nil, 0o644) != nil || os.WriteFile(shell, nil, 0o755) != nil {
		t.Fatal("can't write the shells")
	}
	if got := firstThatRuns([]string{"", "/no/such/fish", "bash", dir, notExecutable, shell}); got != shell {
		t.Fatal(got)
	}
}

func TestWithNoShellThatRunsShIsTheLastResort(t *testing.T) {
	if got := firstThatRuns([]string{"/no/such/fish"}); got != "/bin/sh" {
		t.Fatal(got)
	}
}
