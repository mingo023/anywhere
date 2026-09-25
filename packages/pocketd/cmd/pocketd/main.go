package main

import (
	"fmt"
	"os"

	"pocketd/internal/config"
)

const usage = "usage: pocketd serve | run <cmd> [args...] | attach <id> | hook"

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, usage)
		os.Exit(2)
	}
	sock := config.Sock()
	var err error
	code := 0
	switch {
	case os.Args[1] == "serve":
		err = serve(sock)
	case os.Args[1] == "run" && len(os.Args) > 2:
		code, err = run(sock, "", os.Args[2], os.Args[3:])
	case os.Args[1] == "hook":
		err = hook(sock)
	case os.Args[1] == "attach" && len(os.Args) == 3:
		code, err = run(sock, os.Args[2], "", nil)
	default:
		fmt.Fprintln(os.Stderr, usage)
		os.Exit(2)
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "pocketd:", err)
		os.Exit(1)
	}
	os.Exit(code)
}
