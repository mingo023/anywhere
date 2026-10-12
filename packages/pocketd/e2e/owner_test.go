//go:build unix

package e2e

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"net"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/ops"
	"pocketd/internal/proto"
)

func ownerOverTCP(h *Harness) { h.ownerTCP = true }

type ownerToken struct {
	Token string `json:"token"`
	ID    string `json:"id"`
	Port  int    `json:"port"`
}

func (h *Harness) mintOwner() ownerToken {
	h.t.Helper()
	out, code := h.Pocketd("pair", "--json", "--name", "Work PC")
	var tok ownerToken
	if err := json.Unmarshal([]byte(out), &tok); code != 0 || err != nil || tok.Token == "" {
		h.t.Fatalf("pair --json exited %d: %s", code, out)
	}
	return tok
}

// tcpOps speaks ops' JSON lines on the owner port.
type tcpOps struct {
	t   *testing.T
	c   net.Conn
	dec *json.Decoder
}

func (h *Harness) dialOwner() *tcpOps {
	h.t.Helper()
	c, err := net.Dial("tcp", fmt.Sprintf("127.0.0.1:%d", h.OwnerPort))
	if err != nil {
		h.t.Fatal(err)
	}
	h.t.Cleanup(func() { c.Close() })
	return &tcpOps{h.t, c, json.NewDecoder(bufio.NewReader(c))}
}

func (o *tcpOps) call(m ops.Msg) (ops.Msg, error) {
	o.t.Helper()
	if err := json.NewEncoder(o.c).Encode(m); err != nil {
		return ops.Msg{}, err
	}
	o.c.SetReadDeadline(time.Now().Add(10 * time.Second))
	var r ops.Msg
	err := o.dec.Decode(&r)
	return r, err
}

func TestAnOwnerTokenOverTCPMakesThePeerTheOwner(t *testing.T) {
	h := Start(t, ownerOverTCP)
	tok := h.mintOwner()
	if tok.Port != h.OwnerPort {
		t.Fatalf("port %d, want %d", tok.Port, h.OwnerPort)
	}
	c := h.dialOwner()
	if m, err := c.call(ops.Msg{Op: "status"}); err != nil || m.ErrorCode != proto.CodeScopeDenied {
		t.Fatalf("before auth: %+v %v", m, err)
	}
	if m, err := c.call(ops.Msg{Op: "auth", Token: tok.Token}); err != nil || m.Ev != "ok" {
		t.Fatalf("auth: %+v %v", m, err)
	}
	if m, err := c.call(ops.Msg{Op: "status"}); err != nil || m.Ev != "status" || m.Status.PID == 0 {
		t.Fatalf("status: %+v %v", m, err)
	}
}

func TestAWrongOwnerTokenClosesTheConn(t *testing.T) {
	h := Start(t, ownerOverTCP)
	h.mintOwner()
	c := h.dialOwner()
	if m, err := c.call(ops.Msg{Op: "auth", Token: "not-a-token"}); err != nil || m.ErrorCode != proto.CodeNotPaired {
		t.Fatalf("auth: %+v %v", m, err)
	}
	if _, err := c.call(ops.Msg{Op: "status"}); err == nil {
		t.Fatal("conn still open")
	}
}

func (h *Harness) ownerHello(token string) (*Phone, Message) {
	h.t.Helper()
	ws, _, err := websocket.Dial(context.Background(), fmt.Sprintf("ws://127.0.0.1:%d", h.OwnerPort), nil)
	if err != nil {
		h.t.Fatal(err)
	}
	h.t.Cleanup(func() { ws.CloseNow() })
	p := &Phone{t: h.t, ws: ws}
	p.Send(map[string]any{"type": "hello", "id": "h", "ownerToken": token, "clientId": "e2e-desktop", "protocolVersion": 3, "caps": []string{proto.CapScopes}})
	return p, p.WaitFor("hello reply", func(m Message) bool { return m.Type == "hello.ok" || m.Type == "error" })
}

func TestAWebSocketHelloWithAnOwnerTokenIsTheOwner(t *testing.T) {
	h := Start(t, ownerOverTCP)
	tok := h.mintOwner()
	if _, m := h.ownerHello(tok.Token); m.Type != "hello.ok" || !strings.Contains(m.Raw, `"owner"`) {
		t.Fatalf("got %s", m.Raw)
	}
	if _, m := h.ownerHello(""); m.Code != proto.CodeNotPaired {
		t.Fatalf("without a token: %s", m.Raw)
	}
}

func TestARevokedOwnerTokenIsDisconnected(t *testing.T) {
	h := Start(t, ownerOverTCP)
	tok := h.mintOwner()
	c := h.dialOwner()
	c.call(ops.Msg{Op: "auth", Token: tok.Token})
	p, _ := h.ownerHello(tok.Token)
	if out, code := h.Pocketd("owner", "revoke", tok.ID[:8]); code != 0 {
		t.Fatalf("revoke: %s", out)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	for {
		if _, _, err := p.ws.Read(ctx); err != nil {
			if websocket.CloseStatus(err) != 4401 {
				t.Fatalf("websocket: %v", err)
			}
			break
		}
	}
	var m ops.Msg
	c.c.SetReadDeadline(time.Now().Add(10 * time.Second))
	if err := c.dec.Decode(&m); err == nil || strings.Contains(err.Error(), "timeout") {
		t.Fatalf("ops conn: %+v %v", m, err)
	}
}

func TestAnOwnerCanRevokeItsOwnToken(t *testing.T) {
	h := Start(t, ownerOverTCP)
	tok := h.mintOwner()
	c := h.dialOwner()
	c.call(ops.Msg{Op: "auth", Token: tok.Token})
	if m, err := c.call(ops.Msg{Op: "owner.revokeSelf"}); err != nil || m.Ev != "ok" {
		t.Fatalf("revokeSelf: %+v %v", m, err)
	}
	if out, _ := h.Pocketd("owner", "list"); strings.Contains(out, tok.ID[:8]) {
		t.Fatalf("still listed:\n%s", out)
	}
	if m, err := h.dialOwner().call(ops.Msg{Op: "auth", Token: tok.Token}); err != nil || m.ErrorCode != proto.CodeNotPaired {
		t.Fatalf("auth after revoke: %+v %v", m, err)
	}
}
