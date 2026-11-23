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

	"pocketd/internal/holder"
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
	// SignIn marks an account's sign-in, which a desktop keeps out of its projects and tabs.
	SignIn bool `json:"signin,omitempty"`
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
	Local  string
	SignIn bool
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

// session is a Terminal's process: on a pty pocketd holds, or under a holder.
type session interface {
	Read([]byte) (int, error)
	Write([]byte) (int, error)
	Resize(cols, rows int) error
	Pid() int
	// Wait is the exit code once Read has ended; -1 when unknown.
	Wait() int
	// Stop ends the whole session: SIGHUP and SIGTERM, then SIGKILL after grace.
	Stop(grace time.Duration)
	Close() error
	// Checkpoint keeps the screen after the first read bytes Read returned, for a pocketd that reattaches.
	Checkpoint(read int64, screen []byte)
}

// checkpointEvery is how much output a Terminal reads between checkpoints.
const checkpointEvery = 256 << 10

type Terminal struct {
	info   Info
	mu     sync.Mutex
	sess   session
	open   func(bin string, spec Spec) (session, error)
	vt     *vt.VT
	subs   map[*subscriber]bool
	done   chan struct{}
	code   int
	closed bool
	input  func(id string, b []byte)
	origin string
	muted  bool  // replaying a screen: its queries were asked of a pocketd that is gone
	fed    int   // output since the last checkpoint
	read   int64 // output sess has given the screen
	osc    scanner
	last   *Report
	report func(id string, r Report)
	exit   func()     // forgets the Terminal once its process ends
	change func()     // tells the Manager's watchers; never under mu
	typing sync.Mutex // held by a Prompt until its Enter goes out
}

type Manager struct {
	mu        sync.Mutex
	terminals map[string]*Terminal
	OnInput   func(id string, b []byte) // sees each Write, before the process does
	OnReport  func(id string, r Report) // each root OSC 7501 report, in order, outside the terminal's lock
	// Holder runs each Terminal under its own holder process, so it outlives
	// pocketd (ADR 0008); nil keeps it inside pocketd, as on Windows.
	Holder *holder.Starter

	watchMu  sync.Mutex
	watchers map[*func()]bool
}

// Watch calls fn after each spawn, exit and foreground change, outside every
// Terminal's lock. fn must not block.
func (m *Manager) Watch(fn func()) (stop func()) {
	m.watchMu.Lock()
	defer m.watchMu.Unlock()
	if m.watchers == nil {
		m.watchers = map[*func()]bool{}
	}
	key := &fn
	m.watchers[key] = true
	return func() {
		m.watchMu.Lock()
		delete(m.watchers, key)
		m.watchMu.Unlock()
	}
}

func (m *Manager) changed() {
	m.watchMu.Lock()
	defer m.watchMu.Unlock()
	for fn := range m.watchers {
		(*fn)()
	}
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
	// The process may ask pocketd who it is before it is listed, so Roots waits until it is.
	m.mu.Lock()
	sess, err := m.open(bin, spec)
	if err != nil {
		m.mu.Unlock()
		return nil, err
	}
	s := m.newTerminal(Info{ID: spec.ID, Cmd: spec.Cmd, Args: spec.Args, Cwd: spec.Cwd, Cols: spec.Cols, Rows: spec.Rows, Local: spec.Local, SignIn: spec.SignIn}, sess)
	s.origin = spec.Origin
	s.vt, err = vt.New(spec.Cols, spec.Rows, s.replyToQuery)
	if err != nil {
		m.mu.Unlock()
		sess.Stop(0)
		sess.Close()
		return nil, err
	}
	m.terminals[s.info.ID] = s
	m.mu.Unlock()
	m.start(s)
	return s, nil
}

func (m *Manager) newTerminal(info Info, sess session) *Terminal {
	return &Terminal{info: info, sess: sess, open: m.open, subs: map[*subscriber]bool{}, done: make(chan struct{}),
		input: m.OnInput, report: m.OnReport, change: m.changed}
}

// start runs a listed Terminal. Its change is set before it is listed: a
// listed one may hear SetForeground at once.
func (m *Manager) start(s *Terminal) {
	s.exit = func() {
		m.mu.Lock()
		delete(m.terminals, s.info.ID)
		m.mu.Unlock()
	}
	go s.pump(s.sess)
	m.changed()
}

// Adopted is a Terminal a pocketd from before holders handed down across exec.
type Adopted struct {
	ID, Cmd             string
	Args                []string
	Cwd                 string
	Cols, Rows, FD, Pid int
	Screen              []byte
	Local               string
}

// Adopt takes over a handed-down Terminal, moving its pty into a holder when
// Terminals run under holders.
func (m *Manager) Adopt(a Adopted) (*Terminal, error) {
	sess, err := m.adopt(a)
	if err != nil {
		return nil, err
	}
	return m.attach(Info{ID: a.ID, Cmd: a.Cmd, Args: a.Args, Cwd: a.Cwd, Cols: a.Cols, Rows: a.Rows, Local: a.Local}, sess, a.Screen, nil)
}

