package codex

import (
	"context"
	"encoding/json"
	"log"
	"slices"
	"strings"
	"sync"
	"time"

	"pocketd/internal/broker"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

type Sink interface {
	Apply(timeline.Event)
	SetTitle(string)
	SetTokens(used, window int64)
}

// Session follows one thread: it maps app-server items to timeline events
// and forwards approvals to the phone.
type Session struct {
	c       *Client
	agentID string
	sink    Sink
	broker  *broker.Broker
	ctx     context.Context
	cancel  context.CancelFunc

	mu       sync.Mutex
	ready    bool
	queued   []func()
	streamed map[string]bool
	details  map[string]proto.ToolDetail
}

type item struct {
	Type             string                  `json:"type"`
	ID               string                  `json:"id"`
	Text             string                  `json:"text"`
	Content          []struct{ Text string } `json:"content"`
	Summary          []string                `json:"summary"`
	Command          string                  `json:"command"`
	Status           string                  `json:"status"`
	AggregatedOutput *string                 `json:"aggregatedOutput"`
	ExitCode         *int                    `json:"exitCode"`
	DurationMs       *int64                  `json:"durationMs"`
	Changes          []struct {
		Path string `json:"path"`
		Diff string `json:"diff"`
	} `json:"changes"`
	Server    string          `json:"server"`
	Tool      string          `json:"tool"`
	Arguments json.RawMessage `json:"arguments"`
	Query     string          `json:"query"`
}

type turn struct {
	ID         string `json:"id"`
	Items      []item `json:"items"`
	Status     string `json:"status"`
	DurationMs int64  `json:"durationMs"`
	Error      *struct {
		Message string `json:"message"`
	} `json:"error"`
}

var ResumeRetry = 500 * time.Millisecond

// MaxTurnPages caps how many turns/list pages a resume reads before it
// replays what it has.
var MaxTurnPages = 50

type resumed struct {
	Thread struct {
		Name  *string `json:"name"`
		Turns []turn  `json:"turns"`
	} `json:"thread"`
	TurnsBackwardsCursor *string `json:"turnsBackwardsCursor"`
}

// Open subscribes to a thread and replays its history. Resume fails until the
// thread has its first turn, so it retries until ctx ends.
func Open(ctx context.Context, sock, threadID, agentID string, sink Sink, b *broker.Broker) (*Session, error) {
	sctx, cancel := context.WithCancel(context.Background())
	s := &Session{agentID: agentID, sink: sink, broker: b, ctx: sctx, cancel: cancel,
		streamed: map[string]bool{}, details: map[string]proto.ToolDetail{}}
	c, err := Dial(ctx, sock, s)
	if err != nil {
		cancel()
		return nil, err
	}
	s.c = c
	for {
		res, err := c.Call(ctx, "thread/resume", map[string]any{"threadId": threadID})
		if err == nil {
			var r resumed
			json.Unmarshal(res, &r)
			s.hydrate(ctx, threadID, &r)
			s.replay(r)
			return s, nil
		}
		select {
		case <-ctx.Done():
			s.Close()
			return nil, err
		case <-time.After(ResumeRetry):
		}
	}
}

// hydrate puts the turns a paginated resume left out before r's own, oldest
// first. A page includes the turn its cursor names; r's copy of it wins.
func (s *Session) hydrate(ctx context.Context, threadID string, r *resumed) {
	cursor := r.TurnsBackwardsCursor
	var older []turn
	for page := 0; cursor != nil; page++ {
		if page == MaxTurnPages {
			log.Printf("codex %s: history stops after %d pages", threadID, page)
			break
		}
		res, err := s.c.Call(ctx, "thread/turns/list", map[string]any{"threadId": threadID, "cursor": *cursor, "sortDirection": "desc", "itemsView": "full"})
		if err != nil {
			log.Printf("codex %s: turns/list: %v", threadID, err)
			break
		}
		var p struct {
			Data       []turn  `json:"data"`
			NextCursor *string `json:"nextCursor"`
		}
		json.Unmarshal(res, &p)
		older = append(older, p.Data...)
		cursor = p.NextCursor
	}
	slices.Reverse(older)
	for _, t := range r.Thread.Turns {
		older = slices.DeleteFunc(older, func(o turn) bool { return o.ID == t.ID })
	}
	r.Thread.Turns = append(older, r.Thread.Turns...)
}

func (s *Session) replay(r resumed) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if r.Thread.Name != nil {
		s.sink.SetTitle(*r.Thread.Name)
	}
	for _, t := range r.Thread.Turns {
		for _, it := range t.Items {
			s.started(it)
			if it.Status != "inProgress" {
				s.completed(it)
			}
		}
		if t.Status != "inProgress" {
			s.turnDone(t)
		}
	}
	for _, f := range s.queued {
		f()
	}
	s.queued = nil
	s.ready = true
}

// run applies live messages in order, holding them back until replay is done.
func (s *Session) run(f func()) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if !s.ready {
		s.queued = append(s.queued, f)
		return
	}
	f()
}

