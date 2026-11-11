package daemon

import (
	"cmp"
	"context"
	"errors"
	"log"
	"regexp"
	"strings"
	"sync"

	"pocketd/internal/agent"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
	"pocketd/internal/terminal"
	"pocketd/internal/timeline"
)

// pipeWire carries the lines of an agent run behind `pocketd pipe`, one per
// Read or Write, for the client of whatever protocol its chat speaks.
type pipeWire struct {
	pr    *presence
	lines chan string
	gone  chan struct{}
	once  sync.Once
}

func newPipeWire(pr *presence) *pipeWire {
	return &pipeWire{pr: pr, lines: make(chan string, 256), gone: make(chan struct{})}
}

var errPipeGone = errors.New("chat closed")

// out takes a line the agent wrote.
func (w *pipeWire) out(line string) {
	select {
	case w.lines <- line:
	case <-w.gone:
	case <-w.pr.ctx.Done():
	}
}

func (w *pipeWire) Read(ctx context.Context) ([]byte, error) {
	select {
	case line := <-w.lines:
		return []byte(line), nil
	case <-w.gone:
		return nil, errPipeGone
	case <-w.pr.ctx.Done():
		return nil, errPipeGone
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

func (w *pipeWire) Write(_ context.Context, msg []byte) error {
	w.pr.stream.sendLine(string(msg))
	return nil
}

func (w *pipeWire) Close() { w.once.Do(func() { close(w.gone) }) }

// chatSink takes what the session of a chat behind a pipe makes of its
// agent: the timeline, and the status, which the session tells as a turn
// starts, waits and ends.
type chatSink struct {
	d  *Daemon
	pr *presence
	a  *agent.Agent
}

func (s chatSink) Apply(e timeline.Event)       { s.a.Record(e) }
func (s chatSink) SetTitle(title string)        { s.a.SetTitle(title) }
func (s chatSink) SetTokens(used, window int64) { s.a.SetTokens(used, window) }
func (s chatSink) SetModel(model string)        { s.a.SetModel(model) }
func (s chatSink) Working()                     { s.a.Working() }
func (s chatSink) NeedsYou()                    { s.a.NeedsYou() }
func (s chatSink) TurnEnded(failed bool)        { s.d.turnEnded(s.pr, failed) }
func (s chatSink) Stopped()                     { s.a.Clear() }
func (s chatSink) SetEffort(effort string)      { s.a.SetEffort(effort) }
func (s chatSink) Compacting()                  { s.a.SetCompacting() }
func (s chatSink) Compacted()                   { s.a.Compacted() }

// Loaded starts the timeline over: the agent replays the session's history into it.
func (s chatSink) Loaded() { s.a.ClearTimeline() }

// Bound saves the session as the agent's Conversation, which a restart resumes.
func (s chatSink) Bound(session string) {
	s.a.SetConversation(session)
	s.d.resumed(s.pr, session)
}

// pipeSession is the session a chat behind a pipe drives its agent through.
type pipeSession interface {
	Prompt(text string) error
	Cancel()
	Compact() error
	Settings(model, effort string) error
	Commands() ([]proto.Command, error)
}

// pipeDriver drives an agent through the session of its chat behind a pipe.
type pipeDriver struct {
	termDriver
	d *Daemon
	s pipeSession
}

func (c pipeDriver) Prompt(text string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	return c.s.Prompt(text)
}

// Interrupt drops the prompts sent mid-turn with the turn; the chat puts them back in its queue.
func (c pipeDriver) Interrupt() error {
	if !c.inForeground() {
		return errNotForeground
	}
	if !c.a.Stop() {
		return nil
	}
	c.s.Cancel()
	c.d.cleared(c.pr)
	return nil
}

func (c pipeDriver) Compact() error {
	if !c.inForeground() {
		return errNotForeground
	}
	return c.s.Compact()
}

// Settings applies to the session's next turns; a resume keeps it, as the
// switch is saved with the Terminal.
func (c pipeDriver) Settings(model, effort string) error {
	if !c.inForeground() {
		return errNotForeground
	}
	if err := c.s.Settings(model, effort); err != nil {
		return err
	}
	c.pr.switched(model, effort)
	return nil
}

func (c pipeDriver) Commands() ([]proto.Command, error) {
	if !c.inForeground() {
		return nil, errNotForeground
	}
	return c.s.Commands()
}

// openChat opens pr's chat off the poller. open gives back the prompts that
// waited for a chat that never opened, which then says so.
func (d *Daemon) openChat(pr *presence, open func() (unsent []string, err error)) {
	pr.opening.Go(func() {
		if unsent, err := open(); err != nil {
			d.startFailed(pr, err, unsent)
		}
	})
}

// startFailed ends the turn the user waits on as failed: the chat shows why
// the agent never opened it, in the agent's words, and the prompts it never got.
func (d *Daemon) startFailed(pr *presence, err error, unsent []string) {
	log.Printf("chat %s: %v", pr.a.ID(), err)
	pr.mu.Lock()
	why := pr.exit
	pr.mu.Unlock()
	if why == "" && pr.ctx.Err() != nil {
		// The poller saw the agent gone before its wrapper told how it exited.
		why = exitOf(pr.t, "")
	}
	what := providers.Get(pr.provider).Name + " didn't start"
	if len(unsent) > 0 {
		what += ", so your prompt was not sent"
	}
	pr.a.Working()
	for _, text := range unsent {
		pr.a.Record(timeline.Event{Kind: "user", Text: text})
	}
	why = cmp.Or(why, err.Error())
	pr.a.Record(timeline.Event{Kind: "error", Error: what + ": " + why + signInHint(providers.Get(pr.provider), why)})
	d.turnEnded(pr, true)
	pr.a.SetAttached(false)
}

var signedOut = regexp.MustCompile(`(?i)auth|login|logged in|credential|api key`)

// signInHint says how to sign p in, when why says it isn't and one command does.
func signInHint(p *providers.Spec, why string) string {
	if !signedOut.MatchString(why) || p.SignIn == nil && !p.SignInBare {
		return ""
	}
	return ". Run `" + strings.Join(append([]string{p.Cmd()}, p.SignIn...), " ") + "` in a terminal to sign in"
}

// exitOf says an agent exited, with the last line on its Terminal's screen:
// where it wrote why.
func exitOf(t *terminal.Terminal, status string) string {
	lines := strings.Split(strings.TrimSpace(t.Screen()), "\n")
	if last := strings.TrimSpace(lines[len(lines)-1]); last != "" {
		return "it exited" + status + ": " + last
	}
	return "it exited" + status
}
