package accounts

import (
	"bytes"
	"cmp"
	"context"
	"encoding/json"
	"errors"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"sync"
	"time"

	"pocketd/internal/envdir"
)

const (
	agyTimeout = 12 * time.Second // both runs together; agy's own --print-timeout is 10s
	agyFloor   = 5 * time.Minute  // from the start of the last finished probe
	agyMinimum = "1.1.11"
)

// agyOldNote is why a signed-in agy shows no usage: older ones send /usage to the model as a prompt.
const agyOldNote = "Update agy to " + agyMinimum + " or later to see its usage"

// agySignInLine starts what agy prints when it has no login.
const agySignInLine = "Authentication required"

var (
	errAgyFailed = errors.New("agy couldn't say its usage")
	errAgyUnread = errors.New("agy's usage isn't read yet")
)

// antigravity is agy's built-in login, the only one it has: it keeps its
// credential in the OS keyring, so pocketd reads its usage from its own
// /usage report and knows its sign-in only from that.
type antigravity struct{ noLogin }

func (antigravity) status(s *Store, _ string) (bool, bool) { return s.agy.signedIn(), false }

func (antigravity) usage(ctx context.Context, s *Store, _ string) (Usage, error) {
	return s.agy.probe(ctx, s)
}

type agyOutcome int

const (
	agyUnknown agyOutcome = iota
	agyMissing
	// agyOld is too old to ask, or answered /usage with something other than its own report.
	agyOld
	agySignedOut
	// agyAPIKey runs on a Gemini API key, which has no quota to report.
	agyAPIKey
	agyFailed
	agyOK
)

type agyRun struct {
	out    []byte
	code   int
	signIn bool
}

type agyProbe struct {
	run sync.Mutex // held across a probe: one agy at a time
	mu  sync.Mutex // guards what follows
	// at is when the last probe that ran agy started; the floor counts from it.
	at      time.Time
	outcome agyOutcome
	in      bool
	// last and err are what the last finished probe returned.
	last Usage
	err  error
	done bool
	// gate holds, by binary, whether it passed the version gate. One that
	// didn't, or answered with something other than its report, isn't run again.
	gate map[string]bool
	// forgot is when a sign-in last ended.
	forgot time.Time
	// found is whether the last probe found agy.
	found bool
}

func (p *agyProbe) installed() bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.found
}

func (p *agyProbe) signedIn() bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.in
}

func (p *agyProbe) note() string {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.outcome == agyOld {
		return agyOldNote
	}
	return ""
}

// forget lets the next refresh probe again, as after a sign-in.
func (p *agyProbe) forget() {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.at, p.forgot = time.Time{}, time.Now()
	if p.outcome == agySignedOut {
		p.outcome = agyUnknown
	}
}

func (p *agyProbe) probe(ctx context.Context, s *Store) (Usage, error) {
	p.run.Lock()
	defer p.run.Unlock()
	start := time.Now()
	if u, err, ok := p.cached(start); ok {
		return u, err
	}
	var exe string
	var env []string
	found := false
	if s.Agy != nil {
		exe, env, found = s.Agy()
	}
	p.mu.Lock()
	p.found = found
	p.mu.Unlock()
	if !found {
		return p.settle(agyMissing, Usage{}, start)
	}
	key := binaryKey(exe)
	p.mu.Lock()
	passed, checked := p.gate[key]
	p.mu.Unlock()
	if checked && !passed {
		return p.settle(agyOld, Usage{}, start)
	}
	dir, err := os.MkdirTemp("", "agy-usage-")
	if err != nil {
		return p.settle(agyFailed, Usage{}, start)
	}
	defer os.RemoveAll(dir)
	run := s.runAgy
	if run == nil {
		run = runAgyProcess
	}
	capped, cancel := context.WithTimeout(ctx, agyTimeout)
	defer cancel()
	var r agyRun
	if !checked {
		if r, err = run(capped, exe, env, dir, "--version"); err == nil && !r.signIn {
			passed = newEnough(r.out)
			p.setGate(key, passed)
		}
	}
	if err == nil && !r.signIn && passed {
		r, err = run(capped, exe, env, dir, "--print", "/usage", "--output-format", "json", "--print-timeout", "10s",
			"--settings", `{"disableSlashCommands":false}`, "--disable-slash-commands=false", "--log-file", filepath.Join(dir, "agy.log"))
	}
	switch {
	case r.signIn:
		return p.settle(agySignedOut, Usage{}, start)
	case ctx.Err() != nil:
		return p.cancelled(start)
	case err != nil:
		return p.settle(agyFailed, Usage{}, start)
	case !passed:
		return p.settle(agyOld, Usage{}, start)
	}
	if o, u := readReport(r.out, start); o != agyUnknown {
		return p.settle(o, u, start)
	}
	switch r.code {
	case 0:
		// Latched: asked again, it could take a model turn.
		p.setGate(key, false)
		return p.settle(agyOld, Usage{}, start)
	case 2:
		return p.settle(agyAPIKey, Usage{}, start)
	}
	return p.settle(agyFailed, Usage{}, start)
}

