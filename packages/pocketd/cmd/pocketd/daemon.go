package main

import (
	"cmp"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"pocketd/internal/launchagent"
	"pocketd/internal/lock"
	"pocketd/internal/ops"
	"pocketd/internal/peer"
)

// installCheck is what decides whether the service may run this binary.
type installCheck struct {
	Exe         string   // symlinks resolved
	Temp        []string // dirs a lasting binary can't live in, symlinks resolved
	Home, Sock  string   // POCKET_HOME and POCKETD_SOCK as set
	DefaultHome string
	InTerminal  bool
}

// refusal says why install must not go ahead, or "" if it may.
func (c installCheck) refusal() string {
	for _, dir := range c.Temp {
		if strings.HasPrefix(c.Exe, dir+string(filepath.Separator)) {
			return fmt.Sprintf("%s is a temporary build and will be deleted. Build it with go build -o bin/pocketd ./cmd/pocketd, then run bin/pocketd daemon install.", c.Exe)
		}
	}
	if c.Home != "" && filepath.Clean(c.Home) != c.DefaultHome {
		return fmt.Sprintf("POCKET_HOME is set to %s. The service always uses %s; unset it first.", c.Home, c.DefaultHome)
	}
	if c.Sock != "" && filepath.Clean(c.Sock) != filepath.Join(c.DefaultHome, "pocketd.sock") {
		return fmt.Sprintf("POCKETD_SOCK is set to %s. The service always uses %s; unset it first.", c.Sock, filepath.Join(c.DefaultHome, "pocketd.sock"))
	}
	if c.InTerminal {
		return "This is a Pocket Terminal. Run pocketd daemon install from a terminal outside Pocket."
	}
	return ""
}

func tempDirs() []string {
	dirs := []string{os.TempDir(), "/tmp"}
	if cache, err := os.UserCacheDir(); err == nil {
		dirs = append(dirs, cmp.Or(os.Getenv("GOCACHE"), filepath.Join(cache, "go-build")))
	}
	for i, d := range dirs {
		dirs[i] = filepath.Clean(d)
		if r, err := filepath.EvalSymlinks(dirs[i]); err == nil {
			dirs[i] = r
		}
	}
	return dirs
}

func install() error {
	user, err := os.UserHomeDir()
	if err != nil {
		return err
	}
	exe, err := os.Executable()
	if err == nil {
		exe, err = filepath.EvalSymlinks(exe)
	}
	if err != nil {
		return err
	}
	home := filepath.Join(user, ".coding-pocket")
	holder, running := lock.Holder(home)
	check := installCheck{Exe: exe, Temp: tempDirs(), Home: os.Getenv("POCKET_HOME"), Sock: os.Getenv("POCKETD_SOCK"), DefaultHome: home,
		InTerminal: running && peer.Classify(os.Getpid(), map[int]string{holder: "pocketd"}).Kind == peer.PTY}
	if why := check.refusal(); why != "" {
		return errors.New(why)
	}
	logs := filepath.Join(home, "logs")
	if err := os.MkdirAll(logs, 0o700); err != nil {
		return err
	}
	// launchd would create its log world-readable.
	f, err := os.OpenFile(filepath.Join(logs, "launchd.log"), os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		return err
	}
	f.Close()
	path := launchagent.Path(user)
	loaded, err := launchagent.Install(launchagent.Spec{Exe: exe, LogPath: f.Name()}, path, os.Getuid())
	switch {
	case err != nil:
		return err
	case loaded:
		fmt.Printf("Updated %s. Applies at next login, or run pocketd daemon uninstall then install.\n", path)
	case running:
		fmt.Printf("Installed. The service takes over when the running pocketd (pid %d) exits.\n", holder)
	default:
		fmt.Printf("Installed. pocketd starts at login and restarts if it crashes. Logs: %s\n", filepath.Join(logs, "pocketd.log"))
	}
	return nil
}

func uninstall(force bool) (int, error) {
	user, err := os.UserHomeDir()
	if err != nil {
		return 1, err
	}
	home := filepath.Join(user, ".coding-pocket")
	loaded, _ := launchagent.Loaded(os.Getuid())
	if loaded && !force {
		if n := terminals(filepath.Join(home, "pocketd.sock")); n > 0 {
			fmt.Fprintf(os.Stderr, "%d Terminals are running and will close. Run again with --force.\n", n)
			return 1, nil
		}
	}
	if err := launchagent.Uninstall(launchagent.Path(user), os.Getuid()); err != nil {
		return 1, err
	}
	if loaded {
		fmt.Println("The service is stopped and will not start automatically.")
	} else {
		fmt.Println("The service is removed. A pocketd you started yourself keeps running.")
	}
	fmt.Printf("Sessions, logs and device credentials are kept in %s. Delete that directory only if you want to erase them.\n", home)
	return 0, nil
}

// terminals counts the Terminals the pocketd on sock runs; 0 if none answers.
func terminals(sock string) int {
	c, err := ops.Dial(sock)
	if err != nil {
		return 0
	}
	defer c.Close()
	if c.Send(ops.Msg{Op: "list"}) != nil {
		return 0
	}
	m, err := c.Recv()
	if err != nil {
		return 0
	}
	return len(m.Items)
}
