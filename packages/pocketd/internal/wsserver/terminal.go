package wsserver

import (
	"cmp"
	"context"
	"errors"
	"os"
	"path/filepath"
	"slices"
	"sync"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/ops"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
)

// TerminalService starts the login shells phones open, as daemon.Spawn does
// for the desktop.
type TerminalService interface {
	Spawn(m ops.Msg) (*terminal.Terminal, error)
}

const (
	// attachQueue bounds the output one attach holds while its phone reads slowly.
	attachQueue = 512 << 10
	frameMax    = 32 << 10
	coalesce    = 16 * time.Millisecond
	// listSettle lets the agent that a foreground change announces register
	// before the list goes out, so it names the agent.
	listSettle = 50 * time.Millisecond

	// The size the desktop opens a shell at.
	shellCols, shellRows = 120, 36
)

// terminalConn is a conn's terminal.v1 state.
type terminalConn struct {
	mu        sync.Mutex
	viewers   map[string]*viewer
	stopWatch func()
	// framing is held across each frame's stopped check and write, so a
	// stopped viewer can't write after the snapshot of the attach that replaced it.
	framing sync.Mutex
}

func (c *conn) helloTerminal() {
	if c.s.Terminals == nil || !slices.Contains(c.caps, proto.CapTerminal) || c.who.Kind == peer.PTY {
		return
	}
	c.terminal.mu.Lock()
	defer c.terminal.mu.Unlock()
	if c.terminal.stopWatch != nil {
		return
	}
	kick := make(chan struct{}, 1)
	c.terminal.stopWatch = c.s.Terminals.Watch(func() {
		select {
		case kick <- struct{}{}:
		default:
		}
	})
	go func() {
		for {
			select {
			case <-c.ctx.Done():
				return
			case <-kick:
			}
			select {
			case <-c.ctx.Done():
				return
			case <-time.After(listSettle):
			}
			select {
			case <-kick:
			default:
			}
			c.send(proto.NewTerminalList("", c.terminals()))
		}
	}()
}

func (c *conn) closeTerminal() {
	c.terminal.mu.Lock()
	viewers := c.terminal.viewers
	c.terminal.viewers = nil
	stop := c.terminal.stopWatch
	c.terminal.mu.Unlock()
	if stop != nil {
		stop()
	}
	for _, v := range viewers {
		v.stop()
	}
}

// terminalVerb serves terminal.v1; without the cap its verbs are unknown.
func (c *conn) terminalVerb(m proto.ClientMessage) error {
	if !slices.Contains(c.caps, proto.CapTerminal) {
		return proto.ErrMalformed
	}
	// A process in one Terminal may not read another's screen, as ops attach and screen refuse it.
	if c.who.Kind == peer.PTY {
		c.send(proto.NewErrorCode(m.ID, proto.CodeScopeDenied, m.Type+" isn't open to a process in a Pocket Terminal"))
		return nil
	}
	switch m.Type {
	case "terminal.list":
		return c.terminalList(m)
	case "terminal.attach":
		return c.terminalAttach(m)
	case "terminal.detach":
		return c.terminalDetach(m)
	}
	if c.who.Kind != peer.Owner && c.s.Settings != nil && !c.s.Settings.PhoneTerminals() {
		c.send(proto.NewErrorCode(m.ID, proto.CodeTerminalsOff, "Terminals are off for phones on this Mac"))
		return nil
	}
	switch m.Type {
	case "terminal.input":
		return c.terminalInput(m)
	case "terminal.resize":
		return c.terminalResize(m)
	case "terminal.create":
		return c.terminalCreate(m)
	}
	return c.terminalClose(m)
}

