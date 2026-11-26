package hosts

import (
	"context"
	"encoding/json"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"sync"
	"testing"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/proto"
)

// fakePocketd serves one conn at a unix socket: it answers hello, sends an agent.list, then runs script.
// fakePocketd lists agents on each hello, then runs script for the conn.
func fakePocketd(t *testing.T, script func(read func() map[string]any, send func(any)), agents ...proto.AgentSummary) string {
	t.Helper()
	// t.TempDir is past a unix socket path's limit on macOS.
	dir, err := os.MkdirTemp(socketBase(), "hc")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(dir) })
	socket := filepath.Join(dir, "p.sock")
	ln, err := net.Listen("unix", socket)
	if err != nil {
		t.Fatal(err)
	}
	srv := &http.Server{Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ws, err := websocket.Accept(w, r, nil)
		if err != nil {
			return
		}
		defer ws.CloseNow()
		ctx := r.Context()
		var read func() map[string]any
		read = func() map[string]any {
			_, raw, err := ws.Read(ctx)
			var m map[string]any
			if err == nil {
				json.Unmarshal(raw, &m)
			}
			// What a remote calls once hello is through, answered here so scripts see only their own.
			if m["type"] == "agent.list" {
				raw, _ := json.Marshal(proto.NewAgentList(m["id"].(string), agents))
				ws.Write(ctx, websocket.MessageText, raw)
				return read()
			}
			return m
		}
		send := func(v any) {
			raw, _ := json.Marshal(v)
			ws.Write(ctx, websocket.MessageText, raw)
		}
		hello := read()
		send(map[string]any{"type": "hello.ok", "id": hello["id"], "caps": []string{"launch.v1"}})
		send(map[string]any{"type": "agent.list", "agents": append([]proto.AgentSummary{}, agents...)})
		script(read, send)
		ws.Read(ctx)
	})}
	go srv.Serve(ln)
	t.Cleanup(func() { srv.Close() })
	return socket
}

func TestAClientGivesEachCallItsOwnReplyAndEverythingElseToItsReader(t *testing.T) {
	socket := fakePocketd(t, func(read func() map[string]any, send func(any)) {
		ids := map[any]any{}
		for range 2 {
			m := read()
			ids[m["type"]] = m["id"]
		}
		create := ids["agent.create"]
		send(map[string]any{"type": "agent.progress", "id": create, "step": "setup"})
		send(map[string]any{"type": "ack", "id": ids["agent.prompt"]})
		send(map[string]any{"type": "agent.update", "agent": map[string]any{"id": "a1"}})
		send(map[string]any{"type": "agent.created", "id": create, "agentId": "a2"})
		bad := read()
		send(map[string]any{"type": "error", "id": bad["id"], "message": "That session isn't running"})
	})
	var mu sync.Mutex
	var heard []string
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	c, err := Dial(ctx, socket, "", func(kind string, raw []byte) {
		mu.Lock()
		heard = append(heard, kind)
		mu.Unlock()
	})
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	created, prompted := make(chan []byte, 1), make(chan []byte, 1)
	go func() {
		raw, _ := c.Call(ctx, "agent.created", map[string]any{"type": "agent.create"})
		created <- raw
	}()
	go func() {
		raw, _ := c.Call(ctx, "ack", map[string]any{"type": "agent.prompt"})
		prompted <- raw
	}()
	var got struct{ Type, AgentID string }
	json.Unmarshal(<-created, &got)
	if got.Type != "agent.created" || got.AgentID != "a2" {
		t.Fatalf("the create got %+v", got)
	}
	json.Unmarshal(<-prompted, &got)
	if got.Type != "ack" {
		t.Fatalf("the prompt got %+v", got)
	}
	if _, err := c.Call(ctx, "ack", map[string]any{"type": "agent.interrupt"}); err == nil || err.Error() != "That session isn't running" {
		t.Fatalf("an error reply came back as %v", err)
	}
	mu.Lock()
	defer mu.Unlock()
	if len(heard) != 2 || heard[0] != "agent.list" || heard[1] != "agent.update" {
		t.Fatalf("the reader heard %v", heard)
	}
}

// socketBase is a folder short enough for a unix socket path: macOS caps it at 104 bytes, past t.TempDir, and Windows
// has no /tmp.
func socketBase() string {
	if runtime.GOOS == "windows" {
		return ""
	}
	return "/tmp"
}