func (p *agyProbe) cached(now time.Time) (Usage, error, bool) {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.at.IsZero() || now.Sub(p.at) >= agyFloor {
		return Usage{}, nil, false
	}
	if !p.done {
		return Usage{}, errAgyUnread, true
	}
	return p.last, p.err, true
}

// cancelled leaves the last result as it was, but starts the floor, so a
// hanging agy costs a caller one wait per floor.
func (p *agyProbe) cancelled(start time.Time) (Usage, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.startFloor(start)
	if !p.done {
		return Usage{}, errAgyUnread
	}
	return p.last, p.err
}

// settle keeps outcome o. An outcome with no usage returns an empty one, so
// an earlier read never lingers as the last known.
func (p *agyProbe) settle(o agyOutcome, u Usage, start time.Time) (Usage, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.outcome, p.done = o, true
	switch o {
	case agyMissing, agyOld:
		// These wait on the binary changing, not on the floor.
		p.at = time.Time{}
	default:
		p.startFloor(start)
	}
	switch o {
	case agyMissing, agySignedOut:
		p.in = false
	case agyOld, agyAPIKey, agyOK:
		p.in = true
	}
	if o == agyFailed {
		p.last, p.err = Usage{}, errAgyFailed
		return p.last, p.err
	}
	if o != agyOK {
		u = Usage{At: start.UnixMilli()}
	}
	p.last, p.err = u, nil
	return u, nil
}

// startFloor starts the floor at start, unless a sign-in ended since: a
// probe that began before it is already out of date.
func (p *agyProbe) startFloor(start time.Time) {
	p.at = time.Time{}
	if p.forgot.Before(start) {
		p.at = start
	}
}

func (p *agyProbe) setGate(key string, passed bool) {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.gate == nil {
		p.gate = map[string]bool{}
	}
	p.gate[key] = passed
}

// binaryKey changes when agy is moved or updated.
func binaryKey(exe string) string {
	if fi, err := os.Stat(exe); err == nil {
		return exe + "\x00" + strconv.FormatInt(fi.ModTime().UnixNano(), 10)
	}
	return exe
}

var version = regexp.MustCompile(`(\d+)\.(\d+)\.(\d+)`)

func newEnough(out []byte) bool {
	got, want := version.FindSubmatch(out), version.FindStringSubmatch(agyMinimum)
	if got == nil {
		return false
	}
	for i := 1; i <= 3; i++ {
		g, err := strconv.Atoi(string(got[i]))
		if err != nil {
			return false
		}
		if w, _ := strconv.Atoi(want[i]); g != w {
			return g > w
		}
	}
	return true
}

type agyReport struct {
	Status   string `json:"status"`
	Error    string `json:"error"`
	NumTurns *int   `json:"num_turns"`
	Command  struct {
		Name string `json:"name"`
		Data struct {
			Groups []struct {
				Name    string      `json:"name"`
				Buckets []agyBucket `json:"buckets"`
			} `json:"groups"`
		} `json:"data"`
	} `json:"command"`
}

type agyBucket struct {
	Window    string   `json:"window"`
	Remaining *float64 `json:"remaining_fraction"`
	Reset     string   `json:"reset_time"`
	Disabled  bool     `json:"disabled"`
}

