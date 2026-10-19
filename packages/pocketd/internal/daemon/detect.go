package daemon

import (
	"path/filepath"
	"slices"
	"strings"

	"pocketd/internal/proc"
	"pocketd/internal/providers"
)

func Provider(argv []string) string {
	if len(argv) == 0 {
		return ""
	}
	name := strings.TrimSuffix(filepath.Base(argv[0]), ".exe")
	p := providers.Get(name)
	if p == nil {
		return ""
	}
	if len(argv) > 1 && (slices.Contains(p.NotAgent, argv[1]) || kindOf[name].tool(argv[1])) {
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
	if len(inner) > 1 {
		if p := Provider(inner[:1]); p != "" && kindOf[p].serves(inner[1]) {
			return p
		}
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
