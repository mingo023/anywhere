package main

import (
	"os"
	"testing"

	"pocketd/internal/proc"
)

func TestNearestClaudeOwnsTheHook(t *testing.T) {
	for _, c := range []struct {
		name  string
		chain []proc.Proc
		want  int
	}{
		{"claude's shell", []proc.Proc{{Pid: 30, Argv: []string{"sh", "-c", "pocketd hook"}}, {Pid: 20, Argv: []string{"claude"}}, {Pid: 10, Argv: []string{"-zsh"}}}, 20},
		{"nested claude -p", []proc.Proc{{Pid: 50, Argv: []string{"claude", "-p", "hi"}}, {Pid: 40, Argv: []string{"bash"}}, {Pid: 20, Argv: []string{"claude"}}}, 50},
		{"claude mcp is not claude", []proc.Proc{{Pid: 60, Argv: []string{"claude", "mcp", "serve"}}, {Pid: 20, Argv: []string{"/Users/me/.local/bin/claude"}}}, 20},
		{"no claude", []proc.Proc{{Pid: 10, Argv: []string{"-zsh"}}}, 0},
	} {
		if got := nearestClaude(c.chain); got != c.want {
			t.Errorf("%s: pid %d, want %d", c.name, got, c.want)
		}
	}
}

func TestAncestorsStartAtTheParent(t *testing.T) {
	chain := ancestors(os.Getpid())
	if len(chain) == 0 || chain[0].Pid != os.Getppid() {
		t.Fatalf("ancestors = %+v", chain)
	}
}
