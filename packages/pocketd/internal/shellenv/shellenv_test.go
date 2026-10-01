package shellenv

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"
	"time"
)

func TestParseKeepsMultilineValues(t *testing.T) {
	out := []byte("Bn1A=1\x00MULTI=line1\nline2\x00PWD=/x\x00SHLVL=2\x00_=/usr/bin/env\x00En1")
	got, err := Parse(out, "n1")
	if err != nil || !reflect.DeepEqual(got, []string{"A=1", "MULTI=line1\nline2"}) {
		t.Fatalf("%q %v", got, err)
	}
}

func TestParseIgnoresNoiseOutsideSentinels(t *testing.T) {
	out := []byte("Last login: today\nwelcome\x00Bn1A=1\x00En1\ngoodbye")
	got, err := Parse(out, "n1")
	if err != nil || !reflect.DeepEqual(got, []string{"A=1"}) {
		t.Fatalf("%q %v", got, err)
	}
}

func TestParseRejectsAMissingEndSentinel(t *testing.T) {
	if _, err := Parse([]byte("Bn1A=1\x00"), "n1"); err == nil {
		t.Fatal("a cut-off env parsed")
	}
	if _, err := Parse([]byte("A=1\x00En1"), "n1"); err == nil {
		t.Fatal("an env without a start parsed")
	}
}

func TestBaseKeepsTheAccountNotTheLaunchingTerminal(t *testing.T) {
	got := Base([]string{"HOME=/h", "USER=u", "TMUX=/tmp/t", "PATH=/somewhere", "TERM=tmux-256color", "POCKETD_SOCK=/s"}, "/bin/zsh")
	want := []string{"HOME=/h", "USER=u", "LANG=en_US.UTF-8", "PATH=/usr/bin:/bin:/usr/sbin:/sbin", "SHELL=/bin/zsh", "TERM=xterm-256color", "COLORTERM=truecolor", "TERM_PROGRAM=Pocket"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("%q", got)
	}
}

func home(t *testing.T, files map[string]string) string {
	t.Helper()
	dir := t.TempDir()
	for name, body := range files {
		path := filepath.Join(dir, name)
		os.MkdirAll(filepath.Dir(path), 0o700)
		if err := os.WriteFile(path, []byte(body), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

func need(t *testing.T, shell string) {
	t.Helper()
	if _, err := os.Stat(shell); err != nil {
		t.Skip(shell + " not installed")
	}
}

func get(env []string, key string) string {
	i := slices.IndexFunc(env, func(kv string) bool { return strings.HasPrefix(kv, key+"=") })
	if i < 0 {
		return ""
	}
	return strings.TrimPrefix(env[i], key+"=")
}

func capture(dir, shell string, limit time.Duration) Result {
	return Capture(context.Background(), shell, Base([]string{"HOME=" + dir, "USER=" + os.Getenv("USER")}, shell), limit)
}

func TestZshLoginEnvIncludesRcPath(t *testing.T) {
	need(t, "/bin/zsh")
	dir := home(t, map[string]string{
		".zprofile": "echo profile noise\n",
		".zshrc":    "echo rc noise\nexport PATH=\"$HOME/rcbin:$PATH\"\nexport POCKET_MULTI=$'line1\\nline2'\n",
	})
	r := capture(dir, "/bin/zsh", 5*time.Second)
	if r.Mode != "interactive" || !strings.HasPrefix(get(r.Env, "PATH"), dir+"/rcbin:") || get(r.Env, "POCKET_MULTI") != "line1\nline2" {
		t.Fatalf("%s %q", r, r.Env)
	}
}

func TestFishLoginEnvIncludesConfigPath(t *testing.T) {
	fish := "/opt/homebrew/bin/fish"
	need(t, fish)
	dir := home(t, map[string]string{
		".config/fish/config.fish": "echo fish noise\nset -gx PATH $HOME/rcbin $PATH\nset -gx POCKET_MULTI line1\\nline2\n",
	})
	r := capture(dir, fish, 5*time.Second)
	if r.Mode != "interactive" || !strings.HasPrefix(get(r.Env, "PATH"), dir+"/rcbin:") || get(r.Env, "POCKET_MULTI") != "line1\nline2" {
		t.Fatalf("%s %q", r, r.Env)
	}
}

func TestCaptureGivesUpAfterTheLimit(t *testing.T) {
	need(t, "/bin/zsh")
	dir := home(t, map[string]string{".zprofile": "sleep 10\n"})
	r := capture(dir, "/bin/zsh", time.Second)
	if r.Mode != "inherited" || r.Err != "interactive: timeout; login: timeout" || r.Took > 4*time.Second {
		t.Fatalf("%s took %s", r, r.Took)
	}
}

func TestCaptureFallsBackToALoginShellWhenRcHangs(t *testing.T) {
	need(t, "/bin/zsh")
	dir := home(t, map[string]string{".zshrc": "sleep 10\n"})
	r := capture(dir, "/bin/zsh", time.Second)
	if r.Mode != "login" || get(r.Env, "HOME") != dir {
		t.Fatalf("%s %q", r, r.Env)
	}
}

func TestLoginShellIsAnAbsolutePath(t *testing.T) {
	if s := LoginShell(); !filepath.IsAbs(s) {
		t.Fatal(s)
	}
}
