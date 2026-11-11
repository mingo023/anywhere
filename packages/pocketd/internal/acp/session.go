package acp

import (
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"slices"
	"strconv"
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
// status, which ACP leaves to the client to keep.
type Sink interface {
	Apply(timeline.Event)
	SetTitle(string)
	SetTokens(used, window int64)
	SetModel(string)
	// Bound names the agent's session once it is open.
	Bound(session string)
	// Loaded says the agent loaded its session: the history that follows is all of it.
	Loaded()
	Working()
	NeedsYou()
	TurnEnded(failed bool)
	// Stopped ends a turn the user stopped, which is no failure.
	Stopped()
	Compacting()
	Compacted()
}

var (
	// OpenWait is how long an agent has to answer each call that opens its session.
	OpenWait = 60 * time.Second
	// CallWait is how long it has to answer a switch.
	CallWait = 15 * time.Second
	// StopWait is how long it has to end the turn it was told to stop.
	StopWait = 5 * time.Second
)

// Left is how the pocketd before this one left the session's last turn.
type Left int

const (
	Ended   Left = iota
	Running      // the agent is still on it
	Cut          // it stopped with that pocketd
)

// Session is one agent's ACP session: it sends the prompts, maps what the
// agent says to timeline events, and answers what it asks.
type Session struct {
	agentID string
	name    string // the agent's, for what the user reads
	f       flavor
	sink    Sink
	broker  *broker.Broker
	ctx     context.Context
	cancel  context.CancelFunc

	// mu is never held across a Call: the client's reader takes it for each message.
	mu      sync.Mutex
	c       *Client
	id      string // the session's, once open
	err     error  // why it never opened
	opening bool   // the call that opens it is out, so what the agent says is the session's history
	// replay holds that history until the agent has loaded it whole: a load that fails shows none of it.
	replay  []timeline.Event
	ready   bool // it is open and set to its launch: turns may start
	launch  state.Launch
	options []configOption
	model   string // the model the agent says it is on
	// queue holds the prompts sent before the session opened, while a turn runs or while it compacts, which ACP has no place for.
	queue    []string
	running  bool
	turn     int             // counts the turns and compactions started, so a late answer ends only its own
	began    time.Time       // when this pocketd started the turn running
	adopted  bool            // the turn running was started by a pocketd before this one
	stopping bool            // the user stopped the turn or the compaction
	stray    json.RawMessage // the end of a turn a pocketd before this one started, come before the session opened
	held     []func()        // the agent's requests from before the session opened
	asks     map[string]context.CancelFunc
	msg      block // the message the agent is writing
	blocks   int
	answered bool // the history's last prompt has an answer after it
	tools    map[string]*tool
	todos    []todo
	plans    int
	commands []proto.Command
	// compacting is while the agent summarises its context for Compact.
	compacting bool
	// compactNext is a Compact asked for mid-turn, which waits for the turn to end.
	compactNext bool
}

// block is the message the agent is writing, a chunk at a time.
type block struct {
	kind, id, text string // kind is user, assistant or thinking
}

type configOption struct {
	ID       string `json:"id"`
	Category string `json:"category"`
	Type     string `json:"type"`
	Current  any    `json:"currentValue"`
}

// setting finds the config option an agent keeps one of a launch's settings in.
type setting struct {
	ids      []string // the option's id, the likelier first
	category string   // else the first option of this category
	not      string   // that is not this one
}

var (
	// fx and Antigravity list their provider first, under the model category.
	modelSetting = setting{[]string{"model"}, "model", "provider"}
	// Cursor's thinking is an on/off switch under the effort category.
	effortSetting = setting{[]string{"effort", "reasoning", "reasoning_effort", "variant"}, "thought_level", "thinking"}
	modeSetting   = setting{[]string{"mode"}, "mode", ""}
)

func (g setting) of(options []configOption) *configOption {
	for _, id := range g.ids {
		if i := slices.IndexFunc(options, func(o configOption) bool { return o.ID == id }); i >= 0 {
			return &options[i]
		}
	}
	i := slices.IndexFunc(options, func(o configOption) bool {
		_, toggle := o.Current.(bool)
		return o.Category == g.category && o.ID != g.not && o.Type != "boolean" && !toggle
	})
	if i < 0 {
		return nil
	}
	return &options[i]
}

type todo struct {
	ID      string `json:"id"`
	Content string `json:"content"`
	Status  string `json:"status"`
}

// NewSession is a Session for agentID's provider, which holds prompts until Open.
func NewSession(agentID, provider string, sink Sink, b *broker.Broker) *Session {
	ctx, cancel := context.WithCancel(context.Background())
	return &Session{agentID: agentID, name: providers.Get(provider).Name, f: flavors[provider], sink: sink, broker: b, ctx: ctx, cancel: cancel,
		asks: map[string]context.CancelFunc{}, tools: map[string]*tool{}}
}

// opened is what session/new and session/load answer with.
type opened struct {
	SessionID string `json:"sessionId"`
	Snake     string `json:"session_id"`
	ID        string `json:"id"`
	Models    struct {
		Current string `json:"currentModelId"`
	} `json:"models"`
	ConfigOptions []configOption `json:"configOptions"`
}

// Open opens the agent's session over w in cwd: the saved one, whose history
// the agent replays into the timeline, else a new one. l is applied to it.
// left is how the pocketd before this one left its last turn. An agent that
// never opens it gives back the prompts that waited to be sent.
func (s *Session) Open(w Wire, cwd, saved string, l state.Launch, left Left) (unsent []string, err error) {
	c := Connect(w, s)
	s.mu.Lock()
	s.c, s.launch = c, l
	s.mu.Unlock()
	res, err := s.open(c, cwd, saved, l, left)
	s.mu.Lock()
	if err != nil {
		unsent = s.queue
		s.err, s.queue = err, nil
		s.mu.Unlock()
		return unsent, err
	}
	s.setOptions(res.ConfigOptions)
	id, model := s.id, cmp.Or(s.currentModel(), res.Models.Current)
	s.mu.Unlock()
	s.sink.Bound(id)
	if saved != "" && id != saved {
		s.sink.Apply(timeline.Event{Kind: "notice", Text: s.name + " could not restore the conversation, so this is a new one"})
	}
	s.apply(l, model)
	s.mu.Lock()
	defer s.mu.Unlock()
	s.ready = true
	for _, f := range s.held {
		f()
	}
	s.held = nil
	// The turn left running may have ended while no pocketd listened.
	if s.adopted && s.stray != nil {
		s.finish(s.stray, nil)
	} else if !s.running {
		s.next()
	}
	return nil, nil
}

func (s *Session) open(c *Client, cwd, saved string, l state.Launch, left Left) (opened, error) {
	caps := map[string]any{"fs": map[string]bool{"readTextFile": false, "writeTextFile": false}, "terminal": false}
	for k, v := range s.f.caps {
		caps[k] = v
	}
	init, err := s.call(c, OpenWait, "initialize", map[string]any{"protocolVersion": 1, "clientCapabilities": caps,
		"clientInfo": map[string]string{"name": "anywhere", "title": "Anywhere", "version": "0"}})
	if err != nil {
		return opened{}, err
	}
	if s.f.auth != nil {
		if method := s.f.auth(init); method != "" {
			params := map[string]any{"methodId": method}
			if s.f.authMeta != nil {
				params["_meta"] = s.f.authMeta
			}
			// An agent already signed in may refuse; its session tells.
			s.call(c, OpenWait, "authenticate", params)
		}
	}
	var r opened
	s.mu.Lock()
	s.opening = true
	s.mu.Unlock()
	defer func() {
		s.mu.Lock()
		s.opening = false
		for _, e := range s.replay {
			s.sink.Apply(e)
		}
		s.replay = nil
		s.mu.Unlock()
	}()
	if saved != "" {
		params := map[string]any{"sessionId": saved, "cwd": cwd, "mcpServers": []any{}}
		res, err := s.call(c, OpenWait, "session/load", params)
		// Hermes answers null for a session it doesn't have, and would resume it as a new one without a word.
		gone := err == nil && string(res) == "null"
		s.mu.Lock()
		s.loaded(err == nil && !gone, left)
		s.mu.Unlock()
		if err != nil {
			log.Printf("acp %s: session/load %s: %v", s.agentID, saved, err)
			// A resume replays nothing, so the timeline keeps what it shows.
			res, err = s.call(c, OpenWait, "session/resume", params)
		}
		if err == nil && !gone {
			json.Unmarshal(res, &r)
			s.mu.Lock()
			s.id, s.running, s.adopted = cmp.Or(r.SessionID, r.Snake, r.ID, saved), left == Running, left == Running
			s.mu.Unlock()
			return r, nil
		}
		log.Printf("acp %s: session %s is not to be had: %v", s.agentID, saved, err)
	}
	params := map[string]any{"cwd": cwd, "mcpServers": []any{}}
	if meta := s.f.newMeta[l.Access]; meta != nil && !l.Plan {
		params["_meta"] = meta
	}
	res, err := s.call(c, OpenWait, "session/new", params)
	if err != nil {
		return opened{}, err
	}
	json.Unmarshal(res, &r)
	id := cmp.Or(r.SessionID, r.Snake, r.ID)
	if id == "" {
		return opened{}, errors.New("it gave no session")
	}
	s.mu.Lock()
	s.id = id
	s.mu.Unlock()
	return r, nil
}

// loaded takes the history a session/load replayed: shown in place of what
// the timeline held when the load went ok, else dropped. A turn the pocketd
// before this one cut short ends interrupted, as do the calls it left open.
func (s *Session) loaded(ok bool, left Left) {
	s.flush()
	if ok && left != Running {
		cut := left == Cut && len(s.replay) > 0
		for id, t := range s.tools {
			if t.started {
				out := "Interrupted"
				s.record(timeline.Event{Kind: "tool_end", ToolUseID: id, Output: &out})
				cut = true
			}
		}
		clear(s.tools)
		switch {
		case cut:
			s.record(timeline.Event{Kind: "result", Error: "interrupted"})
		case s.answered:
			s.record(timeline.Event{Kind: "result", OK: true})
		}
	}
	history := s.replay
	s.replay, s.answered = nil, false
	if !ok {
		clear(s.tools)
		return
	}
	s.sink.Loaded()
	for _, e := range history {
		s.sink.Apply(e)
	}
}

// record adds e to the timeline, once the history it is part of has loaded.
func (s *Session) record(e timeline.Event) {
	if s.opening {
		s.replay = append(s.replay, e)
		return
	}
	s.sink.Apply(e)
}

func (s *Session) call(c *Client, wait time.Duration, method string, params map[string]any) (json.RawMessage, error) {
	ctx, cancel := context.WithTimeout(s.ctx, wait)
	defer cancel()
	res, err := c.Call(ctx, method, params)
	if errors.Is(err, context.DeadlineExceeded) {
		err = fmt.Errorf("it didn't answer %s in %v", method, wait)
	}
	return res, err
}

// request calls the open session.
func (s *Session) request(method string, params map[string]any) (json.RawMessage, error) {
	s.mu.Lock()
	c, id := s.c, s.id
	s.mu.Unlock()
	params["sessionId"] = id
	return s.call(c, CallWait, method, params)
}

// apply sets the session to launch l. An agent that can't take one of its
// settings says so in the chat and runs on what it has; model is the one it is on.
func (s *Session) apply(l state.Launch, model string) {
	if mode := s.f.mode(l.Access, l.Plan, false); mode != "" {
		if err := s.setMode(mode); err != nil {
			log.Printf("acp %s: mode %s: %v", s.agentID, mode, err)
		}
	}
	if l.Model != "" && l.Model != model {
		if err := s.setModel(l.Model); err != nil {
			s.sink.Apply(timeline.Event{Kind: "notice", Text: fmt.Sprintf("%s couldn't switch to %s: %v", s.name, l.Model, err)})
		} else {
			model = l.Model
		}
	}
	if model != "" {
		s.sink.SetModel(model)
	}
	if l.Effort != "" {
		if err := s.setEffort(l.Effort); err != nil {
			log.Printf("acp %s: effort %s: %v", s.agentID, l.Effort, err)
		}
	}
}

func unsupported(err error) bool {
	var e *Error
	return errors.As(err, &e) && e.Code == methodNotFound
}

// setOption sets config option o, and takes the options the agent answers with.
func (s *Session) setOption(o *configOption, value string) error {
	res, err := s.request("session/set_config_option", map[string]any{"configId": o.ID, "value": value})
	if err != nil {
		return err
	}
	var r struct {
		ConfigOptions []configOption `json:"configOptions"`
	}
	json.Unmarshal(res, &r)
	s.mu.Lock()
	s.setOptions(r.ConfigOptions)
	s.mu.Unlock()
	return nil
}

func (s *Session) setOptions(options []configOption) {
	if options != nil {
		s.options = options
	}
}

// option is the config option the session keeps g in.
func (s *Session) option(g setting) *configOption {
	s.mu.Lock()
	defer s.mu.Unlock()
	return g.of(s.options)
}

func (s *Session) currentModel() string {
	if o := modelSetting.of(s.options); o != nil {
		model, _ := o.Current.(string)
		return model
	}
	return ""
}

// setModel goes through the session's model option when it has one.
func (s *Session) setModel(model string) error {
	if o := s.option(modelSetting); o != nil {
		if err := s.setOption(o, model); !unsupported(err) {
			return err
		}
	}
	_, err := s.request("session/set_model", map[string]any{"modelId": model})
	return err
}

var errNoEffort = errors.New("its models carry their own effort")

func (s *Session) setEffort(effort string) error {
	if o := s.option(effortSetting); o != nil {
		return s.setOption(o, effort)
	}
	if !s.f.effortMode {
		return errNoEffort
	}
	if _, err := s.request("session/set_mode", map[string]any{"modeId": effort}); !unsupported(err) {
		return err
	}
	return nil
}

func (s *Session) setMode(mode string) error {
	if o := s.option(modeSetting); o != nil && s.f.modeOption {
		return s.setOption(o, mode)
	}
	_, err := s.request("session/set_mode", map[string]any{"modeId": mode})
	return err
}

func (s *Session) usable() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	switch {
	case s.err != nil:
		return s.failed()
	case !s.ready:
		return fmt.Errorf("%s is still starting", s.name)
	}
	return nil
}

