package terminal

import (
	"bytes"
	"crypto/rand"
	"errors"
	"fmt"
	"maps"
	"os"
	"slices"
	"sync"
	"time"

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
	Local      string   `json:"local,omitempty"`
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
	// Local is the Local the Terminal was opened in, fixed for life; empty
	// means it belongs to the Worktree holding Cwd.
	Local string
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
	pty    console
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
	osc    scanner
	last   *Report
	report func(id string, r Report)
	exit   func()     // forgets the Terminal once its process ends
	typing sync.Mutex // held by a Prompt until its Enter goes out
}

type Manager struct {
	mu        sync.Mutex
	terminals map[string]*Terminal
	OnInput   func(id string, b []byte) // sees each Write, before the process does
	OnReport  func(id string, r Report) // each root OSC 7501 report, in order, outside the terminal's lock
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
	f, p, err := start(bin, spec)
	if err != nil {
		return nil, err
	}
	s := &Terminal{
		info:   Info{ID: spec.ID, Cmd: spec.Cmd, Args: spec.Args, Cwd: spec.Cwd, Cols: spec.Cols, Rows: spec.Rows, Local: spec.Local},
		pty:    f,
		proc:   p,
		subs:   map[*subscriber]bool{},
		done:   make(chan struct{}),
		input:  m.OnInput,
		report: m.OnReport,
		origin: spec.Origin,
	}
	s.vt, err = vt.New(spec.Cols, spec.Rows, s.replyToQuery)
	if err != nil {
		p.Kill()
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
	s.exit = func() {
		m.mu.Lock()
		delete(m.terminals, s.info.ID)
		m.mu.Unlock()
	}
	go s.pump(s.pty, s.proc)
}

// Adopted is a Terminal the image before exec handed down.
type Adopted struct {
	ID, Cmd             string
	Args                []string
	Cwd                 string
	Cols, Rows, FD, Pid int
	Screen              []byte
	Local               string
}

// Adopt takes over a handed-down Terminal. Its fd is still non-blocking,
// so the new file is pollable too.
func (m *Manager) Adopt(a Adopted) (*Terminal, error) {
	f, p, err := adopt(a.FD, a.Pid)
	if err != nil {
		return nil, err
	}
	s := &Terminal{
		info:   Info{ID: a.ID, Cmd: a.Cmd, Args: a.Args, Cwd: a.Cwd, Cols: a.Cols, Rows: a.Rows, Local: a.Local},
		pty:    f,
		proc:   p,
		subs:   map[*subscriber]bool{},
		done:   make(chan struct{}),
		input:  m.OnInput,
		report: m.OnReport,
	}
	if s.vt, err = vt.New(a.Cols, a.Rows, s.replyToQuery); err != nil {
		f.Close()
		return nil, err
	}
	s.vt.Write(a.Screen)
	m.start(s)
	return s, nil
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

// pump reads f until it ends. A Replace that swapped f out makes it stop
// quietly: the Terminal lives on in the new process.
func (s *Terminal) pump(f console, p *os.Process) {
	buf := make([]byte, 32*1024)
	for {
		n, err := f.Read(buf)
		if n > 0 {
			chunk := append([]byte(nil), buf[:n]...)
			s.mu.Lock()
			if s.pty != f {
				s.mu.Unlock()
				break
			}
			reports, query := s.osc.feed(chunk)
			for _, r := range reports {
				if r.State == "clear" {
					s.last = nil
				} else {
					s.last = &r
				}
			}
			if query {
				s.replyToQuery([]byte(queryReply))
			}
			s.vt.Write(chunk)
			s.broadcast(Event{Kind: "output", Data: chunk})
			s.mu.Unlock()
			if s.report != nil {
				for _, r := range reports {
					s.report(s.info.ID, r)
				}
			}
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
	if st, err := p.Wait(); err == nil {
		code = st.ExitCode()
	}
	s.mu.Lock()
	if s.pty != f {
		s.mu.Unlock()
		f.Close()
		return
	}
	s.closed = true
	// exit takes Manager.mu, which Roots holds while it reads Pid under s.mu.
	s.mu.Unlock()
	s.exit()
	s.mu.Lock()
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

func (s *Terminal) broadcast(e Event) {
	for sub := range s.subs {
		sub.fn(e)
	}
}

// Program is the latest OSC 7501 report, nil before one or after a clear.
func (s *Terminal) Program() *Report {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.last
}

// ClearProgram forgets the latest report, once its program has ended.
func (s *Terminal) ClearProgram() {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.last = nil
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

func (s *Terminal) Pid() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.proc.Pid
}

func (s *Terminal) Pgrp() (int, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return 0, nil
	}
	return s.group()
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
	s.mu.Lock()
	f := s.pty
	s.mu.Unlock()
	_, err := f.Write(b)
	return err
}

// Replace ends the Terminal's session and runs cmd in its place, keeping the
// id, the size and whoever is attached. Attached screens are reset, and no
// exit is reported for the old process. A closed or paused Terminal refuses,
// and the old session stays as it was when cmd doesn't start. origin is who
// asked for cmd, as Spec.Origin.
func (s *Terminal) Replace(cmd string, args, env []string, origin string) error {
	bin, err := LookPath(cmd, env)
	if err != nil {
		return err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return ErrClosed
	}
	if s.resume != nil {
		return fmt.Errorf("terminal %s: paused", s.info.ID)
	}
	f, p, err := start(bin, Spec{Cmd: cmd, Args: args, Cwd: s.info.Cwd, Env: env, Cols: s.info.Cols, Rows: s.info.Rows})
	if err != nil {
		return err
	}
	oldPty, oldProc := s.pty, s.proc
	s.pty, s.proc = f, p
	s.info.Cmd, s.info.Args, s.origin = cmd, args, origin
	s.last, s.osc = nil, scanner{}
	reset := []byte("\x1bc") // RIS: the new process starts on a blank screen
	s.vt.Write(reset)
	s.broadcast(Event{Kind: "output", Data: reset})
	go endSession(oldPty, oldProc)
	go s.pump(f, p)
	return nil
}

// Prompt types text, then Enter. TUIs treat a fast "text\r" burst as a paste
// and keep the newline, so Enter goes out after a pause. A later Prompt waits
// for that Enter, so two prompts never run together.
func (s *Terminal) Prompt(text string) error {
	return s.prompt(text)
}

// PromptOver is Prompt into an input that may hold text, as Claude's does
// after a stop before its reply: Ctrl+U first clears it. Claude clears one
// line per Ctrl+U and takes a longer burst as a paste, so they go out in short
// bursts, enough for a long prompt.
func (s *Terminal) PromptOver(text string) error {
	clear := bytes.Repeat([]byte{0x15}, 16)
	return s.prompt(text, clear, clear, clear, clear)
}

// PromptStashed is Prompt into an input that holds the user's draft: Claude's
// Ctrl+S puts the draft aside, and Claude puts it back once text is sent.
func (s *Terminal) PromptStashed(text string) error {
	return s.prompt(text, []byte{0x13})
}

// prompt types each of keys, a burst at a time, before text.
func (s *Terminal) prompt(text string, keys ...[]byte) error {
	text, err := Sanitize(text)
	if err != nil {
		return err
	}
	s.typing.Lock()
	for _, k := range keys {
		if err := s.Write(k); err != nil {
			s.typing.Unlock()
			return err
		}
		time.Sleep(10 * time.Millisecond)
	}
	if err := s.Write([]byte(text)); err != nil {
		s.typing.Unlock()
		return err
	}
	go func() {
		defer s.typing.Unlock()
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
