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

// conSession is a process on a ConPTY pocketd holds; Windows has no holders.
type conSession struct {
	c *conpty.Console
	p *os.Process
}

func (s conSession) Read(b []byte) (int, error)  { return s.c.Read(b) }
func (s conSession) Write(b []byte) (int, error) { return s.c.Write(b) }
func (s conSession) Resize(cols, rows int) error { return s.c.Resize(cols, rows) }
func (s conSession) Pid() int                    { return s.p.Pid }
func (s conSession) Close() error                { return s.c.Close() }
func (s conSession) Checkpoint(int64, []byte)    {}

// Stop hangs up the console, so its programs get CTRL_CLOSE_EVENT, then
// kills the whole tree after grace.
func (s conSession) Stop(grace time.Duration) {
	s.c.Hangup()
	time.AfterFunc(grace, func() { s.c.Kill() })
}

func (s conSession) Wait() int {
	if st, err := s.p.Wait(); err == nil {
		return st.ExitCode()
	}
	return -1
}

func (m *Manager) open(bin string, spec Spec) (session, error) {
	c, p, err := start(bin, spec)
	if err != nil {
		return nil, err
	}
	return conSession{c, p}, nil
}

func (m *Manager) adopt(Adopted) (session, error) { return nil, errNoHandoff }

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

// group is the root: Windows has no foreground process group, so the whole
// tree under it stands in for one (proc.Members).
func group(sess session) (int, error) { return sess.Pid(), nil }

// stop hangs up the console, then kills the whole tree after grace, unless it ended first.
func (s *Terminal) stop(grace time.Duration) {
	s.mu.Lock()
	c := s.sess.(conSession).c
	s.mu.Unlock()
	c.Hangup()
	select {
	case <-s.done:
	case <-time.After(grace):
		c.Kill()
	}
}
