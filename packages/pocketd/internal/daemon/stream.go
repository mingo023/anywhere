package daemon

import (
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"strconv"
	"sync"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/claude"
	"pocketd/internal/ops"
	"pocketd/internal/peer"
	"pocketd/internal/proto"
	"pocketd/internal/state"
	"pocketd/internal/timeline"
)

// stream is the stdio of an agent that `pocketd pipe` runs for the chat. A
// claude speaks stream-json: prompts and control requests go in; its turns,
// its status and the tools it asks to use come out. A codex's app-server
// speaks JSON-RPC; see codexChat.
type stream struct {
	mu       sync.Mutex
	conn     *ops.Conn
	inbox    []string                      // lines for claude from before the pipe connected
	replies  map[string]chan controlAnswer // pocketd's control requests by id
	asks     map[string]context.CancelFunc // claude's open can_use_tool requests by id
	stopping bool                          // a stop is under way, so the turn's result is no turn end
	next     int
	messages int // messages claude has streamed, to name each block it drafts
}

func newStream() *stream {
	return &stream{replies: map[string]chan controlAnswer{}, asks: map[string]context.CancelFunc{}}
}

type controlAnswer struct {
	err  string
	body json.RawMessage
}

// send writes v to claude as one line, or keeps it until the pipe connects.
func (s *stream) send(v any) {
	b, _ := json.Marshal(v)
	s.sendLine(string(b))
}

func (s *stream) sendLine(line string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.conn != nil && s.conn.Send(ops.Msg{Op: "in", Text: line}) == nil {
		return
	}
	s.conn = nil
	s.inbox = append(s.inbox, line)
}

// attach takes c as the pipe, handing it what waited for one.
func (s *stream) attach(c *ops.Conn) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	if c.Send(ops.Msg{Ev: "ok"}) != nil {
		return false
	}
	for i, line := range s.inbox {
		if c.Send(ops.Msg{Op: "in", Text: line}) != nil {
			s.inbox = s.inbox[i:]
			return false
		}
	}
	s.inbox, s.conn = nil, c
	return true
}

func (s *stream) detach(c *ops.Conn) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.conn == c {
		s.conn = nil
	}
}

func (s *stream) connected() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.conn != nil
}

// ControlWait is how long claude has to answer a control request.
var ControlWait = 10 * time.Second

var errNoReply = errors.New("The agent didn't answer")

// control sends claude a control request and waits for its answer.
func (s *stream) control(ctx context.Context, request any) (json.RawMessage, error) {
	return s.controlFor(ctx, request, ControlWait)
}