func (s *Session) Notify(method string, params json.RawMessage) {
	var p struct {
		Item       item            `json:"item"`
		Turn       turn            `json:"turn"`
		ItemID     string          `json:"itemId"`
		Delta      string          `json:"delta"`
		ThreadName *string         `json:"threadName"`
		RequestID  json.RawMessage `json:"requestId"`
		TokenUsage struct {
			Last struct {
				TotalTokens int64 `json:"totalTokens"`
			} `json:"last"`
			ModelContextWindow int64 `json:"modelContextWindow"`
		} `json:"tokenUsage"`
	}
	if json.Unmarshal(params, &p) != nil {
		return
	}
	s.run(func() {
		switch method {
		case "item/started":
			s.started(p.Item)
		case "item/completed":
			s.completed(p.Item)
		case "item/agentMessage/delta":
			s.streamed[p.ItemID] = true
			s.sink.Apply(timeline.Event{Kind: "assistant_text", Text: p.Delta})
		case "turn/completed":
			s.turnDone(p.Turn)
		case "thread/name/updated":
			if p.ThreadName != nil {
				s.sink.SetTitle(*p.ThreadName)
			}
		case "thread/tokenUsage/updated":
			s.sink.SetTokens(p.TokenUsage.Last.TotalTokens, p.TokenUsage.ModelContextWindow)
		case "serverRequest/resolved":
			s.broker.Dismiss(s.key(p.RequestID), "allow")
		}
	})
}

func (s *Session) started(it item) {
	switch it.Type {
	case "userMessage":
		var parts []string
		for _, c := range it.Content {
			parts = append(parts, c.Text)
		}
		s.sink.Apply(timeline.Event{Kind: "user", Text: strings.Join(parts, "\n")})
	case "commandExecution", "fileChange", "mcpToolCall", "webSearch":
		name, detail := tool(it)
		s.details[it.ID] = detail
		s.sink.Apply(timeline.Event{Kind: "tool_start", ToolUseID: it.ID, Name: name, Detail: &detail})
	}
}

func (s *Session) completed(it item) {
	switch it.Type {
	case "agentMessage":
		if !s.streamed[it.ID] {
			s.sink.Apply(timeline.Event{Kind: "assistant_text", Text: it.Text})
		}
		delete(s.streamed, it.ID)
	case "reasoning":
		if text := strings.Join(it.Summary, "\n"); text != "" {
			s.sink.Apply(timeline.Event{Kind: "thinking", Text: text})
		}
	case "plan":
		s.sink.Apply(timeline.Event{Kind: "tool_start", ToolUseID: it.ID, Name: "ExitPlanMode", Input: mustJSON(map[string]string{"plan": it.Text})})
	case "contextCompaction":
		s.sink.Apply(timeline.Event{Kind: "compacted", Trigger: "manual"})
	default:
		if _, open := s.details[it.ID]; !open {
			return
		}
		ok := it.Status == "completed" && (it.ExitCode == nil || *it.ExitCode == 0)
		e := timeline.Event{Kind: "tool_end", ToolUseID: it.ID, OK: ok, Output: it.AggregatedOutput}
		if it.DurationMs != nil {
			e.DurationMs = *it.DurationMs
		}
		s.sink.Apply(e)
		delete(s.details, it.ID)
	}
}

func (s *Session) turnDone(t turn) {
	e := timeline.Event{Kind: "result", OK: t.Status == "completed", DurationMs: t.DurationMs}
	if t.Error != nil {
		e.Error = t.Error.Message
	} else if t.Status == "interrupted" {
		e.Error = "interrupted"
	}
	s.sink.Apply(e)
}

func tool(it item) (string, proto.ToolDetail) {
	switch it.Type {
	case "commandExecution":
		return "shell", proto.ToolDetail{Kind: "shell", Command: it.Command}
	case "fileChange":
		d := proto.ToolDetail{Kind: "edit"}
		if len(it.Changes) > 0 {
			d.Path = it.Changes[0].Path
			d.Diff = timeline.ParseUnified(it.Changes[0].Diff)
		}
		return "apply_patch", d
	case "mcpToolCall":
		name := it.Server + "." + it.Tool
		return name, proto.ToolDetail{Kind: "other", Name: name, Input: it.Arguments}
	default:
		return "web_search", proto.ToolDetail{Kind: "search", Query: it.Query}
	}
}

func (s *Session) key(requestID json.RawMessage) string {
	return s.agentID + "\x00" + string(requestID)
}

// Request answers approvals. It asks on its own goroutine because the broker
// blocks until someone decides.
func (s *Session) Request(id json.RawMessage, method string, params json.RawMessage) {
	var p struct {
		ItemID  string `json:"itemId"`
		Command string `json:"command"`
	}
	json.Unmarshal(params, &p)
	s.run(func() {
		var name string
		var detail proto.ToolDetail
		switch method {
		case "item/commandExecution/requestApproval":
			name, detail = "shell", proto.ToolDetail{Kind: "shell", Command: p.Command}
		case "item/fileChange/requestApproval":
			name, detail = "apply_patch", s.details[p.ItemID]
		default:
			return
		}
		go s.ask(id, name, detail)
	})
}

func (s *Session) ask(id json.RawMessage, name string, detail proto.ToolDetail) {
	switch s.broker.Ask(s.ctx, proto.PermissionRequest{AgentID: s.agentID, ToolName: name, Detail: detail}, s.key(id)).Decision {
	case "allow":
		s.c.Reply(id, map[string]string{"decision": "accept"})
	case "deny":
		s.c.Reply(id, map[string]string{"decision": "decline"})
	}
}

func (s *Session) Done() <-chan struct{} { return s.c.Done() }

func (s *Session) Close() {
	s.cancel()
	s.c.Close()
}

func mustJSON(v any) json.RawMessage {
	raw, _ := json.Marshal(v)
	return raw
}
