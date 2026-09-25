package e2e

import (
	"bytes"
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"pocketd/internal/ops"
)

var binDir string

func TestMain(m *testing.M) {
	dir, err := os.MkdirTemp("", "pocketd-e2e")
	if err != nil {
		panic(err)
	}
	for _, b := range [][2]string{{"pocketd", "../cmd/pocketd"}, {"fake/claude", "./fakeclaude"}, {"fake/codex", "./fakecodex"}} {
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
	log       *bytes.Buffer
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
func Start(t *testing.T) *Harness {
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
		Port:      freePort(t),
		Token:     "test-token",
		log:       &bytes.Buffer{},
	}
	config := fmt.Sprintf(`{"token":%q,"port":%d}`, h.Token, h.Port)
	if err := os.WriteFile(filepath.Join(home, "config.json"), []byte(config), 0o600); err != nil {
		t.Fatal(err)
	}
	h.Env = append(os.Environ(),
		"POCKET_HOME="+home,
		"POCKETD_SOCK="+h.Sock,
		"CLAUDE_CONFIG_DIR="+h.ClaudeDir,
		"PATH="+filepath.Join(binDir, "fake")+":"+os.Getenv("PATH"),
	)
	cmd := exec.Command(filepath.Join(binDir, "pocketd"), "serve")
	cmd.Env = h.Env
	cmd.Stdout, cmd.Stderr = h.log, h.log
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		cmd.Process.Kill()
		cmd.Wait()
		if t.Failed() {
			t.Logf("pocketd output:\n%s", h.log)
		}
	})
	h.eventually("ops socket", func() bool {
		c, err := ops.Dial(h.Sock)
		if err == nil {
			c.Close()
		}
		return err == nil
	})
	return h
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

// Spawn starts cmd the way `pocketd run` would from h.Home and returns the session id.
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

// StartClaude starts pocketd with a phone connected and a ready claude.
func StartClaude(t *testing.T) (*Harness, *Phone, string) {
	t.Helper()
	h := Start(t)
	phone := h.Phone()
	return h, phone, h.SpawnReady("claude")
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
