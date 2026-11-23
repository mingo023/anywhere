//go:build unix

package holder

import (
	"bufio"
	"encoding/json"
	"io"
	"net"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"syscall"
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

// vanish drops c as a pocketd that died would: no redial, and no ack of an exit.
func vanish(c *Client) {
	c.mu.Lock()
	c.closing = true
	c.mu.Unlock()
	c.conn.Close()
}

// readUntil reads c until it has shown want, and returns all it read.
func readUntil(t *testing.T, c *Client, want string) string {
	t.Helper()
	got := string(c.Tail)
	buf := make([]byte, 4096)
	deadline := time.AfterFunc(5*time.Second, func() { vanish(c) })
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
	read := readUntil(t, c, "one")
	c.Checkpoint(int64(len(read)-len(c.Tail)), []byte("SCREEN"))
	c.Write([]byte("go\r"))
	readUntil(t, c, "two")
	vanish(c)

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
		vanish(c)
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

func TestAHolderStaysUntilAStopHasKilledWhatIgnoredIt(t *testing.T) {
	sock, done := serve(t, `sh -c 'trap "" HUP TERM; echo bg=$$; exec sleep 60' </dev/null 2>/dev/null & sleep 0.2; echo up; trap 'exit 0' HUP TERM; while :; do sleep 0.1; done`)
	c := dial(t, sock)
	got := readUntil(t, c, "up")
	bg, _ := strconv.Atoi(strings.TrimSpace(strings.SplitN(strings.SplitN(got, "bg=", 2)[1], "\n", 2)[0]))
	t.Cleanup(func() { syscall.Kill(bg, syscall.SIGKILL) })
	c.Stop(time.Second)
	c.Wait()
	c.Close()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("the holder never left")
	}
	for deadline := time.Now().Add(time.Second); syscall.Kill(bg, 0) == nil; time.Sleep(20 * time.Millisecond) {
		if time.Now().After(deadline) {
			t.Fatal("the holder left while what ignored the stop still ran")
		}
	}
}

func TestAStopGetsThroughInputTheProcessDoesntRead(t *testing.T) {
	sock, _ := serve(t, `stty raw -echo; echo ready; sleep 60`)
	c := dial(t, sock)
	readUntil(t, c, "ready")
	c.Write(make([]byte, 1<<20))
	c.Stop(0)
	exited := make(chan int, 1)
	go func() { exited <- c.Wait() }()
	select {
	case <-exited:
	case <-time.After(5 * time.Second):
		t.Fatal("the stop waited behind the input")
	}
	c.Close()
}

func TestAClientWhoseConnectionDropsRedialsAndMissesNothing(t *testing.T) {
	sock, _ := serve(t, `echo one; read x; echo "two $x"; read y; exit 4`)
	c := dial(t, sock)
	readUntil(t, c, "one")
	// What a holder does to a client that took more than its write deadline.
	c.conn.Close()
	for deadline := time.Now().Add(5 * time.Second); ; time.Sleep(20 * time.Millisecond) {
		if _, err := c.Write([]byte("go\r")); err == nil {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("the client never came back")
		}
	}
	got := readUntil(t, c, "two go")
	if strings.Contains(got, "one") {
		t.Fatalf("output replayed twice: %q", got)
	}
	c.Write([]byte("y\r"))
	if code := c.Wait(); code != 4 {
		t.Fatalf("exit code %d", code)
	}
	c.Close()
}

func TestAClientAnotherOneReplacedStopsInsteadOfTakingTheHolderBack(t *testing.T) {
	sock, _ := serve(t, `echo ready; sleep 60`)
	first := dial(t, sock)
	readUntil(t, first, "ready")
	second := dial(t, sock)
	defer second.Stop(0)
	done := make(chan error, 1)
	go func() {
		_, err := io.ReadAll(first)
		done <- err
	}()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		vanish(first)
		t.Fatal("the replaced client kept reading")
	}
	if pg, err := second.Foreground(); err != nil || pg <= 0 {
		t.Fatalf("the newer client lost the holder: %d, %v", pg, err)
	}
}

func TestAProcessKilledByASignalExitsWith128PlusIt(t *testing.T) {
	sock, _ := serve(t, `kill -9 $$`)
	c := dial(t, sock)
	defer c.Close()
	if code := c.Wait(); code != 128+9 {
		t.Fatalf("exit code %d", code)
	}
}

func TestFindWaitsForAHolderSlowToAnswer(t *testing.T) {
	dir, err := os.MkdirTemp("/tmp", "hold")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(dir) })
	ln, err := net.Listen("unix", filepath.Join(dir, "slow.sock"))
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()
	go func() {
		conn, err := ln.Accept()
		if err != nil {
			return
		}
		defer conn.Close()
		// A holder on a Mac too busy to answer for seconds.
		time.Sleep(3 * time.Second)
		hello, _ := json.Marshal(Hello{Version: Version, ID: "slow", Pid: 1})
		writeFrame(conn, kHello, hello)
		writeFrame(conn, kLive, nil)
		io.Copy(io.Discard, conn)
	}()
	found := Find(dir)
	if len(found) != 1 || found[0].ID != "slow" {
		t.Fatalf("found %d", len(found))
	}
	vanish(found[0])
}

func TestAClientTheHoldersRingNoLongerReachesRedrawsFromTheCheckpoint(t *testing.T) {
	sock, _ := serve(t, `echo ready; sleep 60`)
	c := dial(t, sock)
	defer c.Stop(0)
	readUntil(t, c, "ready")
	c.mu.Lock()
	queued, base := c.queued, c.base
	c.mu.Unlock()
	mine, theirs := net.Pipe()
	defer theirs.Close()

	c.resume(mine, replayed{Hello: Hello{From: queued + 100}, snapshot: []byte("SCREEN"), tail: []byte("later")})

	c.mu.Lock()
	defer c.mu.Unlock()
	if got := string(c.queue); !strings.HasSuffix(got, "SCREENlater") {
		t.Fatalf("queue %q", got)
	}
	// What Read hands on lines up with the holder's offsets again.
	if end := c.base + (queued - base) + int64(len("SCREENlater")); end != queued+100+int64(len("later")) || c.queued != end {
		t.Fatalf("base %d, queued %d", c.base, c.queued)
	}
}
