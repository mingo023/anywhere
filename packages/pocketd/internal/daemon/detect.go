package daemon

import (
	"path/filepath"
	"slices"
	"strings"

	"pocketd/internal/proc"
)

var notAgent = map[string][]string{
	"claude": {"mcp", "doctor", "config", "update", "install", "plugin", "setup-token", "migrate-installer", "-v", "--version", "-h", "--help"},
	"codex":  {"login", "logout", "mcp", "mcp-server", "app-server", "completion", "sandbox", "debug", "apply", "a", "cloud", "features", "help", "-h", "--help", "-V", "--version"},
}

func Provider(argv []string) string {
	if len(argv) == 0 {
		return ""
	}
	name := filepath.Base(argv[0])
	skip, ok := notAgent[name]
	if !ok {
		return ""
	}
	if len(argv) > 1 && (slices.Contains(skip, argv[1]) || name == "claude" && strings.HasPrefix(argv[1], "bg-")) {
		return ""
	}
	return name
}

func agentProc(procs []proc.Proc) (provider string, p proc.Proc, ok bool) {
	for _, p := range procs {
		if provider := Provider(p.Argv); provider != "" {
			return provider, p, true
		}
	}
	return "", proc.Proc{}, false
}