func (s *Session) failed() error { return fmt.Errorf("%s didn't start: %w", s.name, s.err) }

// Settings switches the model, the effort or both for the session's next turns.
func (s *Session) Settings(model, effort string) error {
	if err := s.usable(); err != nil {
		return err
	}
	if model != "" {
		if err := s.setModel(model); err != nil {
			return err
		}
	}
	if effort != "" {
		if err := s.setEffort(effort); err != nil {
			return fmt.Errorf("%s can't switch effort: %w", s.name, err)
		}
	}
	return nil
}

// Access switches what the agent does without asking: its own mode where it
// has one for access, and how pocketd answers what it still asks. It leaves plan mode.
func (s *Session) Access(access string) error {
	if err := s.usable(); err != nil {
		return err
	}
	s.mu.Lock()
	planning, full := s.launch.Plan, s.launch.Access == "full"
	s.mu.Unlock()
	if s.f.spawned && (planning || full != (access == "full")) {
		return fmt.Errorf("%s takes plan mode and full access only as it starts, so start a new session to switch", s.name)
	}
	if mode := s.f.mode(access, false, planning); mode != "" {
		if err := s.setMode(mode); err != nil {
			return err
		}
	}
	s.mu.Lock()
	s.launch.Access, s.launch.Plan = access, false
	s.mu.Unlock()
	return nil
}

