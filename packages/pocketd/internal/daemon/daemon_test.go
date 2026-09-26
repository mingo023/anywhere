package daemon

import (
	"context"
	"encoding/json"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/codex"
	"pocketd/internal/codex/codextest"
	"pocketd/internal/hub"
	"pocketd/internal/ops"
	"pocketd/internal/session"
	"pocketd/internal/timeline"
)

func newDaemon(t *testing.T) *Daemon {
	h := hub.New()
	return &Daemon{Sessions: session.NewManager(), Agents: agent.NewRegistry(h), Broker: broker.New(h), Home: t.TempDir(), Exe: "/bin/true"}
}

// fakeCodex puts a codex on PATH that runs appServer for `app-server …`
// and tui otherwise.
func fakeCodex(t *testing.T, appServer, tui string) (env []string, sock string) {
	dir, _ := os.MkdirTemp("/tmp", "pd")
	t.Cleanup(func() { os.RemoveAll(dir) })
	script := "#!/bin/sh\nif [ \"$1\" = app-server ]; then\n" + appServer + "\nexit 0\nfi\n" + tui + "\n"
	os.WriteFile(filepath.Join(dir, "codex"), []byte(script), 0o755)
	env = []string{"PATH=" + dir + ":/bin:/usr/bin", "CODEX_HOME=" + dir}
	return env, codex.Sock(env)
}

func TestClaudeSpawnFailureRemovesSettings(t *testing.T) {
	d := newDaemon(t)
	if _, err := d.Spawn(ops.Msg{Cmd: "claude", Env: []string{"PATH=/nonexistent"}}); err == nil {
		t.Fatal("spawned without claude")
	}
	if left, _ := os.ReadDir(filepath.Join(d.Home, "run")); len(left) != 0 {
		t.Fatalf("left %v", left)
	}
}

func TestCodexTUIExitReleasesAccount(t *testing.T) {
	defer func(w time.Duration) { CodexThreadWait = w }(CodexThreadWait)
	CodexThreadWait = 3 * time.Second
	env, sock := fakeCodex(t, "", "exit 0")
	codextest.Start(t, sock, func(string, json.RawMessage) (any, string) {
		return map[string]any{"data": []string{}}, ""
	})
	d := newDaemon(t)
	s, err := d.Spawn(ops.Msg{Cmd: "codex", Env: env})
	if err != nil {
		t.Fatal(err)
	}
	<-s.Done()
	start := time.Now()
	if _, err := d.Spawn(ops.Msg{Cmd: "codex", Env: env}); err != nil {
		t.Fatal(err)
	}
	if waited := time.Since(start); waited > time.Second {
		t.Fatalf("second spawn waited %v for an exited TUI", waited)
	}
}

func TestCodexThreadAfterTheLockWindowStillReachesThePhone(t *testing.T) {
	defer func(w time.Duration) { CodexThreadWait = w }(CodexThreadWait)
	CodexThreadWait = 100 * time.Millisecond
	env, sock := fakeCodex(t, "", "exec sleep 30")
	late := time.Now().Add(time.Second)
	codextest.Start(t, sock, func(method string, _ json.RawMessage) (any, string) {
		switch {
		case method == "thread/loaded/list" && time.Now().After(late):
			return map[string]any{"data": []string{"th1"}}, ""
		case method == "thread/loaded/list":
			return map[string]any{"data": []string{}}, ""
		}
		return map[string]any{"thread": map[string]any{"turns": []any{}}}, ""
	})
	d := newDaemon(t)
	s, err := d.Spawn(ops.Msg{Cmd: "codex", Env: env})
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	eventually(t, "agent", func() bool { _, err := d.Agents.Get(s.Info().ID); return err == nil })
}

func TestCodexDaemonStartIgnoresItsChildren(t *testing.T) {
	env, sock := fakeCodex(t, "sleep 10 &", "exit 0")
	codextest.Start(t, sock, codextest.OK)
	start := time.Now()
	if _, err := newDaemon(t).Spawn(ops.Msg{Cmd: "codex", Env: env}); err != nil {
		t.Fatal(err)
	}
	if waited := time.Since(start); waited > 5*time.Second {
		t.Fatalf("spawn waited %v on the app-server's child", waited)
	}
}

func TestCodexLockKeyFollowsSymlinks(t *testing.T) {
	real, _ := os.MkdirTemp("/tmp", "pd")
	defer os.RemoveAll(real)
	link := real + "-link"
	os.Symlink(real, link)
	defer os.Remove(link)
	os.WriteFile(filepath.Join(real, "s.sock"), nil, 0o600)
	if a, b := lockKey(filepath.Join(real, "s.sock")), lockKey(filepath.Join(link, "s.sock")); a != b {
		t.Fatalf("%s != %s", a, b)
	}
}

