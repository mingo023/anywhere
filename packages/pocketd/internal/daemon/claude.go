package daemon

import (
	"context"
	"os"
	"time"

	"pocketd/internal/claude"
)

// claudeAt is the claude agent that pid is in terminal id, or nil for any
// other process, like a claude -p run by the agent's Bash tool. A hook can
// beat the poller to a new claude, so a miss looks at the terminal again.
func (d *Daemon) claudeAt(id string, pid int) *presence {
	pr := d.presentIn(id)
	if t := d.Terminals.Get(id); t != nil && (pr == nil || pr.pid != pid) {
		d.observe(t)
		pr = d.presentIn(id)
	}
	if pr == nil || pr.pid != pid || pr.provider != "claude" {
		return nil
	}
	return pr
}

// Input clears a claude turn the user stops with Esc or Ctrl+C, as Claude
// fires no hook for it, and notes a codex's Enter, which starts the turn its
// thread is found by. Only a key written alone counts, so typed or pasted
// text that holds one, or an Esc sequence like an arrow key, doesn't.
func (d *Daemon) Input(id string, b []byte) {
	switch string(b) {
	case "\x1b", "\x1b[27u", "\x03", "\x1b[99;5u":
		if pr := d.presentIn(id); pr != nil && pr.provider == "claude" {
			pr.a.Clear()
		}
	case "\r", "\x1b[13u":
		if pr := d.presentIn(id); pr != nil && pr.provider == "codex" {
			pr.mu.Lock()
			pr.enter = time.Now()
			pr.mu.Unlock()
		}
	}
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
		pr.transcript, pr.stopTail = in.TranscriptPath, d.tail(pr, in.TranscriptPath)
	}
}

// tail feeds pr's timeline from a transcript; hooks set the status. stop
// returns once the lines written so far are in.
func (d *Daemon) tail(pr *presence, path string) (stop func()) {
	ctx, cancel := context.WithCancel(pr.ctx)
	done := make(chan struct{})
	var replay int64
	if fi, err := os.Stat(path); err == nil {
		replay = fi.Size()
	}
	go func() {
		defer close(done)
		keys := map[string]string{}
		var read int64
		claude.Tail(ctx, path, func(line []byte) {
			read += int64(len(line)) + 1
			events, title := claude.Map(line)
			if title != "" {
				pr.a.SetTitle(title)
			}
			if model := claude.Model(line); model != "" {
				pr.a.SetModel(model)
			}
			for _, e := range events {
				d.dismissAnswered(pr.a.ID(), keys, e)
				pr.a.Record(e)
				// Rejecting Claude's own dialog at the desk interrupts the turn without a hook.
				if e.Error == "interrupted" && read > replay && pr.a.Summary().Status == "needsYou" {
					pr.a.Clear()
				}
			}
		})
	}()
	return func() {
		cancel()
		<-done
	}
}
