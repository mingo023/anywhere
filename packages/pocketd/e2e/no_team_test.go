//go:build unix

package e2e

import (
	"context"
	"encoding/json"
	"net"
	"net/http"
	"slices"
	"testing"

	"github.com/coder/websocket"
)

func TestPocketdNeitherOffersNorAnswersTeams(t *testing.T) {
	h := Start(t)
	unix := &http.Client{Transport: &http.Transport{DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
		return (&net.Dialer{}).DialContext(ctx, "unix", h.Sock)
	}}}
	ws, _, err := websocket.Dial(context.Background(), "ws://localhost/", &websocket.DialOptions{HTTPClient: unix})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { ws.CloseNow() })
	p := &Phone{t: t, ws: ws}

	p.Send(map[string]any{"type": "hello", "id": "h", "clientId": "e2e-desktop", "protocolVersion": 3, "caps": []string{"team.v1"}})
	ok := p.WaitFor("hello.ok", func(m Message) bool { return m.Type == "hello.ok" })
	var hello struct{ Caps []string }
	json.Unmarshal([]byte(ok.Raw), &hello)
	if slices.Contains(hello.Caps, "team.v1") {
		t.Fatalf("hello.ok offers team.v1: %s", ok.Raw)
	}

	p.Send(map[string]any{"type": "team.list", "id": "tl"})
	got := p.WaitFor("team.list's answer", func(m Message) bool { return m.Type == "error" || m.Type == "team.list" })
	if got.Type != "error" {
		t.Fatalf("team.list answered: %s", got.Raw)
	}
}
