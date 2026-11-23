//go:build unix

package holder

import (
	"bufio"
	"encoding/binary"
	"encoding/json"
	"errors"
	"io"
	"net"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/creack/pty"

	"pocketd/internal/proc"
)

const (
	// RingSize is how much recent output a holder keeps for a pocketd that comes back.
	RingSize = 4 << 20
	// Linger is how long a holder whose process ended waits for a pocketd to see it.
	Linger = 24 * time.Hour
)

type server struct {
	spec    Spec
	pty     *os.File
	pid     int
	started int64
	done    chan struct{}
	once    sync.Once
	stops   sync.WaitGroup
	typed   chan struct{}

	inMu  sync.Mutex
	input []byte

	mu         sync.Mutex
	cols, rows int
	ring       []byte
	n          int64 // output read so far
	screen     []byte
	at         int64 // the output offset screen stands for
	client     net.Conn
	exited     bool
	code       int
}

// Serve runs spec's process on a new pty, or takes over adopted, and
// serves its socket until a client has seen it end. ready gets one JSON line:
// the process's pid, or why it didn't start.
func Serve(spec Spec, adopted *os.File, ready io.Writer) error {
	ln, err := listen(spec.Sock)
	if err != nil {
		json.NewEncoder(ready).Encode(map[string]string{"error": err.Error()})
		return err
	}
	defer os.Remove(spec.Sock)
	defer ln.Close()
	s := &server{spec: spec, started: time.Now().UnixNano(), done: make(chan struct{}), typed: make(chan struct{}, 1), cols: spec.Cols, rows: spec.Rows}
	wait := func() int { return -1 }
	if adopted != nil {
		s.pty, s.pid = adopted, spec.AdoptPid
	} else {
		cmd := exec.Command(spec.Bin, spec.Args...)
		cmd.Dir, cmd.Env = spec.Cwd, marked(spec.Env)
		if s.pty, err = pty.StartWithSize(cmd, &pty.Winsize{Cols: uint16(spec.Cols), Rows: uint16(spec.Rows)}); err != nil {
			json.NewEncoder(ready).Encode(map[string]string{"error": err.Error()})
			return err
		}
		s.pid = cmd.Process.Pid
		wait = func() int {
			if st, err := cmd.Process.Wait(); err == nil {
				return st.ExitCode()
			}
			return -1
		}
	}
	json.NewEncoder(ready).Encode(map[string]int{"pid": s.pid})
	if c, ok := ready.(io.Closer); ok {
		c.Close()
	}
	go s.pump(wait)
	go s.writeInput()
	go func() {
		for {
			conn, err := ln.Accept()
			if err != nil {
				return
			}
			s.attach(conn)
		}
	}()
	<-s.done
	// A stop's SIGKILL is still due to what ignored its SIGTERM.
	s.stops.Wait()
	return nil
}

// marked is env with POCKETD_PARENT naming this holder, so pocketd's reap
// spares the process's children while the holder lives.
func marked(env []string) []string {
	out := make([]string, 0, len(env)+1)
	for _, kv := range env {
		if !strings.HasPrefix(kv, proc.Marker+"=") {
			out = append(out, kv)
		}
	}
	return append(out, proc.Marker+"="+strconv.Itoa(os.Getpid()))
}

// listen takes sock, unless a live holder answers on it.
func listen(sock string) (net.Listener, error) {
	if c, err := net.DialTimeout("unix", sock, time.Second); err == nil {
		c.Close()
		return nil, errors.New("holder: socket in use: " + sock)
	}
	os.Remove(sock)
	ln, err := net.Listen("unix", sock)
	if err != nil {
		return nil, err
	}
	os.Chmod(sock, 0o600)
	return ln, nil
}

func (s *server) finish() { s.once.Do(func() { close(s.done) }) }

// sendLocked writes to the client, dropping one that doesn't take it: it
// reconnects and replays what it missed.
func (s *server) sendLocked(k kind, p []byte) {
	if s.client == nil {
		return
	}
	s.client.SetWriteDeadline(time.Now().Add(5 * time.Second))
	if err := writeFrame(s.client, k, p); err != nil {
		s.client.Close()
		s.client = nil
	}
}

