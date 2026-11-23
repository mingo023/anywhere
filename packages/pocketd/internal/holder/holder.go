// Package holder runs a Terminal's process outside pocketd, so it outlives a
// pocketd that restarts, upgrades or crashes (ADR 0008). `pocketd hold` owns
// the pty and the process on it; pocketd is its client over a unix socket.
package holder

import (
	"bufio"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"time"
)

// Version is the protocol this release speaks; MinVersion is the oldest
// holder it still reattaches, since holders outlive upgrades.
const (
	Version    = 1
	MinVersion = 1
)

// Dir is where home's holders put their sockets.
func Dir(home string) string { return filepath.Join(home, "holders") }

// Starter starts holders from the pocketd binary at Exe, with sockets in Dir.
type Starter struct{ Exe, Dir string }

// Spec is what `pocketd hold` reads on stdin.
type Spec struct {
	ID     string   `json:"id"`
	Cmd    string   `json:"cmd"`
	Bin    string   `json:"bin"`
	Args   []string `json:"args,omitempty"`
	Cwd    string   `json:"cwd"`
	Env    []string `json:"env"`
	Cols   int      `json:"cols"`
	Rows   int      `json:"rows"`
	Local  string   `json:"local,omitempty"`
	SignIn bool     `json:"signin,omitempty"`
	Sock   string   `json:"sock"`
	// AdoptPid is the process on the pty handed in as fd 3, which a pocketd before holders started.
	AdoptPid int `json:"adoptPid,omitempty"`
}

// Hello is the holder's first frame to each client.
type Hello struct {
	Version int      `json:"version"`
	ID      string   `json:"id"`
	Cmd     string   `json:"cmd"`
	Args    []string `json:"args,omitempty"`
	Cwd     string   `json:"cwd"`
	Local   string   `json:"local,omitempty"`
	SignIn  bool     `json:"signin,omitempty"`
	Cols    int      `json:"cols"`
	Rows    int      `json:"rows"`
	Pid     int      `json:"pid"`
	Started int64    `json:"started"`
	Exited  bool     `json:"exited,omitempty"`
	Code    int      `json:"code,omitempty"`
	// From is the output offset of the first byte replayed after the snapshot.
	From int64 `json:"from"`
}

type kind byte

const (
	kHello      kind = 'H' // holder → client: Hello
	kSnapshot   kind = 'C' // holder → client: the last checkpoint's screen
	kOutput     kind = 'O' // holder → client: output
	kLive       kind = 'L' // holder → client: the replay is over
	kExit       kind = 'X' // holder → client: the exit code
	kForeground kind = 'F' // holder → client: answers kQuery
	kInput      kind = 'I' // client → holder: bytes for the process
	kResize     kind = 'R'
	kStop       kind = 'S' // ends the session, after a grace in ms
	kCheckpoint kind = 'K' // an 8-byte output offset, then the screen at it
	kQuery      kind = 'Q' // asks the pty's foreground process group
	kAck        kind = 'A' // the exit is seen: the holder may go
	kBye        kind = 'B' // holder → client: another client took over
)

const maxFrame = 64 << 20

func writeFrame(w io.Writer, k kind, p []byte) error {
	var head [5]byte
	head[0] = byte(k)
	binary.BigEndian.PutUint32(head[1:], uint32(len(p)))
	_, err := w.Write(append(head[:], p...))
	return err
}

func readFrame(r *bufio.Reader) (kind, []byte, error) {
	var head [5]byte
	if _, err := io.ReadFull(r, head[:]); err != nil {
		return 0, nil, err
	}
	n := binary.BigEndian.Uint32(head[1:])
	if n > maxFrame {
		return 0, nil, fmt.Errorf("holder: frame of %d bytes", n)
	}
	p := make([]byte, n)
	if _, err := io.ReadFull(r, p); err != nil {
		return 0, nil, err
	}
	return kind(head[0]), p, nil
}

type size struct {
	Cols int `json:"cols"`
	Rows int `json:"rows"`
}

type stop struct {
	GraceMS int64 `json:"graceMs"`
}

