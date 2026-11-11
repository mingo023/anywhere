package daemon

import (
	"path/filepath"
	"slices"
	"strings"

	"pocketd/internal/proc"
	"pocketd/internal/providers"
)

// hermesMain is in the script Hermes's launcher has python run with -c.
const hermesMain = "hermes_cli.main"

// invoked is argv as its agent was typed. Hermes runs as
// `python3 -I -c <script> args…`, which reads here as `hermes args…`. Only
// python's own -c counts, among its first words: Hermes takes a -c too.
func invoked(argv []string) []string {
	if len(argv) == 0 || !strings.HasPrefix(filepath.Base(argv[0]), "python") {
		return argv
	}
	for i, arg := range argv[:min(len(argv), 4)] {
		if arg == "-c" && i+1 < len(argv) && strings.Contains(argv[i+1], hermesMain) {
			return append([]string{providers.Hermes.Cmd()}, argv[i+2:]...)
		}
	}
	return argv
}

// named is the name of the command argv runs.
func named(argv []string) string {
	return strings.TrimSuffix(filepath.Base(argv[0]), ".exe")
}

func Provider(argv []string) string {
	if argv = invoked(argv); len(argv) == 0 {
		return ""
	}
	i := slices.IndexFunc(providers.All, func(p *providers.Spec) bool { return p.Cmd() == named(argv) })
	if i < 0 {
		return ""
	}
	p := providers.All[i]
	if len(argv) > 1 && (slices.Contains(p.NotAgent, argv[1]) || kindOf[p.ID].tool(argv[1])) {
		return ""
	}
	return p.ID
}

// piped is the argv `pocketd pipe` runs, or nil for any other process.
func piped(argv []string) []string {
	if len(argv) > 2 && strings.HasPrefix(filepath.Base(argv[0]), "pocketd") && argv[1] == "pipe" {
		return argv[2:]
	}
	return nil
}

// pipedProvider is the provider a pipe runs for the chat, or "": one run
// with a subcommand its kind serves.
func pipedProvider(argv []string) string {
	inner := piped(argv)
	if inner == nil {
		return ""
	}
	for _, p := range providers.All {
		if p.Cmd() == named(inner) && len(inner) > 1 && kindOf[p.ID].serves(inner[1]) {
			return p.ID
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
