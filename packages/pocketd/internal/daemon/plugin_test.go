package daemon

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"pocketd/internal/ops"
)

func readJSON(t *testing.T, path string, v any) {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(raw, v); err != nil {
		t.Fatalf("%s: %v", path, err)
	}
}

func TestWritePluginHooksEveryStatusEvent(t *testing.T) {
	d := newDaemon(t)
	d.Exe = filepath.Join(t.TempDir(), "Pocket's $HOME `id`.app", "pocketd")
	os.MkdirAll(filepath.Dir(d.Exe), 0o700)
	os.WriteFile(d.Exe, []byte("#!/bin/sh\nprintf '%s|' \"$0\" \"$@\"\n"), 0o700)
	if err := d.WritePlugin(); err != nil {
		t.Fatal(err)
	}
	if d.Plugin != filepath.Join(d.Home, "plugin") {
		t.Fatalf("plugin = %q", d.Plugin)
	}
	var manifest struct{ Name, Version string }
	readJSON(t, filepath.Join(d.Plugin, ".claude-plugin", "plugin.json"), &manifest)
	if manifest.Name != "coding-pocket" || manifest.Version != "1.0.0" {
		t.Fatalf("manifest = %+v", manifest)
	}
	type hook struct {
		Type, Command string
		Timeout       int
	}
	var file struct {
		Hooks map[string][]struct {
			Matcher string
			Hooks   []hook
		}
	}
	readJSON(t, filepath.Join(d.Plugin, "hooks", "hooks.json"), &file)
	want := map[string]string{
		"SessionStart": "", "UserPromptSubmit": "", "PermissionRequest": "",
		"PreToolUse":   "AskUserQuestion|ExitPlanMode",
		"Notification": "permission_prompt|elicitation_dialog|elicitation_url_dialog|agent_needs_input",
		"PostToolUse":  "", "PostToolUseFailure": "", "PermissionDenied": "",
		"Stop": "", "StopFailure": "", "PreCompact": "",
	}
	if len(file.Hooks) != len(want) {
		t.Fatalf("events = %v", file.Hooks)
	}
	for event, matcher := range want {
		timeout := 5
		if event == "PermissionRequest" {
			timeout = 610
		}
		g := file.Hooks[event]
		if len(g) != 1 || g[0].Matcher != matcher || len(g[0].Hooks) != 1 || g[0].Hooks[0].Type != "command" || g[0].Hooks[0].Timeout != timeout {
			t.Errorf("%s = %+v", event, g)
			continue
		}
		if out, _ := exec.Command("sh", "-c", g[0].Hooks[0].Command).Output(); string(out) != d.Exe+"|hook|" {
			t.Errorf("%s runs %q", event, out)
		}
	}
}

func TestEnvLoadsThePluginAndNamesTheTerminal(t *testing.T) {
	d := &Daemon{Plugin: "/h/plugin", Sock: "/h/pocketd.sock"}
	got := d.Env([]string{"PATH=/bin", "CLAUDECODE=1", "CLAUDE_CODE_CHILD_SESSION=1", "POCKETD_PTY=outer", "POCKETD_SOCK=/old", "CLAUDE_CODE_PLUGIN_DIRS=/mine:/h/plugin"}, "t1")
	if want := []string{"PATH=/bin", "CLAUDE_CODE_PLUGIN_DIRS=/mine:/h/plugin", "POCKETD_SOCK=/h/pocketd.sock", "POCKETD_PTY=t1"}; !slices.Equal(got, want) {
		t.Errorf("env = %q", got)
	}
	if got, want := d.Env(nil, "t2"), []string{"CLAUDE_CODE_PLUGIN_DIRS=/h/plugin", "POCKETD_SOCK=/h/pocketd.sock", "POCKETD_PTY=t2"}; !slices.Equal(got, want) {
		t.Errorf("empty env = %q", got)
	}
}

func TestEveryPocketTerminalGetsTheEnv(t *testing.T) {
	d := newDaemon(t)
	term, err := d.Spawn(ops.Msg{Cmd: "sh", Args: []string{"-c", "echo pty=$POCKETD_PTY. claudecode=$CLAUDECODE.; sleep 30"}, Env: []string{"PATH=/bin:/usr/bin", "CLAUDECODE=1"}})
	if err != nil {
		t.Fatal(err)
	}
	defer term.Close()
	eventually(t, "the env", func() bool { return strings.Contains(term.Screen(), "pty="+term.Info().ID+". claudecode=.") })
}
