package daemon

import (
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"log"
	"pocketd/internal/agent"
	"slices"
	"strings"
	"sync"
	"time"

	"pocketd/internal/codex"
)

// codexChat drives the app-server a codex that Pocket runs for the chat
// speaks on its stdio, which `pocketd pipe` carries. It is the codex.Wire its
// client reads and writes, and the handler its messages go to first.
type codexChat struct {
	d     *Daemon
	pr    *presence
	s     *codex.Session
	lines chan string
	ready chan struct{} // closed once the thread is followed
	gone  chan struct{}
	once  sync.Once

	mu       sync.Mutex
	c        *codex.Client
	thread   string
	turn     string   // the turn under way, which a stop names
	queued   []string // prompts sent before the thread was ready
	err      error    // why the thread never got ready
	stopping bool     // a stop is under way, so the idle after it is no turn end
	cut      string   // the id of the call that opens the thread
	synced   bool     // its reply came: what came before is in it
}

func newCodexChat(d *Daemon, pr *presence) *codexChat {
	return &codexChat{d: d, pr: pr, lines: make(chan string, 256), ready: make(chan struct{}), gone: make(chan struct{})}
}

var errChatGone = errors.New("codex chat closed")

// out takes a line the app-server wrote.
func (x *codexChat) out(line string) {
	select {
	case x.lines <- line:
	case <-x.gone:
	case <-x.pr.ctx.Done():
	}
}

type rpcFrame struct {
	ID     json.RawMessage `json:"id"`
	Method string          `json:"method"`
}

