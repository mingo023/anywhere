//go:build unix

package shellenv

import (
	"os"
	"path/filepath"
	"reflect"
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

func TestBaseKeepsTheAccountNotTheLaunchingTerminal(t *testing.T) {
	got := Base([]string{"HOME=/h", "USER=u", "TMUX=/tmp/t", "PATH=/somewhere", "TERM=tmux-256color", "POCKETD_SOCK=/s"}, "/bin/zsh")
	want := []string{"HOME=/h", "USER=u", "LANG=en_US.UTF-8", "PATH=/usr/bin:/bin:/usr/sbin:/sbin", "SHELL=/bin/zsh", "TERM=xterm-256color", "COLORTERM=truecolor", "TERM_PROGRAM=ghostty"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("%q", got)
	}
}
