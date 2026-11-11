package pi

import (
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"os"
	"slices"
	"strings"
	"sync"
	"time"

	"pocketd/internal/broker"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
	"pocketd/internal/state"
	"pocketd/internal/timeline"
)

// Sink takes what a Session makes of its agent: the timeline, and the
// status, which pi tells only as the events of a run.
type Sink interface {
	Apply(timeline.Event)
	SetTitle(string)
	SetTokens(used, window int64)
	SetModel(string)
	SetEffort(string)
	// Bound names the agent's session once pi tells it.
	Bound(session string)
	Working()
	NeedsYou()
	TurnEnded(failed bool)
	// Stopped ends a turn the user stopped, which is no failure.
	Stopped()
	Compacting()
	Compacted()
}

var (
	// OpenWait is how long pi has to answer each command that opens its session.
	OpenWait = 60 * time.Second
	// CallWait is how long it has to answer a switch.
	CallWait = 15 * time.Second
)

// flavor is what omp does unlike the pi it forked from.
type flavor struct {
	// commands lists what it takes after a slash.
	commands string
	// allCommands runs all its commands without a TUI, not only its skills.
	allCommands bool
	// ownEvents has it tell of itself in events pi doesn't have.
	ownEvents bool
	// fast is the priority service tier, which only some providers' models have.
	fast bool
}

var flavors = map[string]flavor{
	providers.Pi.ID:  {commands: "get_commands"},
	providers.Omp.ID: {commands: "get_available_commands", allCommands: true, ownEvents: true, fast: true},
}

// prompt is one pi took that it hasn't shown as the user's yet.
type prompt struct{ id, text string }

// Session is one agent's pi session: it sends the prompts, maps the events
// of pi's runs to timeline events, and answers what its extensions ask.
type Session struct {
	// Shown is whether the agent already shows the session's history, as a
	// moved agent does: Open replays none of it.
	Shown   bool
	agentID string
	name    string // the agent's, for what the user reads
	f       flavor
	sink    Sink
	broker  *broker.Broker
	ctx     context.Context
	cancel  context.CancelFunc

	// mu is never held across a Call: the client's reader takes it for each record.
	mu     sync.Mutex
	c      *Client
	err    error // why it never opened
	ready  bool  // its state and history are read: turns may start
	ended  bool  // pi told the end of a run while they were read
	window int64 // its model's context window
	// queue holds the prompts sent before the session opened, while it compacts or while a turn stops: pi
	// refuses the first two, and would run the last in the turn being stopped.
	queue      []string
	sent       []*prompt
	turn       string // the id of the prompt that started the turn
	running    bool
	active     bool // pi runs its agent: the turn's prompt did more than a command
	stopping   bool // the user stopped the turn
	retrying   bool
	compacting bool
	// compactNext is a Compact asked for mid-turn, which waits for the turn to end.
	compactNext bool
	settles     bool // pi tells when it goes on with nothing
	began       time.Time
	failure     *string // the error pi's last reply ended with, the turn's unless pi tries again
	usage       proto.TurnUsage
	held        []func() // the dialogs asked before the session opened
	asks        map[string]context.CancelFunc
	msg         block // the block pi is writing
	blocks      int
	done        map[int]bool    // the blocks of pi's message that are in the timeline
	tools       map[string]bool // the turn's calls shown
	// tasks are the calls that run sub-agents.
	tasks    map[string]*task
	commands []proto.Command // as omp last told them
}

// block is a block of the message pi is writing, a delta at a time.
type block struct {
	kind  string // assistant or thinking
	index int
	text  string
	open  bool
}

// NewSession is a Session for agentID's provider, which holds prompts until Open.
func NewSession(agentID, provider string, sink Sink, b *broker.Broker) *Session {
	ctx, cancel := context.WithCancel(context.Background())
	return &Session{agentID: agentID, name: providers.Get(provider).Name, f: flavors[provider], sink: sink, broker: b, ctx: ctx, cancel: cancel,
		asks: map[string]context.CancelFunc{}, done: map[int]bool{}, tools: map[string]bool{}, tasks: map[string]*task{}}
}

type model struct {
	Provider string `json:"provider"`
	ID       string `json:"id"`
	Window   int64  `json:"contextWindow"`
}

func (m model) name() string {
	if m.Provider == "" || m.ID == "" {
		return ""
	}
	return m.Provider + "/" + m.ID
}

