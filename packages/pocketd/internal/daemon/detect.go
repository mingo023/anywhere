package daemon

import (
	"path/filepath"
	"slices"
	"strings"

	"pocketd/internal/proc"
)

var notAgent = map[string][]string{
	"claude": {"auth", "mcp", "doctor", "config", "update", "install", "plugin", "setup-token", "migrate-installer", "-v", "--version", "-h", "--help"},
	"codex":  {"login", "logout", "mcp", "mcp-server", "app-server", "completion", "sandbox", "debug", "apply", "a", "cloud", "features", "help", "-h", "--help", "-V", "--version"},
}

func Provider(argv []string) string {
	if len(argv) == 0 {
		return ""
	}
	name := strings.TrimSuffix(filepath.Base(argv[0]), ".exe")
	skip, ok := notAgent[name]
	if !ok {
		return ""
	}
	if len(argv) > 1 && (slices.Contains(skip, argv[1]) || name == "claude" && strings.HasPrefix(argv[1], "bg-")) {
		return ""
	}
	return name
}

// piped is the argv `pocketd pipe` runs, or nil for any other process.
func piped(argv []string) []string {
	if len(argv) > 2 && strings.HasPrefix(filepath.Base(argv[0]), "pocketd") && argv[1] == "pipe" {
		return argv[2:]
	}
	return nil
}

// pipedProvider is the provider a pipe runs for the chat, or "".
func pipedProvider(argv []string) string {
	inner := piped(argv)
	if len(inner) > 1 && inner[1] == "app-server" && Provider(inner[:1]) == "codex" {
		return "codex"
	}
	return Provider(inner)
}

// agentProc finds the agent in procs. A pipe is its agent's, in the same group.
func agentProc(procs []proc.Proc) (provider string, p proc.Proc, ok bool) {
	for _, p := range procs {
		if provider := pipedProvider(p.Argv); provider != "" {
			return provider, p, true
		}
	}
	for _, p := range procs {
		if provider := Provider(p.Argv); provider != "" {
			return provider, p, true
		}
	}
	return "", proc.Proc{}, false
}
