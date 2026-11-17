package mcp

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"slices"
	"strings"
	"sync"
	"time"
)

// Versions are the MCP protocol revisions the server speaks, newest first.
var Versions = []string{"2025-06-18", "2025-03-26", "2024-11-05"}

// Server is the MCP endpoint: POST /mcp takes JSON-RPC and answers with
// JSON; GET /mcp is the stream that tells a client its tool list changed.
// /mcp?direct=1 lists only the two blocking tools, for Codex's second server.
type Server struct {
	Tokens  *Tokens
	Tools   *Toolkit
	Version string
	// Hooks takes /hooks/<id> on the same listener; nil answers 404.
	Hooks http.Handler

	mu      sync.Mutex
	streams map[string][]chan struct{} // by terminal
}

func (s *Server) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/mcp", s.serveMCP)
	mux.HandleFunc("/hooks/", func(w http.ResponseWriter, r *http.Request) {
		if s.Hooks == nil {
			http.NotFound(w, r)
			return
		}
		s.Hooks.ServeHTTP(w, r)
	})
	return mux
}

func (s *Server) Serve(ctx context.Context, l net.Listener) error {
	srv := &http.Server{Handler: s.Handler(), ReadHeaderTimeout: 10 * time.Second}
	context.AfterFunc(ctx, func() { srv.Close() })
	err := srv.Serve(l)
	if errors.Is(err, http.ErrServerClosed) {
		return nil
	}
	return err
}

type rpcRequest struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params,omitempty"`
}

type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

type rpcResponse struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Result  any             `json:"result,omitempty"`
	Error   *rpcError       `json:"error,omitempty"`
}

func (s *Server) grant(r *http.Request) (Grant, bool) {
	auth := r.Header.Get("Authorization")
	token, ok := strings.CutPrefix(auth, "Bearer ")
	if !ok {
		return Grant{}, false
	}
	g, err := s.Tokens.Resolve(strings.TrimSpace(token))
	return g, err == nil
}

func (s *Server) serveMCP(w http.ResponseWriter, r *http.Request) {
	g, ok := s.grant(r)
	if !ok {
		w.Header().Set("WWW-Authenticate", `Bearer realm="anywhere"`)
		http.Error(w, "unauthorized", http.StatusUnauthorized)
		return
	}
	if v := r.Header.Get("MCP-Protocol-Version"); v != "" && !slices.Contains(Versions, v) {
		http.Error(w, "unsupported MCP-Protocol-Version "+v, http.StatusBadRequest)
		return
	}
	direct := r.URL.Query().Get("direct") == "1"
	switch r.Method {
	case http.MethodGet:
		s.stream(w, r, g)
	case http.MethodPost:
		s.post(w, r, g, direct)
	case http.MethodDelete:
		w.WriteHeader(http.StatusOK)
	default:
		w.Header().Set("Allow", "GET, POST, DELETE")
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}

func (s *Server) post(w http.ResponseWriter, r *http.Request, g Grant, direct bool) {
	body, err := io.ReadAll(io.LimitReader(r.Body, 4<<20))
	if err != nil {
		http.Error(w, "bad body", http.StatusBadRequest)
		return
	}
	trimmed := strings.TrimSpace(string(body))
	var reqs []rpcRequest
	batch := strings.HasPrefix(trimmed, "[")
	if batch {
		err = json.Unmarshal(body, &reqs)
	} else {
		var one rpcRequest
		err = json.Unmarshal(body, &one)
		reqs = []rpcRequest{one}
	}
	if err != nil {
		writeJSON(w, rpcResponse{JSONRPC: "2.0", ID: json.RawMessage("null"), Error: &rpcError{-32700, "parse error"}})
		return
	}
	var out []rpcResponse
	for _, req := range reqs {
		if len(req.ID) == 0 {
			continue // a notification
		}
		out = append(out, s.call(r.Context(), g, direct, req))
	}
	if len(out) == 0 {
		w.WriteHeader(http.StatusAccepted)
		return
	}
	if batch {
		writeJSON(w, out)
		return
	}
	writeJSON(w, out[0])
}

func writeJSON(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(v)
}

func (s *Server) call(ctx context.Context, g Grant, direct bool, req rpcRequest) rpcResponse {
	res := rpcResponse{JSONRPC: "2.0", ID: req.ID}
	switch req.Method {
	case "initialize":
		var p struct {
			ProtocolVersion string `json:"protocolVersion"`
		}
		json.Unmarshal(req.Params, &p)
		version := Versions[0]
		if slices.Contains(Versions, p.ProtocolVersion) {
			version = p.ProtocolVersion
		}
		res.Result = map[string]any{
			"protocolVersion": version,
			"capabilities":    map[string]any{"tools": map[string]any{"listChanged": true}},
			"serverInfo":      map[string]any{"name": "anywhere", "version": s.Version},
			"instructions":    "Anywhere's orchestrator: delegate work to other agent sessions, then read their results. Call orchestrator_capabilities first; tool_search loads the other tools.",
		}
	case "ping":
		res.Result = map[string]any{}
	case "tools/list":
		res.Result = map[string]any{"tools": s.Tools.List(g.Terminal, direct)}
	case "tools/call":
		var p struct {
			Name      string          `json:"name"`
			Arguments json.RawMessage `json:"arguments"`
		}
		if err := json.Unmarshal(req.Params, &p); err != nil {
			res.Error = &rpcError{-32602, "bad params"}
			return res
		}
		out, changed := s.Tools.Call(ctx, g, p.Name, p.Arguments)
		res.Result = out
		if changed {
			s.notify(g.Terminal)
		}
	default:
		res.Error = &rpcError{-32601, fmt.Sprintf("method not found: %s", req.Method)}
	}
	return res
}

// stream holds a GET open and sends tools/list_changed when a tool_search
// loads tools for the session.
func (s *Server) stream(w http.ResponseWriter, r *http.Request, g Grant) {
	flusher, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "no streaming", http.StatusMethodNotAllowed)
		return
	}
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.WriteHeader(http.StatusOK)
	flusher.Flush()
	ch := make(chan struct{}, 1)
	s.mu.Lock()
	if s.streams == nil {
		s.streams = map[string][]chan struct{}{}
	}
	s.streams[g.Terminal] = append(s.streams[g.Terminal], ch)
	s.mu.Unlock()
	defer func() {
		s.mu.Lock()
		s.streams[g.Terminal] = slices.DeleteFunc(s.streams[g.Terminal], func(c chan struct{}) bool { return c == ch })
		s.mu.Unlock()
	}()
	keep := time.NewTicker(25 * time.Second)
	defer keep.Stop()
	for {
		select {
		case <-r.Context().Done():
			return
		case <-keep.C:
			if _, err := io.WriteString(w, ": keepalive\n\n"); err != nil {
				return
			}
			flusher.Flush()
		case <-ch:
			msg := `{"jsonrpc":"2.0","method":"notifications/tools/list_changed"}`
			if _, err := fmt.Fprintf(w, "event: message\ndata: %s\n\n", msg); err != nil {
				return
			}
			flusher.Flush()
		}
	}
}

func (s *Server) notify(terminal string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	for _, ch := range s.streams[terminal] {
		select {
		case ch <- struct{}{}:
		default:
		}
	}
}