// terminals lists every running terminal by folder, naming the agent each runs.
func (c *conn) terminals() []proto.Terminal {
	if c.s.Terminals == nil {
		return nil
	}
	agents := map[string]proto.AgentSummary{}
	for _, a := range c.s.Agents.List() {
		if prev, ok := agents[a.TerminalID]; a.TerminalID != "" && (!ok || a.UpdatedAt > prev.UpdatedAt) {
			agents[a.TerminalID] = a
		}
	}
	var out []proto.Terminal
	for _, i := range c.s.Terminals.List() {
		out = append(out, proto.Terminal{ID: i.ID, Cwd: i.Cwd, Cols: i.Cols, Rows: i.Rows, Title: i.Foreground, LocalID: i.Local, AgentID: agents[i.ID].ID})
	}
	slices.SortFunc(out, func(a, b proto.Terminal) int { return cmp.Or(cmp.Compare(a.Cwd, b.Cwd), cmp.Compare(a.ID, b.ID)) })
	return out
}

func (c *conn) terminalList(m proto.ClientMessage) error {
	c.send(proto.NewTerminalList(m.ID, c.terminals()))
	return nil
}

func (c *conn) findTerminal(id string) *terminal.Terminal {
	if c.s.Terminals == nil {
		return nil
	}
	return c.s.Terminals.Get(id)
}

func noTerminal(id string) proto.Error {
	return proto.NewErrorCode(id, proto.CodeTerminalEnded, "This terminal has ended")
}

// terminalAttach replaces any attach this conn has to the terminal, so a
// phone that lost track gets a fresh snapshot.
func (c *conn) terminalAttach(m proto.ClientMessage) error {
	t := c.findTerminal(m.TerminalID)
	if t == nil {
		c.send(noTerminal(m.ID))
		return nil
	}
	v := &viewer{c: c, t: t, id: m.TerminalID, wake: make(chan struct{}, 1), done: make(chan struct{})}
	c.terminal.mu.Lock()
	if old := c.terminal.viewers[m.TerminalID]; old != nil {
		old.stop()
	}
	snap, detach, err := t.Attach(false, v.event(0))
	if err != nil {
		c.terminal.mu.Unlock()
		c.send(noTerminal(m.ID))
		return nil
	}
	info := t.Info()
	v.detach, v.cols, v.rows = detach, info.Cols, info.Rows
	if c.terminal.viewers == nil {
		c.terminal.viewers = map[string]*viewer{}
	}
	c.terminal.viewers[m.TerminalID] = v
	c.terminal.mu.Unlock()
	c.send(proto.NewTerminalAttached(m.ID, m.TerminalID, info.Cols, info.Rows))
	go v.run(snap)
	return nil
}

func (c *conn) terminalDetach(m proto.ClientMessage) error {
	c.terminal.mu.Lock()
	v := c.terminal.viewers[m.TerminalID]
	delete(c.terminal.viewers, m.TerminalID)
	c.terminal.mu.Unlock()
	if v != nil {
		v.stop()
	}
	c.send(proto.NewAck(m.ID))
	return nil
}

func (c *conn) terminalInput(m proto.ClientMessage) error {
	t := c.findTerminal(m.TerminalID)
	if t == nil || t.Write(m.Data) != nil {
		c.send(noTerminal(m.ID))
	}
	return nil
}

func (c *conn) terminalResize(m proto.ClientMessage) error {
	t := c.findTerminal(m.TerminalID)
	if t == nil {
		c.send(noTerminal(m.ID))
		return nil
	}
	t.Resize(m.Cols, m.Rows)
	c.send(proto.NewAck(m.ID))
	return nil
}

func (c *conn) terminalClose(m proto.ClientMessage) error {
	t := c.findTerminal(m.TerminalID)
	if t == nil {
		c.send(noTerminal(m.ID))
		return nil
	}
	t.Close()
	c.send(proto.NewAck(m.ID))
	return nil
}

