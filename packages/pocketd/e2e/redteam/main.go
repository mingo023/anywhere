// Command redteam is the inside stage of the no-self-approval red-team. The
// e2e test runs it in a scratch pocketd's Terminal, so every call it makes
// comes from a PTY peer. It writes one "<probe> <outcome>" line per call to its
// out file.
//
//	redteam inside <out> <claude terminal> <agent id> <request id>
//	redteam probe <sock> <out>   (residuals: reports the scopes a reparented process gets)
package main

import (
	"cmp"
	"context"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
	"unsafe"

	"github.com/coder/websocket"
	"golang.org/x/sys/unix"

	"pocketd/internal/ops"
)

func main() {
	switch {
	case len(os.Args) == 6 && os.Args[1] == "inside":
		inside(os.Args[2], os.Args[3], os.Args[4], os.Args[5])
	case len(os.Args) == 4 && os.Args[1] == "probe":
		os.WriteFile(os.Args[3], []byte(scopes(os.Args[2])), 0o600)
	default:
		fmt.Fprintln(os.Stderr, "usage: redteam inside <out> <terminal> <agent> <request> | probe <sock> <out>")
		os.Exit(2)
	}
}

func inside(out, claudeTerm, agentID, requestID string) {
	sock, own := os.Getenv("POCKETD_SOCK"), os.Getenv("POCKETD_PTY")
	var lines []string
	note := func(probe, outcome string) { lines = append(lines, probe+" "+outcome) }
	ask := []byte(`{"hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"ls"}}`)
	for _, p := range []struct {
		name string
		m    ops.Msg
	}{
		{"ops.spawn", ops.Msg{Op: "spawn", Cmd: "true"}},
		{"ops.attach", ops.Msg{Op: "attach", ID: claudeTerm}},
		{"ops.screen", ops.Msg{Op: "screen", ID: claudeTerm}},
		{"ops.input", ops.Msg{Op: "input", ID: claudeTerm, Data: []byte("1\r")}},
		{"ops.prompt", ops.Msg{Op: "prompt", ID: claudeTerm, Text: "1"}},
		{"ops.devices", ops.Msg{Op: "devices"}},
		{"ops.devices.rename", ops.Msg{Op: "devices.rename", ID: "x", Text: "x"}},
		{"ops.devices.revoke", ops.Msg{Op: "devices.revoke", ID: "x"}},
		{"ops.pair.begin", ops.Msg{Op: "pair.begin"}},
		{"hook.other", ops.Msg{Op: "hook", ID: claudeTerm, Data: ask}},
		{"hook.own", ops.Msg{Op: "hook", ID: own, Data: ask}},
		{"ops.config-set", ops.Msg{Op: "config-set", Key: "phone.maxAccess", Text: "auto"}},
		{"ops.launch-exit", ops.Msg{Op: "launch-exit", ID: claudeTerm, Text: "agent", Code: 1}},
	} {
		note(p.name, opsOutcome(sock, p.m))
	}
	wsOutcomes(sock, agentID, requestID, note)
	if os.Getenv("REDTEAM_RESIDUALS") == "1" {
		residuals(sock, note)
	}
	// The test polls for out, so it must appear whole.
	if os.WriteFile(out+".tmp", []byte(strings.Join(lines, "\n")+"\n"), 0o600) == nil {
		os.Rename(out+".tmp", out)
	}
}

// opsOutcome is the error code pocketd answers m with, "ok" for any other
// reply, or "silent" when none comes: input and prompt reply only to refuse.
func opsOutcome(sock string, m ops.Msg) string {
	c, err := ops.Dial(sock)
	if err != nil {
		return "dial-failed"
	}
	defer c.Close()
	got := make(chan ops.Msg, 1)
	go func() {
		c.Send(m)
		r, _ := c.Recv()
		got <- r
	}()
	select {
	case r := <-got:
		if r.Ev == "error" {
			return cmp.Or(r.ErrorCode, "error")
		}
		return "ok"
	case <-time.After(2 * time.Second):
		return "silent"
	}
}

type frame struct {
	Type   string   `json:"type"`
	ID     string   `json:"id"`
	Code   string   `json:"code"`
	Scopes []string `json:"scopes"`
}

func dialSock(ctx context.Context, sock string) (*websocket.Conn, error) {
	unixClient := &http.Client{Transport: &http.Transport{DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
		return (&net.Dialer{}).DialContext(ctx, "unix", sock)
	}}}
	ws, _, err := websocket.Dial(ctx, "ws://localhost/", &websocket.DialOptions{HTTPClient: unixClient})
	return ws, err
}

