package daemon

import (
	"context"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"time"

	"pocketd/internal/claude"
	"pocketd/internal/proc"
	"pocketd/internal/terminal"
	"pocketd/internal/timeline"
)

const claudeApp = "claude-code"

// claudeAt is the claude agent that pid is in terminal id, or nil for any
// other process, like a claude -p run by the agent's Bash tool. A hook can
// beat the poller to a new claude, so a miss looks at the terminal again.
func (d *Daemon) claudeAt(id string, pid int) *presence {
	if pr := d.presentAt(id, pid); pr != nil && pr.provider == "claude" {
		return pr
	}
	return nil
}

// presentAt is the agent that pid is in terminal id, or nil.
func (d *Daemon) presentAt(id string, pid int) *presence {
	pr := d.presentIn(id)
	if t := d.Terminals.Get(id); t != nil && (pr == nil || pr.pid != pid) {
		d.observe(t)
		pr = d.presentIn(id)
	}
	if pr == nil || pr.pid != pid {
		return nil
	}
	return pr
}

// NearestClaude is the claude whose hook this is. A claude -p run by
// Claude's Bash tool is nearer than the claude that ran it. A claude that
// `pocketd pipe` runs is that pipe's agent.
func NearestClaude(chain []proc.Proc) int {
	for i, p := range chain {
		if Provider(p.Argv) != "claude" {
			continue
		}
		for _, up := range chain[i+1:] {
			if piped(up.Argv) != nil {
				return up.Pid
			}
			if Provider(up.Argv) != "" {
				break
			}
		}
		return p.Pid
	}
	return 0
}

// AskOpen reports whether the agent in terminal id waits on the user: a
// permission hook is open, or it shows Needs you.
func (d *Daemon) AskOpen(id string) bool {
	pr := d.presentIn(id)
	if pr == nil {
		return false
	}
	pr.mu.Lock()
	asking := len(pr.asks) > 0
	pr.mu.Unlock()
	return asking || pr.a.Summary().Status == "needsYou"
}

// Input clears a claude turn the user stops with Esc or Ctrl+C, as Claude
// fires no hook for it, and notes a codex's Enter, which starts the turn its
// thread is found by. Only a key written alone counts, so typed or pasted
// text that holds one, or an Esc sequence like an arrow key, doesn't.
func (d *Daemon) Input(id string, b []byte) {
	if pr := d.presentIn(id); pr != nil && pr.stream != nil {
		return
	}
	if pr := d.presentIn(id); pr != nil && pr.provider == "claude" && len(b) > 0 {
		pr.typedKeys(b)
	}
	switch string(b) {
	case "\x1b", "\x1b[27u", "\x03", "\x1b[99;5u":
		if pr := d.presentIn(id); pr != nil && pr.provider == "claude" {
			pr.a.Escaped()
			if !pr.isReported() {
				d.cleared(pr)
			}
		}
	case "\r", "\x1b[13u":
		if pr := d.presentIn(id); pr != nil && pr.provider == "codex" {
			pr.mu.Lock()
			pr.enter = time.Now()
			pr.mu.Unlock()
		}
	}
}

// Report takes one OSC 7501 report from terminal id. The first one makes the
// claude there report-driven: its status no longer comes from hooks.
func (d *Daemon) Report(id string, r terminal.Report) {
	if pr := d.presentIn(id); pr != nil {
		d.apply(pr, r)
	}
}

// catchUp applies the terminal's latest report to pr, for a report that came
// before pr existed or whose effect on the status was lost since.
func (d *Daemon) catchUp(pr *presence) {
	pr.report.Lock()
	defer pr.report.Unlock()
	if r := pr.t.Program(); r != nil && r.App == claudeApp {
		d.applyLocked(pr, *r)
	}
}

func (d *Daemon) apply(pr *presence, r terminal.Report) {
	pr.report.Lock()
	defer pr.report.Unlock()
	d.applyLocked(pr, r)
}

func (d *Daemon) applyLocked(pr *presence, r terminal.Report) {
	if pr.provider != "claude" || pr.stream != nil {
		return
	}
	pr.mu.Lock()
	pr.reported = true
	pr.mu.Unlock()
	pr.a.SetAttached(true)
	switch r.State {
	case "working":
		pr.working("")
	case "blocked":
		pr.a.NeedsYou()
	case "done":
		d.turnEnded(pr, false)
	case "error":
		d.turnEnded(pr, true)
	case "idle", "clear":
		d.cleared(pr)
	}
	if r.State == "clear" {
		r.Msg = ""
	}
	pr.a.SetActivity(r.Msg)
}