func (c *conn) terminalCreate(m proto.ClientMessage) error {
	if c.s.TerminalHost == nil {
		return c.notImplemented(m)
	}
	cwd, refusal := c.shellPlace(m)
	if refusal != nil {
		c.send(*refusal)
		return nil
	}
	spec := ops.Msg{Cwd: cwd, Local: m.LocalID, Login: true, Cols: cmp.Or(m.Cols, shellCols), Rows: cmp.Or(m.Rows, shellRows)}
	// A shell missing from PATH makes Spawn read the login environment again, which takes seconds.
	go func() {
		t, err := c.s.TerminalHost.Spawn(spec)
		if err != nil {
			c.send(proto.NewCodedError(m.ID, "spawn_failed", "Couldn't start a terminal", err.Error()))
			return
		}
		// The desktop opens a tab for every terminal a create announces, as it does for agents.
		c.s.Hub.Publish(proto.NewAgentCreating("", m.RequestID, t.Info().ID, cwd, false))
		c.send(proto.NewTerminalCreated(m.ID, m.RequestID, t.Info().ID))
	}()
	return nil
}

// shellPlace is the folder a new shell opens in: a tree, or inside a Local's project.
func (c *conn) shellPlace(m proto.ClientMessage) (string, *proto.Error) {
	notATree := proto.NewErrorCode(m.ID, proto.CodeNotATree, "This folder isn't a project on your Mac")
	cwd := filepath.Clean(m.Cwd)
	if m.LocalID != "" {
		var lc proto.Local
		ok := c.s.Locals != nil
		if ok {
			lc, ok = c.s.Locals.Get(m.LocalID)
		}
		if !ok {
			e := proto.NewErrorCode(m.ID, proto.CodeUnknownLocal, "This Local isn't on your Mac anymore")
			return "", &e
		}
		if m.Cwd == "" {
			cwd = lc.Project
		}
		if !filepath.IsAbs(cwd) || !inside(cwd, lc.Project) {
			return "", &notATree
		}
	} else {
		if m.Cwd == "" || !filepath.IsAbs(cwd) || c.s.Registry == nil {
			return "", &notATree
		}
		if _, ok := worktree.Containing(c.s.Registry.Load(), cwd); !ok {
			return "", &notATree
		}
	}
	if st, err := os.Stat(cwd); err != nil || !st.IsDir() {
		return "", &notATree
	}
	return cwd, nil
}

// viewer is one conn's attach to one terminal. The terminal calls event
// under its lock while the PTY pump waits, so event only queues; run writes
// the queue to the socket. A queue that outgrows attachQueue is dropped, and
// a fresh snapshot replaces what it held.
type viewer struct {
	c    *conn
	t    *terminal.Terminal
	id   string
	wake chan struct{}
	done chan struct{}
	// cols and rows are the size the phone was last told; only run reads them after attach.
	cols, rows int

	mu sync.Mutex
	// gen names the live subscription; a superseded one's events are dropped.
	gen     int
	queue   []queued
	bytes   int
	stale   bool
	exited  *int
	detach  func()
	stopped bool
}

// queued is output, or a resize when data is nil.
type queued struct {
	data       []byte
	cols, rows int
}

func (v *viewer) event(gen int) func(terminal.Event) {
	return func(e terminal.Event) {
		v.mu.Lock()
		defer v.mu.Unlock()
		if gen != v.gen || v.stopped {
			return
		}
		switch e.Kind {
		case "output":
			if v.stale {
				return
			}
			if v.bytes+len(e.Data) > attachQueue {
				v.queue, v.bytes, v.stale = nil, 0, true
				break
			}
			v.queue = append(v.queue, queued{data: e.Data})
			v.bytes += len(e.Data)
		case "resize":
			if !v.stale {
				v.queue = append(v.queue, queued{cols: e.Cols, rows: e.Rows})
			}
		case "exit":
			code := e.Code
			v.exited = &code
		default:
			return
		}
		select {
		case v.wake <- struct{}{}:
		default:
		}
	}
}

func (v *viewer) stop() {
	v.mu.Lock()
	if v.stopped {
		v.mu.Unlock()
		return
	}
	v.stopped = true
	detach := v.detach
	v.mu.Unlock()
	detach()
	close(v.done)
}

