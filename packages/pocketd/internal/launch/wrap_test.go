package launch

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"testing"
)

// stub stands in for pocketd: it appends its arguments to log.
func stub(t *testing.T) (exe, log string) {
	dir := t.TempDir()
	exe, log = filepath.Join(dir, "pocketd"), filepath.Join(dir, "log")
	if err := os.WriteFile(exe, []byte("#!/bin/sh\necho \"$@\" >> '"+log+"'\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	return exe, log
}

func runWrapped(t *testing.T, shell, setup string, argv []string) string {
	exe, log := stub(t)
	cmd := exec.Command(shell, Wrap(shell, exe, setup, argv, nil)...)
	cmd.Env = []string{"HOME=" + t.TempDir(), "PATH=/usr/bin:/bin"}
	cmd.Run()
	got, _ := os.ReadFile(log)
	return string(got)
}

func TestWrapRecordsTheExitBeforeTheShellTakesOver(t *testing.T) {
	shells := posixShells(t)
	for _, shell := range shells {
		if got := runWrapped(t, shell, "", []string{"/bin/sh", "-c", "exit 3"}); got != "hook exit agent 3\n" {
			t.Errorf("%s: got %q", shell, got)
		}
	}
}

func TestAFailedSetupStopsBeforeTheAgent(t *testing.T) {
	shells := posixShells(t)
	for _, shell := range shells {
		if got := runWrapped(t, shell, "echo setting up\nfalse", []string{"/bin/sh", "-c", "exit 3"}); got != "hook exit setup 1\n" {
			t.Errorf("%s: got %q", shell, got)
		}
	}
}

func TestASetupThatCallsExitIsReportedAsSetup(t *testing.T) {
	shells := posixShells(t)
	for _, shell := range shells {
		if got := runWrapped(t, shell, "exit 4", []string{"/bin/sh", "-c", "exit 3"}); got != "hook exit setup 4\n" {
			t.Errorf("%s: got %q", shell, got)
		}
	}
}

func TestAPassingSetupIsReportedThenRunsTheAgent(t *testing.T) {
	shells := posixShells(t)
	for _, shell := range shells {
		if got := runWrapped(t, shell, "true", []string{"/bin/sh", "-c", "exit 0"}); got != "hook exit setup 0\nhook exit agent 0\n" {
			t.Errorf("%s: got %q", shell, got)
		}
	}
}

func TestWrapHandsPowerShellTheAgentThenKeepsItOpen(t *testing.T) {
	got := Wrap(`C:\pwsh.exe`, `C:\p\pocketd.exe`, "", []string{"claude", "it's"}, nil)
	want := []string{"-NoLogo", "-NoExit", "-Command", `& 'claude' 'it''s'; $s = if ($?) { 0 } elseif ($LASTEXITCODE) { $LASTEXITCODE } else { 1 }; & 'C:\p\pocketd.exe' hook exit agent $s`}
	if !slices.Equal(got, want) {
		t.Errorf("got %q", got)
	}
}

func TestWrapRunsAShellFunctionNamedLikeTheAgent(t *testing.T) {
	for shell, env := range shellsWith(t) {
		exe, log := stub(t)
		cmd := exec.Command(shell, Wrap(shell, exe, "", []string{"cc", "hi"}, nil)...)
		cmd.Env = env
		out, _ := cmd.Output()
		if got, _ := os.ReadFile(log); string(got) != "hook exit agent 0\n" || !strings.HasPrefix(string(out), "hi\n") {
			t.Errorf("%s: out %q, log %q", shell, out, got)
		}
	}
}

func TestAnAccountsAgentAndSignInRunInItsFolderWithoutTheKeysTheUsersRcSets(t *testing.T) {
	home := t.TempDir()
	os.WriteFile(filepath.Join(home, ".zshenv"), []byte("export ANTHROPIC_API_KEY=sk CLAUDE_CONFIG_DIR=/rc\n"), 0o644)
	os.MkdirAll(filepath.Join(home, ".config", "fish"), 0o755)
	os.WriteFile(filepath.Join(home, ".config", "fish", "config.fish"), []byte("set -gx ANTHROPIC_API_KEY sk\nset -gx CLAUDE_CONFIG_DIR /rc\n"), 0o644)
	for _, name := range []string{"/bin/zsh", "fish"} {
		shell, err := exec.LookPath(name)
		if err != nil {
			continue
		}
		exe, _ := stub(t)
		out := filepath.Join(t.TempDir(), "env")
		argv := []string{"/bin/sh", "-c", `echo "$CLAUDE_CONFIG_DIR|$ANTHROPIC_API_KEY" > ` + out}
		account := []string{"CLAUDE_CONFIG_DIR=/a/it's"}
		for _, run := range [][]string{append([]string{shell}, Wrap(shell, exe, "", argv, account)...), inAccount(shell, argv, account)} {
			os.Remove(out)
			cmd := exec.Command(run[0], run[1:]...)
			cmd.Env = []string{"HOME=" + home, "ZDOTDIR=" + home, "PATH=/usr/bin:/bin"}
			cmd.Run()
			if got, _ := os.ReadFile(out); string(got) != "/a/it's|\n" {
				t.Errorf("%s %v: %q", shell, run[1:3], got)
			}
		}
	}
}

func TestPowerShellSetsTheAccountBeforeTheAgent(t *testing.T) {
	got := Wrap(`C:\pwsh.exe`, `C:\p\pocketd.exe`, "", []string{"codex"}, []string{`CODEX_HOME=C:\a`})[3]
	if !strings.HasPrefix(got, "Remove-Item Env:ANTHROPIC_API_KEY, Env:ANTHROPIC_AUTH_TOKEN, ") || !strings.Contains(got, "\n$env:CODEX_HOME = 'C:\\a'\n& 'codex'") {
		t.Errorf("%q", got)
	}
}

func TestWindowsPowerShellHandsTheAgentAPromptsQuotesIntact(t *testing.T) {
	shell := `C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`
	argv := []string{"claude", `fix "x"`}
	if got := Wrap(shell, `C:\p\pocketd.exe`, "", argv, nil)[3]; !strings.HasPrefix(got, `& 'claude' 'fix \"x\"';`) {
		t.Errorf("wrap: %q", got)
	}
	if got := InShell(shell, argv)[3]; got != `& 'claude' 'fix \"x\"'` {
		t.Errorf("in shell: %q", got)
	}
}

// posixShells are /bin/sh and fish when installed. Windows terminals run
// PowerShell instead, which TestWrapHandsPowerShellTheAgentThenKeepsItOpen covers.
func posixShells(t *testing.T) []string {
	if runtime.GOOS == "windows" {
		t.Skip("no /bin/sh on Windows")
	}
	shells := []string{"/bin/sh"}
	if fish, err := exec.LookPath("fish"); err == nil {
		shells = append(shells, fish)
	}
	return shells
}