// Open reads the session pi started on over w: the one its command line
// named, whose history goes in the timeline, else a new one. l's effort is
// set where pi's command line had no flag for it. midTurn is a turn the
// pocketd before this one left running. A pi that never tells its state gives
// back the prompts that waited to be sent.
func (s *Session) Open(w Wire, l state.Launch, midTurn bool) (unsent []string, err error) {
	c := Connect(w, s)
	s.mu.Lock()
	s.c = c
	s.mu.Unlock()
	var st struct {
		SessionID string `json:"sessionId"`
		Model     model  `json:"model"`
		Thinking  string `json:"thinkingLevel"`
		Streaming bool   `json:"isStreaming"`
		Messages  int    `json:"messageCount"`
	}
	data, err := s.call(c, OpenWait, "get_state", nil)
	if err == nil && (json.Unmarshal(data, &st) != nil || st.SessionID == "") {
		err = fmt.Errorf("it gave no session")
	}
	if err != nil {
		s.mu.Lock()
		unsent = s.queue
		s.err, s.queue = err, nil
		s.mu.Unlock()
		return unsent, err
	}
	s.sink.Bound(st.SessionID)
	if name := st.Model.name(); name != "" {
		s.sink.SetModel(name)
	}
	if l.Effort != "" && l.Effort != st.Thinking {
		if _, err := s.call(c, CallWait, "set_thinking_level", map[string]any{"level": l.Effort}); err != nil {
			log.Printf("pi %s: thinking %s: %v", s.agentID, l.Effort, err)
		}
	}
	if fast, ok := l.Options["fast"]; ok && s.f.fast {
		if err := s.setFast(c, fast); err != nil {
			s.notice(fmt.Sprintf("%s couldn't set fast to %s: %v", s.name, fast, err))
		}
	}
	var history struct {
		Messages []message `json:"messages"`
	}
	if st.Messages > 0 && !s.Shown {
		if data, err := s.call(c, OpenWait, "get_messages", nil); err != nil {
			log.Printf("pi %s: get_messages: %v", s.agentID, err)
		} else {
			json.Unmarshal(data, &history)
		}
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.window = st.Model.Window
	streaming := st.Streaming && !s.ended
	failed := s.replay(history.Messages, streaming)
	s.ready = true
	if !streaming {
		clear(s.tools)
	}
	switch {
	case streaming:
		s.running, s.active, s.began = true, true, time.Now()
		s.sink.Working()
	// The turn left running ended while no pocketd listened.
	case midTurn:
		s.sink.TurnEnded(failed)
	}
	for _, f := range s.held {
		f()
	}
	s.held = nil
	s.drain()
	return nil, nil
}

func (s *Session) setFast(c *Client, fast string) error {
	_, err := s.call(c, CallWait, "set_fast_mode", map[string]any{"enabled": fast == "true"})
	return err
}

func (s *Session) call(c *Client, wait time.Duration, kind string, fields map[string]any) (json.RawMessage, error) {
	ctx, cancel := context.WithTimeout(s.ctx, wait)
	defer cancel()
	data, err := c.Call(ctx, kind, fields)
	if errors.Is(err, context.DeadlineExceeded) {
		err = fmt.Errorf("it didn't answer %s in %v", kind, wait)
	}
	return data, err
}

func (s *Session) usable() (*Client, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	switch {
	case s.err != nil:
		return nil, s.failed()
	case !s.ready:
		return nil, fmt.Errorf("%s is still starting", s.name)
	}
	return s.c, nil
}

func (s *Session) failed() error { return fmt.Errorf("%s didn't start: %w", s.name, s.err) }

// Saved reports whether pi can resume the session: it writes the file it
// names only once the session holds a prompt or a reply.
func (s *Session) Saved() (bool, error) {
	c, err := s.usable()
	if err != nil {
		return false, err
	}
	var st struct {
		File string `json:"sessionFile"`
	}
	data, err := s.call(c, CallWait, "get_state", nil)
	if err != nil {
		return false, err
	}
	if json.Unmarshal(data, &st) != nil {
		return false, nil
	}
	_, err = os.Stat(st.File)
	return err == nil, nil
}

// Settings switches the model, named provider/id, the thinking level or both for the session's next turns.
func (s *Session) Settings(name, effort string) error {
	c, err := s.usable()
	if err != nil {
		return err
	}
	if name != "" {
		provider, id, ok := strings.Cut(name, "/")
		if !ok || provider == "" || id == "" {
			return fmt.Errorf("%s names a model as provider/id", s.name)
		}
		data, err := s.call(c, CallWait, "set_model", map[string]any{"provider": provider, "modelId": id})
		if err != nil {
			return err
		}
		var m model
		json.Unmarshal(data, &m)
		s.mu.Lock()
		s.window = cmp.Or(m.Window, s.window)
		s.mu.Unlock()
	}
	if effort != "" {
		if _, err := s.call(c, CallWait, "set_thinking_level", map[string]any{"level": effort}); err != nil {
			return err
		}
	}
	return nil
}

// Commands lists what the agent takes after a slash: pi's skills, and
// everything omp tells of.
func (s *Session) Commands() ([]proto.Command, error) {
	c, err := s.usable()
	if err != nil {
		return nil, err
	}
	s.mu.Lock()
	told := slices.Clone(s.commands)
	s.mu.Unlock()
	if told != nil {
		return told, nil
	}
	data, err := s.call(c, CallWait, s.f.commands, nil)
	if err != nil {
		return nil, err
	}
	return commands(data, !s.f.allCommands), nil
}

// commands reads a list of slash commands; skills keeps only pi's skills,
// as its other commands need its TUI.
func commands(data json.RawMessage, skills bool) []proto.Command {
	var r struct {
		Commands []struct {
			Name        string `json:"name"`
			Description string `json:"description"`
			Source      string `json:"source"`
			Input       struct {
				Hint string `json:"hint"`
			} `json:"input"`
		} `json:"commands"`
	}
	json.Unmarshal(data, &r)
	out := []proto.Command{}
	for _, c := range r.Commands {
		if c.Name == "" || strings.ContainsAny(c.Name, " \t\n/\\") || skills && c.Source != "skill" {
			continue
		}
		if !slices.ContainsFunc(out, func(o proto.Command) bool { return o.Name == c.Name }) {
			out = append(out, proto.Command{Name: c.Name, Description: c.Description, ArgumentHint: c.Input.Hint})
		}
	}
	return out
}

// Prompt has pi take text: as a turn, or into the turn under way, where pi
// reads it once the tools it runs have ended.
func (s *Session) Prompt(text string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	switch {
	case s.err != nil:
		return s.failed()
	case !s.ready || s.compacting || s.stopping:
		s.queue = append(s.queue, text)
	default:
		s.send(text)
	}
	return nil
}

func (s *Session) drain() {
	queue := s.queue
	s.queue = nil
	for _, text := range queue {
		s.send(text)
	}
}

// send writes text as a prompt. pi shows it as the user's when it reads it,
// which is when it goes in the timeline.
func (s *Session) send(text string) {
	fields := map[string]any{"message": text}
	starts := !s.running
	if starts {
		s.running, s.began = true, time.Now()
		s.sink.Working()
	} else {
		fields["streamingBehavior"] = "steer"
	}
	p := &prompt{text: text}
	s.sent = append(s.sent, p)
	var err error
	p.id, err = s.c.Send("prompt", fields, func(data json.RawMessage, err error) {
		var r struct {
			Disposition  string `json:"disposition"`
			AgentInvoked *bool  `json:"agentInvoked"`
		}
		json.Unmarshal(data, &r)
		s.mu.Lock()
		defer s.mu.Unlock()
		switch {
		case err != nil:
			s.refused(p, starts, err)
		// An extension took it as its command, or omp as one of its own: no agent runs for it.
		case r.Disposition == "handled" || r.AgentInvoked != nil && !*r.AgentInvoked:
			s.handled(p.id)
		}
	})
	if starts {
		s.turn = p.id
	}
	if err != nil {
		s.refused(p, starts, err)
	}
}

// refused shows the prompt pi didn't take, and why: as the failure of the
// turn it started, else as a notice in the turn under way.
func (s *Session) refused(p *prompt, starts bool, err error) {
	took := s.take(p.id)
	switch {
	case starts:
		s.fail(p.id, err.Error())
	case took:
		s.notice(fmt.Sprintf("%s didn't take “%s”: %v", s.name, p.text, err))
	}
}

// handled ends the turn of the prompt named id, which ran as a command and
// no agent: nothing else tells its end.
func (s *Session) handled(id string) {
	s.take(id)
	if id == s.turn && !s.active {
		s.settle(false)
	}
}

// take shows the prompt named id as the user's, if pi hasn't: it shows none
// it refused or took as a command.
func (s *Session) take(id string) bool {
	i := slices.IndexFunc(s.sent, func(p *prompt) bool { return p.id == id })
	if i < 0 {
		return false
	}
	s.user(s.sent[i].text)
	s.sent = slices.Delete(s.sent, i, i+1)
	return true
}

// fail ends the turn the prompt named id started with pi's reason.
func (s *Session) fail(id, reason string) {
	if id == s.turn && s.running {
		s.failure = &reason
		s.settle(false)
	}
}

// Cancel stops the turn or the compaction under way, and drops the prompts
// that waited in it. Whatever pi's extensions asked is answered cancelled.
func (s *Session) Cancel() {
	s.mu.Lock()
	busy, c := s.running || s.compacting, s.c
	s.stopping, s.queue, s.compactNext = s.running, nil, false
	asks := s.asks
	s.asks = map[string]context.CancelFunc{}
	if !busy && len(asks) > 0 {
		s.sink.Stopped()
	}
	s.mu.Unlock()
	for _, cancel := range asks {
		cancel()
	}
	if !busy {
		return
	}
	// pi goes on with the prompts it holds after an abort, unless they go first.
	c.Send("clear_queue", nil, nil)
	// pi answers once it is idle, so a turn it ended without saying so ends
	// here; one it told the end of may have a prompt that waited running by now.
	c.Send("abort", nil, func(json.RawMessage, error) {
		s.mu.Lock()
		defer s.mu.Unlock()
		if s.stopping {
			s.settle(true)
		}
	})
}

// Compact has pi summarise its context, once the turn under way ends: pi
// would stop it to do so.
func (s *Session) Compact() error {
	if _, err := s.usable(); err != nil {
		return err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.running {
		s.compactNext = true
		return nil
	}
	return s.compact()
}

func (s *Session) compact() error {
	s.compacting = true
	s.sink.Compacting()
	_, err := s.c.Send("compact", nil, func(data json.RawMessage, err error) {
		s.mu.Lock()
		defer s.mu.Unlock()
		if err != nil {
			s.compacted(false, nil, "manual", err.Error())
		} else {
			s.compacted(true, data, "manual", "")
		}
	})
	return err
}

// compacted ends the compaction under way: ok with what it came to, or with
// why it failed; neither is one the user stopped. pi tells its end as an
// event and as compact's response, and the first counts.
func (s *Session) compacted(ok bool, result json.RawMessage, reason, failure string) {
	if !s.compacting {
		return
	}
	s.compacting = false
	var r struct {
		After int64 `json:"estimatedTokensAfter"`
	}
	json.Unmarshal(result, &r)
	switch {
	case ok:
		trigger := "auto"
		if reason == "manual" {
			trigger = reason
		}
		s.sink.Apply(timeline.Event{Kind: "compacted", Trigger: trigger})
		if r.After > 0 {
			s.sink.SetTokens(r.After, s.window)
		}
	case failure != "":
		// pi's event says so before its reason; compact's response doesn't.
		s.notice(fmt.Sprintf("%s couldn't compact: %s", s.name, strings.TrimPrefix(failure, "Compaction failed: ")))
	}
	s.sink.Compacted()
	s.drain()
}

// settle ends the turn: pi goes on with nothing by itself.
func (s *Session) settle(aborted bool) {
	if !s.running {
		return
	}
	s.flush()
	e := timeline.Event{Kind: "result", DurationMs: time.Since(s.began).Milliseconds()}
	if s.usage != (proto.TurnUsage{}) {
		usage := s.usage
		e.Usage = &usage
	}
	stopped := s.stopping || aborted
	switch {
	case stopped:
		e.Error = "interrupted"
	case s.failure != nil:
		e.Error = cmp.Or(*s.failure, s.name+"'s turn failed")
	default:
		e.OK = true
	}
	s.running, s.active, s.stopping, s.retrying, s.failure, s.usage, s.turn = false, false, false, false, nil, proto.TurnUsage{}, ""
	// A backend may number its calls afresh each turn.
	clear(s.tools)
	s.sink.Apply(e)
	if stopped {
		// pi dropped the prompts it held with the turn.
		s.sent = nil
		s.sink.Stopped()
	} else {
		s.sink.TurnEnded(!e.OK)
	}
	go s.stats(s.c)
	if s.compactNext {
		s.compactNext = false
		if err := s.compact(); err != nil {
			s.compacted(false, nil, "manual", err.Error())
		}
		return
	}
	s.drain()
}

// stats tells the context pi holds now that the turn has ended.
func (s *Session) stats(c *Client) {
	data, err := s.call(c, CallWait, "get_session_stats", nil)
	var r struct {
		Context struct {
			Tokens int64 `json:"tokens"`
			Window int64 `json:"contextWindow"`
		} `json:"contextUsage"`
	}
	// pi knows no tokens right after a compaction, whose estimate stands.
	if err != nil || json.Unmarshal(data, &r) != nil || r.Context.Tokens == 0 {
		return
	}
	s.mu.Lock()
	s.window = cmp.Or(r.Context.Window, s.window)
	s.mu.Unlock()
	s.sink.SetTokens(r.Context.Tokens, r.Context.Window)
}

func (s *Session) notice(text string) {
	if text != "" {
		s.sink.Apply(timeline.Event{Kind: "notice", Text: text})
	}
}

func (s *Session) Close() {
	s.cancel()
	s.mu.Lock()
	c := s.c
	s.mu.Unlock()
	if c != nil {
		c.Close()
	}
}
