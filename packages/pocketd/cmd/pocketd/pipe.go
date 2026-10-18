package main

import (
	"errors"
	"os"
	"os/exec"
	"os/signal"
	"strings"
	"syscall"

	"pocketd/internal/launch"
	"pocketd/internal/pipe"
	"pocketd/internal/shellenv"
)

// pipeCmd runs argv with its stdio relayed to pocketd, which drives it from
// the chat. argv[0] may name a shell function, as in the user's terminal.
func pipeCmd(sock string, argv []string) (int, error) {
	id := os.Getenv("POCKETD_PTY")
	if id == "" {
		return 0, errors.New("pipe runs only in a Pocket terminal")
	}
	if !strings.ContainsRune(argv[0], os.PathSeparator) {
		argv = launch.InShell(shellenv.LoginShell(), argv)
	}
	cmd := exec.Command(argv[0], argv[1:]...)
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