// dropServer is an app-server whose thread appears on the second list and
// whose resumed connection closes when drop is closed.
func dropServer(t *testing.T, sock string, drop <-chan struct{}) {
	os.MkdirAll(filepath.Dir(sock), 0o700)
	ln, err := net.Listen("unix", sock)
	if err != nil {
		t.Fatal(err)
	}
	lists := 0
	srv := &http.Server{Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ws, err := websocket.Accept(w, r, &websocket.AcceptOptions{CompressionMode: websocket.CompressionDisabled})
		if err != nil {
			return
		}
		defer ws.CloseNow()
		for {
			_, raw, err := ws.Read(context.Background())
			if err != nil {
				return
			}
			var f struct {
				ID     json.RawMessage `json:"id"`
				Method string          `json:"method"`
			}
			json.Unmarshal(raw, &f)
			if f.ID == nil {
				continue
			}
			var result any = map[string]any{}
			switch f.Method {
			case "thread/loaded/list":
				lists++
				result = map[string]any{"data": []string{}}
				if lists > 1 {
					result = map[string]any{"data": []string{"th1"}}
				}
			case "thread/resume":
				result = map[string]any{"thread": map[string]any{"turns": []any{}}}
			}
			out, _ := json.Marshal(map[string]any{"id": f.ID, "result": result})
			ws.Write(context.Background(), websocket.MessageText, out)
			if f.Method == "thread/resume" {
				<-drop
				return
			}
		}
	})}
	go srv.Serve(ln)
	t.Cleanup(func() { srv.Close() })
}

func TestCodexAgentLeavesWhenAppServerDrops(t *testing.T) {
	env, sock := fakeCodex(t, "", "exec sleep 30")
	drop := make(chan struct{})
	dropServer(t, sock, drop)
	d := newDaemon(t)
	s, err := d.Spawn(ops.Msg{Cmd: "codex", Env: env})
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	eventually(t, "agent", func() bool { _, err := d.Agents.Get(s.Info().ID); return err == nil })
	close(drop)
	eventually(t, "agent removed", func() bool { _, err := d.Agents.Get(s.Info().ID); return err != nil })
}

func eventually(t *testing.T, what string, ok func() bool) {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(20 * time.Millisecond) {
		if ok() {
			return
		}
	}
	t.Fatalf("timed out waiting for %s", what)
}

func TestHookOffersDesktopChoices(t *testing.T) {
	const payload = `{"session_id":"s1","permission_mode":"default","tool_name":"Bash","tool_input":{"command":"touch c.txt"},
		"permission_suggestions":[{"type":"addDirectories","directories":["/w"],"destination":"session"}]}`
	for _, c := range []struct {
		answer broker.Answer
		want   string
	}{
		{broker.Answer{Decision: "allow"}, `{"behavior":"allow"}`},
		{broker.Answer{Decision: "allow", Option: optionAlways}, `{"behavior":"allow","updatedPermissions":[{"type":"addDirectories","directories":["/w"],"destination":"session"}]}`},
		{broker.Answer{Decision: "allow", Option: optionAuto}, `{"behavior":"allow","updatedPermissions":[{"type":"setMode","mode":"auto","destination":"session"}]}`},
		{broker.Answer{Decision: "deny"}, `{"behavior":"deny","message":"Denied from phone","interrupt":true}`},
		{broker.Answer{Decision: "deny", Message: "use b.txt"}, `{"behavior":"deny","message":"Denied from phone","interrupt":true}`},
	} {
		d := newDaemon(t)
		drv := &promptRecorder{prompts: make(chan string, 1)}
		a := d.Agents.Add("s1", "/w", "claude", drv)
		out := make(chan []byte, 1)
		go func() { out <- d.Hook(context.Background(), []byte(payload)) }()
		for len(d.Broker.Open()) == 0 {
			time.Sleep(time.Millisecond)
		}
		req := d.Broker.Open()[0]
		opts, _ := json.Marshal(req.Options)
		if string(opts) != `[{"id":"always","label":"Yes, and always allow access to /w"},{"id":"auto","label":"Yes, and switch to auto mode"}]` || !req.Feedback {
			t.Fatalf("%s feedback=%v", opts, req.Feedback)
		}
		d.Broker.Resolve(req.RequestID, c.answer)
		var got struct {
			HookSpecificOutput struct{ Decision json.RawMessage }
		}
		json.Unmarshal(<-out, &got)
		if string(got.HookSpecificOutput.Decision) != c.want {
			t.Errorf("%+v: got %s", c.answer, got.HookSpecificOutput.Decision)
		}
		if c.answer.Message != "" {
			a.Apply(timeline.Event{Kind: "result", Error: "interrupted"})
			if p := <-drv.prompts; p != c.answer.Message {
				t.Errorf("prompted %q", p)
			}
		}
	}
}

type promptRecorder struct {
	agent.Driver
	prompts chan string
}

func (p *promptRecorder) Prompt(text string) error {
	p.prompts <- text
	return nil
}
