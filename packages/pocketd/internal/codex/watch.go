package codex

import (
	"context"
	"encoding/json"
	"time"
)

// Watcher hears the thread broadcasts an app-server sends every client.
type Watcher interface {
	Connected()
	ThreadStarted(id string, root bool)
	ThreadStatus(id, typ string, flags []string)
	ThreadClosed(id string)
}

var WatchRetry = time.Second

// Watch keeps a connection to the app-server at sock, redialing WatchRetry
// after each failure until ctx ends.
func Watch(ctx context.Context, sock string, w Watcher) {
	for {
		h := watchHandler{w: w, up: make(chan struct{})}
		c, err := Dial(ctx, sock, h)
		if err == nil {
			w.Connected()
		}
		close(h.up)
		if err == nil {
			select {
			case <-c.Done():
			case <-ctx.Done():
			}
			c.Close()
		}
		select {
		case <-ctx.Done():
			return
		case <-time.After(WatchRetry):
		}
	}
}

// watchHandler holds broadcasts back until w has heard Connected.
type watchHandler struct {
	w  Watcher
	up chan struct{}
}

func (h watchHandler) Notify(method string, params json.RawMessage) {
	<-h.up
	var p struct {
		Thread struct {
			ID           string  `json:"id"`
			Parent       *string `json:"parentThreadId"`
			ThreadSource string  `json:"threadSource"`
			Ephemeral    bool    `json:"ephemeral"`
		} `json:"thread"`
		ThreadID string `json:"threadId"`
		Status   struct {
			Type  string   `json:"type"`
			Flags []string `json:"activeFlags"`
		} `json:"status"`
	}
	if json.Unmarshal(params, &p) != nil {
		return
	}
	id := p.ThreadID
	if method == "thread/started" {
		id = p.Thread.ID
	}
	if id == "" {
		return
	}
	switch th := p.Thread; method {
	case "thread/started":
		h.w.ThreadStarted(id, th.Parent == nil && th.ThreadSource == "user" && !th.Ephemeral)
	case "thread/status/changed":
		h.w.ThreadStatus(id, p.Status.Type, p.Status.Flags)
	case "thread/closed":
		h.w.ThreadClosed(id)
	}
}

func (watchHandler) Request(json.RawMessage, string, json.RawMessage) {}