// call sends msg and reads until the frame that answers its id.
func call(ctx context.Context, ws *websocket.Conn, msg map[string]any) frame {
	raw, _ := json.Marshal(msg)
	if err := ws.Write(ctx, websocket.MessageText, raw); err != nil {
		return frame{Type: "error", Code: "write-failed"}
	}
	for {
		_, raw, err := ws.Read(ctx)
		if err != nil {
			return frame{Type: "error", Code: "read-failed"}
		}
		var f frame
		if json.Unmarshal(raw, &f) == nil && f.ID == msg["id"] {
			return f
		}
	}
}

func outcome(f frame) string {
	if f.Type == "error" {
		return cmp.Or(f.Code, "error")
	}
	return "ok"
}

func wsOutcomes(sock, agentID, requestID string, note func(string, string)) {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	ws, err := dialSock(ctx, sock)
	if err != nil {
		note("ws.hello", "dial-failed")
		return
	}
	defer ws.CloseNow()
	hello := call(ctx, ws, map[string]any{"type": "hello", "id": "h", "clientId": "redteam", "protocolVersion": 3, "caps": []string{"scopes.v1"}})
	note("ws.scopes", strings.Join(hello.Scopes, ","))
	for _, p := range []struct {
		name string
		msg  map[string]any
	}{
		{"ws.resolve", map[string]any{"type": "permission.resolve", "id": "1", "requestId": requestID, "decision": "allow"}},
		{"ws.pair.begin", map[string]any{"type": "pair.begin", "id": "2"}},
		{"ws.prompt", map[string]any{"type": "agent.prompt", "id": "3", "agentId": agentID, "text": "1"}},
		{"ws.close", map[string]any{"type": "agent.close", "id": "4", "agentId": agentID}},
		{"ws.create", map[string]any{"type": "agent.create", "id": "5", "requestId": "r", "spec": map[string]any{
			"project": "/", "checkout": map[string]any{"worktree": "/"}, "provider": "claude", "access": "full", "plan": false}}},
		{"ws.config.set", map[string]any{"type": "config.set", "id": "6", "key": "phone.maxAccess", "value": "auto"}},
	} {
		note(p.name, outcome(call(ctx, ws, p.msg)))
	}
}

// scopes is what pocketd grants this process on its socket, comma-separated.
func scopes(sock string) string {
	for deadline := time.Now().Add(5 * time.Second); os.Getppid() != 1 && time.Now().Before(deadline); {
		time.Sleep(50 * time.Millisecond)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	ws, err := dialSock(ctx, sock)
	if err != nil {
		return "dial-failed"
	}
	defer ws.CloseNow()
	return strings.Join(call(ctx, ws, map[string]any{"type": "hello", "id": "h", "clientId": "redteam", "protocolVersion": 3, "caps": []string{"scopes.v1"}}).Scopes, ",")
}

// residuals records the escapes that telling Terminals apart by process
// ancestry leaves open; none of them fails the gate.
func residuals(sock string, note func(string, string)) {
	exe, _ := os.Executable()
	dir, err := os.MkdirTemp("", "redteam")
	if err != nil {
		note("residual.tmp", "failed")
		return
	}
	defer os.RemoveAll(dir)
	nohup := filepath.Join(dir, "nohup")
	exec.Command("sh", "-c", `nohup "$0" probe "$1" "$2" >/dev/null 2>&1 &`, exe, sock, nohup).Run()
	note("residual.nohup", waitFile(nohup))
	label := fmt.Sprintf("pocket.redteam.%d", os.Getpid())
	launched := filepath.Join(dir, "launchctl")
	if exec.Command("launchctl", "submit", "-l", label, "--", exe, "probe", sock, launched).Run() != nil {
		note("residual.launchctl", "submit-failed")
	} else {
		note("residual.launchctl", waitFile(launched))
		exec.Command("launchctl", "remove", label).Run()
	}
	note("residual.tiocsti", tiocsti())
}

func waitFile(path string) string {
	for deadline := time.Now().Add(3 * time.Second); time.Now().Before(deadline); time.Sleep(100 * time.Millisecond) {
		if b, err := os.ReadFile(path); err == nil && len(b) > 0 {
			return string(b)
		}
	}
	return "timeout"
}

// tiocsti pushes one byte into this process's own tty input queue, as an
// agent's tool subprocess could; pocketd never sees such bytes.
func tiocsti() string {
	tty, err := os.OpenFile("/dev/tty", os.O_RDWR, 0)
	if err != nil {
		return "no-tty"
	}
	defer tty.Close()
	c := byte('#')
	if _, _, errno := unix.Syscall(unix.SYS_IOCTL, tty.Fd(), unix.TIOCSTI, uintptr(unsafe.Pointer(&c))); errno != 0 {
		return "refused"
	}
	return "allowed"
}
