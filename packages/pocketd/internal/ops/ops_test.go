package ops

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"pocketd/internal/peer"
	"pocketd/internal/terminal"
)

func serve(t *testing.T, srv *Server) string {
	t.Helper()
	// Unix socket paths are capped at 104 bytes on macOS; t.TempDir() is too long.
	dir, err := os.MkdirTemp("/tmp", "ops")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(dir) })
	ln, err := Listen(filepath.Join(dir, "s.sock"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { ln.Close() })
	go srv.Serve(ln)
	return filepath.Join(dir, "s.sock")
}

func dial(t *testing.T, path string) *Conn {
	t.Helper()
	c, err := Dial(path)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { c.Close() })
	return c
}

func start(t *testing.T, srv *Server) *Conn {
	t.Helper()
	return dial(t, serve(t, srv))
}

func recv(t *testing.T, c *Conn, ev string) Msg {
	t.Helper()
	for {
		m, err := c.Recv()
		if err != nil {
			t.Fatalf("waiting for %s: %v", ev, err)
		}
		if m.Ev == ev {
			return m
		}
	}
}

func TestSpawnAttachAndScreen(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager()})
	c.Send(Msg{Op: "spawn", Cmd: "sh", Args: []string{"-c", "read x; echo got:$x; sleep 5"}, Cols: 40, Rows: 5})
	id := recv(t, c, "spawned").ID

	c.Send(Msg{Op: "attach", ID: id})
	if snap := recv(t, c, "snapshot"); snap.Cols != 40 || snap.Rows != 5 {
		t.Fatalf("snapshot size %dx%d", snap.Cols, snap.Rows)
	}
	c.Send(Msg{Op: "prompt", ID: id, Text: "hi"})
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		c.Send(Msg{Op: "screen", ID: id})
		if strings.Contains(recv(t, c, "screen").Text, "got:hi") {
			c.Send(Msg{Op: "close", ID: id})
			recv(t, c, "exit")
			return
		}
		time.Sleep(50 * time.Millisecond)
	}
	t.Fatal("prompt never reached the process")
}

func TestListShowsLiveTerminals(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager()})
	c.Send(Msg{Op: "spawn", Cmd: "sleep", Args: []string{"5"}})
	id := recv(t, c, "spawned").ID
	c.Send(Msg{Op: "list"})
	items := recv(t, c, "terminals").Items
	if len(items) != 1 || items[0].ID != id || items[0].Cmd != "sleep" {
		t.Fatalf("items = %+v", items)
	}
}

func TestUnknownTerminalIsAnError(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager()})
	c.Send(Msg{Op: "input", ID: "nope"})
	if m := recv(t, c, "error"); m.Error != "no such terminal" {
		t.Fatalf("error = %q", m.Error)
	}
}

func TestHookBlocksUntilAnswered(t *testing.T) {
	answer := make(chan []byte)
	c := start(t, &Server{Terminals: terminal.NewManager(), Hook: func(_ context.Context, who peer.Principal, m Msg) ([]byte, error) {
		return fmt.Appendf(<-answer, "%s/%d/%s", m.ID, who.Pid, m.Data), nil
	}})
	c.Send(Msg{Op: "hook", ID: "t1", Data: []byte("x")})
	c.Send(Msg{Op: "list"})
	if m, _ := c.Recv(); m.Ev != "terminals" {
		t.Fatalf("hook answered before its decision: %+v", m)
	}
	answer <- []byte("seen:")
	if m := recv(t, c, "hook"); string(m.Data) != fmt.Sprintf("seen:t1/%d/x", os.Getpid()) {
		t.Fatalf("data = %q", m.Data)
	}
}

func TestHookIsCancelledWhenTheCallerLeaves(t *testing.T) {
	cancelled := make(chan struct{})
	c := start(t, &Server{Terminals: terminal.NewManager(), Hook: func(ctx context.Context, _ peer.Principal, _ Msg) ([]byte, error) {
		<-ctx.Done()
		close(cancelled)
		return nil, nil
	}})
	c.Send(Msg{Op: "hook"})
	c.Close()
	select {
	case <-cancelled:
	case <-time.After(5 * time.Second):
		t.Fatal("hook never saw its context end")
	}
}

func TestErrorReplies(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager(), Spawn: func(Msg) (*terminal.Terminal, error) {
		return nil, errors.New("no such command")
	}})
	c.Send(Msg{Op: "hook"})
	if m := recv(t, c, "error"); m.Error != "hooks unsupported" {
		t.Fatalf("hook: %q", m.Error)
	}
	c.Send(Msg{Op: "spawn", Cmd: "nope"})
	if m := recv(t, c, "error"); m.Error != "no such command" {
		t.Fatalf("spawn: %q", m.Error)
	}
}