type foreground struct {
	Pgid  int    `json:"pgid"`
	Error string `json:"error,omitempty"`
}

// Client is pocketd's end of one holder. Read, Write and the rest make it a
// Terminal's session.
type Client struct {
	Hello
	Sock string
	// Snapshot and Tail redraw the screen as it stands: the last checkpoint and the output after it.
	Snapshot, Tail []byte

	wmu  sync.Mutex
	conn net.Conn // swapped under wmu when the client redials
	r    *bufio.Reader
	qmu  sync.Mutex // one foreground query at a time
	fg   chan foreground

	mu      sync.Mutex
	cond    *sync.Cond
	queue   []byte
	ended   bool
	exited  bool // the holder reported the exit, rather than going away
	closing bool // Close ran, so the connection ending is no drop to redial
	code    int
	base    int64  // the output offset of the first byte Read returns
	queued  int64  // the output offset after the last byte queued
	sized   []byte // the last resize and stop asked, sent again to a holder redialed
	stopped []byte
	typed   []byte // input the dropped connection didn't take, sent once redialed
}

// The holder drops a client that takes more than its write deadline, as a
// pocketd on a loaded Mac may; the client dials it again until it answers.
const (
	redialFirst = 100 * time.Millisecond
	redialMax   = 2 * time.Second
	redialWait  = 10 * time.Second
	// findWait is how long Find gives each holder to replay: a busy Mac may take seconds.
	findWait = 10 * time.Second
)

var errReplaced = errors.New("holder: another client took over")

type replayed struct {
	Hello
	snapshot, tail []byte
}

// Dial attaches to the holder on sock and reads its replay.
func Dial(sock string, timeout time.Duration) (*Client, error) {
	conn, r, rp, err := attach(sock, timeout)
	if err != nil {
		return nil, err
	}
	base := rp.From + int64(len(rp.tail))
	c := &Client{Hello: rp.Hello, Sock: sock, Snapshot: rp.snapshot, Tail: rp.tail, conn: conn, r: r, fg: make(chan foreground, 1), code: -1, base: base, queued: base}
	c.cond = sync.NewCond(&c.mu)
	go c.demux()
	return c, nil
}

func attach(sock string, timeout time.Duration) (net.Conn, *bufio.Reader, replayed, error) {
	conn, err := net.DialTimeout("unix", sock, timeout)
	if err != nil {
		return nil, nil, replayed{}, err
	}
	r := bufio.NewReaderSize(conn, 64<<10)
	conn.SetReadDeadline(time.Now().Add(timeout))
	rp, err := replay(r)
	if err != nil {
		conn.Close()
		return nil, nil, replayed{}, err
	}
	conn.SetReadDeadline(time.Time{})
	return conn, r, rp, nil
}

func replay(r *bufio.Reader) (replayed, error) {
	var rp replayed
	k, p, err := readFrame(r)
	if err != nil {
		return rp, err
	}
	if k != kHello {
		return rp, fmt.Errorf("holder: %c before hello", k)
	}
	if err := json.Unmarshal(p, &rp.Hello); err != nil {
		return rp, err
	}
	if rp.Version < MinVersion {
		return rp, fmt.Errorf("holder: version %d, this pocketd reads %d and later", rp.Version, MinVersion)
	}
	for {
		k, p, err := readFrame(r)
		if err != nil {
			return rp, err
		}
		switch k {
		case kSnapshot:
			rp.snapshot = p
		case kOutput:
			rp.tail = append(rp.tail, p...)
		case kLive:
			return rp, nil
		case kBye:
			return rp, errReplaced
		default:
			return rp, fmt.Errorf("holder: %c during replay", k)
		}
	}
}

func (c *Client) demux() {
	for r := c.r; r != nil; r = c.redial() {
		if errors.Is(c.relay(r), errReplaced) {
			break
		}
	}
	c.mu.Lock()
	c.ended = true
	c.cond.Broadcast()
	c.mu.Unlock()
}