// Reattach takes over the Terminal a holder kept running while no pocketd was there.
func (m *Manager) Reattach(c *holder.Client) (*Terminal, error) {
	return m.attach(Info{ID: c.ID, Cmd: c.Cmd, Args: c.Args, Cwd: c.Cwd, Cols: c.Cols, Rows: c.Rows, Local: c.Local, SignIn: c.SignIn}, c, c.Snapshot, c.Tail)
}

// attach lists a Terminal whose process already runs, its screen redrawn from
// screen and the output after it.
func (m *Manager) attach(info Info, sess session, screen, tail []byte) (*Terminal, error) {
	s := m.newTerminal(info, sess)
	var err error
	if s.vt, err = vt.New(info.Cols, info.Rows, s.replyToQuery); err != nil {
		sess.Close()
		return nil, err
	}
	s.muted = true
	s.vt.Write(screen)
	for _, r := range s.feed(tail) {
		s.keep(r)
	}
	s.vt.Write(tail)
	s.muted = false
	sess.Checkpoint(0, s.vt.Snapshot())
	m.mu.Lock()
	m.terminals[s.info.ID] = s
	m.mu.Unlock()
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
	if s.muted {
		return
	}
	for sub := range s.subs {
		if sub.tty {
			return
		}
	}
	s.sess.Write(b)
}

// feed scans chunk for OSC 7501 reports, answering a query unless muted.
func (s *Terminal) feed(chunk []byte) []Report {
	reports, query := s.osc.feed(chunk)
	if query {
		s.replyToQuery([]byte(queryReply))
	}
	return reports
}

// keep makes r the latest report.
func (s *Terminal) keep(r Report) {
	if r.State == "clear" {
		s.last = nil
	} else {
		s.last = &r
	}
}

// pump reads sess until it ends. A Replace that swapped sess out makes it
// stop quietly: the Terminal lives on in the new process.
func (s *Terminal) pump(sess session) {
	buf := make([]byte, 32*1024)
	for {
		n, err := sess.Read(buf)
		if n > 0 {
			chunk := append([]byte(nil), buf[:n]...)
			s.mu.Lock()
			if s.sess != sess {
				s.mu.Unlock()
				break
			}
			reports := s.feed(chunk)
			for _, r := range reports {
				s.keep(r)
			}
			s.vt.Write(chunk)
			s.read += int64(n)
			s.broadcast(Event{Kind: "output", Data: chunk})
			if s.fed += n; s.fed >= checkpointEvery {
				s.fed = 0
				sess.Checkpoint(s.read, s.vt.Snapshot())
			}
			s.mu.Unlock()
			if s.report != nil {
				for _, r := range reports {
					s.report(s.info.ID, r)
				}
			}
		}
		if err != nil {
			break
		}
	}
	code := sess.Wait()
	s.mu.Lock()
	if s.sess != sess {
		s.mu.Unlock()
		sess.Close()
		return
	}
	s.closed = true
	// exit takes Manager.mu, which Roots holds while it reads Pid under s.mu.
	s.mu.Unlock()
	s.exit()
	s.change()
	s.mu.Lock()
	s.code = code
	s.broadcast(Event{Kind: "exit", Code: s.code})
	s.vt.Free()
	s.sess.Close()
	s.mu.Unlock()
	close(s.done)
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
	return s.sess.Pid()
}

// Pgrp is the foreground process group. A holder answers it over its socket,
// so the Terminal isn't locked meanwhile.
func (s *Terminal) Pgrp() (int, error) {
	s.mu.Lock()
	closed, sess := s.closed, s.sess
	s.mu.Unlock()
	if closed {
		return 0, nil
	}
	return group(sess)
}

func (s *Terminal) SetForeground(text string) {
	s.mu.Lock()
	if s.closed || s.info.Foreground == text {
		s.mu.Unlock()
		return
	}
	s.info.Foreground = text
	s.broadcast(Event{Kind: "foreground", Text: text})
	s.mu.Unlock()
	s.change()
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
	sess := s.sess
	s.mu.Unlock()
	_, err := sess.Write(b)
	return err
}

// Replace ends the Terminal's session and runs cmd in its place, keeping the
// id, the size and whoever is attached. Attached screens are reset, and no
// exit is reported for the old process. A closed Terminal refuses, and the
// old session stays as it was when cmd doesn't start. origin is who
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
	sess, err := s.open(bin, Spec{ID: s.info.ID, Cmd: cmd, Args: args, Cwd: s.info.Cwd, Env: env, Cols: s.info.Cols, Rows: s.info.Rows, Local: s.info.Local})
	if err != nil {
		return err
	}
	old := s.sess
	s.sess, s.fed, s.read = sess, 0, 0
	s.info.Cmd, s.info.Args, s.origin = cmd, args, origin
	s.last, s.osc = nil, scanner{}
	reset := []byte("\x1bc") // RIS: the new process starts on a blank screen
	s.vt.Write(reset)
	s.broadcast(Event{Kind: "output", Data: reset})
	go old.Stop(0)
	go s.pump(sess)
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
	s.sess.Resize(cols, rows)
	s.vt.Resize(cols, rows)
	// Kept now, or a reattach before more output would bring back the screen at its old size.
	s.sess.Checkpoint(s.read, s.vt.Snapshot())
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
