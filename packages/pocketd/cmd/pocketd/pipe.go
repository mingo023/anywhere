package main

import (
	"errors"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"slices"
	"strings"
	"syscall"

	"pocketd/internal/launch"
	"pocketd/internal/pipe"
	"pocketd/internal/shellenv"
	"pocketd/internal/state"
)

// pipeCmd runs argv with its stdio relayed to pocketd, which drives it from
// the chat. argv[0] may name a shell function, as in the user's terminal.
// What argv carries after state.Carry is for pocketd, which reads it off
// this process's own command line.
func pipeCmd(sock string, argv []string) (int, error) {
	id := os.Getenv("POCKETD_PTY")
	if id == "" {
		return 0, errors.New("pipe runs only in a Pocket terminal")
	}
	if argv, _, _ = state.Cut(argv); len(argv) == 0 {
		return 0, errors.New("pipe needs a command to run")
	}
	if onPath(argv[0]) {
		argv = launch.InShell(shellenv.LoginShell(), argv)
	}
	cmd := exec.Command(argv[0], argv[1:]...)
	cmd.Env = agentEnv(os.Environ())
	cmd.Stderr = os.Stderr
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return 0, err
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return 0, err
	}
	// The agent shares the Terminal's process group, so a Ctrl+C there reaches
	// it already; a stop sent to this process only is passed on.
	sigs := make(chan os.Signal, 1)
	signal.Notify(sigs, os.Interrupt, syscall.SIGTERM, syscall.SIGHUP)
	if err := cmd.Start(); err != nil {
		return 0, err
	}
	go func() {
		for s := range sigs {
			if s != os.Interrupt {
				cmd.Process.Signal(s)
			}
		}
	}()
	var r pipe.Relay
	go r.Serve(sock, id, stdin)
	r.Out(stdout)
	err = cmd.Wait()
	if exit := (*exec.ExitError)(nil); errors.As(err, &exit) {
		return exit.ExitCode(), nil
	}
	return 0, err
}

// onPath: on Windows a path may be written with either slash.
func onPath(name string) bool { return !strings.Contains(filepath.ToSlash(name), "/") }

// agentDefaults is the env a chat agent gets unless the user sets it.
// CLAUDE_CODE_MCP_STARTUP_WAIT_MS caps, in ms, how long claude -p given
// --mcp-config holds its first turn for MCP servers still connecting. Left
// alone it waits out each one: 13s for a server awaiting a login. Anywhere's
// own server is local and connects well inside the cap.
// CLAUDE_CODE_ENABLE_TODO_TOOLS gives claude -p TaskCreate and TaskUpdate,
// which only the interactive claude has by default; the Tasks card draws them.
// CLAUDE_CODE_ARTIFACT gives claude -p the Artifact tools, which it hides by
// default; a disableArtifact setting still turns them off.
var agentDefaults = []string{"CLAUDE_CODE_MCP_STARTUP_WAIT_MS=1000", "CLAUDE_CODE_ENABLE_TODO_TOOLS=1", "CLAUDE_CODE_ARTIFACT=1"}

// agentEnv is env with each of agentDefaults it doesn't set.
func agentEnv(env []string) []string {
	out := env
	for _, kv := range agentDefaults {
		key, _, _ := strings.Cut(kv, "=")
		if !slices.ContainsFunc(env, func(e string) bool { return strings.HasPrefix(e, key+"=") }) {
			out = append(out, kv)
		}
	}
	return out
}
