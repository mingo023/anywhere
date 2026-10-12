package terminal

import (
	"cmp"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"time"

	"pocketd/internal/conpty"
)

type console = *conpty.Console

// LookPath resolves cmd against the caller's Path and PATHEXT, not the
// daemon's. Windows env names ignore case; the last entry wins.
func LookPath(cmd string, env []string) (string, error) {
	if strings.ContainsAny(cmd, `/\:`) {
		return cmd, nil
	}
	path, found := "", false
	exts := ".COM;.EXE;.BAT;.CMD"
	for _, kv := range env {
		k, v, _ := strings.Cut(kv, "=")
		switch {
		case strings.EqualFold(k, "PATH"):
			path, found = v, true
		case strings.EqualFold(k, "PATHEXT"):
			exts = cmp.Or(v, exts)
		}
	}
	if !found {
		return exec.LookPath(cmd)
	}
	tries := []string{cmd}
	if ext := filepath.Ext(cmd); ext == "" || !slices.ContainsFunc(filepath.SplitList(exts), func(e string) bool { return strings.EqualFold(e, ext) }) {
		tries = nil
		for _, e := range filepath.SplitList(exts) {
			tries = append(tries, cmd+strings.ToLower(e))
		}
	}
	for _, dir := range filepath.SplitList(path) {
		for _, name := range tries {
			f := filepath.Join(dir, name)
			if st, err := os.Stat(f); err == nil && !st.IsDir() {
				return f, nil
			}
		}
	}
	return "", &exec.Error{Name: cmd, Err: exec.ErrNotFound}
}

func start(bin string, spec Spec) (*conpty.Console, *os.Process, error) {
	c, err := conpty.Start(append([]string{bin}, spec.Args...), spec.Cwd, spec.Env, spec.Cols, spec.Rows)
	if err != nil {
		return nil, nil, err
	}
	p, err := os.FindProcess(c.Pid())
	if err != nil {
		c.Kill()
		c.Close()
		return nil, nil, err
	}
	return c, p, nil
}

var errNoHandoff = errors.New("terminal: Windows can't hand a terminal to another pocketd")

func adopt(fd, pid int) (*conpty.Console, *os.Process, error) { return nil, nil, errNoHandoff }

func inherit(c *conpty.Console, on bool) (int, error) {
	if !on {
		return 0, nil
	}
	return 0, errNoHandoff
}

func setsize(c *conpty.Console, cols, rows int) error { return c.Resize(cols, rows) }

// group is the root: Windows has no foreground process group, so the whole
// tree under it stands in for one (proc.Members).
func (s *Terminal) group() (int, error) { return s.proc.Pid, nil }

// stop hangs up the console, so its programs get CTRL_CLOSE_EVENT, then
// kills the whole tree after grace.
func (s *Terminal) stop(grace time.Duration) {
	s.pty.Hangup()
	select {
	case <-s.done:
	case <-time.After(grace):
		s.pty.Kill()
	}
}