// Read drops the notifications that come before the thread opens: a pipe
// hands a new pocketd what codex said while none listened, and the thread's
// history has all of it.
func (x *codexChat) Read(ctx context.Context) ([]byte, error) {
	for {
		select {
		case line := <-x.lines:
			if x.keep(line) {
				return []byte(line), nil
			}
		case <-x.gone:
			return nil, errChatGone
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}
}

func (x *codexChat) keep(line string) bool {
	var f rpcFrame
	json.Unmarshal([]byte(line), &f)
	x.mu.Lock()
	defer x.mu.Unlock()
	if f.Method == "" && x.cut != "" && string(f.ID) == x.cut {
		x.synced = true
	}
	return x.synced || f.Method == "" || len(f.ID) > 0
}

func (x *codexChat) Write(_ context.Context, msg []byte) error {
	var f rpcFrame
	json.Unmarshal(msg, &f)
	if f.Method == "thread/start" || f.Method == "thread/resume" {
		x.mu.Lock()
		x.cut = string(f.ID)
		x.mu.Unlock()
	}
	x.pr.stream.sendLine(string(msg))
	return nil
}

func (x *codexChat) Close() { x.once.Do(func() { close(x.gone) }) }

type threadStatus struct {
	Type        string   `json:"type"`
	ActiveFlags []string `json:"activeFlags"`
}

func (x *codexChat) Notify(method string, params json.RawMessage) {
	var p struct {
		ThreadID string       `json:"threadId"`
		Status   threadStatus `json:"status"`
		Turn     struct {
			ID string `json:"id"`
		} `json:"turn"`
	}
	json.Unmarshal(params, &p)
	switch method {
	case "thread/status/changed":
		if x.wait() && p.ThreadID == x.threadID() {
			x.status(p.Status)
		}
	case "turn/started", "turn/completed":
		if x.wait() && p.ThreadID == x.threadID() {
			x.mu.Lock()
			if x.turn = p.Turn.ID; method == "turn/completed" {
				x.turn = ""
			}
			x.mu.Unlock()
		}
	}
	x.s.Notify(method, params)
}

func (x *codexChat) Request(id json.RawMessage, method string, params json.RawMessage) {
	x.s.Request(id, method, params)
}

// wait holds a message back until the thread is followed, as the reply that
// opens it may still be on its way to start.
func (x *codexChat) wait() bool {
	select {
	case <-x.ready:
		return true
	case <-x.gone:
		return false
	}
}

func (x *codexChat) threadID() string {
	x.mu.Lock()
	defer x.mu.Unlock()
	return x.thread
}

// status takes the thread's status. The idle a stop brings was ended by the stop.
func (x *codexChat) status(st threadStatus) {
	x.mu.Lock()
	stopped := st.Type == "idle" && x.stopping
	if st.Type != "active" {
		x.stopping = false
	}
	x.mu.Unlock()
	if !stopped {
		x.d.codexStatus(x.pr, st.Type, st.ActiveFlags)
	}
}

func (x *codexChat) driver(c termDriver) agent.Driver { return codexChatDriver{c, x.d, x} }

// attach opens the thread: the one a resumed or handed-down codex had, else a new one.
func (x *codexChat) attach() {
	saved := ""
	if h := x.d.hintOf(x.pr); h != nil {
		saved = h.saved.ConversationID
	}
	go x.d.chatCodex(x, saved)
}

func (d *Daemon) chatCodex(x *codexChat, saved string) {
	pr := x.pr
	x.s = codex.NewSession(pr.a.ID(), codexSink{pr.a}, d.Broker)
	ctx := pr.ctx
	stop := context.AfterFunc(ctx, x.Close)
	c, err := codex.Connect(ctx, x, x, map[string]any{"experimentalApi": true})
	if err != nil {
		x.fail(err)
		return
	}
	method, params := "thread/start", map[string]any{"cwd": pr.t.Info().Cwd}
	if saved != "" {
		method, params = "thread/resume", map[string]any{"threadId": saved}
	}
	res, err := c.Call(ctx, method, params)
	if err != nil {
		c.Close()
		x.fail(err)
		return
	}
	var r struct {
		Thread struct {
			ID     string       `json:"id"`
			Status threadStatus `json:"status"`
			Turns  []struct {
				ID     string `json:"id"`
				Status string `json:"status"`
			} `json:"turns"`
		} `json:"thread"`
	}
	json.Unmarshal(res, &r)
	thread := r.Thread.ID
	if pr.a.Summary().ProviderSessionID == thread {
		pr.a.ClearTimeline()
	}
	pr.a.SetConversation(thread)
	x.s.Replay(ctx, c, res)
	stop()
	context.AfterFunc(ctx, x.s.Close)

	x.mu.Lock()
	x.c, x.thread = c, thread
	if n := len(r.Thread.Turns); n > 0 && r.Thread.Turns[n-1].Status == "inProgress" {
		x.turn = r.Thread.Turns[n-1].ID
	}
	queued := x.queued
	x.queued = nil
	x.mu.Unlock()
	close(x.ready)

	// A turn that ended while no pocketd listened ends now; a new thread's idle is no turn end.
	st := r.Thread.Status
	if status := pr.a.Summary().Status; st.Type == "active" || status == "working" || status == "needsYou" {
		d.codexStatus(pr, st.Type, st.ActiveFlags)
	}
	d.resumed(pr, thread)
	for _, text := range queued {
		if err := x.prompt(text); err != nil {
			log.Printf("codex %s: prompt: %v", thread, err)
		}
	}
}

func (x *codexChat) fail(err error) {
	log.Printf("codex chat %s: %v", x.pr.a.ID(), err)
	x.mu.Lock()
	x.err = err
	x.mu.Unlock()
	x.pr.a.SetAttached(false)
	x.Close()
}

// CodexCallWait is how long codex has to answer a call from the chat.
var CodexCallWait = 10 * time.Second

func (x *codexChat) call(method string, params map[string]any) error {
	x.mu.Lock()
	c, thread := x.c, x.thread
	x.mu.Unlock()
	params["threadId"] = thread
	ctx, cancel := context.WithTimeout(x.pr.ctx, CodexCallWait)
	defer cancel()
	_, err := c.Call(ctx, method, params)
	return err
}

// prompt starts a turn, or steers the one under way, as codex adds a
// prompt sent mid-turn to it.
func (x *codexChat) prompt(text string) error {
	return x.call("turn/start", map[string]any{"input": []map[string]any{{"type": "text", "text": text}}})
}

// opened reports whether the thread is ready, and if not, whether it may still be.
func (x *codexChat) opened() (bool, error) {
	if x.err != nil {
		return false, errors.New("Codex didn't start: " + x.err.Error())
	}
	return x.c != nil, nil
}

// usable is nil once the thread is ready to take calls.
func (x *codexChat) usable() error {
	x.mu.Lock()
	defer x.mu.Unlock()
	if ok, err := x.opened(); !ok {
		return cmp.Or(err, errStarting)
	}
	return nil
}

var (
	errNoTurn   = errors.New("Codex has no turn to stop")
	errStarting = errors.New("Codex is still starting")
)

// codexChatDriver drives a codex through its app-server.
type codexChatDriver struct {
	termDriver
	d *Daemon
	x *codexChat
}

func (c codexChatDriver) Prompt(text string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	x := c.x
	x.mu.Lock()
	ok, err := x.opened()
	if !ok && err == nil {
		x.queued = append(x.queued, text)
	}
	x.mu.Unlock()
	if !ok {
		return err
	}
	return x.prompt(text)
}

func (c codexChatDriver) Interrupt() error {
	if !c.inForeground() {
		return errNotForeground
	}
	x := c.x
	x.mu.Lock()
	turn := x.turn
	x.stopping = turn != ""
	x.mu.Unlock()
	if turn == "" {
		return errNoTurn
	}
	c.a.Stop()
	err := x.call("turn/interrupt", map[string]any{"turnId": turn})
	// The turn may have ended on its own meanwhile.
	if err != nil && !strings.Contains(err.Error(), "no active turn") {
		x.mu.Lock()
		x.stopping = false
		x.mu.Unlock()
		return err
	}
	c.d.cleared(c.pr)
	return nil
}

func (c codexChatDriver) Compact() error {
	if !c.inForeground() {
		return errNotForeground
	}
	if err := c.x.usable(); err != nil {
		return err
	}
	c.a.SetCompacting()
	return c.x.call("thread/compact/start", map[string]any{})
}

// Settings applies to the thread's next turns; a resume keeps it, as the
// switch is saved with the Terminal.
func (c codexChatDriver) Settings(model, effort string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	if err := c.x.usable(); err != nil {
		return err
	}
	params := map[string]any{}
	if model != "" {
		params["model"] = model
	}
	if effort != "" {
		params["effort"] = effort
	}
	if err := c.x.call("thread/settings/update", params); err != nil {
		return err
	}
	c.pr.switched(model, effort)
	return nil
}

// codexStatus sets pr's status from its codex thread's.
func (d *Daemon) codexStatus(pr *presence, typ string, flags []string) {
	switch {
	case typ == "active" && (slices.Contains(flags, "waitingOnApproval") || slices.Contains(flags, "waitingOnUserInput")):
		pr.a.NeedsYou()
	case typ == "active":
		pr.a.Working()
	case typ == "idle":
		d.turnEnded(pr, false)
	case typ == "systemError":
		d.turnEnded(pr, true)
	}
}