// readReport reads agy's /usage report, or the error it gave before any
// turn, which it exits 0 on. agyUnknown is neither, such as a model's answer
// to "/usage": that took a turn.
func readReport(out []byte, now time.Time) (agyOutcome, Usage) {
	var r agyReport
	if json.Unmarshal(out, &r) != nil || r.NumTurns == nil || *r.NumTurns != 0 {
		return agyUnknown, Usage{}
	}
	switch {
	case r.Status == "ERROR" && strings.Contains(strings.ToLower(r.Error), "authenticat"):
		return agySignedOut, Usage{}
	case r.Status == "ERROR":
		return agyFailed, Usage{}
	case r.Status != "SUCCESS" || r.Command.Name != "usage":
		return agyUnknown, Usage{}
	}
	u := Usage{At: now.UnixMilli()}
	for _, g := range r.Command.Data.Groups {
		if !strings.Contains(strings.ToLower(g.Name), "gemini") {
			continue
		}
		for _, b := range g.Buckets {
			// No meter beats a false 100 %.
			if b.Disabled || b.Remaining == nil {
				continue
			}
			switch {
			case b.Window == "5h" && u.Session == nil:
				u.Session = b.window()
			case b.Window == "weekly" && u.Week == nil:
				u.Week = b.window()
			}
		}
	}
	return agyOK, u
}

func (b agyBucket) window() *Window {
	w := &Window{Percent: min(max((1-*b.Remaining)*100, 0), 100)}
	// An unused bucket resets at the epoch, which means no reset time.
	if t, err := time.Parse(time.RFC3339, b.Reset); err == nil && t.Unix() > 0 {
		w.ResetsAt = t.UnixMilli()
	}
	return w
}

// runAgyProcess runs agy in dir, stopping it at the sign-in line. A run that
// never started returns its error, and one the deadline killed ctx's.
func runAgyProcess(ctx context.Context, exe string, env []string, dir string, args ...string) (agyRun, error) {
	run, cancel := context.WithCancel(ctx)
	defer cancel()
	out := &cappedBuffer{max: 1 << 20}
	// Which stream carries the sign-in text isn't known, so both are watched.
	watch, outWatch := &signInWatch{cancel: cancel}, &signInWatch{cancel: cancel}
	cmd := exec.CommandContext(run, exe, args...)
	cmd.Dir = dir
	cmd.Env = append(noBrowser(env, dir), "AGY_CLI_DISABLE_AUTO_UPDATE=true")
	cmd.Stdout, cmd.Stderr = io.MultiWriter(out, outWatch), watch
	cmd.WaitDelay = 2 * time.Second
	err := cmd.Run()
	switch {
	case watch.signIn || outWatch.signIn:
		return agyRun{signIn: true}, nil
	case ctx.Err() != nil:
		return agyRun{}, ctx.Err()
	case cmd.ProcessState == nil:
		return agyRun{}, err
	}
	return agyRun{out: out.buf.Bytes(), code: cmd.ProcessState.ExitCode()}, nil
}

// noBrowser is env with PATH led by commands that open nothing in place of
// open and xdg-open, which agy runs signed out to open its sign-in page.
func noBrowser(env []string, dir string) []string {
	bin := filepath.Join(dir, "no-browser")
	os.Mkdir(bin, 0o700)
	for _, name := range []string{"open", "xdg-open"} {
		os.WriteFile(filepath.Join(bin, name), []byte("#!/bin/sh\n"), 0o700)
	}
	path := cmp.Or(envdir.Value(env, "PATH"), os.Getenv("PATH"))
	return append(slices.Clip(env), "PATH="+bin+string(os.PathListSeparator)+path)
}

// cappedBuffer keeps its buffer unexported: an embedded one would lend it
// ReadFrom, which io.Copy prefers to Write, skipping the cap.
type cappedBuffer struct {
	buf bytes.Buffer
	max int
}

func (c *cappedBuffer) Write(b []byte) (int, error) {
	if room := c.max - c.buf.Len(); room > 0 {
		c.buf.Write(b[:min(len(b), room)])
	}
	return len(b), nil
}

// signInWatch reads a stream of agy's for the sign-in line and stops agy there,
// before it waits on a browser login. It keeps nothing else.
type signInWatch struct {
	cancel func()
	line   []byte
	signIn bool
}

const signInWatchMax = 64 << 10

func (w *signInWatch) Write(b []byte) (int, error) {
	n := len(b)
	for !w.signIn && len(b) > 0 {
		end := bytes.IndexByte(b, '\n')
		part := b
		if end >= 0 {
			part = b[:end]
		}
		w.line = append(w.line, part[:min(len(part), signInWatchMax-len(w.line))]...)
		if bytes.Contains(w.line, []byte(agySignInLine)) {
			w.signIn = true
			w.cancel()
		}
		if end < 0 {
			break
		}
		w.line, b = w.line[:0], b[end+1:]
	}
	if w.signIn {
		w.line = nil
	}
	return n, nil
}
