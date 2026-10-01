package e2e

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"syscall"
	"testing"
	"time"

	"github.com/coder/websocket"

	"pocketd/internal/ops"
	"pocketd/internal/registry"
)

var binDir string

func TestMain(m *testing.M) {
	dir, err := os.MkdirTemp("", "pocketd-e2e")
	if err != nil {
		panic(err)
	}
	for _, b := range [][2]string{{"pocketd", "../cmd/pocketd"}, {"fake/claude", "./fakeclaude"}, {"fake/codex", "./fakecodex"}, {"redteam", "./redteam"}} {
		out, err := exec.Command("go", "build", "-o", filepath.Join(dir, b[0]), b[1]).CombinedOutput()
		if err != nil {
			fmt.Fprintf(os.Stderr, "build %s: %v\n%s", b[1], err, out)
			os.Exit(1)
		}
	}
	binDir = dir
	code := m.Run()
	os.RemoveAll(dir)
	os.Exit(code)
}

type Harness struct {
	t         *testing.T
	Home      string
	Sock      string
	ClaudeDir string
	Port      int
	Token     string
	Env       []string
	Repo      string
	log       *bytes.Buffer
	cmd       *exec.Cmd
	exited    chan struct{}
}

func freePort(t *testing.T) int {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()
	return ln.Addr().(*net.TCPAddr).Port
}

// Start runs `pocketd serve` against a throwaway POCKET_HOME, with the fake
// claude first on PATH.
func Start(t *testing.T, opts ...func(*Harness)) *Harness {
	t.Helper()
	// Unix socket paths are capped at 104 bytes on macOS; t.TempDir() is too long.
	home, err := os.MkdirTemp("/tmp", "pk")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(home) })
	h := &Harness{
		t:         t,
		Home:      home,
		Sock:      filepath.Join(home, "pocketd.sock"),
		ClaudeDir: filepath.Join(home, "claude"),
		Token:     "test-token",
	}
	h.Env = append(os.Environ(),
		"HOME="+home,
		"POCKET_HOME="+home,
		"POCKETD_SOCK="+h.Sock,
		"CLAUDE_CONFIG_DIR="+h.ClaudeDir,
		"PATH="+filepath.Join(binDir, "fake")+":"+os.Getenv("PATH"),
		"POCKETD_RESTORE_SHELL=/bin/sh",
	)
	for _, opt := range opts {
		opt(h)
	}
	h.up()
	return h
}

// up starts pocketd, again if another process takes the free port before pocketd listens on it.
func (h *Harness) up() {
	h.t.Helper()
	for try := 1; !h.serve(); try++ {
		if try == 5 || !strings.Contains(h.log.String(), "address already in use") {
			h.t.Fatal("pocketd exited")
		}
	}
}

// Restart stops pocketd with SIGTERM and starts it again on the same home.
func (h *Harness) Restart() {
	h.t.Helper()
	if code := h.Stop(syscall.SIGTERM); code != 0 {
		h.t.Fatalf("pocketd exited %d", code)
	}
	h.up()
}

// serve starts pocketd on a free port and reports whether it came up.
func (h *Harness) serve() bool {
	log := &bytes.Buffer{}
	h.Port, h.log = freePort(h.t), log
	// resumeAgents is off: a resumed agent would run from the login shell's PATH, where the real claude is.
	config := fmt.Sprintf(`{"token":%q,"port":%d,"listen":"loopback","restore":{"resumeAgents":false}}`, h.Token, h.Port)
	if err := os.WriteFile(filepath.Join(h.Home, "config.json"), []byte(config), 0o600); err != nil {
		h.t.Fatal(err)
	}
	cmd := exec.Command(filepath.Join(binDir, "pocketd"), "serve")
	cmd.Env = h.Env
	cmd.Stdout, cmd.Stderr = log, log
	if err := cmd.Start(); err != nil {
		h.t.Fatal(err)
	}
	exited := make(chan struct{})
	go func() {
		cmd.Wait()
		close(exited)
	}()
	h.cmd, h.exited = cmd, exited
	h.t.Cleanup(func() {
		cmd.Process.Kill()
		<-exited
		if h.t.Failed() {
			h.t.Logf("pocketd output:\n%s", log)
		}
	})
	h.eventually("ops socket", func() bool {
		select {
		case <-exited:
			return true
		default:
		}
		c, err := ops.Dial(h.Sock)
		if err == nil {
			c.Close()
		}
		return err == nil
	})
	select {
	case <-exited:
		return false
	default:
		return true
	}
}

