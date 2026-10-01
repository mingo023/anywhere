package ops

import (
	"context"
	"fmt"
	"net"
	"net/http"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/peer"
	"pocketd/internal/terminal"
)

// TestMain doubles as a PTY peer: spawned inside a Terminal with OPS_PEER set,
// the test binary sends each op in OPS_PEER_OPS and prints what came back.
func TestMain(m *testing.M) {
	if sock := os.Getenv("OPS_PEER"); sock != "" {
		c, err := Dial(sock)
		if err != nil {
			fmt.Println("dial:", err)
			return
		}
		for _, op := range strings.Split(os.Getenv("OPS_PEER_OPS"), ",") {
			c.Send(Msg{Op: op, ID: os.Getenv("OPS_TARGET"), Cmd: "true"})
			r, _ := c.Recv()
			fmt.Printf("%s:%s:%s\n", op, r.Ev, r.ErrorCode)
		}
		return
	}
	os.Exit(m.Run())
}

func dialWS(t *testing.T, sock string) *websocket.Conn {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	unix := &http.Client{Transport: &http.Transport{DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
		return (&net.Dialer{}).DialContext(ctx, "unix", sock)
	}}}
	ws, _, err := websocket.Dial(ctx, "ws://localhost/", &websocket.DialOptions{HTTPClient: unix})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { ws.CloseNow() })
	return ws
}

func TestAWebsocketUpgradeAndLineJSONShareOneSocket(t *testing.T) {
	sock := serve(t, &Server{Terminals: terminal.NewManager(), WS: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		who, ok := peer.From(r.Context())
		ws, err := websocket.Accept(w, r, nil)
		if err != nil {
			return
		}
		ws.Write(r.Context(), websocket.MessageText, fmt.Appendf(nil, "%v %v %d", ok, who.Kind == peer.Owner, who.Pid))
		ws.Close(websocket.StatusNormalClosure, "")
	})})
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	_, got, err := dialWS(t, sock).Read(ctx)
	if want := fmt.Sprintf("true true %d", os.Getpid()); err != nil || string(got) != want {
		t.Fatalf("%q %v, want %q", got, err, want)
	}
	c := dial(t, sock)
	c.Send(Msg{Op: "list"})
	recv(t, c, "terminals")
}

func TestASilentConnIsClosedAfterThePeekDeadline(t *testing.T) {
	c, err := net.Dial("unix", serve(t, &Server{Terminals: terminal.NewManager(), PeekTimeout: 50 * time.Millisecond}))
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	c.SetReadDeadline(time.Now().Add(5 * time.Second))
	if _, err := c.Read(make([]byte, 1)); err == nil || os.IsTimeout(err) {
		t.Fatalf("read = %v, want the conn closed", err)
	}
}

func TestAPtyPeerCanListButNotAttachOrTypeIntoAnAsk(t *testing.T) {
	sock := serve(t, &Server{Terminals: terminal.NewManager(), AskOpen: func(string) bool { return true }})
	c := dial(t, sock)
	c.Send(Msg{Op: "spawn", Cmd: "sleep", Args: []string{"30"}})
	other := recv(t, c, "spawned").ID
	env := append(os.Environ(), "OPS_PEER="+sock, "OPS_TARGET="+other, "OPS_PEER_OPS=list,attach,screen,input,prompt,spawn,close")
	c.Send(Msg{Op: "spawn", Cmd: "sh", Args: []string{"-c", os.Args[0] + "; sleep 30"}, Env: env, Cols: 80, Rows: 12})
	id := recv(t, c, "spawned").ID
	want := []string{"list:terminals:", "attach:error:scope_denied", "screen:error:scope_denied", "input:error:ask_open", "prompt:error:ask_open", "spawn:error:scope_denied", "close:error:scope_denied"}
	var screen string
	for deadline := time.Now().Add(10 * time.Second); time.Now().Before(deadline); time.Sleep(50 * time.Millisecond) {
		c.Send(Msg{Op: "screen", ID: id})
		if screen = recv(t, c, "screen").Text; strings.Contains(screen, want[len(want)-1]) {
			break
		}
	}
	for _, w := range want {
		if !strings.Contains(screen, w) {
			t.Errorf("missing %q in:\n%s", w, screen)
		}
	}
}
