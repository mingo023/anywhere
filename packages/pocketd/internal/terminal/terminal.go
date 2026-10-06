package terminal

import (
	"cmp"
	"crypto/rand"
	"errors"
	"fmt"
	"maps"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/creack/pty"
	"golang.org/x/sys/unix"

	"pocketd/internal/proc"
	"pocketd/internal/vt"
)

type Info struct {
	ID         string   `json:"id"`
	Cmd        string   `json:"cmd"`
	Args       []string `json:"args,omitempty"`
	Cwd        string   `json:"cwd"`
	Cols       int      `json:"cols"`
	Rows       int      `json:"rows"`
	Foreground string   `json:"foreground,omitempty"`
}

type Spec struct {
	ID   string
	Cmd  string
	Args []string
	Cwd  string
	Env  []string
	Cols int
	Rows int
	// Origin is who asked for the Terminal; its first agent takes it.
	Origin string
}

type Event struct {
	Kind string
	Data []byte
	Cols int
	Rows int
	Code int
	Text string
}

var ErrClosed = errors.New("terminal closed")

type subscriber struct {
	fn  func(Event)
	tty bool
}

type Terminal struct {
	info   Info
	mu     sync.Mutex
	pty    *os.File
	proc   *os.Process
	vt     *vt.VT
	subs   map[*subscriber]bool
	done   chan struct{}
	code   int
	closed bool
	input  func(id string, b []byte)
	origin string
	resume chan struct{} // closed by Resume; nil unless paused
	parked chan struct{} // closed once the pump stops reading for this pause
}

type Manager struct {
	mu        sync.Mutex
	terminals map[string]*Terminal
	OnInput   func(id string, b []byte) // sees each Write, before the process does
}

func NewManager() *Manager {
	return &Manager{terminals: map[string]*Terminal{}}
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

func (m *Manager) Spawn(spec Spec) (*Terminal, error) {
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
	if f, err = pollable(f); err != nil {
		cmd.Process.Kill()
		cmd.Wait()
		return nil, err
	}
	s := &Terminal{
		info:   Info{ID: spec.ID, Cmd: spec.Cmd, Args: spec.Args, Cwd: spec.Cwd, Cols: spec.Cols, Rows: spec.Rows},
		pty:    f,
		proc:   cmd.Process,
		subs:   map[*subscriber]bool{},
		done:   make(chan struct{}),
		input:  m.OnInput,
		origin: spec.Origin,
	}
	s.vt, err = vt.New(spec.Cols, spec.Rows, s.replyToQuery)
	if err != nil {
		cmd.Process.Kill()
		f.Close()
		return nil, err
	}
	m.start(s)
	return s, nil
}

func (m *Manager) start(s *Terminal) {
	m.mu.Lock()
	m.terminals[s.info.ID] = s
	m.mu.Unlock()
	go s.pump(func() {
		m.mu.Lock()
		delete(m.terminals, s.info.ID)
		m.mu.Unlock()
	})
}

// Adopted is a Terminal the image before exec handed down.
type Adopted struct {
	ID, Cmd             string
	Args                []string
	Cwd                 string
	Cols, Rows, FD, Pid int
	Screen              []byte
}

// Adopt takes over a handed-down Terminal. Its fd is still non-blocking,
// so the new file is pollable too.
func (m *Manager) Adopt(a Adopted) (*Terminal, error) {
	syscall.CloseOnExec(a.FD)
	f := os.NewFile(uintptr(a.FD), "/dev/ptmx")
	p, _ := os.FindProcess(a.Pid) // never fails on unix
	s := &Terminal{
		info:  Info{ID: a.ID, Cmd: a.Cmd, Args: a.Args, Cwd: a.Cwd, Cols: a.Cols, Rows: a.Rows},
		pty:   f,
		proc:  p,
		subs:  map[*subscriber]bool{},
		done:  make(chan struct{}),
		input: m.OnInput,
	}
	var err error
	if s.vt, err = vt.New(a.Cols, a.Rows, s.replyToQuery); err != nil {
		f.Close()
		return nil, err
	}
	s.vt.Write(a.Screen)
	m.start(s)
	return s, nil
}

// pollable moves f to a non-blocking fd the runtime poller owns, so a read
// deadline can stop the pump. creack/pty's file blocks a thread instead.
func pollable(f *os.File) (*os.File, error) {
	defer f.Close()
	fd, err := unix.FcntlInt(f.Fd(), unix.F_DUPFD_CLOEXEC, 0)
	if err != nil {
		return nil, err
	}
	if err := unix.SetNonblock(fd, true); err != nil {
		unix.Close(fd)
		return nil, err
	}
	return os.NewFile(uintptr(fd), f.Name()), nil
}

// setsize is pty.Setsize without Fd, which would make f blocking again.
func setsize(f *os.File, cols, rows int) error {
	rc, err := f.SyscallConn()
	if err != nil {
		return err
	}
	var ioErr error
	err = rc.Control(func(fd uintptr) {
		ioErr = unix.IoctlSetWinsize(int(fd), unix.TIOCSWINSZ, &unix.Winsize{Col: uint16(cols), Row: uint16(rows)})
	})
	return cmp.Or(err, ioErr)
}

func (m *Manager) Get(id string) *Terminal {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.terminals[id]
}

func (m *Manager) List() []Info {
	m.mu.Lock()
	defer m.mu.Unlock()
	out := []Info{}
	for _, s := range m.terminals {
		out = append(out, s.Info())
	}
	return out
}

func (m *Manager) All() []*Terminal {
	m.mu.Lock()
	defer m.mu.Unlock()
	return slices.Collect(maps.Values(m.terminals))
}

func (m *Manager) Roots() map[int]string {
	m.mu.Lock()
	defer m.mu.Unlock()
	roots := make(map[int]string, len(m.terminals))
	for id, s := range m.terminals {
		roots[s.Pid()] = id
	}
	return roots
}

// replyToQuery runs under s.mu (inside vt.Write). A real terminal attached
// through `pocketd run` answers queries itself; answering twice corrupts input.
func (s *Terminal) replyToQuery(b []byte) {
	for sub := range s.subs {
		if sub.tty {
			return
		}
	}
	s.pty.Write(b)
}

func (s *Terminal) pump(onExit func()) {
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
		if errors.Is(err, os.ErrDeadlineExceeded) {
			s.park()
			continue
		}
		if err != nil {
			break
		}
	}
	code := -1
	if st, err := s.proc.Wait(); err == nil {
		code = st.ExitCode()
	}
	onExit()
	s.mu.Lock()
	s.closed = true
	s.code = code
	s.broadcast(Event{Kind: "exit", Code: s.code})
	s.vt.Free()
	s.pty.Close()
	s.mu.Unlock()
	close(s.done)
}

