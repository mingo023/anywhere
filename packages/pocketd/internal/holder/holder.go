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

	conn net.Conn
	r    *bufio.Reader
	wmu  sync.Mutex
	qmu  sync.Mutex // one foreground query at a time
	fg   chan foreground

	mu     sync.Mutex
	cond   *sync.Cond
	queue  []byte
	ended  bool
	exited bool // the holder reported the exit, rather than going away
	code   int
	base   int64 // the output offset of the first byte Read returns
}

// Dial attaches to the holder on sock and reads its replay.
func Dial(sock string, timeout time.Duration) (*Client, error) {
	conn, err := net.DialTimeout("unix", sock, timeout)
	if err != nil {
		return nil, err
	}
	c := &Client{Sock: sock, conn: conn, r: bufio.NewReaderSize(conn, 64<<10), fg: make(chan foreground, 1), code: -1}
	c.cond = sync.NewCond(&c.mu)
	conn.SetReadDeadline(time.Now().Add(timeout))
	if err := c.replay(); err != nil {
		conn.Close()
		return nil, err
	}
	conn.SetReadDeadline(time.Time{})
	go c.demux()
	return c, nil
}

func (c *Client) replay() error {
	k, p, err := readFrame(c.r)
	if err != nil {
		return err
	}
	if k != kHello {
		return fmt.Errorf("holder: %c before hello", k)
	}
	if err := json.Unmarshal(p, &c.Hello); err != nil {
		return err
	}
	if c.Version < MinVersion {
		return fmt.Errorf("holder: version %d, this pocketd reads %d and later", c.Version, MinVersion)
	}
	for {
		k, p, err := readFrame(c.r)
		if err != nil {
			return err
		}
		switch k {
		case kSnapshot:
			c.Snapshot = p
		case kOutput:
			c.Tail = append(c.Tail, p...)
		case kLive:
			c.base = c.From + int64(len(c.Tail))
			return nil
		default:
			return fmt.Errorf("holder: %c during replay", k)
		}
	}
}

func (c *Client) demux() {
	for {
		k, p, err := readFrame(c.r)
		if err != nil {
			break
		}
		switch k {
		case kOutput:
			c.mu.Lock()
			c.queue = append(c.queue, p...)
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
		}
	}
	c.mu.Lock()
	c.ended = true
	c.cond.Broadcast()
	c.mu.Unlock()
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
	if err := c.send(kInput, p); err != nil {
		return 0, err
	}
	return len(p), nil
}

func (c *Client) Resize(cols, rows int) error {
	b, _ := json.Marshal(size{cols, rows})
	return c.send(kResize, b)
}

// Stop ends the whole session: SIGHUP and SIGTERM, then SIGKILL after grace.
func (c *Client) Stop(grace time.Duration) {
	b, _ := json.Marshal(stop{grace.Milliseconds()})
	c.send(kStop, b)
}

// Checkpoint gives the holder screen, the screen after the first read bytes Read returned.
func (c *Client) Checkpoint(read int64, screen []byte) {
	p := make([]byte, 8, 8+len(screen))
	binary.BigEndian.PutUint64(p, uint64(c.base+read))
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
	exited := c.exited || c.Hello.Exited
	c.mu.Unlock()
	if exited {
		c.send(kAck, nil)
	}
	return c.conn.Close()
}

// Find attaches to every holder in dir. A socket nobody answers on belongs to
// a holder that died, and goes. Two holders of one Terminal, as a crash in the
// middle of a Replace leaves, keep the newer; the older one's session ends.
func Find(dir string) []*Client {
	entries, _ := os.ReadDir(dir)
	newest := map[string]*Client{}
	for _, e := range entries {
		if !strings.HasSuffix(e.Name(), ".sock") {
			continue
		}
		sock := filepath.Join(dir, e.Name())
		c, err := Dial(sock, 2*time.Second)
		if err != nil {
			var op *net.OpError
			if errors.As(err, &op) && op.Op == "dial" {
				os.Remove(sock)
			}
			continue
		}
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
