package daemon

import (
	"context"
	"slices"
	"strings"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/codex"
	"pocketd/internal/timeline"
)

// CodexMapWindow is how long after an Enter a thread that turns active is
// taken for the one that codex started.
var CodexMapWindow = 3 * time.Second

func (d *Daemon) attachCodex(pr *presence) {
	if embedded(pr.argv, pr.env) {
		pr.a.SetAttached(false)
		return
	}
	d.mu.Lock()
	defer d.mu.Unlock()
	pr.sock = codex.Sock(pr.env)
	if d.watchers[pr.sock] != nil {
		return
	}
	if d.watchers == nil {
		d.watchers = map[string]context.CancelFunc{}
	}
	ctx, cancel := context.WithCancel(context.Background())
	d.watchers[pr.sock] = cancel
	go codex.Watch(ctx, pr.sock, &codexSock{d: d, sock: pr.sock, nonRoot: map[string]bool{}})
}

var embeddedFlags = []string{"--no-daemon", "--oss", "-p", "--profile", "-c", "--config", "--enable", "--disable",
	"--search", "--strict-config", "--dangerously-bypass-hook-trust", "--remote"}

// embedded is a codex that bypasses the account's app-server, so no watcher
// sees its threads.
func embedded(argv, env []string) bool {
	if len(argv) > 1 && (argv[1] == "exec" || argv[1] == "e") {
		return true
	}
	for _, arg := range argv[1:] {
		if flag, _, _ := strings.Cut(arg, "="); slices.Contains(embeddedFlags, flag) {
			return true
		}
	}
	return slices.ContainsFunc(env, func(kv string) bool { return strings.HasPrefix(kv, "CODEX_EXEC_SERVER_URL=") })
}

// unwatch stops the watcher on sock once no codex uses it. d.mu is held.
func (d *Daemon) unwatch(sock string) {
	for _, pr := range d.present {
		if pr.sock == sock {
			return
		}
	}
	if stop := d.watchers[sock]; stop != nil {
		stop()
		delete(d.watchers, sock)
	}
}

type codexSock struct {
	d       *Daemon
	sock    string
	nonRoot map[string]bool // unlocked: Watch hands over one broadcast at a time
}

func (w *codexSock) Connected() {}

func (w *codexSock) ThreadStarted(id string, root bool) {
	if !root {
		w.nonRoot[id] = true
	}
}

func (w *codexSock) ThreadStatus(id, typ string, flags []string) {
	pr := w.bound(id)
	if pr == nil && typ == "active" && !w.nonRoot[id] {
		if pr = w.typedIn(); pr != nil {
			w.d.bind(pr, id)
		}
	}
	if pr == nil {
		return
	}
	switch {
	case typ == "active" && (slices.Contains(flags, "waitingOnApproval") || slices.Contains(flags, "waitingOnUserInput")):
		pr.a.NeedsYou()
	case typ == "active":
		pr.a.Working()
	case typ == "idle":
		w.d.turnEnded(pr, false)
	case typ == "systemError":
		w.d.turnEnded(pr, true)
	}
	w.d.resumed(pr, id)
}

func (w *codexSock) ThreadClosed(id string) {
	delete(w.nonRoot, id)
	if pr := w.bound(id); pr != nil {
		pr.mu.Lock()
		defer pr.mu.Unlock()
		pr.unfollow()
		pr.thread, pr.unfollow = "", nil
	}
}

func (w *codexSock) bound(thread string) *presence {
	w.d.mu.Lock()
	defer w.d.mu.Unlock()
	for _, pr := range w.d.present {
		if pr.sock != w.sock {
			continue
		}
		pr.mu.Lock()
		ok := pr.thread == thread
		pr.mu.Unlock()
		if ok {
			return pr
		}
	}
	return nil
}

// typedIn is the codex on the socket that got Enter last, within
// CodexMapWindow, unless its thread is running: that Enter went to it.
func (w *codexSock) typedIn() *presence {
	w.d.mu.Lock()
	defer w.d.mu.Unlock()
	var last *presence
	var at time.Time
	for _, pr := range w.d.present {
		if pr.sock != w.sock {
			continue
		}
		pr.mu.Lock()
		enter, thread := pr.enter, pr.thread
		pr.mu.Unlock()
		status := pr.a.Summary().Status
		running := thread != "" && (status == "working" || status == "needsYou")
		if !running && time.Since(enter) < CodexMapWindow && enter.After(at) {
			last, at = pr, enter
		}
	}
	return last
}

// bind follows thread in pr's timeline, which starts over even for the same
// thread, as the follower replays it all. The old follower stops first, so
// none of its events land in the new timeline.
func (d *Daemon) bind(pr *presence, thread string) {
	pr.mu.Lock()
	defer pr.mu.Unlock()
	if pr.unfollow != nil {
		pr.unfollow()
	}
	if pr.a.Summary().ProviderSessionID == thread {
		pr.a.Timeline.Clear()
	}
	ctx, cancel := context.WithCancel(pr.ctx)
	done := make(chan struct{})
	pr.thread, pr.enter, pr.unfollow = thread, time.Time{}, func() {
		cancel()
		<-done
	}
	pr.a.SetConversation(thread)
	go func() {
		defer close(done)
		d.follow(ctx, pr, thread)
	}()
}

func (d *Daemon) follow(ctx context.Context, pr *presence, thread string) {
	s, err := codex.Open(ctx, pr.sock, thread, pr.a.ID(), codexSink{pr.a}, d.Broker)
	if err != nil {
		return
	}
	defer s.Close()
	select {
	case <-ctx.Done():
	case <-s.Done():
	}
}

// codexSink takes a thread's timeline and title; the watcher sets the status.
type codexSink struct{ *agent.Agent }

func (s codexSink) Apply(e timeline.Event) {
	s.Record(e)
	if e.Kind == "compacted" {
		s.Compacted()
	}
}
