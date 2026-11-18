package main

import (
	"slices"
	"testing"
)

func TestAChatAgentHoldsItsFirstTurnForMCPServersAtMostASecond(t *testing.T) {
	got := agentEnv([]string{"HOME=/h"})
	if want := []string{"HOME=/h", "CLAUDE_CODE_MCP_STARTUP_WAIT_MS=1000"}; !slices.Equal(got, want) {
		t.Fatalf("env = %v, want %v", got, want)
	}
}

func TestAStartupWaitTheUserSetIsKept(t *testing.T) {
	env := []string{"CLAUDE_CODE_MCP_STARTUP_WAIT_MS=5000", "HOME=/h"}
	if got := agentEnv(env); !slices.Equal(got, env) {
		t.Fatalf("env = %v", got)
	}
}
