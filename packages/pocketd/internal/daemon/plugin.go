package daemon

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"pocketd/internal/proc"
)

var hookEvents = map[string]string{
	"SessionStart":       "",
	"UserPromptSubmit":   "",
	"PreToolUse":         "AskUserQuestion|ExitPlanMode",
	"PermissionRequest":  "",
	"Notification":       "permission_prompt|elicitation_dialog|elicitation_url_dialog|agent_needs_input",
	"PostToolUse":        "",
	"PostToolUseFailure": "",
	"PermissionDenied":   "",
	"Stop":               "",
	"StopFailure":        "",
	"PreCompact":         "",
}

func (d *Daemon) WritePlugin() error {
	dir := filepath.Join(d.Home, "plugin")
	hooks := map[string]any{}
	for event, matcher := range hookEvents {
		timeout := 5
		if event == "PermissionRequest" {
			timeout = 610
		}
		group := map[string]any{"hooks": []any{map[string]any{"type": "command", "command": shellQuote(d.Exe) + " hook", "timeout": timeout}}}
		if matcher != "" {
			group["matcher"] = matcher
		}
		hooks[event] = []any{group}
	}
	files := map[string]any{
		".claude-plugin/plugin.json": map[string]string{"name": "anywhere", "version": "1.0.0", "description": "Shows Claude sessions in Anywhere."},
		"hooks/hooks.json":           map[string]any{"hooks": hooks},
	}
	for name, v := range files {
		path := filepath.Join(dir, name)
		if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
			return err
		}
		raw, _ := json.MarshalIndent(v, "", "  ")
		if err := os.WriteFile(path, raw, 0o600); err != nil {
			return err
		}
	}
	d.Plugin = dir
	return nil
}

// shellQuote quotes s for the sh -c that Claude runs a hook command with.
func shellQuote(s string) string {
	return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'"
}

// Env is env for terminal id: a claude in it loads the plugin, and its hooks
// tell pocketd which terminal they come from. Without CLAUDECODE and
// CLAUDE_CODE_CHILD_SESSION, a pocketd started from a Claude session still
// gets transcripts from the claudes it runs.
func (d *Daemon) Env(env []string, terminalID string) []string {
	var out, plugins []string
	for _, kv := range env {
		k, v, _ := strings.Cut(kv, "=")
		switch k {
		case "CLAUDE_CODE_PLUGIN_DIRS":
			for _, dir := range strings.Split(v, ":") {
				if dir != "" && dir != d.Plugin {
					plugins = append(plugins, dir)
				}
			}
		case "CLAUDECODE", "CLAUDE_CODE_CHILD_SESSION", "POCKETD_SOCK", "POCKETD_PTY", proc.Marker:
		default:
			out = append(out, kv)
		}
	}
	return append(out, "CLAUDE_CODE_PLUGIN_DIRS="+strings.Join(append(plugins, d.Plugin), ":"), "POCKETD_SOCK="+d.Sock, "POCKETD_PTY="+terminalID,
		proc.Marker+"="+strconv.Itoa(os.Getpid()))
}
