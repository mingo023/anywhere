package daemon

import (
	"context"
	"log"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"time"

	"pocketd/internal/claude"
	"pocketd/internal/proc"
	"pocketd/internal/providers"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
	"pocketd/internal/timeline"
)

const claudeApp = "claude-code"

type claudeKind struct{}

func (claudeKind) tool(arg string) bool { return strings.HasPrefix(arg, "bg-") }

func (claudeKind) serves(string) bool { return false }

func (claudeKind) pipe(d *Daemon, pr *presence) pipeChat { return claudeStream{d, pr} }

func (claudeKind) started(*Daemon, *presence) {}

func (claudeKind) attach(d *Daemon, pr *presence) {
	if h := d.hintOf(pr); h != nil && claude.InProjects(h.saved.TranscriptPath, pr.env) {
		pr.mu.Lock()
		if pr.transcript == "" {
			pr.transcript, pr.stopTail = h.saved.TranscriptPath, d.tail(pr, h.saved.TranscriptPath, "", !h.handoff)
		}
		pr.mu.Unlock()
	}
	if pr.stream == nil {
		if p := launchPrompt(pr.argv); p != "" {
			pr.entered(p)
		}
		d.discover(pr)
	}
	expired := time.After(ClaudeAttachWait)
	go func() {
		select {
		case <-pr.ctx.Done():
		case <-expired:
			pr.mu.Lock()
			defer pr.mu.Unlock()
			if !pr.hooked && !pr.reported && (pr.stream == nil || !pr.stream.connected()) {
				pr.a.SetAttached(false)
			}
		}
	}()
}

// typeIn clears Claude's input after a stop first: a prompt stopped before
// its reply goes back there, and would run on into text. A draft the user
// left there would too, so it is put aside instead, with whatever else the
// input holds. It notes the prompt first, so it is known before Claude writes it.
func (claudeKind) typeIn(c termDriver, text string) error {
	c.pr.typedPrompt(text)
	refilled := c.a.Refilled()
	if c.pr.isDrafted() {
		return c.t.PromptStashed(text)
	}
	if refilled {
		return c.t.PromptOver(text)
	}
	return c.t.Prompt(text)
}

func (claudeKind) settings(c termDriver, model, effort string) error {
	return c.typeSettings(model, effort)
}

// keys clears a turn the user stops with Esc or Ctrl+C, as Claude fires no
// hook for it. Only a key written alone counts, so typed or pasted text that
// holds one, or an Esc sequence like an arrow key, doesn't.
func (claudeKind) keys(d *Daemon, pr *presence, b []byte) {
	if len(b) > 0 {
		pr.typedKeys(b)
	}
	switch string(b) {
	case "\x1b", "\x1b[27u", "\x03", "\x1b[99;5u":
		pr.a.Escaped()
		if !pr.isReported() {
			d.cleared(pr)
		}
	}
}

// resumable: a conversation that has had no prompt has no transcript, and claude can't resume it.
func (claudeKind) resumable(e state.Terminal) bool {
	_, err := os.Stat(e.TranscriptPath)
	return err == nil
}

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

// Input hands the keys the user writes to an agent in its TUI to its kind.
func (d *Daemon) Input(id string, b []byte) {
	pr := d.presentIn(id)
	if pr == nil || pr.stream != nil {
		return
	}
	kindOf[pr.provider].keys(d, pr, b)
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
		pr.transcript, pr.stopTail = in.TranscriptPath, d.tail(pr, in.TranscriptPath, in.Model, in.Source == "resume")
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
	pr.transcript, pr.stopTail = path, d.tail(pr, path, "", false)
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
// returns once the lines written so far are in. A resumed transcript, one a
// new claude took over, ends the background calls it replays still running.
func (d *Daemon) tail(pr *presence, path, hookModel string, resumed bool) (stop func()) {
	ctx, cancel := context.WithCancel(pr.ctx)
	done := make(chan struct{})
	var replay int64
	if fi, err := os.Stat(path); err == nil {
		replay = fi.Size()
	}
	resumed = resumed && replay > 0
	go func() {
		defer close(done)
		var spawns sync.Map
		subagents := make(chan struct{})
		kick := make(chan struct{}, 1)
		go func() {
			defer close(subagents)
			claude.TailSubagents(ctx, path, kick, func(id string) bool {
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
			if g, ok := claude.Goal(line); ok {
				pr.a.SetGoal(g)
			}
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
					// A sub-agent's transcript starts with its call, so look for it now.
					if e.Name == "Task" || e.Name == "Agent" {
						select {
						case kick <- struct{}{}:
						default:
						}
					}
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
			if resumed && read >= replay {
				resumed = false
				pr.a.EndLasting()
			}
		})
	}()
	return func() {
		cancel()
		<-done
	}
}

// Retitle names the Claude conversations followed in the Terminals that
// match, as /rename in each would, so `claude --resume` lists them by the
// name Pocket shows.
func (d *Daemon) Retitle(match func(terminal.Info) bool, title string) {
	d.mu.Lock()
	var prs []*presence
	for _, pr := range d.present {
		if pr.provider == providers.Claude.ID && match(pr.t.Info()) {
			prs = append(prs, pr)
		}
	}
	d.mu.Unlock()
	for _, pr := range prs {
		pr.mu.Lock()
		path := pr.transcript
		pr.mu.Unlock()
		if path == "" || !claude.InProjects(path, pr.env) {
			continue
		}
		if err := claude.Retitle(path, title); err != nil {
			log.Printf("retitle %s: %v", pr.a.ID(), err)
		}
	}
}
