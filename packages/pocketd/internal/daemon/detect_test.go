package daemon

import (
	"testing"

	"pocketd/internal/proc"
)

func TestProvider(t *testing.T) {
	for _, c := range []struct {
		argv []string
		want string
	}{
		{[]string{"claude"}, "claude"},
		{[]string{"/Users/me/.local/bin/claude", "--resume"}, "claude"},
		{[]string{"claude", "-p", "fix the tests"}, "claude"},
		{[]string{"claude", "mcp", "serve"}, ""},
		{[]string{"claude", "doctor"}, ""},
		{[]string{"claude", "--version"}, ""},
		{[]string{"claude", "bg-agent"}, ""},
		{[]string{"codex"}, "codex"},
		{[]string{"/opt/homebrew/bin/codex", "resume", "--last"}, "codex"},
		{[]string{"codex", "exec", "fix the tests"}, "codex"},
		{[]string{"codex", "bg-agent"}, "codex"},
		{[]string{"codex", "app-server"}, ""},
		{[]string{"codex", "login"}, ""},
		{[]string{"codex", "a"}, ""},
		{[]string{"codex", "-V"}, ""},
		{[]string{"node", "/usr/local/bin/codex"}, ""},
		{[]string{"sh", "-c", "claude"}, ""},
		{[]string{"claude-helper"}, ""},
		{nil, ""},
	} {
		if got := Provider(c.argv); got != c.want {
			t.Errorf("Provider(%q) = %q, want %q", c.argv, got, c.want)
		}
	}
}

func TestAgentProcIsTheFirstMatchInPidOrder(t *testing.T) {
	npmCodex := []proc.Proc{
		{Pid: 200, Argv: []string{"node", "/usr/local/bin/codex"}},
		{Pid: 201, Argv: []string{"/usr/local/lib/node_modules/@openai/codex/vendor/aarch64-apple-darwin/codex/codex"}},
	}
	nested := []proc.Proc{
		{Pid: 300, Argv: []string{"claude"}},
		{Pid: 301, Argv: []string{"claude", "-p", "summarize"}},
	}
	for _, c := range []struct {
		procs    []proc.Proc
		provider string
		pid      int
	}{
		{nil, "", 0},
		{[]proc.Proc{{Pid: 100, Argv: []string{"sh"}}, {Pid: 101, Argv: []string{"sleep", "30"}}}, "", 0},
		{npmCodex, "codex", 201},
		{nested, "claude", 300},
	} {
		provider, p, ok := agentProc(c.procs)
		if provider != c.provider || p.Pid != c.pid || ok != (c.pid != 0) {
			t.Errorf("agentProc(%+v) = %q, %d, %v", c.procs, provider, p.Pid, ok)
		}
	}
}
