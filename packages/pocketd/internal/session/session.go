package session

import (
	"crypto/rand"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/creack/pty"

	"pocketd/internal/vt"
)

type Info struct {
	ID   string   `json:"id"`
	Cmd  string   `json:"cmd"`
	Args []string `json:"args,omitempty"`
	Cwd  string   `json:"cwd"`
	Cols int      `json:"cols"`
	Rows int      `json:"rows"`
}

type Spec struct {
	ID   string
	Cmd  string
	Args []string
	Cwd  string
	Env  []string
	Cols int
	Rows int
}

type Event struct {
	Kind string
	Data []byte
	Cols int
	Rows int
	Code int
}

type subscriber struct {
	fn  func(Event)
	tty bool
}

type Session struct {
	info   Info
	mu     sync.Mutex
	pty    *os.File
	cmd    *exec.Cmd
	vt     *vt.VT
	subs   map[*subscriber]bool
	done   chan struct{}
	code   int
	closed bool
}

type Manager struct {
	mu       sync.Mutex
	sessions map[string]*Session
}

func NewManager() *Manager {
	return &Manager{sessions: map[string]*Session{}}
}

func NewID() string {
	var b [16]byte
	rand.Read(b[:])
	b[6] = b[6]&0x0f | 0x40
	b[8] = b[8]&0x3f | 0x80
	return fmt.Sprintf("%x-%x-%x-%x-%x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:])
}

// LookPath resolves cmd against the caller's PATH, not the daemon's: a
// daemon started by launchd has a minimal PATH. The last PATH entry wins, as
// it does for exec.Cmd.Env.
func LookPath(cmd string, env []string) (string, error) {
	if strings.Contains(cmd, "/") {
		return cmd, nil
	}
	path, found := "", false
	for _, kv := range env {
		if p, ok := strings.CutPrefix(kv, "PATH="); ok {
			path, found = p, true
		}
	}
	if !found {
		return exec.LookPath(cmd)
	}
	for _, dir := range filepath.SplitList(path) {
		f := filepath.Join(dir, cmd)
		if st, err := os.Stat(f); err == nil && !st.IsDir() && st.Mode()&0o111 != 0 {
			return f, nil
		}
	}
	return "", &exec.Error{Name: cmd, Err: exec.ErrNotFound}
}

func (m *Manager) Spawn(spec Spec) (*Session, error) {
	if spec.ID == "" {
		spec.ID = NewID()
	}
	if spec.Cols <= 0 || spec.Rows <= 0 {
		spec.Cols, spec.Rows = 80, 24
	}
	if spec.Env == nil {
		spec.Env = os.Environ()
	}
	bin, err := LookPath(spec.Cmd, spec.Env)
	if err != nil {
		return nil, err
	}
	cmd := exec.Command(bin, spec.Args...)
	cmd.Dir = spec.Cwd
	cmd.Env = spec.Env
	f, err := pty.StartWithSize(cmd, &pty.Winsize{Cols: uint16(spec.Cols), Rows: uint16(spec.Rows)})
	if err != nil {
		return nil, err
	}
	s := &Session{
		info: Info{ID: spec.ID, Cmd: spec.Cmd, Args: spec.Args, Cwd: spec.Cwd, Cols: spec.Cols, Rows: spec.Rows},
		pty:  f,
		cmd:  cmd,
		subs: map[*subscriber]bool{},
		done: make(chan struct{}),
	}
	s.vt, err = vt.New(spec.Cols, spec.Rows, s.replyToQuery)
	if err != nil {
		cmd.Process.Kill()
		f.Close()
		return nil, err
	}
	m.mu.Lock()
	m.sessions[s.info.ID] = s
	m.mu.Unlock()
	go s.pump(func() {
		m.mu.Lock()
		delete(m.sessions, s.info.ID)
		m.mu.Unlock()
	})
	return s, nil
}

func (m *Manager) Get(id string) *Session {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.sessions[id]
}

func (m *Manager) List() []Info {
	m.mu.Lock()
	defer m.mu.Unlock()
	out := []Info{}
	for _, s := range m.sessions {
		out = append(out, s.Info())
	}
	return out
}

// replyToQuery runs under s.mu (inside vt.Write). A real terminal attached
// through `pocketd run` answers queries itself; answering twice corrupts input.
func (s *Session) replyToQuery(b []byte) {
	for sub := range s.subs {
		if sub.tty {
			return
		}
	}
	s.pty.Write(b)
}

func (s *Session) pump(onExit func()) {
	buf := make([]byte, 32*1024)
	for {
		n, err := s.pty.Read(buf)
		if n > 0 {
			chunk := append([]byte(nil), buf[:n]...)
			s.mu.Lock()
			s.vt.Write(chunk)
			s.broadcast(Event{Kind: "output", Data: chunk})
			s.mu.Unlock()
		}
		if err != nil {
			break
		}
	}
	s.cmd.Wait()
	onExit()
	s.mu.Lock()
	s.closed = true
	s.code = s.cmd.ProcessState.ExitCode()
	s.broadcast(Event{Kind: "exit", Code: s.code})
	s.vt.Free()
	s.pty.Close()
	s.mu.Unlock()
	close(s.done)
}

func (s *Session) broadcast(e Event) {
	for sub := range s.subs {
		sub.fn(e)
	}
}

func (s *Session) Info() Info {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.info
}

// Attach returns a snapshot of the screen and streams every later event to
// fn. fn runs with the session locked, so it must not call back into s.
func (s *Session) Attach(tty bool, fn func(Event)) (snapshot []byte, detach func(), err error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return nil, nil, errors.New("session closed")
	}
	sub := &subscriber{fn: fn, tty: tty}
	s.subs[sub] = true
	return s.vt.Snapshot(), func() {
		s.mu.Lock()
		delete(s.subs, sub)
		s.mu.Unlock()
	}, nil
}

func (s *Session) Write(b []byte) error {
	_, err := s.pty.Write(b)
	return err
}

// Prompt types text, then Enter. TUIs treat a fast "text\r" burst as a paste
// and keep the newline, so Enter goes out after a pause.
func (s *Session) Prompt(text string) error {
	if err := s.Write([]byte(text)); err != nil {
		return err
	}
	go func() {
		time.Sleep(150 * time.Millisecond)
		s.Write([]byte("\r"))
	}()
	return nil
}

func (s *Session) Resize(cols, rows int) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return
	}
	s.info.Cols, s.info.Rows = cols, rows
	pty.Setsize(s.pty, &pty.Winsize{Cols: uint16(cols), Rows: uint16(rows)})
	s.vt.Resize(cols, rows)
	s.broadcast(Event{Kind: "resize", Cols: cols, Rows: rows})
}

func (s *Session) Screen() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return ""
	}
	return s.vt.Plain()
}

func (s *Session) Close() {
	s.cmd.Process.Kill()
}

func (s *Session) Done() <-chan struct{} { return s.done }

func (s *Session) ExitCode() int {
	<-s.done
	return s.code
}