func (c *Client) relay(r *bufio.Reader) error {
	for {
		k, p, err := readFrame(r)
		if err != nil {
			return err
		}
		switch k {
		case kOutput:
			c.mu.Lock()
			c.queue = append(c.queue, p...)
			c.queued += int64(len(p))
			c.cond.Broadcast()
			c.mu.Unlock()
		case kExit:
			var code int
			json.Unmarshal(p, &code)
			c.mu.Lock()
			c.exited, c.code = true, code
			c.cond.Broadcast()
			c.mu.Unlock()
		case kForeground:
			var f foreground
			json.Unmarshal(p, &f)
			select {
			case c.fg <- f:
			default:
			}
		case kBye:
			return errReplaced
		}
	}
}

// redial attaches to the holder again, and returns nil once there is nothing
// to come back to: the holder is gone, the exit was seen, or Close ran.
func (c *Client) redial() *bufio.Reader {
	for wait := redialFirst; ; wait = min(2*wait, redialMax) {
		c.mu.Lock()
		over := c.exited || c.closing && c.stopped == nil
		c.mu.Unlock()
		if over {
			return nil
		}
		conn, r, rp, err := attach(c.Sock, redialWait)
		if err == nil {
			if c.resume(conn, rp) {
				return r
			}
			return nil
		}
		if errors.Is(err, errReplaced) || errors.Is(err, syscall.ENOENT) || errors.Is(err, syscall.ECONNREFUSED) {
			return nil
		}
		time.Sleep(wait)
	}
}

func (c *Client) resume(conn net.Conn, rp replayed) bool {
	c.wmu.Lock()
	defer c.wmu.Unlock()
	c.mu.Lock()
	if c.exited || c.closing {
		if c.stopped != nil && !c.exited {
			writeFrame(conn, kStop, c.stopped)
		}
		c.mu.Unlock()
		conn.Close()
		return false
	}
	if skip := c.queued - rp.From; skip >= 0 {
		if skip < int64(len(rp.tail)) {
			c.queue = append(c.queue, rp.tail[skip:]...)
		}
	} else {
		// The holder's ring no longer reaches what came last: redraw from its
		// checkpoint, on a cleared screen (RIS) rather than over the old one.
		read := c.queued - c.base - int64(len(c.queue))
		c.queue = append(append([]byte("\x1bc"), rp.snapshot...), rp.tail...)
		c.base = rp.From + int64(len(rp.tail)) - read - int64(len(c.queue))
	}
	c.queued = max(c.queued, rp.From+int64(len(rp.tail)))
	c.cond.Broadcast()
	sized, stopped, typed := c.sized, c.stopped, c.typed
	c.typed = nil
	c.mu.Unlock()
	c.conn.Close()
	c.conn = conn
	// Each may have gone to the dropped connection.
	if typed != nil {
		writeFrame(conn, kInput, typed)
	}
	if sized != nil {
		writeFrame(conn, kResize, sized)
	}
	if stopped != nil {
		writeFrame(conn, kStop, stopped)
	}
	return true
}

// Read returns output in order; io.EOF once the process ended or the holder went away.
func (c *Client) Read(p []byte) (int, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	for len(c.queue) == 0 && !c.ended && !c.exited {
		c.cond.Wait()
	}
	if len(c.queue) == 0 {
		return 0, io.EOF
	}
	n := copy(p, c.queue)
	c.queue = c.queue[n:]
	return n, nil
}

func (c *Client) send(k kind, p []byte) error {
	c.wmu.Lock()
	defer c.wmu.Unlock()
	c.conn.SetWriteDeadline(time.Now().Add(5 * time.Second))
	return writeFrame(c.conn, k, p)
}

