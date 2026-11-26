package main

import (
	"runtime"
	"slices"
	"testing"
)

func TestAChatAgentHoldsItsFirstTurnForMCPServersAtMostASecond(t *testing.T) {
	if got := agentEnv([]string{"HOME=/h"}); !slices.Contains(got, "CLAUDE_CODE_MCP_STARTUP_WAIT_MS=1000") {
		t.Fatalf("env = %v", got)
	}
}

func TestAChatAgentHasClaudesTaskTools(t *testing.T) {
	if got := agentEnv([]string{"HOME=/h"}); !slices.Contains(got, "CLAUDE_CODE_ENABLE_TODO_TOOLS=1") {
		t.Fatalf("env = %v", got)
	}
}

func TestAChatAgentHasClaudesArtifactTools(t *testing.T) {
	if got := agentEnv([]string{"HOME=/h"}); !slices.Contains(got, "CLAUDE_CODE_ARTIFACT=1") {
		t.Fatalf("env = %v", got)
	}
}

func TestWhatTheUserSetIsKept(t *testing.T) {
	env := []string{"CLAUDE_CODE_MCP_STARTUP_WAIT_MS=5000", "CLAUDE_CODE_ENABLE_TODO_TOOLS=0", "CLAUDE_CODE_ARTIFACT=0", "HOME=/h"}
	if got := agentEnv(env); !slices.Equal(got, env) {
		t.Fatalf("env = %v", got)
	}
}

func TestACommandWrittenAsAPathWithEitherSlashIsRunAsIs(t *testing.T) {
	paths := []string{"./claude", "/usr/local/bin/claude"}
	if runtime.GOOS == "windows" {
		paths = append(paths, `C:\bin\claude.exe`, "C:/bin/claude.exe", `.\claude`)
	}
	for _, p := range paths {
		if onPath(p) {
			t.Errorf("%s was taken for a command on PATH", p)
		}
	}
	if !onPath("claude") {
		t.Error("claude was taken for a path")
	}
}
