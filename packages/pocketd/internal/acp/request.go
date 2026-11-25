package acp

import (
	"cmp"
	"context"
	"encoding/json"
	"fmt"
	"maps"
	"slices"
	"strconv"
	"strings"
	"time"

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
	case "elicitation/create":
		if s.f.elicits {
			return func() { s.elicit(id, params) }
		}
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
	if s.f.elicits && decision(asked, p.Options) {
		s.decide(id, asked.ID, asked.Title, p.Options)
		return
	}
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
	if s.f.heldPlan && s.launch.Plan {
		// fx's and Hermes's plan modes ask before every read.
		if kind == "read" || kind == "search" {
			s.selected(id, pick(p.Options, allowOnce...))
		} else {
			s.selected(id, pick(p.Options, reject...))
		}
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

// decision is whether asked is a question the agent puts to the user, each
// choice an option, rather than a tool's approval, which can be refused. A
// guard: Antigravity 1.5.16 asks through elicitation/create, and no
// interaction_ request has been seen from it.
func decision(asked toolCall, options []option) bool {
	return strings.HasPrefix(asked.ID, "interaction_") && strings.TrimSpace(asked.Title) != "" && len(options) > 0 &&
		!slices.ContainsFunc(options, func(o option) bool { return strings.HasPrefix(o.Kind, "reject") })
}

// decisionQuestion asks title with a choice per option.
func decisionQuestion(title string, options []option) question {
	q := question{ID: title, Prompt: title}
	for _, o := range options {
		q.Options = append(q.Options, questionOption{ID: o.ID, Label: cmp.Or(o.Name, o.ID)})
	}
	q.Options = distinct(q.Options)
	return q
}

// distinct tells apart options that share a label by their ids: answers come back as labels.
func distinct(options []questionOption) []questionOption {
	out := slices.Clone(options)
	for i, o := range options {
		if slices.ContainsFunc(options, func(other questionOption) bool { return other.ID != o.ID && other.Label == o.Label }) {
			out[i].Label += " (" + o.ID + ")"
		}
	}
	return out
}

// decide asks the user the agent's decision and answers with the choice they
// pick. A permission request has no room for typed text, so that cancels.
func (s *Session) decide(id json.RawMessage, call, title string, options []option) {
	q := decisionQuestion(title, options)
	s.ask(id, s.asking(id, call, []question{q}), func(a broker.Answer) {
		s.keep(id, a)
		chosen := ""
		if picked := q.picked(a.Answers[q.Prompt]); a.Decision == "allow" && len(picked) == 1 {
			chosen = picked[0].ID
		}
		s.selected(id, chosen)
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
	// Other is the form field that takes a typed answer, "" for none.
	Other string
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
	s.ask(id, s.asking(id, toolCallID(params), qs), func(a broker.Answer) {
		s.keep(id, a)
		if a.Decision != "allow" {
			a.Answers = nil
		}
		s.c.Reply(id, reply(qs, a.Answers))
	})
}

func toolCallID(params json.RawMessage) string {
	var p struct {
		ToolCallID string `json:"toolCallId"`
	}
	json.Unmarshal(params, &p)
	return p.ToolCallID
}

type unanswered struct {
	call string
	card proto.PermissionRequest
	qs   []question
}

// asking is the card that asks qs. call is the agent's id for the question;
// Grok's has none, so one is made unique.
func (s *Session) asking(id json.RawMessage, call string, qs []question) proto.PermissionRequest {
	card := s.questionCard(qs)
	if call == "" {
		call = strings.Trim(string(id), `"`) + "-" + strconv.FormatInt(time.Now().UnixNano(), 36)
	}
	s.waiting[string(id)] = unanswered{"question-" + call, card, qs}
	return card
}

// keep records the question asked by id and how the user answered it, ahead
// of the agent's next words, as Claude's AskUserQuestion call, which the
// chat shows with its answers.
func (s *Session) keep(id json.RawMessage, a broker.Answer) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.keepLocked(string(id), a)
}

func (s *Session) keepLocked(key string, a broker.Answer) {
	u, ok := s.waiting[key]
	delete(s.waiting, key)
	// A closing session leaves the question to the pocketd after it, which the agent asks again.
	if !ok || s.ctx.Err() != nil {
		return
	}
	var said []string
	for _, q := range u.qs {
		if answer := a.Answers[q.Prompt]; a.Decision == "allow" && answer != "" {
			said = append(said, fmt.Sprintf(`"%s"="%s"`, q.Prompt, answer))
		}
	}
	out := "The user doesn't want to proceed with this tool use."
	if len(said) > 0 {
		out = "User has answered your questions: " + strings.Join(said, ", ") + ". You can now continue with the user's answers in mind."
	}
	s.flush()
	s.record(timeline.Event{Kind: "tool_start", ToolUseID: u.call, Name: "AskUserQuestion", Input: u.card.Detail.Input})
	s.record(timeline.Event{Kind: "tool_end", ToolUseID: u.call, OK: len(said) > 0, Output: &out})
}

// questionCard is the request that shows qs as Claude's AskUserQuestion.
func (s *Session) questionCard(qs []question) proto.PermissionRequest {
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
	return proto.PermissionRequest{AgentID: s.agentID, ToolName: "AskUserQuestion", Detail: timeline.Detail("AskUserQuestion", input)}
}

// elicit asks the user an elicitation/create form. One the card can't show
// is refused, and Antigravity then asks in the chat instead ([B]).
func (s *Session) elicit(id json.RawMessage, params json.RawMessage) {
	qs, ok := formQuestions(params)
	if !ok {
		s.c.Refuse(id, invalidParams, "Anywhere can't show this form")
		return
	}
	s.ask(id, s.asking(id, toolCallID(params), qs), func(a broker.Answer) {
		s.keep(id, a)
		s.c.Reply(id, formAnswer(qs, a))
	})
}

// property is a form field of the shapes a question card can show.
type property struct {
	Type        string    `json:"type"`
	Title       string    `json:"title"`
	Description string    `json:"description"`
	Enum        []any     `json:"enum"`
	OneOf       []choice  `json:"oneOf"`
	AnyOf       []choice  `json:"anyOf"`
	Items       *property `json:"items"`
}

type choice struct {
	Const any    `json:"const"`
	Title string `json:"title"`
}

// formQuestions reads a form's fields as questions, false for a form the
// card can't show. A field "K_other" holds the typed answer to field K.
func formQuestions(params json.RawMessage) ([]question, bool) {
	var p struct {
		Mode   string `json:"mode"`
		Schema struct {
			Properties map[string]property `json:"properties"`
		} `json:"requestedSchema"`
	}
	if json.Unmarshal(params, &p) != nil || (p.Mode != "" && p.Mode != "form") || len(p.Schema.Properties) == 0 {
		return nil, false
	}
	props := p.Schema.Properties
	// The map lost the form's order. Antigravity's keys are qN, and this keeps q2 before q10.
	keys := slices.SortedFunc(maps.Keys(props), func(a, b string) int { return cmp.Or(cmp.Compare(len(a), len(b)), strings.Compare(a, b)) })
	var qs []question
	for _, key := range keys {
		if base, ok := strings.CutSuffix(key, "_other"); ok {
			if _, ok := props[base]; ok {
				continue
			}
		}
		q, ok := props[key].question(key)
		if !ok {
			return nil, false
		}
		if _, ok := props[key+"_other"]; ok {
			q.Other = key + "_other"
		}
		qs = append(qs, q)
	}
	return qs, true
}

func (p property) question(key string) (question, bool) {
	q := question{ID: key, Prompt: cmp.Or(p.Title, p.Description, key)}
	var ok bool
	switch {
	case p.Type == "string":
		q.Options, ok = p.choices()
	case p.Type == "array" && p.Items != nil:
		q.Multi = true
		q.Options, ok = p.Items.choices()
		ok = ok && len(q.Options) > 0
	}
	q.Options = distinct(q.Options)
	return q, ok
}

// choices is the options p offers, false if one isn't a string.
func (p property) choices() ([]questionOption, bool) {
	var out []questionOption
	for _, v := range p.Enum {
		s, ok := v.(string)
		if !ok {
			return nil, false
		}
		out = append(out, questionOption{ID: s, Label: s})
	}
	for _, c := range append(p.OneOf, p.AnyOf...) {
		s, ok := c.Const.(string)
		if !ok {
			return nil, false
		}
		out = append(out, questionOption{ID: s, Label: cmp.Or(c.Title, s)})
	}
	return out, true
}

// formAnswer answers a form with the consts the user picked, or what they
// typed where the form takes text. Skip declines; a stop or dismiss cancels.
func formAnswer(qs []question, a broker.Answer) map[string]any {
	switch a.Decision {
	case "allow":
	case "":
		return map[string]any{"action": "cancel"}
	default:
		return map[string]any{"action": "decline"}
	}
	content := map[string]any{}
	for _, q := range qs {
		answer := a.Answers[q.Prompt]
		picked := q.picked(answer)
		switch {
		case answer == "":
		case !q.Multi && len(picked) == 1:
			content[q.ID] = picked[0].ID
		case q.Multi && len(picked) > 0 && len(picked) == len(strings.Split(answer, ", ")):
			ids := []string{}
			for _, o := range picked {
				ids = append(ids, o.ID)
			}
			content[q.ID] = ids
		case q.Other != "":
			content[q.Other] = answer
		case len(q.Options) == 0:
			content[q.ID] = answer
		}
	}
	if len(content) == 0 {
		return map[string]any{"action": "decline"}
	}
	return map[string]any{"action": "accept", "content": content}
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