func (c *Client) Write(p []byte) (int, error) {
	c.wmu.Lock()
	defer c.wmu.Unlock()
	c.conn.SetWriteDeadline(time.Now().Add(5 * time.Second))
	err := writeFrame(c.conn, kInput, p)
	if err == nil {
		return len(p), nil
	}
	// A timeout may have written part of the frame on a connection still up.
	if ne, ok := err.(net.Error); ok && ne.Timeout() {
		return 0, err
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.closing || c.exited || c.ended {
		return 0, err
	}
	c.typed = append(c.typed, p...)
	return len(p), nil
}

func (c *Client) Resize(cols, rows int) error {
	b, _ := json.Marshal(size{cols, rows})
	c.mu.Lock()
	c.sized = b
	c.mu.Unlock()
	return c.send(kResize, b)
}

// Stop ends the whole session: SIGHUP and SIGTERM, then SIGKILL after grace.
func (c *Client) Stop(grace time.Duration) {
	b, _ := json.Marshal(stop{grace.Milliseconds()})
	c.mu.Lock()
	c.stopped = b
	c.mu.Unlock()
	c.send(kStop, b)
}

// Checkpoint gives the holder screen, the screen after the first read bytes Read returned.
func (c *Client) Checkpoint(read int64, screen []byte) {
	c.mu.Lock()
	at := c.base + read
	c.mu.Unlock()
	p := make([]byte, 8, 8+len(screen))
	binary.BigEndian.PutUint64(p, uint64(at))
	c.send(kCheckpoint, append(p, screen...))
}

// Foreground is the pty's foreground process group; 0 means none.
func (c *Client) Foreground() (int, error) {
	c.qmu.Lock()
	defer c.qmu.Unlock()
	select {
	case <-c.fg:
	default:
	}
	if err := c.send(kQuery, nil); err != nil {
		return 0, err
	}
	select {
	case f := <-c.fg:
		if f.Error != "" {
			return 0, errors.New(f.Error)
		}
		return f.Pgid, nil
	case <-time.After(2 * time.Second):
		return 0, errors.New("holder: no answer about the foreground")
	}
}

func (c *Client) Pid() int { return c.Hello.Pid }

// Wait is the exit code once Read has ended; -1 when the holder went away without one.
func (c *Client) Wait() int {
	c.mu.Lock()
	defer c.mu.Unlock()
	for !c.ended && !c.exited {
		c.cond.Wait()
	}
	return c.code
}

// Close lets the holder go once its exit is seen, and drops the connection;
// a holder whose process runs keeps it running.
func (c *Client) Close() error {
	c.mu.Lock()
	c.closing = true
	exited := c.exited || c.Hello.Exited
	c.mu.Unlock()
	if exited {
		c.send(kAck, nil)
	}
	c.wmu.Lock()
	defer c.wmu.Unlock()
	return c.conn.Close()
}

// Find attaches to every holder in dir. A socket nobody answers on belongs to
// a holder that died, and goes. Two holders of one Terminal, as a crash in the
// middle of a Replace leaves, keep the newer; the older one's session ends.
func Find(dir string) []*Client {
	entries, _ := os.ReadDir(dir)
	var (
		wg  sync.WaitGroup
		mu  sync.Mutex
		all []*Client
	)
	for _, e := range entries {
		if !strings.HasSuffix(e.Name(), ".sock") {
			continue
		}
		sock := filepath.Join(dir, e.Name())
		wg.Go(func() {
			c, err := Dial(sock, findWait)
			if err != nil {
				var op *net.OpError
				if errors.As(err, &op) && op.Op == "dial" {
					os.Remove(sock)
				}
				return
			}
			mu.Lock()
			all = append(all, c)
			mu.Unlock()
		})
	}
	wg.Wait()
	newest := map[string]*Client{}
	for _, c := range all {
		if old := newest[c.ID]; old != nil {
			if old.Started > c.Started {
				old, c = c, old
			}
			old.Stop(0)
			go func() {
				old.Wait()
				old.Close()
			}()
		}
		newest[c.ID] = c
	}
	out := make([]*Client, 0, len(newest))
	for _, c := range newest {
		out = append(out, c)
	}
	return out
}

// EndAll ends the session of every holder in dir and returns once all have
// exited, or grace and a little more have passed.
func EndAll(dir string, grace time.Duration) {
	var wg sync.WaitGroup
	for _, c := range Find(dir) {
		wg.Go(func() {
			c.Stop(grace)
			done := make(chan struct{})
			go func() {
				c.Wait()
				close(done)
			}()
			select {
			case <-done:
			case <-time.After(grace + 2*time.Second):
			}
			c.Close()
		})
	}
	wg.Wait()
}