func (d *Daemon) presentIn(id string) *presence {
	d.mu.Lock()
	defer d.mu.Unlock()
	return d.present[id]
}

// sessionStart binds pr to the conversation Claude started, resumed or
// cleared to. The old tail stops before SetConversation clears the timeline,
// so none of its lines land in the new conversation.
func (d *Daemon) sessionStart(pr *presence, in hookInput) {
	pr.mu.Lock()
	defer pr.mu.Unlock()
	moved := in.TranscriptPath != pr.transcript
	if moved && pr.stopTail != nil {
		pr.stopTail()
	}
	pr.a.SetConversation(in.SessionID)
	pr.a.SetAttached(true)
	pr.hooked = true
	if in.Model != "" {
		pr.a.SetModel(in.Model)
	}
	if in.Cwd != "" {
		pr.a.SetCwd(in.Cwd)
	}
	if in.Source == "compact" {
		pr.a.Compacted()
	}
	if moved {
		pr.transcript, pr.stopTail = in.TranscriptPath, d.tail(pr, in.TranscriptPath, in.Model)
	}
}

// DiscoverEvery is how often a claude no SessionStart bound has its transcript looked for.
var DiscoverEvery = time.Second

// discover binds pr to the transcript its claude writes until a SessionStart
// does, as Claude skips hooks in a folder the user hasn't trusted. It takes
// only a transcript that appeared since pr started, was typed into Claude's
// own input, so a claude -p isn't taken for it, and is up to a prompt pr's
// claude received: from its launch, from pocketd, or typed in its terminal.
func (d *Daemon) discover(pr *presence) {
	known := map[string]bool{}
	for _, p := range claude.Sessions(pr.a.Summary().Cwd, pr.env) {
		known[p] = true
	}
	opened := map[string]claude.Opening{}
	tick := time.NewTicker(DiscoverEvery)
	go func() {
		defer tick.Stop()
		for {
			select {
			case <-pr.ctx.Done():
				return
			case <-tick.C:
			}
			if !d.discovered(pr, known, opened) {
				return
			}
		}
	}()
}

// discovered looks once, and reports whether to look again. opened keeps how
// far each transcript was read, which it reads on only while it may be pr's.
func (d *Daemon) discovered(pr *presence, known map[string]bool, opened map[string]claude.Opening) bool {
	pr.mu.Lock()
	hooked, current := pr.hooked, pr.transcript
	pr.mu.Unlock()
	if hooked {
		return false
	}
	paths := claude.Sessions(pr.a.Summary().Cwd, pr.env)
	// One look binds at a time, so two agents can't both take a transcript.
	d.binding.Lock()
	defer d.binding.Unlock()
	taken, rivals := d.others(pr)
	for _, p := range paths {
		if o := opened[p]; !known[p] && !taken[p] && (o.Prompt == "" || o.Interactive && (current == "" || o.Cleared)) {
			opened[p] = o.Read(p)
		}
	}
	path := newSession(paths, known, taken, opened, claimant{bound: current != "", sent: pr.unshown()}, rivals)
	if path == "" {
		return true
	}
	pr.mu.Lock()
	defer pr.mu.Unlock()
	if pr.hooked || pr.transcript != current {
		return !pr.hooked
	}
	if pr.stopTail != nil {
		pr.stopTail()
	}
	known[path] = true
	pr.a.SetConversation(strings.TrimSuffix(filepath.Base(path), ".jsonl"))
	pr.transcript, pr.stopTail = path, d.tail(pr, path, "")
	return true
}

// others is the transcripts the agents other than pr follow, and the claudes
// in pr's folder that no SessionStart bound, which may claim one too.
func (d *Daemon) others(pr *presence) (taken map[string]bool, rivals []claimant) {
	d.mu.Lock()
	var all []*presence
	for _, o := range d.present {
		if o != pr {
			all = append(all, o)
		}
	}
	d.mu.Unlock()
	cwd := pr.a.Summary().Cwd
	taken = map[string]bool{}
	for _, o := range all {
		o.mu.Lock()
		transcript, hooked := o.transcript, o.hooked
		o.mu.Unlock()
		taken[transcript] = true
		if o.provider == "claude" && !hooked && claude.SameProject(o.a.Summary().Cwd, cwd) {
			rivals = append(rivals, claimant{bound: transcript != "", sent: o.unshown()})
		}
	}
	return taken, rivals
}

// claimant is a claude that may take a transcript: one it follows none of
// yet, or one a /clear started, up to a prompt it received that no
// transcript has shown.
type claimant struct {
	bound bool
	sent  []received
}