// park holds the pump while paused. A Resume that beat the deadline leaves
// nothing to wait for.
func (s *Terminal) park() {
	s.mu.Lock()
	parked, resume := s.parked, s.resume
	s.mu.Unlock()
	if resume == nil {
		return
	}
	close(parked)
	<-resume
}

// Pause stops reading the pty, so output waits in the kernel, and returns
// once the pump has stopped or wait has passed.
func (s *Terminal) Pause(wait time.Duration) error {
	s.mu.Lock()
	if s.closed {
		s.mu.Unlock()
		return ErrClosed
	}
	if s.resume == nil {
		if err := s.pty.SetReadDeadline(time.Now()); err != nil {
			s.mu.Unlock()
			return err
		}
		s.resume, s.parked = make(chan struct{}), make(chan struct{})
	}
	parked := s.parked
	s.mu.Unlock()
	select {
	case <-parked:
		return nil
	case <-s.done:
		return ErrClosed
	case <-time.After(wait):
		return fmt.Errorf("terminal %s: reader didn't pause", s.info.ID)
	}
}

// Resume reads the pty again and keeps it from surviving an exec.
func (s *Terminal) Resume() {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.resume == nil {
		return
	}
	inherit(s.pty, false)
	s.pty.SetReadDeadline(time.Time{})
	close(s.resume)
	s.resume, s.parked = nil, nil
}

// Handed is what the next image needs to adopt a Terminal.
type Handed struct {
	FD, Pid int
	Screen  []byte
}

// Handoff readies a paused Terminal for exec: its pty survives it, and
// Screen redraws what it shows.
func (s *Terminal) Handoff() (Handed, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return Handed{}, ErrClosed
	}
	if s.resume == nil {
		return Handed{}, fmt.Errorf("terminal %s: handoff needs a paused terminal", s.info.ID)
	}
	fd, err := inherit(s.pty, true)
	if err != nil {
		return Handed{}, err
	}
	return Handed{FD: fd, Pid: s.proc.Pid, Screen: s.vt.Snapshot()}, nil
}