// Stop sends sig to pocketd and returns its exit code.
func (h *Harness) Stop(sig os.Signal) int {
	h.t.Helper()
	h.cmd.Process.Signal(sig)
	select {
	case <-h.exited:
	case <-time.After(10 * time.Second):
		h.t.Fatal("pocketd did not exit")
	}
	return h.cmd.ProcessState.ExitCode()
}

// pocketd runs the CLI with env and returns its combined output and exit code.
func pocketd(t *testing.T, env []string, args ...string) (string, int) {
	t.Helper()
	cmd := exec.Command(filepath.Join(binDir, "pocketd"), args...)
	cmd.Env = env
	out, err := cmd.CombinedOutput()
	if exit := (*exec.ExitError)(nil); err != nil && !errors.As(err, &exit) {
		t.Fatal(err)
	}
	return string(out), cmd.ProcessState.ExitCode()
}

func (h *Harness) Pocketd(args ...string) (string, int) {
	h.t.Helper()
	return pocketd(h.t, h.Env, args...)
}

func (h *Harness) eventually(what string, ok func() bool) {
	h.t.Helper()
	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		if ok() {
			return
		}
		time.Sleep(50 * time.Millisecond)
	}
	h.t.Fatalf("timed out waiting for %s", what)
}

func (h *Harness) Ops() *ops.Conn {
	h.t.Helper()
	c, err := ops.Dial(h.Sock)
	if err != nil {
		h.t.Fatal(err)
	}
	h.t.Cleanup(func() { c.Close() })
	return c
}

// Spawn starts cmd the way `pocketd run` would from h.Home and returns the terminal id.
func (h *Harness) Spawn(cmd string, args ...string) string {
	h.t.Helper()
	c := h.Ops()
	c.Send(ops.Msg{Op: "spawn", Cmd: cmd, Args: args, Cwd: h.Home, Env: h.Env, Cols: 100, Rows: 30})
	m, err := c.Recv()
	if err != nil || m.Ev != "spawned" {
		h.t.Fatalf("spawn %s: %+v %v", cmd, m, err)
	}
	return m.ID
}

// SpawnReady spawns cmd and waits for its fake to come up.
func (h *Harness) SpawnReady(cmd string) string {
	h.t.Helper()
	id := h.Spawn(cmd)
	h.WaitScreen(id, "fake "+cmd+" ready")
	return id
}

// StartClaude starts pocketd with a phone connected and a ready claude, and
// returns the claude's terminal and agent.
func StartClaude(t *testing.T) (h *Harness, phone *Phone, term, agent string) {
	t.Helper()
	h = Start(t)
	phone = h.Phone()
	term = h.SpawnReady("claude")
	agent = phone.WaitFor("the claude's agent", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.TerminalID == term && m.Agent.ProviderSessionID != "" && m.Agent.Attached
	}).Agent.ID
	return h, phone, term, agent
}

func (h *Harness) Screen(id string) string {
	h.t.Helper()
	c := h.Ops()
	c.Send(ops.Msg{Op: "screen", ID: id})
	m, err := c.Recv()
	if err != nil {
		h.t.Fatal(err)
	}
	return m.Text
}

func (h *Harness) Prompt(id, text string) {
	h.t.Helper()
	h.Ops().Send(ops.Msg{Op: "prompt", ID: id, Text: text})
}

func (h *Harness) WaitScreen(id, want string) {
	h.t.Helper()
	h.eventually(fmt.Sprintf("screen to show %q", want), func() bool {
		return strings.Contains(h.Screen(id), want)
	})
}

