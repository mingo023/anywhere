package main

import (
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"os/user"
	"path/filepath"
	"runtime"
	"slices"

	"pocketd/internal/config"
	"pocketd/internal/service"
)

const serviceUsage = "usage: pocketd service install|uninstall|status [--json] [--label <name>]"

type serviceArgs struct {
	action, label string
	json          bool
}

func parseService(args []string, stderr io.Writer) (serviceArgs, bool) {
	if len(args) == 0 || !slices.Contains([]string{"install", "uninstall", "status"}, args[0]) {
		return serviceArgs{}, false
	}
	a := serviceArgs{action: args[0]}
	flags := flag.NewFlagSet("pocketd service "+args[0], flag.ContinueOnError)
	flags.SetOutput(stderr)
	flags.BoolVar(&a.json, "json", false, "print the state as JSON")
	flags.StringVar(&a.label, "label", config.HostLabel(), "service name")
	if flags.Parse(args[1:]) != nil || flags.NArg() > 0 {
		return a, false
	}
	return a, true
}

type serviceState struct {
	Label     string `json:"label"`
	Installed bool   `json:"installed"`
	Running   bool   `json:"running"`
	Linger    *bool  `json:"linger,omitempty"`
}

// serviceMain runs the action, then prints the service's state.
func serviceMain(args []string, stdout, stderr io.Writer) (int, error) {
	a, ok := parseService(args, stderr)
	if !ok {
		fmt.Fprintln(stderr, serviceUsage)
		return 2, nil
	}
	exe, err := os.Executable()
	if err == nil {
		exe, err = filepath.EvalSymlinks(exe)
	}
	if err != nil {
		return 1, err
	}
	s := service.Spec{Exe: exe, Home: config.Home(), Label: a.label}
	switch a.action {
	case "install":
		if why := (installCheck{Exe: exe, Temp: tempDirs()}).refusal(); why != "" {
			return 1, errors.New(why)
		}
		err = service.Install(s)
	case "uninstall":
		err = service.Uninstall(s)
	}
	if err != nil {
		return 1, err
	}
	st := serviceState{Label: s.Label}
	if st.Installed, st.Running, err = service.Status(s); err != nil {
		return 1, err
	}
	u, _ := user.Current()
	if runtime.GOOS == "linux" && u != nil {
		on := lingering(u.Username)
		st.Linger = &on
	}
	if a.json {
		out, _ := json.Marshal(st)
		fmt.Fprintln(stdout, string(out))
		return 0, nil
	}
	fmt.Fprintf(stdout, "%s: installed %t, running %t. Home: %s\n", st.Label, st.Installed, st.Running, s.Home)
	if st.Linger != nil && !*st.Linger && st.Installed {
		fmt.Fprintf(stdout, "Linger is off, so the service stops when %[1]s logs out. Fix: sudo loginctl enable-linger %[1]s\n", u.Username)
	}
	return 0, nil
}

// lingering: without it systemd stops a user's services at logout.
func lingering(name string) bool {
	_, err := os.Stat(filepath.Join("/var/lib/systemd/linger", name))
	return err == nil
}
