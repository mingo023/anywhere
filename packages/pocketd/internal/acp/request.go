package acp

import (
	"cmp"
	"context"
	"encoding/json"
	"slices"
	"strings"

	"pocketd/internal/broker"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

// Request answers what the agent asks of its client. What the session knows
// waits until it is open: a pipe hands a new pocketd the requests the one
// before left open.
func (s *Session) Request(id json.RawMessage, name string, params json.RawMessage) {
	s.mu.Lock()
	defer s.mu.Unlock()
	answer := s.answerer(id, method(name), params)
	switch {
	case answer == nil && s.f.lenient:
		s.c.Reply(id, struct{}{})
	case answer == nil:
		s.c.Refuse(id, methodNotFound, "Method not found: "+name)
	case !s.ready:
		s.held = append(s.held, answer)
	default:
		answer()
	}
}

// answerer is what answers the agent's request name, or nil for one the session doesn't know.
func (s *Session) answerer(id json.RawMessage, name string, params json.RawMessage) func() {
	switch name {
	case "session/request_permission":
		return func() { s.permission(id, params) }
	case "cursor/ask_question":
		return func() { s.question(id, params, cursorAnswers) }
	case "x.ai/ask_user_question":
		return func() { s.question(id, params, grokAnswers) }
	}
	if tell := tells[name]; tell != nil {
		return func() { s.c.Reply(id, tell(s, params)) }
	}
	return nil
}

// permission answers a tool's permission request by the session's access, or asks the user.
func (s *Session) permission(id json.RawMessage, params json.RawMessage) {
	var p struct {
		Options []option `json:"options"`
	}
	json.Unmarshal(params, &p)
	asked := call(params)
	kind := strings.ToLower(asked.Kind)
	if t := s.tools[asked.ID]; t != nil {
		// OpenCode asks about a call it told of, and may name what it asks for
		// the question: a write outside the project comes as its folder.
		title, was := t.Title, t.Kind
		t.merge(asked)
		t.Title, t.Kind = cmp.Or(title, t.Title), cmp.Or(was, t.Kind)
		if !t.started {
			s.show(t)
		}
		asked, kind = t.toolCall, cmp.Or(kind, strings.ToLower(t.Kind))
	} else if t := s.awaited(asked); t != nil {
		t.Content = asked.Content
		s.show(t)
	}
	kind = cmp.Or(kind, "other")
	if s.f.heldPlan && s.launch.Plan && kind != "read" && kind != "search" {
		s.selected(id, pick(p.Options, reject...))
		return
	}
	if option := auto(s.launch.Access, kind, p.Options); option != "" {
		s.selected(id, option)
		return
	}
	name, detail := asked.describe()
	req := proto.PermissionRequest{AgentID: s.agentID, ToolName: name, Detail: detail}
	for _, o := range p.Options {
		if o.Kind == "allow_always" {
			req.Options = append(req.Options, proto.PermissionOption{ID: o.ID, Label: o.Name})
		}
	}
	s.ask(id, req, func(a broker.Answer) {
		switch {
		case a.Decision == "allow" && slices.ContainsFunc(req.Options, func(o proto.PermissionOption) bool { return o.ID == a.Option }):
			s.selected(id, a.Option)
		case a.Decision == "allow":
			s.selected(id, pick(p.Options, allowOnce...))
		case a.Decision == "deny":
			s.selected(id, pick(p.Options, reject...))
		default:
			s.selected(id, "")
		}
	})
}

// awaited is the edit that waits to show for the diff asked brings: Hermes
// asks about an edit as a call of its own making, which alone has the diff.
func (s *Session) awaited(asked toolCall) *tool {
	path, _, _, ok := asked.edit()
	if !ok {
		return nil
	}
	for _, t := range s.tools {
		if !t.started && t.Kind == "edit" && strings.HasSuffix(path, t.path()) {
			return t
		}
	}
	return nil
}

// selected answers a permission request with option; "" is cancelled.
func (s *Session) selected(id json.RawMessage, option string) {
	outcome := map[string]string{"outcome": "cancelled"}
	if option != "" {
		outcome = map[string]string{"outcome": "selected", "optionId": option}
	}
	s.c.Reply(id, map[string]any{"outcome": outcome})
}

// ask shows req to the user, off the reader, and has reply answer the agent
// once they decide. The agent needs them meanwhile. A stop answers "".
func (s *Session) ask(id json.RawMessage, req proto.PermissionRequest, reply func(broker.Answer)) {
	ctx, cancel := context.WithCancel(s.ctx)
	key := string(id)
	s.asks[key] = cancel
	go func() {
		defer cancel()
		if s.broker.Auto(req) {
			s.mu.Lock()
			delete(s.asks, key)
			s.mu.Unlock()
			reply(broker.Answer{Decision: "allow"})
			return
		}
		s.sink.NeedsYou()
		a := s.broker.Ask(ctx, req, s.agentID+"\x00"+key)
		s.mu.Lock()
		delete(s.asks, key)
		if len(s.asks) == 0 && s.running && !s.stopping {
			s.sink.Working()
		}
		s.mu.Unlock()
		reply(a)
	}()
}

type question struct {
	ID      string
	Prompt  string
	Header  string
	Multi   bool
	Options []questionOption
}

type questionOption struct{ ID, Label, Description string }

func str(m map[string]any, keys ...string) string {
	for _, k := range keys {
		if s, ok := m[k].(string); ok && strings.TrimSpace(s) != "" {
			return strings.TrimSpace(s)
		}
	}
	return ""
}

// questions reads what an agent asks the user, in whichever of the shapes Cursor and Grok send.
func questions(params json.RawMessage) []question {
	var p struct {
		Questions []map[string]any `json:"questions"`
		Input     struct {
			Questions []map[string]any `json:"questions"`
		} `json:"input"`
	}
	json.Unmarshal(params, &p)
	var out []question
	for _, raw := range append(p.Questions, p.Input.Questions...) {
		q := question{ID: str(raw, "id"), Prompt: str(raw, "question", "prompt", "text", "header"), Header: str(raw, "header", "title"),
			Multi: raw["multiSelect"] == true || raw["allowMultiple"] == true || raw["multiple"] == true}
		options, _ := raw["options"].([]any)
		for _, o := range options {
			switch o := o.(type) {
			case string:
				q.Options = append(q.Options, questionOption{ID: o, Label: o})
			case map[string]any:
				if label := str(o, "label", "value", "text", "id"); label != "" {
					q.Options = append(q.Options, questionOption{ID: cmp.Or(str(o, "id", "optionId", "value"), label), Label: label, Description: str(o, "description", "detail")})
				}
			}
		}
		if q.Prompt != "" || len(q.Options) > 0 {
			q.ID = cmp.Or(q.ID, q.Prompt)
			out = append(out, q)
		}
	}
	return out
}

// picked is the options of q the user's answer names: a label, or several a comma apart.
func (q question) picked(answer string) []questionOption {
	var out []questionOption
	for _, o := range q.Options {
		if answer == o.Label || q.Multi && slices.Contains(strings.Split(answer, ", "), o.Label) {
			out = append(out, o)
		}
	}
	return out
}

// question asks the user as Claude's AskUserQuestion does, which the chat
// has a card for, and answers the agent in its own terms; nil answers skip.
func (s *Session) question(id json.RawMessage, params json.RawMessage, reply func([]question, map[string]string) any) {
	qs := questions(params)
	type asked struct {
		Question    string           `json:"question"`
		Header      string           `json:"header,omitempty"`
		Options     []map[string]any `json:"options"`
		MultiSelect bool             `json:"multiSelect"`
	}
	in := make([]asked, len(qs))
	for i, q := range qs {
		in[i] = asked{Question: q.Prompt, Header: q.Header, Options: []map[string]any{}, MultiSelect: q.Multi}
		for _, o := range q.Options {
			in[i].Options = append(in[i].Options, map[string]any{"label": o.Label, "description": o.Description})
		}
	}
	input, _ := json.Marshal(map[string]any{"questions": in})
	req := proto.PermissionRequest{AgentID: s.agentID, ToolName: "AskUserQuestion", Detail: timeline.Detail("AskUserQuestion", input)}
	s.ask(id, req, func(a broker.Answer) {
		if a.Decision != "allow" {
			a.Answers = nil
		}
		s.c.Reply(id, reply(qs, a.Answers))
	})
}

func cursorAnswers(qs []question, answers map[string]string) any {
	if len(answers) == 0 {
		return map[string]any{"outcome": map[string]string{"outcome": "skipped", "reason": "User skipped"}}
	}
	out := []map[string]any{}
	for _, q := range qs {
		ids := []string{}
		for _, o := range q.picked(answers[q.Prompt]) {
			ids = append(ids, o.ID)
		}
		out = append(out, map[string]any{"questionId": q.ID, "selectedOptionIds": ids})
	}
	return map[string]any{"outcome": map[string]any{"outcome": "answered", "answers": out}}
}

func grokAnswers(qs []question, answers map[string]string) any {
	if len(answers) == 0 {
		return map[string]string{"outcome": "skip_interview"}
	}
	out := map[string]any{}
	for _, q := range qs {
		answer, ok := answers[q.Prompt]
		switch {
		case !ok || answer == "":
		case q.Multi:
			out[q.Prompt] = strings.Split(answer, ", ")
		default:
			out[q.Prompt] = answer
		}
	}
	return map[string]any{"outcome": "accepted", "answers": out}
}