// launchReady gives pocketd's login shell the fake agents and a Project that
// Claude trusts, with setup as its Repo's setup. It drops
// CLAUDE_CODE_SANDBOXED, which a test run inside Claude Code inherits and
// which makes every folder trusted.
func launchReady(setup string) func(*Harness) {
	return func(h *Harness) {
		h.Env = slices.DeleteFunc(h.Env, func(kv string) bool { return strings.HasPrefix(kv, "CLAUDE_CODE_SANDBOXED=") })
		fake := filepath.Join(binDir, "fake")
		sh := fmt.Sprintf("export PATH='%s':$PATH\nexport CLAUDE_CONFIG_DIR='%s'\n", fake, h.ClaudeDir)
		fish := fmt.Sprintf("set -gx PATH '%s' $PATH\nset -gx CLAUDE_CONFIG_DIR '%s'\n", fake, h.ClaudeDir)
		os.MkdirAll(filepath.Join(h.Home, ".config", "fish"), 0o755)
		for name, body := range map[string]string{".zshenv": sh, ".bash_profile": sh, ".profile": sh, ".config/fish/config.fish": fish} {
			if err := os.WriteFile(filepath.Join(h.Home, name), []byte(body), 0o644); err != nil {
				h.t.Fatal(err)
			}
		}
		repo := filepath.Join(h.Home, "repo")
		os.MkdirAll(repo, 0o755)
		repo, _ = filepath.EvalSymlinks(repo)
		for _, args := range [][]string{{"init", "-q", "-b", "main"}, {"commit", "-q", "--allow-empty", "-m", "init"}} {
			cmd := exec.Command("git", append([]string{"-C", repo, "-c", "user.name=e2e", "-c", "user.email=e2e@x", "-c", "commit.gpgsign=false"}, args...)...)
			if out, err := cmd.CombinedOutput(); err != nil {
				h.t.Fatalf("git %v: %s", args, out)
			}
		}
		h.Repo = repo
		reg, _ := json.Marshal(registry.File{Projects: []string{repo}, Repos: map[string]registry.Repo{repo: {Worktrees: filepath.Join(h.Home, "wt"), Setup: setup}}})
		os.WriteFile(filepath.Join(h.Home, "desktop.json"), reg, 0o600)
		os.MkdirAll(h.ClaudeDir, 0o755)
		os.WriteFile(filepath.Join(h.ClaudeDir, ".claude.json"), []byte(`{"projects":{"`+repo+`":{"hasTrustDialogAccepted":true}}}`), 0o600)
	}
}

// Owner opens the desktop's channel: the unix socket, no token (E03).
func (h *Harness) Owner() *Phone {
	h.t.Helper()
	unix := &http.Client{Transport: &http.Transport{DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
		return (&net.Dialer{}).DialContext(ctx, "unix", h.Sock)
	}}}
	ws, _, err := websocket.Dial(context.Background(), "ws://localhost/", &websocket.DialOptions{HTTPClient: unix})
	if err != nil {
		h.t.Fatal(err)
	}
	h.t.Cleanup(func() { ws.CloseNow() })
	p := &Phone{t: h.t, ws: ws}
	p.Send(map[string]any{"type": "hello", "id": "h", "clientId": "e2e-desktop", "protocolVersion": 3})
	p.WaitFor("hello.ok", func(m Message) bool { return m.Type == "hello.ok" })
	return p
}

// Paired pairs a new phone (E02) and says hello with caps. No caps is sent
// as [], since E02's hello refuses a null list.
func (h *Harness) Paired(caps ...string) *Phone {
	h.t.Helper()
	if caps == nil {
		caps = []string{}
	}
	owner := h.Ops()
	owner.Send(ops.Msg{Op: "pair.begin", Text: fmt.Sprintf("127.0.0.1:%d", h.Port)})
	begin, _ := owner.Recv()
	p := h.Dial()
	p.Send(map[string]any{"type": "pair", "id": "p", "code": begin.Pair.Code, "name": "iPhone", "platform": "ios", "protocol": map[string]any{"min": 3, "max": 3}})
	paired := p.WaitFor("pair.ok", func(m Message) bool { return m.Type == "pair.ok" })
	p = h.Dial()
	p.Send(map[string]any{"type": "hello", "id": "h", "token": paired.Token, "clientId": "e2e", "protocolVersion": 3, "caps": caps})
	p.WaitFor("hello.ok", func(m Message) bool { return m.Type == "hello.ok" })
	return p
}