// Commands lists the slash commands the agent told of.
func (s *Session) Commands() ([]proto.Command, error) {
	if err := s.usable(); err != nil {
		return nil, err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	return slices.Clone(s.commands), nil
}

// Prompt starts a turn on text, once the session is open and the turn under way has ended.
func (s *Session) Prompt(text string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	switch {
	case s.err != nil:
		return s.failed()
	case !s.ready || s.running || s.compacting:
		s.queue = append(s.queue, text)
	default:
		s.start(text)
	}
	return nil
}

// start sends text as a turn. ACP echoes no prompt, so it goes in the timeline here.
func (s *Session) start(text string) {
	s.running, s.began = true, time.Now()
	s.turn++
	s.sink.Apply(timeline.Event{Kind: "user", Text: text})
	s.sink.Working()
	c, id, turn := s.c, s.id, s.turn
	go func() {
		res, err := c.Call(s.ctx, "session/prompt", map[string]any{"sessionId": id, "prompt": []map[string]string{{"type": "text", "text": text}}})
		if s.ctx.Err() != nil {
			return
		}
		s.mu.Lock()
		defer s.mu.Unlock()
		// A stop the agent was slow to answer has ended the turn already.
		if s.running && s.turn == turn {
			s.finish(res, err)
		}
	}()
}

// next starts the compaction asked for mid-turn, else the turn of the oldest prompt that waits.
func (s *Session) next() {
	switch {
	case !s.ready:
	case s.compactNext:
		s.compactNext = false
		s.compact()
	case len(s.queue) > 0:
		text := s.queue[0]
		s.queue = s.queue[1:]
		s.start(text)
	}
}

// finish ends the turn with what session/prompt answered.
func (s *Session) finish(res json.RawMessage, err error) {
	var r struct {
		StopReason string `json:"stopReason"`
		Usage      *struct {
			Input  int `json:"inputTokens"`
			Output int `json:"outputTokens"`
			Cached int `json:"cachedReadTokens"`
		} `json:"usage"`
	}
	json.Unmarshal(res, &r)
	s.flush()
	stopped := s.stopping || err == nil && r.StopReason == "cancelled"
	s.running, s.adopted, s.stopping, s.stray = false, false, false, nil
	e := timeline.Event{Kind: "result"}
	if !s.began.IsZero() {
		e.DurationMs = time.Since(s.began).Milliseconds()
	}
	s.began = time.Time{}
	if r.Usage != nil {
		e.Usage = &proto.TurnUsage{InputTokens: r.Usage.Input, OutputTokens: r.Usage.Output, CacheReadTokens: r.Usage.Cached}
	}
	switch {
	case stopped:
		e.Error = "interrupted"
	case err != nil:
		e.Error = err.Error()
	case r.StopReason == "end_turn" || r.StopReason == "":
		e.OK = true
	default:
		e.Error = fmt.Sprintf("%s ended the turn (%s)", s.name, r.StopReason)
	}
	s.sink.Apply(e)
	if stopped {
		s.sink.Stopped()
	} else {
		s.sink.TurnEnded(!e.OK)
	}
	s.next()
}

// Compact has the agent summarise its context, once the turn under way ends.
func (s *Session) Compact() error {
	if err := s.usable(); err != nil {
		return err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	switch {
	case s.f.compact == "":
		return fmt.Errorf("%s compacts its context by itself", s.name)
	case s.compacting || s.compactNext:
		return fmt.Errorf("%s is compacting already", s.name)
	case s.running:
		s.compactNext = true
	default:
		s.compact()
	}
	return nil
}

func (s *Session) compact() {
	s.compacting = true
	s.turn++
	s.sink.Compacting()
	method, params := s.f.compact, map[string]any{"sessionId": s.id}
	if strings.HasPrefix(method, "/") {
		method, params["prompt"] = "session/prompt", []map[string]string{{"type": "text", "text": s.f.compact}}
	}
	c, turn := s.c, s.turn
	go func() {
		_, err := c.Call(s.ctx, method, params)
		if s.ctx.Err() != nil {
			return
		}
		s.mu.Lock()
		defer s.mu.Unlock()
		// A stop the agent was slow to answer has ended it already.
		if s.compacting && s.turn == turn {
			s.compacted(err)
		}
	}()
}

// compacted ends the compaction under way with what the agent answered; one
// the user stopped ends without a word.
func (s *Session) compacted(err error) {
	stopped := s.stopping
	s.compacting, s.stopping = false, false
	switch {
	case stopped:
	case err != nil:
		s.sink.Apply(timeline.Event{Kind: "notice", Text: fmt.Sprintf("%s couldn't compact: %v", s.name, err)})
	default:
		s.sink.Apply(timeline.Event{Kind: "compacted", Trigger: "manual"})
	}
	if !stopped {
		s.sink.Compacted()
	}
	s.next()
}

// Stray ends the turn a pocketd before this one started, or its compaction,
// which Grok answers with no stopReason.
func (s *Session) Stray(result json.RawMessage) {
	s.mu.Lock()
	defer s.mu.Unlock()
	switch {
	case s.id == "":
		s.stray = result
	case s.adopted:
		s.finish(result, nil)
	}
}

// Cancel stops the turn or the compaction under way and drops the prompts
// and the compaction that waited for its end. Whatever the agent asked is
// answered cancelled. An agent that never answers the call it was told to
// stop would hold up the next one, so the call ends here once StopWait is up.
func (s *Session) Cancel() {
	s.mu.Lock()
	busy, c, id, turn := s.running || s.compacting, s.c, s.id, s.turn
	s.stopping, s.compactNext, s.queue = busy, false, nil
	asks := s.asks
	s.asks = map[string]context.CancelFunc{}
	s.mu.Unlock()
	if busy {
		c.Notify("session/cancel", map[string]any{"sessionId": id})
		time.AfterFunc(StopWait, func() {
			s.mu.Lock()
			defer s.mu.Unlock()
			if !s.stopping || s.turn != turn || s.ctx.Err() != nil {
				return
			}
			switch {
			case s.running:
				s.finish(nil, nil)
			case s.compacting:
				s.compacted(nil)
			}
		})
	}
	for _, cancel := range asks {
		cancel()
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

func snake(s string) string { return strings.ReplaceAll(s, "-", "_") }

// method is an agent's own method without the underscore ACP marks extensions with.
func method(name string) string { return strings.TrimPrefix(name, "_") }

func (s *Session) Notify(name string, params json.RawMessage) {
	var p struct {
		Update       json.RawMessage `json:"update"`
		Notification json.RawMessage `json:"notification"`
		Payload      json.RawMessage `json:"payload"`
	}
	json.Unmarshal(params, &p)
	s.mu.Lock()
	defer s.mu.Unlock()
	// What an agent said before its session opened, it said to a pocketd before this one: its history has it.
	if s.id == "" && !s.opening {
		return
	}
	switch name := method(name); name {
	case "session/update":
		s.update(cmp.Or(string(p.Update), string(params)))
	// Grok wraps some updates.
	case "x.ai/session_notification":
		inner := cmp.Or(string(p.Notification), string(p.Payload), string(params))
		json.Unmarshal([]byte(inner), &p)
		s.update(cmp.Or(string(p.Update), inner))
	default:
		if tell := tells[name]; tell != nil {
			tell(s, params)
		}
	}
}

func (s *Session) update(raw string) {
	var u struct {
		Kind      string          `json:"sessionUpdate"`
		Snake     string          `json:"session_update"`
		Type      string          `json:"type"`
		Content   json.RawMessage `json:"content"`
		MessageID string          `json:"messageId"`
		Title     string          `json:"title"`
		Used      int64           `json:"used"`
		Size      int64           `json:"size"`
		After     int64           `json:"tokens_after"` // Grok's, once it compacted
		Usage     struct {
			Total int64 `json:"totalTokens"`
		} `json:"usage"`
		Entries  []todo         `json:"entries"`
		Text     string         `json:"text"`
		Options  []configOption `json:"configOptions"`
		Commands []struct {
			Name        string `json:"name"`
			Description string `json:"description"`
			Input       struct {
				Hint string `json:"hint"`
			} `json:"input"`
		} `json:"availableCommands"`
	}
	if json.Unmarshal([]byte(raw), &u) != nil {
		return
	}
	// Grok and Cursor also name an update's kind and a message's text these other ways.
	kind, said := cmp.Or(u.Kind, u.Snake, u.Type), cmp.Or(text(u.Content), u.Text)
	switch kind {
	case "user_message_chunk":
		s.chunk("user", u.MessageID, said)
	case "agent_message_chunk", "agent_message":
		s.chunk("assistant", u.MessageID, said)
	case "agent_thought_chunk", "agent_thought":
		s.chunk("thinking", u.MessageID, said)
	case "tool_call", "tool_call_update", "tool_call_content_chunk":
		s.tool(call(json.RawMessage(raw)), kind == "tool_call")
	case "plan", "current_plan":
		if u.Entries != nil {
			s.tasks(u.Entries)
		}
		if u.Text != "" {
			s.plan(u.Text)
		}
	case "usage_update":
		s.sink.SetTokens(u.Used, u.Size)
	case "session_info_update":
		if u.Title != "" {
			s.sink.SetTitle(u.Title)
		}
	case "available_commands_update":
		s.commands = nil
		for _, c := range u.Commands {
			s.commands = append(s.commands, proto.Command{Name: c.Name, Description: c.Description, ArgumentHint: c.Input.Hint})
		}
	case "config_option_update":
		s.setOptions(u.Options)
	case "auto_compact_completed":
		s.sink.SetTokens(u.After, 0)
	default:
		// Grok tells a turn's tokens as it ends.
		if u.Usage.Total > 0 {
			s.sink.SetTokens(u.Usage.Total, 0)
		}
	}
}

// chunk adds text to the message the agent is writing, which drafts in the
// chat until flush writes it whole. A session's history comes whole at once.
func (s *Session) chunk(kind, id, text string) {
	// The agent echoes no prompt as it runs; the one that started the turn is in the timeline.
	// OpenCode writes its summary as a reply, which a compaction shows none of.
	if kind == "user" && !s.opening || s.compacting {
		return
	}
	if s.msg.kind != kind || s.msg.id != id {
		s.flush()
		s.msg = block{kind: kind, id: id}
		s.blocks++
	}
	// Hermes starts the reply after a tool with blank lines, and sends a reply
	// it rewrote whole again, under the id it streamed it with.
	if whole := strings.TrimLeft(text, "\n"); s.msg.text == "" {
		text = whole
	} else if len(whole) > len(s.msg.text) && strings.HasPrefix(whole, s.msg.text) {
		text = whole[len(s.msg.text):]
	}
	if text == "" {
		return
	}
	s.msg.text += text
	if !s.opening {
		s.sink.Apply(timeline.Event{Kind: kind + "_draft", Text: text, Draft: strconv.Itoa(s.blocks)})
	}
}

// flush writes the message the agent was writing, now that something else follows it.
func (s *Session) flush() {
	b := s.msg
	if s.msg = (block{}); b.text == "" {
		return
	}
	switch b.kind {
	case "user":
		// A prompt in the history starts a turn, which ends the one answered before it.
		if s.answered {
			s.record(timeline.Event{Kind: "result", OK: true})
		}
		s.record(timeline.Event{Kind: "user", Text: b.text})
		s.answered = false
		return
	case "assistant":
		if s.opening && s.f.summary != "" && strings.HasPrefix(b.text, s.f.summary) {
			if s.answered {
				s.record(timeline.Event{Kind: "result", OK: true})
			}
			s.record(timeline.Event{Kind: "compacted"})
			s.answered = false
			return
		}
		s.record(timeline.Event{Kind: "assistant_text", Text: b.text})
	default:
		s.record(timeline.Event{Kind: "thinking", Text: b.text})
	}
	s.answered = s.opening
}

// tool takes what the agent tells of a call. It shows once there is something
// to show of it, and ends once the agent says how it went.
func (s *Session) tool(u toolCall, starts bool) {
	t := s.tools[u.ID]
	if t == nil {
		// Hermes ends each permission check as a call it never started.
		if u.ID == "" || !starts && u.Title == "" {
			return
		}
		t = &tool{}
		s.tools[u.ID] = t
	}
	t.merge(u)
	if !t.started && !t.waits() {
		s.show(t)
	}
	if t.started && t.done() {
		s.record(timeline.Event{Kind: "tool_end", ToolUseID: t.ID, OK: t.Status == "completed", Output: t.output()})
		delete(s.tools, t.ID)
	}
}

func (s *Session) show(t *tool) {
	s.flush()
	t.started = true
	name, detail := t.describe()
	s.record(timeline.Event{Kind: "tool_start", ToolUseID: t.ID, Name: name, Detail: &detail})
	s.answered = s.opening
}

// tasks shows the agent's plan entries as the chat's task list.
func (s *Session) tasks(entries []todo) {
	s.flush()
	s.plans++
	input, _ := json.Marshal(map[string]any{"todos": entries})
	s.record(timeline.Event{Kind: "tool_start", ToolUseID: "plan-" + strconv.Itoa(s.plans), Name: "TodoWrite", Input: input})
}

// plan shows the plan the agent wrote.
func (s *Session) plan(text string) {
	s.flush()
	s.plans++
	input, _ := json.Marshal(map[string]string{"plan": text})
	s.record(timeline.Event{Kind: "tool_start", ToolUseID: "plan-" + strconv.Itoa(s.plans), Name: "ExitPlanMode", Input: input})
}

// tells are the agents' own methods that only tell the client something,
// which Cursor sends as a request or as a notification. Each returns what a
// request is answered with.
var tells = map[string]func(*Session, json.RawMessage) any{
	"cursor/create_plan":  (*Session).cursorPlan,
	"x.ai/exit_plan_mode": (*Session).grokPlan,
	"cursor/update_todos": (*Session).setTodos,
	"cursor/task":         (*Session).task,
}

func (s *Session) cursorPlan(params json.RawMessage) any {
	var p struct {
		Plan string `json:"plan"`
	}
	json.Unmarshal(params, &p)
	if p.Plan != "" {
		s.plan(p.Plan)
	}
	return map[string]any{"outcome": map[string]string{"outcome": "accepted"}}
}

// grokPlan answers abandoned: Grok's plan ends its turn, and the turn that
// builds it is the user's to start.
func (s *Session) grokPlan(params json.RawMessage) any {
	var p struct {
		Plan        string `json:"plan"`
		PlanContent string `json:"planContent"`
		Content     string `json:"content"`
		Input       struct {
			Plan string `json:"plan"`
		} `json:"input"`
	}
	json.Unmarshal(params, &p)
	if plan := strings.TrimSpace(cmp.Or(p.PlanContent, p.Plan, p.Content, p.Input.Plan)); plan != "" {
		s.plan(plan)
	}
	return map[string]string{"outcome": "abandoned"}
}

// setTodos takes Cursor's todos: all of them, or with merge the ones that changed.
func (s *Session) setTodos(params json.RawMessage) any {
	var p struct {
		Merge bool   `json:"merge"`
		Todos []todo `json:"todos"`
	}
	if json.Unmarshal(params, &p) != nil || p.Todos == nil {
		return struct{}{}
	}
	if !p.Merge {
		s.todos = nil
	}
	for _, t := range p.Todos {
		i := slices.IndexFunc(s.todos, func(o todo) bool { return t.ID != "" && o.ID == t.ID })
		if i < 0 {
			s.todos = append(s.todos, t)
			continue
		}
		s.todos[i].Status = cmp.Or(t.Status, s.todos[i].Status)
		s.todos[i].Content = cmp.Or(t.Content, s.todos[i].Content)
	}
	s.tasks(s.todos)
	return struct{}{}
}

// task shows a sub-agent Cursor runs as the call that started it.
func (s *Session) task(params json.RawMessage) any {
	var p struct {
		ID          string `json:"toolCallId"`
		Description string `json:"description"`
		Model       string `json:"model"`
		DurationMs  *int64 `json:"durationMs"`
	}
	if json.Unmarshal(params, &p) != nil || p.ID == "" {
		return struct{}{}
	}
	t := s.tools[p.ID]
	if t == nil {
		t = &tool{toolCall: toolCall{ID: p.ID}}
		s.tools[p.ID] = t
	}
	if !t.started {
		s.flush()
		t.started = true
		s.record(timeline.Event{Kind: "tool_start", ToolUseID: p.ID, Name: "task", Detail: &proto.ToolDetail{Kind: "task", Description: p.Description, Model: p.Model}})
	}
	if p.DurationMs != nil {
		s.record(timeline.Event{Kind: "tool_end", ToolUseID: p.ID, OK: true})
		delete(s.tools, p.ID)
	}
	return struct{}{}
}
