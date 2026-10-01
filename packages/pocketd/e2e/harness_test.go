package e2e

import (
	"bytes"
	"errors"
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
		Token:     "test-token",
	}
	h.Env = append(os.Environ(),
		"HOME="+home,
		"POCKET_HOME="+home,
		"POCKETD_SOCK="+h.Sock,
		"CLAUDE_CONFIG_DIR="+h.ClaudeDir,
		"PATH="+filepath.Join(binDir, "fake")+":"+os.Getenv("PATH"),
	)
	// Another process can take the free port before pocketd listens on it.
	for try := 1; !h.serve(); try++ {
		if try == 5 || !strings.Contains(h.log.String(), "address already in use") {
			t.Fatal("pocketd exited")
		}
	}
	return h
}

// serve starts pocketd on a free port and reports whether it came up.
func (h *Harness) serve() bool {
	log := &bytes.Buffer{}
	h.Port, h.log = freePort(h.t), log
	config := fmt.Sprintf(`{"token":%q,"port":%d,"listen":"loopback"}`, h.Token, h.Port)
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
