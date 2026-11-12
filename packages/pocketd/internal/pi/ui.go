package pi

import (
	"context"
	"encoding/json"

	"pocketd/internal/broker"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

// dialog is what an extension asks of pi's UI.
type dialog struct {
	ID      string   `json:"id"`
	Method  string   `json:"method"`
	Title   string   `json:"title"`
	Message string   `json:"message"`
	Options []string `json:"options"`
}

// ui answers an extension_ui_request. A dialog waits for the user; a
// notification is a notice; what only a terminal shows, as a status line or a
// widget, is dropped.
func (s *Session) ui(raw json.RawMessage) {
	var d dialog
	json.Unmarshal(raw, &d)
	title := plain(d.Title)
	switch d.Method {
	case "confirm":
		detail := proto.ToolDetail{Kind: "other", Name: title}
		if message := plain(d.Message); message != "" {
			detail.Input, _ = json.Marshal(map[string]string{"message": message})
		}
		s.ask(d.ID, proto.PermissionRequest{AgentID: s.agentID, ToolName: title, Detail: detail}, func(a broker.Answer) map[string]any {
			if a.Decision == "" {
				return map[string]any{"cancelled": true}
			}
			return map[string]any{"confirmed": a.Decision == "allow"}
		})
	// Asked as Claude's AskUserQuestion, which the chat has a card for: a
	// choice of the options, or with none whatever the user writes.
	case "select", "input", "editor":
		options := []map[string]string{}
		values := map[string]string{}
		for _, o := range d.Options {
			options = append(options, map[string]string{"label": plain(o), "description": ""})
			values[plain(o)] = o
		}
		input, _ := json.Marshal(map[string]any{"questions": []map[string]any{{"question": title, "options": options, "multiSelect": false}}})
		req := proto.PermissionRequest{AgentID: s.agentID, ToolName: "AskUserQuestion", Detail: timeline.Detail("AskUserQuestion", input)}
		s.ask(d.ID, req, func(a broker.Answer) map[string]any {
			answer := a.Answers[title]
			if a.Decision != "allow" || answer == "" {
				return map[string]any{"cancelled": true}
			}
			// pi matches a choice by the option as it wrote it.
			if value, ok := values[answer]; ok {
				answer = value
			}
			return map[string]any{"value": answer}
		})
	case "notify":
		s.notice(plain(d.Message))
	// omp takes back a dialog it asked.
	case "cancel":
		if cancel := s.asks[d.ID]; cancel != nil {
			cancel()
		}
	}
}

// ask shows req to the user, off the reader, and answers the dialog named id
// with what reply makes of their answer. The agent needs them meanwhile. A
// stop answers "".
func (s *Session) ask(id string, req proto.PermissionRequest, reply func(broker.Answer) map[string]any) {
	ctx, cancel := context.WithCancel(s.ctx)
	s.asks[id] = cancel
	c := s.c
	go func() {
		defer cancel()
		s.sink.NeedsYou()
		a := s.broker.Ask(ctx, req, s.agentID+"\x00"+id)
		s.mu.Lock()
		_, mine := s.asks[id]
		delete(s.asks, id)
		switch {
		// A stop took it, and says so itself.
		case !mine || len(s.asks) > 0 || s.stopping:
		case s.running:
			s.sink.Working()
		// An extension asked between turns.
		default:
			s.sink.Stopped()
		}
		s.mu.Unlock()
		res := reply(a)
		res["type"], res["id"] = "extension_ui_response", id
		c.Write(res)
	}()
}
