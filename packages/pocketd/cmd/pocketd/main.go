package main

import (
	"errors"
	"fmt"
	"os"
	"slices"

	"golang.org/x/term"

	"pocketd/internal/config"
	"pocketd/internal/lock"
)

const usage = "usage: pocketd serve | run <cmd> [args...] | attach <id> | hook | pair [--host h:p] | devices [--json | rename <id> <name> | revoke <id>] | status [--json] | stats [--since 7d] [--json] | daemon install | daemon uninstall [--force] | --version | worktree list|create|remove | config set phone.maxAccess <ask|edits|auto> | config set restore.resumeAgents <true|false>"

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, usage)
		os.Exit(2)
	}
	if os.Args[1] == "worktree" {
		os.Exit(worktreeMain(os.Args[2:], os.Stdout, os.Stderr))
	}
	sock := config.Sock()
	var err error
	code := 0
	switch {
	case slices.Equal(os.Args[1:], []string{"--version"}):
		fmt.Println("pocketd", versionString())
	case os.Args[1] == "serve":
		err = serve(sock)
	case os.Args[1] == "run" && len(os.Args) > 2:
		code, err = run(sock, "", os.Args[2], os.Args[3:])
	case os.Args[1] == "hook" && len(os.Args) == 5 && os.Args[2] == "exit":
		err = hookExit(sock, os.Args[3], os.Args[4])
	case os.Args[1] == "config" && len(os.Args) == 5 && os.Args[2] == "set":
		err = configSet(sock, os.Args[3], os.Args[4], os.Stdout)
	case os.Args[1] == "hook":
		err = hook(sock)
	case os.Args[1] == "attach" && len(os.Args) == 3:
		code, err = run(sock, os.Args[2], "", nil)
	case os.Args[1] == "pair":
		code, err = pairCmd(sock, os.Args[2:], term.IsTerminal(int(os.Stdin.Fd())), os.Stdout, os.Stderr)
	case os.Args[1] == "devices":
		err = devicesCmd(sock, os.Args[2:], os.Stdout)
	case slices.Equal(os.Args[1:], []string{"status"}), slices.Equal(os.Args[1:], []string{"status", "--json"}):
		code, err = status(sock, len(os.Args) == 3)
	case os.Args[1] == "stats":
		code, err = stats(os.Args[2:])
	case slices.Equal(os.Args[1:], []string{"daemon", "install"}):
		err = install()
	case slices.Equal(os.Args[1:], []string{"daemon", "uninstall"}), slices.Equal(os.Args[1:], []string{"daemon", "uninstall", "--force"}):
		code, err = uninstall(len(os.Args) == 4)
	default:
		fmt.Fprintln(os.Stderr, usage)
		os.Exit(2)
	}
	if running := (lock.ErrRunning{}); errors.As(err, &running) {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(75)
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "pocketd:", err)
		os.Exit(1)
	}
	os.Exit(code)
}
