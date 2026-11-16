package hosts

import (
	"bufio"
	"context"
	"errors"
	"io"
	"math/rand/v2"
	"os"
	"strings"
	"time"

	"pocketd/internal/ops"
)

type LinkState string

const (
	StateConnecting LinkState = "connecting"
	StateOnline     LinkState = "online"
	// StateRetrying tries again by itself.
	StateRetrying LinkState = "retrying"
	// StateFailed waits for a retry.
	StateFailed LinkState = "failed"
)

// Link is a host's connection.
type Link struct {
	State   LinkState
	Version string
	Uptime  uint64
	Failure Failure
}

// Backoff counts failures in a row; a link that reached pocketd starts over, so a flapping host retries quickly.
type Backoff struct{ attempt int }

func (b *Backoff) Reset() { b.attempt = 0 }

// Fail is the link to show after failure, and how long to wait before trying again; false waits for the user.
func (b *Backoff) Fail(failure Failure) (Link, time.Duration, bool) {
	wait, retry := failure.retryAfter(b.attempt)
	b.attempt++
	if retry {
		return Link{State: StateRetrying, Failure: failure}, wait, true
	}
	return Link{State: StateFailed, Failure: failure}, 0, false
}

// jitter shortens a wait by up to half so hosts that dropped together retry apart, but lengthens a throttled one: early retries extend sshd's penalty.
func jitter(wait time.Duration, failure Failure) time.Duration {
	random := float64(rand.IntN(1000)) / 2000
	if failure == FailThrottled {
		return time.Duration(float64(wait) * (1 + random))
	}
	return time.Duration(float64(wait) * (1 - random))
}

const (
	// readyWithin is how long a tunnel may take to forward once ssh signed in.
	readyWithin = 15 * time.Second
	// exitWithin is how long ssh may take to exit once its forward closed, before that means pocketd isn't there.
	exitWithin = 500 * time.Millisecond
	// checkEvery is how often an open link asks pocketd for its status, which also notices pocketd stopping.
	checkEvery = 10 * time.Second
)

type eventKind uint8

const (
	evStop eventKind = iota + 1
	evRetry
	// evReady and evExited carry the number of the tunnel they're about.
	evReady
	evExited
)

type event struct {
	kind   eventKind
	tunnel int
}

// next is the next event within wait (forever when negative), skipping ones from any tunnel but open; false when wait ends.
// ctx ending is evStop.
func next(ctx context.Context, events <-chan event, open int, wait time.Duration) (event, bool) {
	var timeout <-chan time.Time
	if wait >= 0 {
		timer := time.NewTimer(wait)
		defer timer.Stop()
		timeout = timer.C
	}
	for {
		select {
		case <-ctx.Done():
			return event{kind: evStop}, true
		case e := <-events:
			if (e.kind == evReady || e.kind == evExited) && e.tunnel != open {
				continue
			}
			return e, true
		case <-timeout:
			return event{}, false
		}
	}
}

type end uint8

const (
	endFailed end = iota
	endStop
	endRetry
	// endUnpaired is a Windows host's pocketd refusing the owner token; its file is gone, so the next connect pairs again.
	endUnpaired
	// endExited is ssh exiting; its stderr says why.
	endExited
)

func ended(e event) end {
	switch e.kind {
	case evStop:
		return endStop
	case evRetry:
		return endRetry
	}
	return endExited
}

// supervisor keeps one ssh tunnel to a host's pocketd up; done closes once ssh is gone and its socket removed.
type supervisor struct {
	m      *Manager
	id     string
	events chan event
	cancel context.CancelFunc
	done   chan struct{}
}

func (s *supervisor) retry() {
	select {
	case s.events <- event{kind: evRetry}:
	default:
	}
}

// send is a tunnel's reader reporting; it may outlive the supervisor.
func (s *supervisor) send(e event) {
	select {
	case s.events <- e:
	case <-s.done:
	}
}

func (s *supervisor) stop() {
	s.cancel()
	<-s.done
}

func (s *supervisor) run(ctx context.Context) {
	defer close(s.done)
	defer os.Remove(s.m.paths.Socket(s.id))
	var backoff Backoff
	tunnel, repaired := 0, false
	for {
		s.m.report(s, Link{State: StateConnecting})
		tunnel++
		e, failure := s.connect(ctx, tunnel, &backoff)
		switch {
		case e == endStop || ctx.Err() != nil:
			return
		case e == endRetry:
			backoff.Reset()
			repaired = false
			continue
		case e == endUnpaired && !repaired:
			repaired = true
			continue
		case e == endUnpaired:
			failure = FailUnpaired
		}
		link, wait, retry := backoff.Fail(failure)
		s.m.report(s, link)
		if retry {
			wait = jitter(wait, failure)
		} else {
			wait = -1
		}
		switch e, _ := next(ctx, s.events, 0, wait); e.kind {
		case evStop:
			return
		case evRetry:
			backoff.Reset()
			repaired = false
		}
	}
}