func (s *stream) controlFor(ctx context.Context, request any, wait time.Duration) (json.RawMessage, error) {
	s.mu.Lock()
	s.next++
	id := "pocket-" + strconv.Itoa(s.next)
	reply := make(chan controlAnswer, 1)
	s.replies[id] = reply
	s.mu.Unlock()
	defer func() {
		s.mu.Lock()
		delete(s.replies, id)
		s.mu.Unlock()
	}()
	s.send(map[string]any{"type": "control_request", "request_id": id, "request": request})
	select {
	case a := <-reply:
		if a.err != "" {
			return nil, errors.New(a.err)
		}
		return a.body, nil
	case <-time.After(wait):
		return nil, errNoReply
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

func (s *stream) replied(id, subtype, e string, body json.RawMessage) {
	s.mu.Lock()
	defer s.mu.Unlock()
	a := controlAnswer{body: body}
	if subtype == "error" {
		a = controlAnswer{err: cmp.Or(e, "The agent refused")}
	}
	select {
	case s.replies[id] <- a:
	default:
	}
}

// ask opens can_use_tool request id; nil when it is open already, as a
// pipe that reconnects sends its open requests again.
func (s *stream) ask(id string) context.Context {
	s.mu.Lock()
	defer s.mu.Unlock()
	if _, ok := s.asks[id]; ok {
		return nil
	}
	ctx, cancel := context.WithCancel(context.Background())
	s.asks[id] = cancel
	return ctx
}

func (s *stream) cancel(id string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if cancel := s.asks[id]; cancel != nil {
		cancel()
		delete(s.asks, id)
	}
}

func (s *stream) stop() {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.stopping = true
}

// stopped reports whether the turn that just ended was stopped; set says
// whether a new one starts.
func (s *stream) stopped(starting bool) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	stopped := s.stopping
	s.stopping = false
	return stopped && !starting
}

// draft is the text a stream event adds to the block claude is writing; the
// transcript writes the whole block later, in its place.
func (s *stream) draft(l streamLine) (timeline.Event, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	ev := l.Event
	if ev.Type == "message_start" {
		s.messages++
	}
	if ev.Type != "content_block_delta" {
		return timeline.Event{}, false
	}
	block := strconv.Itoa(s.messages) + ":" + strconv.Itoa(ev.Index)
	switch {
	case ev.Delta.Type == "text_delta" && ev.Delta.Text != "":
		return timeline.Event{Kind: "assistant_draft", Text: ev.Delta.Text, Draft: block}, true
	case ev.Delta.Type == "thinking_delta" && ev.Delta.Thinking != "":
		return timeline.Event{Kind: "thinking_draft", Text: ev.Delta.Thinking, Draft: block}, true
	}
	return timeline.Event{}, false
}

func midTurn(status string) bool { return status == "working" || status == "needsYou" }

func promptLine(text string) map[string]any {
	return map[string]any{"type": "user", "message": map[string]any{"role": "user", "content": text}}
}

func controlReply(id string, response any) map[string]any {
	return map[string]any{"type": "control_response", "response": map[string]any{"subtype": "success", "request_id": id, "response": response}}
}

func controlRefusal(id, e string) map[string]any {
	return map[string]any{"type": "control_response", "response": map[string]any{"subtype": "error", "request_id": id, "error": e}}
}

var errNotPiped = &peer.Refusal{Code: proto.CodeHookForged, Message: "pipe sender is not the chat agent in this Terminal"}

// Pipe serves the conn `pocketd pipe` dials from terminal m.ID, relaying the
// agent it runs, until either side goes.
func (d *Daemon) Pipe(_ context.Context, p peer.Principal, m ops.Msg, c *ops.Conn) error {
	if p.Kind != peer.PTY {
		return errNotPiped
	}
	if p.Terminal != m.ID {
		return errNotOwnTerminal
	}
	pr := d.presentAt(m.ID, p.Pid)
	if pr == nil || pr.stream == nil {
		return errNotPiped
	}
	if !pr.stream.attach(c) {
		return nil
	}
	defer context.AfterFunc(pr.ctx, func() { c.Close() })()
	pr.a.SetAttached(true)
	for {
		msg, err := c.Recv()
		if err != nil {
			pr.stream.detach(c)
			return nil
		}
		if msg.Ev == "out" {
			pr.chat.out(msg.Text)
		}
	}
}

type streamLine struct {
	Type      string          `json:"type"`
	Subtype   string          `json:"subtype"`
	RequestID string          `json:"request_id"`
	Request   json.RawMessage `json:"request"`
	Response  struct {
		Subtype   string          `json:"subtype"`
		RequestID string          `json:"request_id"`
		Error     string          `json:"error"`
		Response  json.RawMessage `json:"response"`
	} `json:"response"`
	SessionID      string `json:"session_id"`
	Cwd            string `json:"cwd"`
	PermissionMode string `json:"permissionMode"`
	Status         string `json:"status"`
	IsError        bool   `json:"is_error"`
	// A stream_event's: what claude streams of the message it writes, when run with --include-partial-messages.
	Event struct {
		Type  string `json:"type"`
		Index int    `json:"index"`
		Delta struct {
			Type     string `json:"type"`
			Text     string `json:"text"`
			Thinking string `json:"thinking"`
		} `json:"delta"`
	} `json:"event"`
	ParentToolUseID *string `json:"parent_tool_use_id"`
}

type canUseTool struct {
	Subtype               string            `json:"subtype"`
	ToolName              string            `json:"tool_name"`
	Input                 json.RawMessage   `json:"input"`
	PermissionSuggestions []json.RawMessage `json:"permission_suggestions"`
}

// streamLine takes one line claude wrote. Its status comes from here, not
// from its hooks: init starts each turn and result ends it.
func (d *Daemon) streamLine(pr *presence, text string) {
	var l streamLine
	if json.Unmarshal([]byte(text), &l) != nil {
		return
	}
	s := pr.stream
	switch {
	case l.Type == "control_request":
		d.canUseTool(pr, l.RequestID, l.Request)
	case l.Type == "control_response":
		s.replied(l.Response.RequestID, l.Response.Subtype, l.Response.Error, l.Response.Response)
	case l.Type == "control_cancel_request":
		s.cancel(l.RequestID)
	case l.Type == "system" && l.Subtype == "init":
		s.stopped(true)
		if l.PermissionMode != "" {
			pr.setMode(l.PermissionMode)
		}
		pr.working("")
		d.bindInit(pr, l.SessionID, l.Cwd)
		d.resumed(pr, l.SessionID)
	case l.Type == "system" && l.Subtype == "status" && l.Status == "compacting":
		pr.a.SetCompacting()
	case l.Type == "stream_event" && l.ParentToolUseID == nil:
		if e, ok := s.draft(l); ok {
			pr.a.Record(e)
		}
	case l.Type == "result":
		// A stop already ended the turn it ends.
		if !s.stopped(false) {
			d.turnEnded(pr, l.IsError || l.Subtype != "success")
		}
	}
}

// bindInit follows the transcript of the session init names, for a claude
// whose SessionStart hook didn't come, as in a folder the user hasn't trusted.
func (d *Daemon) bindInit(pr *presence, session, cwd string) {
	pr.mu.Lock()
	hooked := pr.hooked
	pr.mu.Unlock()
	if path := claude.Transcript(cwd, session, pr.env); !hooked && path != "" {
		d.sessionStart(pr, hookInput{SessionID: session, TranscriptPath: path, Cwd: cwd})
	}
}

// canUseTool asks the phone about a tool claude wants to use, as a
// PermissionRequest hook would. A request it can't take is refused.
func (d *Daemon) canUseTool(pr *presence, id string, raw json.RawMessage) {
	s := pr.stream
	var req canUseTool
	if json.Unmarshal(raw, &req) != nil || req.Subtype != "can_use_tool" {
		s.send(controlRefusal(id, "Pocket doesn't take "+req.Subtype))
		return
	}
	ctx := s.ask(id)
	if ctx == nil {
		return
	}
	pr.mu.Lock()
	mode := ""
	if pr.launch.Access == "auto" && !pr.launch.Plan {
		mode = "auto"
	}
	pr.mu.Unlock()
	in := hookInput{ToolName: req.ToolName, ToolInput: req.Input, PermissionSuggestions: req.PermissionSuggestions, PermissionMode: mode}
	go func() {
		defer s.cancel(id)
		decision := d.decide(ctx, pr, in)
		if decision == nil {
			return
		}
		if decision.Behavior == "allow" && decision.UpdatedInput == nil {
			decision.UpdatedInput = req.Input
		}
		if decision.Interrupt {
			s.stop()
		}
		s.send(controlReply(id, decision))
	}()
}

// claudeStream is a claude's chat over stream-json.
type claudeStream struct {
	d  *Daemon
	pr *presence
}

func (c claudeStream) driver(t termDriver) agent.Driver { return streamDriver{t, c.d} }
func (c claudeStream) attach()                          { claudeKind{}.attach(c.d, c.pr) }
func (c claudeStream) out(line string)                  { c.d.streamLine(c.pr, line) }

// streamDriver drives a claude through its stream, which queues a prompt
// sent mid-turn and switches models without a dialog.
type streamDriver struct {
	termDriver
	d *Daemon
}

func (c streamDriver) Prompt(text string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	// Claude runs a command sent mid-turn once the turn ends, and a goal can keep it going for long.
	if name, arg, ok := slashCommand(text); ok && name == "goal" && arg == "clear" && midTurn(c.a.Summary().Status) {
		if err := c.Interrupt(); err != nil {
			return err
		}
	}
	c.pr.stream.send(promptLine(text))
	return nil
}

// Interrupt stops the turn and the background sub-agents the chat holds it
// for. Like Claude's own Stop, it leaves background commands running.
// Claude may write no notification for a task it stops, so the call ends here.
// Tasks stop at once, so a silent claude costs one ControlWait, not one per task.
func (c streamDriver) Interrupt() error {
	if !c.inForeground() {
		return errNotForeground
	}
	turn := c.a.Stop()
	ids := c.a.Timeline.LastingAgents()
	if !turn && len(ids) == 0 {
		return nil
	}
	if turn {
		c.pr.stream.stop()
	}
	errs := make([]error, len(ids))
	var wg sync.WaitGroup
	for i, id := range ids {
		wg.Go(func() {
			_, errs[i] = c.pr.stream.control(c.pr.ctx, map[string]any{"subtype": "stop_task", "task_id": id})
		})
	}
	wg.Wait()
	out := "Stopped"
	var failed error
	for i, id := range ids {
		if errs[i] != nil {
			failed = errs[i]
			continue
		}
		c.a.Record(timeline.Event{Kind: "tool_end", TaskID: id, OK: true, Output: &out})
	}
	if turn {
		if _, err := c.pr.stream.control(c.pr.ctx, map[string]any{"subtype": "interrupt"}); err != nil {
			c.pr.stream.stopped(true)
			return err
		}
	} else if failed != nil {
		// A held turn ends with its last sub-agent.
		return failed
	}
	c.d.cleared(c.pr)
	return nil
}

// BtwWait is how long claude has to answer a side question, which takes a model call.
var BtwWait = 2 * time.Minute

// Btw asks claude a side question; it answers from the conversation without adding to it.
func (c streamDriver) Btw(question string) (string, error) {
	if !c.inForeground() {
		return "", errNotForeground
	}
	body, err := c.pr.stream.controlFor(c.pr.ctx, map[string]any{"subtype": "side_question", "question": question}, BtwWait)
	if err != nil {
		return "", err
	}
	var r struct {
		Response *string `json:"response"`
	}
	if json.Unmarshal(body, &r) != nil || r.Response == nil {
		return "", errors.New("The agent had no answer")
	}
	return *r.Response, nil
}

func (c streamDriver) Compact() error {
	if !c.inForeground() {
		return errNotForeground
	}
	c.a.SetCompacting()
	c.pr.stream.send(promptLine("/compact"))
	return nil
}

// Settings switches for this session only; a resume keeps it, as the
// switch is saved with the Terminal.
func (c streamDriver) Settings(model, effort string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	s := c.pr.stream
	if model != "" {
		if _, err := s.control(c.pr.ctx, map[string]any{"subtype": "set_model", "model": model}); err != nil {
			return err
		}
	}
	if effort != "" {
		if _, err := s.control(c.pr.ctx, map[string]any{"subtype": "apply_flag_settings", "settings": map[string]any{"effortLevel": effort}}); err != nil {
			return err
		}
	}
	c.pr.switched(model, effort)
	return nil
}

// Access switches claude's permission mode; a resume keeps it, as the
// switch is saved with the Terminal.
func (c streamDriver) Access(access string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	if _, err := c.pr.stream.control(c.pr.ctx, map[string]any{"subtype": "set_permission_mode", "mode": state.PermissionMode(access)}); err != nil {
		return err
	}
	c.pr.accessSwitched(access)
	return nil
}

// Commands asks claude for its slash commands: its initialize answer lists
// them, and claude answers one at any time.
func (c streamDriver) Commands() ([]proto.Command, error) {
	if !c.inForeground() {
		return nil, errNotForeground
	}
	body, err := c.pr.stream.control(c.pr.ctx, map[string]any{"subtype": "initialize"})
	if err != nil {
		return nil, err
	}
	var r struct {
		Commands []proto.Command `json:"commands"`
	}
	json.Unmarshal(body, &r)
	return r.Commands, nil
}