// inherit sets whether f survives exec, and returns its fd. Fd would make f blocking again.
func inherit(f *os.File, on bool) (int, error) {
	rc, err := f.SyscallConn()
	if err != nil {
		return 0, err
	}
	var fd int
	var ioErr error
	err = rc.Control(func(p uintptr) {
		fd = int(p)
		flags := unix.FD_CLOEXEC
		if on {
			flags = 0
		}
		_, ioErr = unix.FcntlInt(p, unix.F_SETFD, flags)
	})
	return fd, cmp.Or(err, ioErr)
}

func (s *Terminal) broadcast(e Event) {
	for sub := range s.subs {
		sub.fn(e)
	}
}

func (s *Terminal) Info() Info {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.info
}

// TakeOrigin is Spec.Origin once, then "": a later agent in the same Terminal
// was started by whoever typed it.
func (s *Terminal) TakeOrigin() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	o := s.origin
	s.origin = ""
	return o
}

func (s *Terminal) Pid() int { return s.proc.Pid }

func (s *Terminal) Pgrp() (int, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return 0, nil
	}
	return proc.Foreground(s.pty)
}

func (s *Terminal) SetForeground(text string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed || s.info.Foreground == text {
		return
	}
	s.info.Foreground = text
	s.broadcast(Event{Kind: "foreground", Text: text})
}

// Attach returns a snapshot of the screen and streams every later event to
// fn. fn runs with the terminal locked, so it must not call back into s.
func (s *Terminal) Attach(tty bool, fn func(Event)) (snapshot []byte, detach func(), err error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return nil, nil, ErrClosed
	}
	sub := &subscriber{fn: fn, tty: tty}
	s.subs[sub] = true
	return s.vt.Snapshot(), func() {
		s.mu.Lock()
		delete(s.subs, sub)
		s.mu.Unlock()
	}, nil
}

func (s *Terminal) Write(b []byte) error {
	if s.input != nil {
		s.input(s.info.ID, b)
	}
	_, err := s.pty.Write(b)
	return err
}

// Prompt types text, then Enter. TUIs treat a fast "text\r" burst as a paste
// and keep the newline, so Enter goes out after a pause.
func (s *Terminal) Prompt(text string) error {
	text, err := Sanitize(text)
	if err != nil {
		return err
	}
	if err := s.Write([]byte(text)); err != nil {
		return err
	}
	go func() {
		time.Sleep(150 * time.Millisecond)
		s.Write([]byte("\r"))
	}()
	return nil
}

func (s *Terminal) Resize(cols, rows int) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return
	}
	s.info.Cols, s.info.Rows = cols, rows
	setsize(s.pty, cols, rows)
	s.vt.Resize(cols, rows)
	s.broadcast(Event{Kind: "resize", Cols: cols, Rows: rows})
}

func (s *Terminal) Screen() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return ""
	}
	return s.vt.Plain()
}

// CloseGrace is how long Close waits after SIGTERM before SIGKILL.
var CloseGrace = 2 * time.Second

// Close ends the Terminal's whole session in the background; Done reports the end.
func (s *Terminal) Close() { go s.stop(CloseGrace) }

// stop signals every process group in the shell's session: job-control jobs
// get their own groups, and nohup'd children ignore the pty's SIGHUP.
// SIGHUP goes with SIGTERM because an interactive shell ignores SIGTERM.
func (s *Terminal) stop(grace time.Duration) {
	sid := s.Pid()
	signal := func(sigs ...syscall.Signal) {
		pgids, err := proc.Session(sid)
		if err != nil {
			pgids = []int{sid}
		}
		for _, pg := range pgids {
			for _, sig := range sigs {
				syscall.Kill(-pg, sig)
			}
		}
	}
	signal(syscall.SIGHUP, syscall.SIGTERM)
	for deadline := time.Now().Add(grace); time.Now().Before(deadline); time.Sleep(50 * time.Millisecond) {
		if pgids, err := proc.Session(sid); err == nil && len(pgids) == 0 {
			return
		}
	}
	signal(syscall.SIGKILL)
}

// CloseAll stops every Terminal at once. It returns when all have ended or grace+500ms has passed.
func (m *Manager) CloseAll(grace time.Duration) {
	var wg sync.WaitGroup
	for _, s := range m.All() {
		wg.Go(func() {
			s.stop(grace)
			<-s.done
		})
	}
	ended := make(chan struct{})
	go func() {
		wg.Wait()
		close(ended)
	}()
	select {
	case <-ended:
	case <-time.After(grace + 500*time.Millisecond):
	}
}

func (s *Terminal) Done() <-chan struct{} { return s.done }

func (s *Terminal) ExitCode() int {
	<-s.done
	return s.code
}