// promptLag is how much earlier than pocketd noted a prompt Claude may stamp
// it: its clock keeps milliseconds. A transcript up to the same text from
// before, such as a closed agent's, isn't the claimant's.
const promptLag = 10 * time.Millisecond

func (c claimant) claims(o claude.Opening) bool {
	return o.Interactive && o.Prompt != "" && (!c.bound || o.Cleared) &&
		slices.ContainsFunc(c.sent, func(r received) bool {
			return sameText(o.Prompt, r.text) && (o.At.IsZero() || !o.At.Before(r.at.Add(-promptLag)))
		})
}

// newSession is the one transcript in paths that appeared since known, no
// other agent follows, and pr claims while no rival does, as one prompt sent
// to two claudes can't tell their transcripts apart; the next one may. ""
// when none or several are.
func newSession(paths []string, known, taken map[string]bool, opens map[string]claude.Opening, pr claimant, rivals []claimant) string {
	var found []string
	for _, p := range paths {
		o := opens[p]
		if known[p] || taken[p] || !pr.claims(o) || slices.ContainsFunc(rivals, func(r claimant) bool { return r.claims(o) }) {
			continue
		}
		found = append(found, p)
	}
	if len(found) != 1 {
		return ""
	}
	return found[0]
}

// launchPrompt is the prompt a claude's argv starts it on. launch.Argv puts
// it last, after "--"; typed in a shell, it's the last word, after the
// command or a word that isn't a flag, which may take it as its value.
func launchPrompt(argv []string) string {
	n := len(argv)
	if i := slices.Index(argv, "--"); i > 0 {
		if i == n-2 {
			return argv[i+1]
		}
		return ""
	}
	if n >= 2 && !strings.HasPrefix(argv[n-1], "-") && (n == 2 || !strings.HasPrefix(argv[n-2], "-")) {
		return argv[n-1]
	}
	return ""
}

// sameText says whether Claude's echo is the prompt sent, as the desktop
// matches them: Claude may tidy its whitespace, echoes a bash command as
// "! cmd", and text left in its input runs on into the prompt.
func sameText(echo, sent string) bool {
	return strings.HasSuffix(tidy(echo), tidy(sent))
}

func tidy(text string) string {
	text = strings.Join(strings.Fields(text), " ")
	if cmd, ok := strings.CutPrefix(text, "! "); ok {
		return "!" + cmd
	}
	return text
}

// tail feeds pr's timeline from a transcript; hooks set the status. stop
// returns once the lines written so far are in.
func (d *Daemon) tail(pr *presence, path, hookModel string) (stop func()) {
	ctx, cancel := context.WithCancel(pr.ctx)
	done := make(chan struct{})
	var replay int64
	if fi, err := os.Stat(path); err == nil {
		replay = fi.Size()
	}
	go func() {
		defer close(done)
		var spawns sync.Map
		subagents := make(chan struct{})
		go func() {
			defer close(subagents)
			claude.TailSubagents(ctx, path, func(id string) bool {
				_, ok := spawns.Load(id)
				return ok
			}, func(events []timeline.Event) {
				for _, e := range events {
					if e.Kind == "tool_start" {
						spawns.Store(e.ToolUseID, true)
					}
					pr.a.Record(e)
				}
			})
		}()
		defer func() { <-subagents }()
		keys := map[string]string{}
		var read int64
		used, window := int64(-1), int64(0)
		var turns claude.Turns
		claude.Tail(ctx, path, func(line []byte) {
			read += int64(len(line)) + 1
			events := turns.Map(line)
			if model := claude.Model(line); model != "" {
				pr.a.SetModel(model)
				if u, ok := claude.Tokens(line); ok {
					used, window = u, claude.ContextWindow(model, hookModel, u)
				}
			}
			// A resumed transcript replays every old answer; only the last is the context now.
			if used >= 0 && read >= replay {
				pr.a.SetTokens(used, window)
				used = -1
			}
			for _, e := range events {
				if e.Kind == "tool_start" {
					spawns.Store(e.ToolUseID, true)
				}
				d.dismissAnswered(pr.a.ID(), keys, e)
				if e.Kind == "user" {
					pr.shown(e.Text)
				}
				pr.a.Record(e)
				// Rejecting Claude's own dialog at the desk interrupts the turn without a hook.
				if e.Error == "interrupted" && read > replay && pr.a.Summary().Status == "needsYou" {
					d.cleared(pr)
				}
			}
		})
	}()
	return func() {
		cancel()
		<-done
	}
}
