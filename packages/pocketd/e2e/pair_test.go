package e2e

import (
	"context"
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
	"testing"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/ops"
)

func TestAPhonePairsSaysHelloAndIsRemoved(t *testing.T) {
	h := Start(t)
	owner := h.Ops()
	owner.Send(ops.Msg{Op: "pair.begin", Text: fmt.Sprintf("127.0.0.1:%d", h.Port)})
	begin, err := owner.Recv()
	if err != nil || begin.Ev != "pair.begin" {
		t.Fatalf("%+v %v", begin, err)
	}
	p := h.Dial()
	p.Send(map[string]any{"type": "pair", "id": "p", "code": begin.Pair.Code, "name": "iPhone", "platform": "ios", "protocol": map[string]int{"min": 3, "max": 3}})
	paired := p.WaitFor("pair.ok", func(m Message) bool { return m.Type == "pair.ok" })
	if m, err := owner.Recv(); err != nil || m.Ev != "pair.ok" || m.ID != paired.DeviceID || m.Text != "iPhone" {
		t.Fatalf("owner: %+v %v", m, err)
	}

	p = h.Dial()
	p.Send(map[string]any{"type": "hello", "id": "h", "token": paired.Token, "clientId": "e2e", "protocolVersion": 3})
	p.WaitFor("hello.ok", func(m Message) bool { return m.Type == "hello.ok" })

	owner.Send(ops.Msg{Op: "devices.revoke", ID: paired.DeviceID})
	if m, err := owner.Recv(); err != nil || m.Ev != "ok" {
		t.Fatalf("revoke: %+v %v", m, err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	for {
		_, _, err := p.ws.Read(ctx)
		var ce websocket.CloseError
		if errors.As(err, &ce) && ce.Code == 4401 && ce.Reason == "revoked" {
			return
		}
		if err != nil {
			t.Fatalf("want close 4401 revoked, got %v", err)
		}
	}
}

func TestPairOutsideATerminalExitsTwo(t *testing.T) {
	h := Start(t)
	cmd := exec.Command(filepath.Join(binDir, "pocketd"), "pair")
	cmd.Env = h.Env
	out, err := cmd.CombinedOutput()
	var exit *exec.ExitError
	if !errors.As(err, &exit) || exit.ExitCode() != 2 || string(out) != "Run pocketd pair in a terminal.\n" {
		t.Fatalf("%v %q", err, out)
	}
}