func TestUnknownOpIsAnError(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager()})
	c.Send(Msg{Op: "spawn", Cmd: "sleep", Args: []string{"5"}})
	id := recv(t, c, "spawned").ID
	c.Send(Msg{Op: "bogus", ID: id})
	if m := recv(t, c, "error"); m.ErrorCode != "scope_denied" || m.Error != "bogus is not a known verb" || m.ID != id {
		t.Fatalf("%+v", m)
	}
}

func TestSocketIsPrivate(t *testing.T) {
	st, err := os.Stat(serve(t, &Server{Terminals: terminal.NewManager()}))
	if err != nil || st.Mode().Perm() != 0o600 {
		t.Fatalf("%v %v", st.Mode(), err)
	}
}

func TestSnapshotPrecedesLiveOutput(t *testing.T) {
	path := serve(t, &Server{Terminals: terminal.NewManager()})
	c := dial(t, path)
	c.Send(Msg{Op: "spawn", Cmd: "yes", Cols: 20, Rows: 5})
	id := recv(t, c, "spawned").ID
	for range 200 {
		a := dial(t, path)
		a.Send(Msg{Op: "attach", ID: id})
		if m, err := a.Recv(); err != nil || m.Ev != "snapshot" {
			t.Fatalf("first event %q, want snapshot (%v)", m.Ev, err)
		}
		a.Close()
	}
	c.Send(Msg{Op: "close", ID: id})
}

func TestAttachStreamsForeground(t *testing.T) {
	terms := terminal.NewManager()
	c := start(t, &Server{Terminals: terms})
	c.Send(Msg{Op: "spawn", Cmd: "sleep", Args: []string{"5"}})
	id := recv(t, c, "spawned").ID
	c.Send(Msg{Op: "attach", ID: id})
	recv(t, c, "snapshot")
	terms.Get(id).SetForeground("npm run dev")
	if m := recv(t, c, "foreground"); m.ID != id || m.Text != "npm run dev" {
		t.Fatalf("%+v", m)
	}
}

func TestAPromptOver64KiBIsRefusedWithItsCode(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager()})
	c.Send(Msg{Op: "spawn", Cmd: "sleep", Args: []string{"5"}})
	id := recv(t, c, "spawned").ID
	c.Send(Msg{Op: "prompt", ID: id, Text: strings.Repeat("a", terminal.MaxPrompt+1)})
	if m := recv(t, c, "error"); m.ErrorCode != "prompt_too_large" || m.ID != id {
		t.Fatalf("%+v", m)
	}
}

func TestARefusedHookRepliesWithItsCode(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager(), Hook: func(context.Context, peer.Principal, Msg) ([]byte, error) {
		return nil, &peer.Refusal{Code: "hook_forged", Message: "forged"}
	}})
	c.Send(Msg{Op: "hook", ID: "t1"})
	if m := recv(t, c, "error"); m.ErrorCode != "hook_forged" || m.ID != "t1" {
		t.Fatalf("%+v", m)
	}
}

func TestStatusAnswersWithWhatTheServerReports(t *testing.T) {
	c := start(t, &Server{Terminals: terminal.NewManager(), Status: func() Status { return Status{PID: 42, Terminals: 2} }})
	c.Send(Msg{Op: "status"})
	if st := recv(t, c, "status").Status; st == nil || st.PID != 42 || st.Terminals != 2 {
		t.Fatalf("%+v", st)
	}
}

func TestStatusTextNamesEveryField(t *testing.T) {
	st := Status{PID: 42, Version: "abc123", Home: "/h", Sock: "/h/pocketd.sock", Log: "/h/logs/pocketd.log", Uptime: 3725,
		Listen: []string{"100.64.1.2:7331"}, Terminals: 3, Agents: map[string]int{"working": 1, "idle": 2},
		KeepingAwake: true, ShellEnv: "interactive 490ms", Service: "loaded"}
	want := `pocketd abc123, pid 42, up 1h2m5s
home       /h
socket     /h/pocketd.sock
log        /h/logs/pocketd.log
listening  100.64.1.2:7331
service    loaded
terminals  3
agents     2 idle, 1 working
awake      yes
tailnet    no
shell env  interactive 490ms
`
	if got := st.Text(); got != want {
		t.Fatalf("got\n%s\nwant\n%s", got, want)
	}
}
