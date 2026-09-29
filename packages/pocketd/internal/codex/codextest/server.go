// Package codextest is a scripted Codex app-server on a unix socket.
package codextest

import (
	"context"
	"encoding/json"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"

	"github.com/coder/websocket"
)

type Frame struct {
	ID     json.RawMessage `json:"id,omitempty"`
	Method string          `json:"method,omitempty"`
	Params json.RawMessage `json:"params,omitempty"`
	Result json.RawMessage `json:"result,omitempty"`
}

// Answer returns a call's result, or an error message.
type Answer func(method string, params json.RawMessage) (any, string)

type Server struct {
	t      *testing.T
	Sock   string
	answer Answer
	calls  chan Frame
	ready  chan struct{}
	srv    *http.Server
	mu     sync.Mutex
	ws     *websocket.Conn
	conns  map[*websocket.Conn]bool // true once initialized
}

// Start listens on sock. Push goes to the newest connection: a session
// connects after the watcher.
func Start(t *testing.T, sock string, answer Answer) *Server {
	t.Helper()
	os.MkdirAll(filepath.Dir(sock), 0o700)
	ln, err := net.Listen("unix", sock)
	if err != nil {
		t.Fatal(err)
	}
	s := &Server{t: t, Sock: sock, answer: answer, calls: make(chan Frame, 1000), ready: make(chan struct{}), conns: map[*websocket.Conn]bool{}}
	s.srv = &http.Server{Handler: http.HandlerFunc(s.serve)}
	go s.srv.Serve(ln)
	t.Cleanup(s.Close)
	return s
}

// Close stops the server and drops its connections, which http.Server.Close
// leaves open once hijacked.
func (s *Server) Close() {
	s.srv.Close()
	s.mu.Lock()
	defer s.mu.Unlock()
	for ws := range s.conns {
		ws.CloseNow()
	}
}

// Conns counts the clients still connected.
func (s *Server) Conns() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return len(s.conns)
}

func OK(string, json.RawMessage) (any, string) { return map[string]any{}, "" }

func (s *Server) serve(w http.ResponseWriter, r *http.Request) {
	ws, err := websocket.Accept(w, r, &websocket.AcceptOptions{CompressionMode: websocket.CompressionDisabled})
	if err != nil {
		return
	}
	s.mu.Lock()
	s.conns[ws] = false
	s.mu.Unlock()
	for {
		_, raw, err := ws.Read(context.Background())
		if err != nil {
			s.mu.Lock()
			delete(s.conns, ws)
			s.mu.Unlock()
			return
		}
		var f Frame
		json.Unmarshal(raw, &f)
		s.calls <- f
		if f.Method == "" || f.ID == nil {
			continue
		}
		result, errMsg := s.answer(f.Method, f.Params)
		msg := map[string]any{"id": f.ID, "result": result}
		if errMsg != "" {
			msg = map[string]any{"id": f.ID, "error": map[string]any{"code": -1, "message": errMsg}}
		}
		s.write(ws, msg)
		if f.Method == "initialize" {
			s.mu.Lock()
			s.ws = ws
			s.conns[ws] = true
			s.mu.Unlock()
			select {
			case <-s.ready:
			default:
				close(s.ready)
			}
		}
	}
}

func (s *Server) write(ws *websocket.Conn, v any) {
	raw, _ := json.Marshal(v)
	s.mu.Lock()
	defer s.mu.Unlock()
	ws.Write(context.Background(), websocket.MessageText, raw)
}

// Push sends a notification, or a server request when id is not nil.
func (s *Server) Push(method string, id any, params string) {
	<-s.ready
	msg := map[string]any{"method": method, "params": json.RawMessage(params)}
	if id != nil {
		msg["id"] = id
	}
	s.mu.Lock()
	ws := s.ws
	s.mu.Unlock()
	s.write(ws, msg)
}

func (s *Server) Broadcast(method, params string) {
	<-s.ready
	raw, _ := json.Marshal(map[string]any{"method": method, "params": json.RawMessage(params)})
	s.mu.Lock()
	defer s.mu.Unlock()
	for ws, up := range s.conns {
		if up {
			ws.Write(context.Background(), websocket.MessageText, raw)
		}
	}
}

func (s *Server) wait(what string, ok func(Frame) bool) Frame {
	s.t.Helper()
	timeout := time.After(10 * time.Second)
	for {
		select {
		case f := <-s.calls:
			if ok(f) {
				return f
			}
		case <-timeout:
			s.t.Fatalf("app-server never got %s", what)
			return Frame{}
		}
	}
}

// Next returns the next call or notification the client sent with method.
func (s *Server) Next(method string) Frame {
	s.t.Helper()
	return s.wait(method, func(f Frame) bool { return f.Method == method })
}

// Reply returns the result the client sent for server request id.
func (s *Server) Reply(id string) string {
	s.t.Helper()
	return string(s.wait("reply "+id, func(f Frame) bool { return f.Method == "" && string(f.ID) == id }).Result)
}