// forget drops v from its conn once its terminal ended.
func (v *viewer) forget() {
	v.c.terminal.mu.Lock()
	if v.c.terminal.viewers[v.id] == v {
		delete(v.c.terminal.viewers, v.id)
	}
	v.c.terminal.mu.Unlock()
	v.stop()
}

func (v *viewer) run(snapshot []byte) {
	if v.frame(proto.FrameSnapshot, snapshot) != nil {
		return
	}
	for {
		select {
		case <-v.done:
			return
		case <-v.wake:
		}
		if !v.gather() || !v.flush() {
			return
		}
	}
}

// gather waits up to coalesce for more output, or less once a frame's worth is in.
func (v *viewer) gather() bool {
	timer := time.NewTimer(coalesce)
	defer timer.Stop()
	for {
		v.mu.Lock()
		ready := v.bytes >= frameMax || v.stale || v.exited != nil
		v.mu.Unlock()
		if ready {
			return true
		}
		select {
		case <-v.done:
			return false
		case <-v.wake:
		case <-timer.C:
			return true
		}
	}
}

// flush writes what the queue holds; false once the attach is over.
func (v *viewer) flush() bool {
	v.mu.Lock()
	items, stale, exited := v.queue, v.stale, v.exited
	v.queue, v.bytes = nil, 0
	v.mu.Unlock()
	if stale && exited == nil {
		return v.resnapshot()
	}
	var out []byte
	send := func() error {
		for len(out) > 0 {
			n := min(len(out), frameMax)
			if err := v.frame(proto.FrameOutput, out[:n]); err != nil {
				return err
			}
			out = out[n:]
		}
		return nil
	}
	for _, q := range items {
		if q.data != nil {
			out = append(out, q.data...)
			continue
		}
		if send() != nil {
			return false
		}
		v.resized(q.cols, q.rows)
	}
	if send() != nil {
		return false
	}
	if exited != nil {
		v.c.send(proto.NewTerminalExited(v.id, *exited))
		v.forget()
		return false
	}
	return true
}

// resnapshot subscribes afresh and sends the screen as it is now, in place
// of the output the queue dropped.
func (v *viewer) resnapshot() bool {
	v.mu.Lock()
	if v.stopped {
		v.mu.Unlock()
		return false
	}
	v.gen++
	v.stale = false
	gen, old := v.gen, v.detach
	v.mu.Unlock()
	snap, detach, err := v.t.Attach(false, v.event(gen))
	if err != nil {
		// It ended while the queue was dropped; the old subscription's exit went with it.
		v.c.send(proto.NewTerminalExited(v.id, v.t.ExitCode()))
		v.forget()
		return false
	}
	v.mu.Lock()
	v.detach = detach
	stopped := v.stopped
	v.mu.Unlock()
	old()
	if stopped {
		detach()
		return false
	}
	info := v.t.Info()
	v.resized(info.Cols, info.Rows)
	return v.frame(proto.FrameSnapshot, snap) == nil
}

func (v *viewer) resized(cols, rows int) {
	if cols == v.cols && rows == v.rows {
		return
	}
	v.cols, v.rows = cols, rows
	v.c.send(proto.NewTerminalResized(v.id, cols, rows))
}

var errViewerStopped = errors.New("viewer stopped")

func (v *viewer) frame(kind byte, payload []byte) error {
	v.c.terminal.framing.Lock()
	defer v.c.terminal.framing.Unlock()
	v.mu.Lock()
	stopped := v.stopped
	v.mu.Unlock()
	if stopped {
		return errViewerStopped
	}
	return writeFrame(v.c.ctx, v.c.ws, kind, v.id, payload)
}

// writeFrame sends one binary terminal frame: proto.FrameSnapshot or proto.FrameOutput.
func writeFrame(ctx context.Context, ws *websocket.Conn, kind byte, terminalID string, payload []byte) error {
	frame, err := proto.AppendFrame(make([]byte, 0, 2+len(terminalID)+len(payload)), kind, terminalID, payload)
	if err != nil {
		return err
	}
	return ws.Write(ctx, websocket.MessageBinary, frame)
}