// writeInput writes what clients typed apart from the frame reader, which a
// process that stops reading would otherwise block, stops included.
func (s *server) writeInput() {
	for {
		select {
		case <-s.typed:
		case <-s.done:
			return
		}
		s.inMu.Lock()
		p := s.input
		s.input = nil
		s.inMu.Unlock()
		s.pty.Write(p)
	}
}

func (s *server) pump(wait func() int) {
	buf := make([]byte, 32<<10)
	for {
		n, err := s.pty.Read(buf)
		if n > 0 {
			s.mu.Lock()
			s.ring = append(s.ring, buf[:n]...)
			// Trimmed in halves, so each byte is copied at most once more.
			if len(s.ring) > 2*RingSize {
				s.ring = append(s.ring[:0:0], s.ring[len(s.ring)-RingSize:]...)
			}
			s.n += int64(n)
			s.sendLocked(kOutput, buf[:n])
			s.mu.Unlock()
		}
		if err != nil {
			break
		}
	}
	code := wait()
	s.mu.Lock()
	s.exited, s.code = true, code
	b, _ := json.Marshal(code)
	s.sendLocked(kExit, b)
	s.mu.Unlock()
	s.pty.Close()
	time.AfterFunc(Linger, s.finish)
}

// attach makes conn the client, in place of any before it, and replays the
// screen to it: the last checkpoint and the output since.
func (s *server) attach(conn net.Conn) {
	s.mu.Lock()
	if s.client != nil {
		s.client.Close()
	}
	s.client = conn
	start := s.n - int64(len(s.ring))
	from := max(s.at, start)
	hello, _ := json.Marshal(Hello{Version: Version, ID: s.spec.ID, Cmd: s.spec.Cmd, Args: s.spec.Args, Cwd: s.spec.Cwd, Local: s.spec.Local, SignIn: s.spec.SignIn,
		Cols: s.cols, Rows: s.rows, Pid: s.pid, Started: s.started, Exited: s.exited, Code: s.code, From: from})
	s.sendLocked(kHello, hello)
	s.sendLocked(kSnapshot, s.screen)
	if tail := s.ring[from-start:]; len(tail) > 0 {
		s.sendLocked(kOutput, tail)
	}
	s.sendLocked(kLive, nil)
	if s.exited {
		b, _ := json.Marshal(s.code)
		s.sendLocked(kExit, b)
	}
	s.mu.Unlock()
	go s.serve(conn)
}

func (s *server) serve(conn net.Conn) {
	r := bufio.NewReader(conn)
	for {
		k, p, err := readFrame(r)
		if err != nil {
			break
		}
		switch k {
		case kInput:
			s.inMu.Lock()
			s.input = append(s.input, p...)
			s.inMu.Unlock()
			select {
			case s.typed <- struct{}{}:
			default:
			}
		case kResize:
			var sz size
			if json.Unmarshal(p, &sz) == nil {
				s.mu.Lock()
				s.cols, s.rows = sz.Cols, sz.Rows
				s.mu.Unlock()
				pty.Setsize(s.pty, &pty.Winsize{Cols: uint16(sz.Cols), Rows: uint16(sz.Rows)})
			}
		case kStop:
			var st stop
			json.Unmarshal(p, &st)
			s.stops.Go(func() { proc.EndSession(s.pid, time.Duration(st.GraceMS)*time.Millisecond) })
		case kCheckpoint:
			if len(p) >= 8 {
				at := int64(binary.BigEndian.Uint64(p))
				s.mu.Lock()
				if at <= s.n && at >= s.at {
					s.screen, s.at = append([]byte(nil), p[8:]...), at
				}
				s.mu.Unlock()
			}
		case kQuery:
			var f foreground
			pg, err := proc.Foreground(s.pty)
			if f.Pgid = pg; err != nil {
				f.Error = err.Error()
			}
			b, _ := json.Marshal(f)
			s.mu.Lock()
			if s.client == conn {
				s.sendLocked(kForeground, b)
			}
			s.mu.Unlock()
		case kAck:
			s.mu.Lock()
			exited := s.exited
			s.mu.Unlock()
			if exited {
				s.finish()
			}
		}
	}
	s.mu.Lock()
	if s.client == conn {
		s.client = nil
	}
	s.mu.Unlock()
	conn.Close()
}
