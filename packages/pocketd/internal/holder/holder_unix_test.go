//go:build unix

package holder

import (
	"bufio"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"testing"
	"time"
)

// serve runs a holder for sh -c script in this process.
func serve(t *testing.T, script string) (sock string, done chan error) {
	t.Helper()
	dir, err := os.MkdirTemp("/tmp", "hold")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(dir) })
	sock = filepath.Join(dir, "t.sock")
	r, w := io.Pipe()
	done = make(chan error, 1)
	go func() {
		done <- Serve(Spec{ID: "t1", Cmd: "sh", Bin: "/bin/sh", Args: []string{"-c", script}, Cwd: dir, Env: os.Environ(), Cols: 80, Rows: 24, Sock: sock}, nil, w)
	}()
	line, err := bufio.NewReader(r).ReadString('\n')
	if !strings.Contains(line, "pid") {
		t.Fatalf("holder not ready: %q, %v", line, err)
	}
	return sock, done
}

func dial(t *testing.T, sock string) *Client {
	t.Helper()
	c, err := Dial(sock, 2*time.Second)
	if err != nil {
		t.Fatal(err)
	}
	return c
}

// readUntil reads c until it has shown want, and returns all it read.
func readUntil(t *testing.T, c *Client, want string) string {
	t.Helper()
	got := string(c.Tail)
	buf := make([]byte, 4096)
	deadline := time.AfterFunc(5*time.Second, func() { c.conn.Close() })
	defer deadline.Stop()
	for !strings.Contains(got, want) {
		n, err := c.Read(buf)
		got += string(buf[:n])
		if err != nil {
			t.Fatalf("read ended before %q: %v; got %q", want, err, got)
		}
	}
	return got
}

func TestAHolderRelaysInputAndOutputAndKeepsTheExitCodeUntilSeen(t *testing.T) {
	sock, done := serve(t, `echo ready; read x; echo "got $x"; exit 3`)
	c := dial(t, sock)
	readUntil(t, c, "ready")
	c.Write([]byte("abc\r"))
	readUntil(t, c, "got abc")
	if code := c.Wait(); code != 3 {
		t.Fatalf("exit code %d", code)
	}
	if _, err := c.Read(make([]byte, 1)); err != io.EOF {
		t.Fatalf("read after exit: %v", err)
	}
	select {
	case <-done:
		t.Fatal("the holder left before its exit was seen")
	case <-time.After(100 * time.Millisecond):
	}
	c.Close()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("the holder stayed after its exit was seen")
	}
	if _, err := os.Stat(sock); !os.IsNotExist(err) {
		t.Fatalf("socket left behind: %v", err)
	}
}

func TestAClientThatComesBackGetsTheLastCheckpointAndTheOutputSinceIt(t *testing.T) {
	sock, _ := serve(t, `echo one; read x; echo two; read y`)
	c := dial(t, sock)
	readUntil(t, c, "one")
	c.Checkpoint([]byte("SCREEN"))
	c.Write([]byte("go\r"))
	readUntil(t, c, "two")
	c.conn.Close()

	again := dial(t, sock)
	defer again.Stop(0)
	if string(again.Snapshot) != "SCREEN" {
		t.Fatalf("snapshot %q", again.Snapshot)
	}
	if tail := string(again.Tail); !strings.Contains(tail, "two") || strings.Contains(tail, "one") {
		t.Fatalf("tail %q", tail)
	}
	if again.ID != "t1" || again.Cmd != "sh" || again.Exited {
		t.Fatalf("hello %+v", again.Hello)
	}
}

func TestTheProcessSeesItsHolderAsTheParentPocketdReapsBy(t *testing.T) {
	sock, _ := serve(t, `echo "parent=$POCKETD_PARENT"; read x`)
	c := dial(t, sock)
	defer c.Stop(0)
	readUntil(t, c, "parent="+strconv.Itoa(os.Getpid()))
}

func TestStopEndsTheSessionAndAQueryNamesItsForeground(t *testing.T) {
	sock, _ := serve(t, `echo ready; sleep 60`)
	c := dial(t, sock)
	readUntil(t, c, "ready")
	if pg, err := c.Foreground(); err != nil || pg <= 0 {
		t.Fatalf("foreground %d, %v", pg, err)
	}
	c.Stop(0)
	if code := c.Wait(); code == 0 {
		t.Fatalf("exit code %d after a stop", code)
	}
	c.Close()
}

func TestFindDropsSocketsNobodyAnswersOn(t *testing.T) {
	sock, _ := serve(t, `echo ready; read x`)
	dead := filepath.Join(filepath.Dir(sock), "dead.sock")
	os.WriteFile(dead, nil, 0o600)
	found := Find(filepath.Dir(sock))
	if len(found) != 1 || found[0].ID != "t1" {
		t.Fatalf("found %d", len(found))
	}
	found[0].Stop(0)
	if _, err := os.Stat(dead); !os.IsNotExist(err) {
		t.Fatalf("dead socket kept: %v", err)
	}
}

func TestAClientThatFindsTheProcessEndedLetsTheHolderGo(t *testing.T) {
	sock, done := serve(t, `exit 3`)
	for deadline := time.Now().Add(5 * time.Second); ; time.Sleep(20 * time.Millisecond) {
		c := dial(t, sock)
		c.conn.Close()
		if c.Exited {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("the process never ended")
		}
	}
	func() {
		// One thread, so the client's reader hasn't seen the exit frame when Close runs.
		defer runtime.GOMAXPROCS(runtime.GOMAXPROCS(1))
		dial(t, sock).Close()
	}()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("the holder stayed after its exit was seen")
	}
}
