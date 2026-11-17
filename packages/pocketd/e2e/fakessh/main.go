// Fakessh stands in for ssh in e2e. It runs the command under sh with HOME=$FAKESSH_HOME, as if that folder were
// the host's home, and with -L forwards a unix socket here to one there. Without that folder the connection is
// refused, a key sign-in is refused while $FAKESSH_HOME/.fakessh-no-keys exists, and a password sign-in must give
// the password in $FAKESSH_HOME/.fakessh-password.
package main

import (
	"fmt"
	"io"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

func main() {
	args := os.Args[1:]
	var forward string
	password := false
	// Every option pocketd passes takes a value, and the destination always follows --.
	for len(args) > 1 && args[0] != "--" {
		switch args[0] {
		case "-L":
			forward = args[1]
		case "-o":
			password = password || args[1] == "BatchMode=no"
		}
		args = args[2:]
	}
	if len(args) < 3 {
		fmt.Fprintln(os.Stderr, "fakessh: want options -- destination command")
		os.Exit(255)
	}
	dest, command := args[1], strings.Join(args[2:], " ")
	home := os.Getenv("FAKESSH_HOME")
	if st, err := os.Stat(home); home == "" || err != nil || !st.IsDir() {
		fmt.Fprintf(os.Stderr, "ssh: connect to host %s port 22: Connection refused\n", dest)
		os.Exit(255)
	}
	if _, err := os.Stat(filepath.Join(home, ".fakessh-no-keys")); err == nil && !password {
		fmt.Fprintf(os.Stderr, "dev@%s: Permission denied (publickey).\n", dest)
		os.Exit(255)
	}
	if password {
		want, _ := os.ReadFile(filepath.Join(home, ".fakessh-password"))
		got, _ := exec.Command(os.Getenv("SSH_ASKPASS")).Output()
		if len(want) == 0 || strings.TrimSpace(string(got)) != strings.TrimSpace(string(want)) {
			fmt.Fprintf(os.Stderr, "dev@%s: Permission denied (publickey,password).\n", dest)
			os.Exit(255)
		}
	}
	if forward != "" {
		local, remote, _ := strings.Cut(forward, ":")
		os.Remove(local)
		ln, err := net.Listen("unix", local)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Bad local forwarding specification: %v\n", err)
			os.Exit(255)
		}
		go serve(ln, remote)
	}
	cmd := exec.Command("sh", "-c", command)
	cmd.Env = append(os.Environ(), "HOME="+home)
	cmd.Stdin, cmd.Stdout, cmd.Stderr = os.Stdin, os.Stdout, os.Stderr
	cmd.Run()
	os.Exit(cmd.ProcessState.ExitCode())
}

// serve forwards each connection to remote, closing it at once when nothing listens there, as ssh does.
func serve(ln net.Listener, remote string) {
	for {
		local, err := ln.Accept()
		if err != nil {
			return
		}
		go func() {
			defer local.Close()
			there, err := net.Dial("unix", remote)
			if err != nil {
				return
			}
			defer there.Close()
			go func() {
				io.Copy(there, local)
				there.Close()
			}()
			io.Copy(local, there)
		}()
	}
}