// connect forwards a socket here to pocketd on the host, and checks it until something ends the link.
func (s *supervisor) connect(ctx context.Context, tunnel int, backoff *Backoff) (end, Failure) {
	m := s.m
	h, ok := m.host(s.id)
	if !ok {
		return endStop, ""
	}
	probe := func(command string) (string, Failure) {
		stdout, _, _, err := m.run(ctx, h, command, nil)
		return string(stdout), failureOf(err)
	}
	var found Probe
	parsed, windows := false, h.Windows
	if !windows {
		out, failure := probe(probeCommand(m.paths.RemoteDir))
		if failure != "" {
			return endFailed, failure
		}
		found, parsed = parseProbe(out)
	}
	if !parsed {
		out, failure := probe(windowsProbeCommand(m.paths.RemoteDir))
		if failure != "" {
			return endFailed, failure
		}
		if found, parsed = parseWindowsProbe(out); !parsed {
			return endFailed, FailOther
		}
		windows = true
	}
	if !found.Running {
		return endFailed, FailNotRunning
	}
	local := m.paths.Socket(s.id)
	var fwd []string
	var token, command string
	var err error
	if windows {
		owner, ok := loadOwner(m.paths.Token(s.id))
		if !ok {
			var failure Failure
			if owner, failure = m.pair(ctx, h); failure != "" {
				return endFailed, failure
			}
		}
		fwd, err = tcpForward(local, int(owner.Port))
		token, command = owner.Token, windowsTunnelCommand()
	} else {
		fwd, err = forward(local, found.Home, m.paths.RemoteDir)
		command = tunnelCommand
	}
	if err != nil {
		return endFailed, failureOf(err)
	}
	cmd, err := m.command(context.Background(), m.sshArgs(h, fwd, command))
	if err != nil {
		return endFailed, FailOther
	}
	// stdin stays open for cat on the host: if pocketd dies, the pipe closes and ssh ends with it.
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return endFailed, FailOther
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return endFailed, FailOther
	}
	var stderr strings.Builder
	cmd.Stderr = &stderr
	if cmd.Start() != nil {
		return endFailed, FailOther
	}
	go func() {
		lines := bufio.NewScanner(stdout)
		for lines.Scan() {
			if lines.Text() == "READY" {
				s.send(event{evReady, tunnel})
			}
		}
		s.send(event{evExited, tunnel})
	}()
	e, failure := s.watch(ctx, local, token, tunnel, backoff)
	// Closing stdin ends cat on the host, and ssh with it, before the kill makes sure.
	stdin.Close()
	cmd.Process.Kill()
	cmd.Wait()
	switch e {
	case endUnpaired:
		os.Remove(m.paths.Token(s.id))
	case endExited:
		return endFailed, classify(stderr.String())
	}
	return e, failure
}

// watch waits for the tunnel, then checks pocketd through it.
func (s *supervisor) watch(ctx context.Context, socket, token string, tunnel int, backoff *Backoff) (end, Failure) {
	switch e, ok := next(ctx, s.events, tunnel, readyWithin); {
	case !ok:
		return endFailed, FailTimedOut
	case e.kind != evReady:
		return ended(e), ""
	}
	for {
		st, err := status(socket, token)
		switch {
		case err == nil:
			backoff.Reset()
			if st.OS == "windows" {
				s.m.setWindows(s.id, true)
			}
			s.m.report(s, Link{State: StateOnline, Version: st.Version, Uptime: uint64(max(st.Uptime, 0))})
		case errors.Is(err, errClosed):
			// The forward closes at once when nothing listens on the host, but also while ssh exits on a lost link.
			e, ok := next(ctx, s.events, tunnel, exitWithin)
			if !ok || e.kind == evReady {
				return endFailed, FailNotRunning
			}
			return ended(e), ""
		case errors.Is(err, errNotOwner):
			return endUnpaired, ""
		}
		// A quiet link times out instead; ServerAlive ends ssh if it's gone, which ends this wait.
		if e, ok := next(ctx, s.events, tunnel, checkEvery); ok && e.kind != evReady {
			return ended(e), ""
		}
	}
}

var (
	errClosed = errors.New("pocketd closed the connection")
	// errNotOwner is what pocketd answers an auth whose token it doesn't know.
	errNotOwner = errors.New("not an owner token")
)

func status(socket, token string) (ops.Status, error) {
	reply, err := request(socket, token, ops.Msg{Op: "status"})
	if err == nil && reply.Status == nil {
		err = errors.New("no status")
	}
	if err != nil {
		return ops.Status{}, err
	}
	return *reply.Status, nil
}

// request sends msg to the pocketd at socket, after token when its owner port wants one, and waits up to 5s for the reply.
func request(socket, token string, msg ops.Msg) (ops.Msg, error) {
	c, err := ops.Dial(socket)
	if err != nil {
		return ops.Msg{}, err
	}
	defer c.Close()
	// ops.Conn has no deadline; closing it ends a Recv still waiting.
	timer := time.AfterFunc(5*time.Second, func() { c.Close() })
	defer timer.Stop()
	if token != "" {
		c.Send(ops.Msg{Op: "auth", Token: token})
		if _, err := recv(c); err != nil {
			return ops.Msg{}, err
		}
	}
	c.Send(msg)
	return recv(c)
}

func recv(c *ops.Conn) (ops.Msg, error) {
	m, err := c.Recv()
	switch {
	case errors.Is(err, io.EOF):
		return m, errClosed
	case err != nil:
		return m, err
	case m.Ev == "error" && m.Error == errNotOwner.Error():
		return m, errNotOwner
	case m.Ev == "error":
		return m, errors.New(m.Error)
	}
	return m, nil
}
