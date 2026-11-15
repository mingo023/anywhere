package factory

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"time"

	"pocketd/internal/atomicfile"
	"pocketd/internal/proto"
	"pocketd/internal/worktree"
)

const (
	LocalRunning = "running"
	LocalPassed  = "passed"
	LocalFailed  = "failed"

	FixMailed  = "mailed"
	FixEnded   = "ended"
	FixStopped = "stopped"
)

const (
	ActIntegrate = "integrate"
	ActOpenPR    = "openPR"
)

// LocalChecks is what Checks knows about the commit in the lead's checkout.
type LocalChecks struct {
	Head   string
	Pushed bool
	// State is "" until the commands ran on Head, then running, passed or failed.
	State       string
	Done, Total int
	// Failed is the command that failed.
	Failed string
	// Fix follows a fix note: mailed, then ended or stopped with the lead's turn.
	Fix string
	// Hint: a hint waits to be sent.
	Hint bool
	// Same: the fix turn before this commit changed nothing.
	Same bool
	// Error is a fault: a failure that is not a failed check.
	Error string
	// Quiet: GitHub reported no check for Head within quietAfter.
	Quiet bool
}

// Record is what Checks keeps about one commit of a Ticket's branch.
type Record struct {
	Head   string               `json:"head"`
	Pushed int64                `json:"pushed,omitempty"` // Unix ms
	State  string               `json:"state,omitempty"`
	Total  int                  `json:"total,omitempty"`
	Checks []proto.FactoryCheck `json:"checks,omitempty"`
	Fix    string               `json:"fix,omitempty"`
	Hint   string               `json:"hint,omitempty"` // your text, until its note is sent
	Same   bool                 `json:"same,omitempty"`
	Error  string               `json:"error,omitempty"`
	Quiet  bool                 `json:"quiet,omitempty"`
}

func recordFiles(home, ticket string) []string {
	names, _ := filepath.Glob(filepath.Join(dir(home, ticket), "checks", "*.json"))
	slices.Sort(names)
	return names
}

// ReadRecord is ticket's newest check record.
func ReadRecord(home, ticket string) (Record, bool) {
	names := recordFiles(home, ticket)
	if len(names) == 0 {
		return Record{}, false
	}
	raw, err := os.ReadFile(names[len(names)-1])
	var r Record
	if err != nil || json.Unmarshal(raw, &r) != nil {
		return Record{}, false
	}
	return r, true
}

// WriteRecord saves r over ticket's newest record, or as the next one when fresh or there is none.
func WriteRecord(home, ticket string, r Record, fresh bool) error {
	names := recordFiles(home, ticket)
	folder := filepath.Join(dir(home, ticket), "checks")
	path := filepath.Join(folder, fmt.Sprintf("%04d.json", len(names)+1))
	if !fresh && len(names) > 0 {
		path = names[len(names)-1]
	}
	if err := os.MkdirAll(folder, 0o700); err != nil {
		return err
	}
	raw, err := json.Marshal(r)
	if err != nil {
		return err
	}
	return atomicfile.Write(path, raw, 0o600)
}

// PRBody is the draft PR's body: the spec's Brief and Acceptance criteria, then the Ticket's key.
func PRBody(spec, key string) string {
	var parts []string
	for _, heading := range []string{"## Brief", "## Acceptance criteria"} {
		if s := section(spec, heading); s != "" {
			parts = append(parts, s)
		}
	}
	if key != "" {
		parts = append(parts, "Ticket "+key)
	}
	return strings.Join(parts, "\n\n")
}

// section is heading's lines in spec, up to the next "## " heading.
func section(spec, heading string) string {
	var out []string
	for _, line := range strings.Split(spec, "\n") {
		switch {
		case strings.TrimSpace(line) == heading:
			out = []string{heading}
		case out != nil && strings.HasPrefix(line, "## "):
			return strings.TrimSpace(strings.Join(out, "\n"))
		case out != nil:
			out = append(out, line)
		}
	}
	return strings.TrimSpace(strings.Join(out, "\n"))
}

// checksNext is Next for a Ticket at Checks. The table in docs/plans/2026-11-12-light-factory-5-checks.md
// lists the rows in the order the stages below read them.
func checksNext(f Facts) (proto.Ticket, []Action) {
	for _, stage := range []func(*Facts) ([]Action, bool){faulted, integration, pullRequest} {
		if acts, done := stage(&f); done {
			return f.Ticket, acts
		}
	}
	acts, _ := work(&f, "Draft PR open")
	return f.Ticket, acts
}

func work(f *Facts, why string, acts ...Action) ([]Action, bool) {
	f.Ticket.Status, f.Ticket.Gate, f.Ticket.Why = proto.TicketWorking, "", why
	return acts, true
}

func fail(f *Facts, why string) ([]Action, bool) {
	f.Ticket.Status, f.Ticket.Gate, f.Ticket.Why = proto.TicketFailed, "", why
	return nil, true
}

// faulted is row 6.
func faulted(f *Facts) ([]Action, bool) {
	if f.Local.Error != "" {
		return fail(f, f.Local.Error)
	}
	return nil, false
}

// integration is rows 7 and 8.
func integration(f *Facts) ([]Action, bool) {
	if f.Team == nil {
		return nil, false
	}
	switch f.Team.State {
	case "review", "conflict":
		return work(f, "Merging the worker branches", Action{Kind: ActIntegrate})
	case "merging":
		return work(f, "Merging the worker branches")
	}
	return nil, false
}

// pullRequest is rows 9 and 10.
func pullRequest(f *Facts) ([]Action, bool) {
	if f.Ticket.PR == 0 && f.PR != nil && f.PR.State == "OPEN" {
		f.Ticket.PR = f.PR.Number
	}
	if f.Ticket.PR == 0 || !f.Local.Pushed {
		return work(f, "Opening the draft PR", Action{Kind: ActOpenPR})
	}
	return nil, false
}

// later runs do on the engine's goroutine, after what is queued.
func (e *Engine) later(do func()) { e.post(event{do: do}) }

func (e *Engine) say(ticket, actor, kind, text string) {
	e.note(ticket, proto.FactoryEvent{At: e.now().UnixMilli(), Station: proto.StationChecks, Actor: actor, Kind: kind, Text: text})
}

// atChecks is the live Tickets at Checks.
func (e *Engine) atChecks() []proto.Ticket {
	all, _ := e.Board.Snapshot()
	return slices.DeleteFunc(all, func(t proto.Ticket) bool { return t.State != proto.TicketLive || t.Station != proto.StationChecks })
}

// current is the record of the commit in t's checkout: the newest one when it is for that commit or holds a fix
// note or a hint, else a new one.
func (e *Engine) current(t proto.Ticket) (rec Record, fresh bool) {
	head, _ := worktree.Head(t.Worktree)
	rec, ok := ReadRecord(e.Home, t.ID)
	if !ok || (rec.Head != head && rec.Fix == "" && rec.Hint == "") {
		return Record{Head: head}, true
	}
	return rec, false
}

func (e *Engine) save(ticket string, rec Record, fresh bool) {
	if err := WriteRecord(e.Home, ticket, rec, fresh); err != nil {
		log.Printf("factory: the check record of %s: %v", ticket, err)
	}
}

func (e *Engine) local(t proto.Ticket) LocalChecks {
	rec, fresh := e.current(t)
	if fresh {
		return LocalChecks{Head: rec.Head}
	}
	l := LocalChecks{Head: rec.Head, Pushed: rec.Pushed != 0, State: rec.State, Done: len(rec.Checks), Total: rec.Total, Fix: rec.Fix, Hint: rec.Hint != "", Same: rec.Same, Error: rec.Error, Quiet: rec.Quiet}
	if n := len(rec.Checks); l.State == LocalFailed && n > 0 {
		l.Failed = rec.Checks[n-1].Command
	}
	return l
}

// fault records a failure that is not a failed check.
func (e *Engine) fault(t proto.Ticket, why string) {
	rec, fresh := e.current(t)
	if rec.Error == why {
		return
	}
	rec.Error = why
	e.save(t.ID, rec, fresh)
}

// integrate merges the Team's branches into the lead's checkout. The result comes back through TeamChanged.
func (e *Engine) integrate(t proto.Ticket) {
	again := func(why string) {
		e.fault(t, why)
		e.post(event{ticket: t.ID})
	}
	if open, _ := worktree.MergeInProgress(t.Worktree); open {
		again("A merge is still open in the checkout")
		return
	}
	if dirty, _ := worktree.Dirty(t.Worktree); dirty {
		if _, err := worktree.Commit(t.Worktree, "Lead's changes before the merge"); err != nil {
			again("Couldn't commit the lead's changes: " + err.Error())
			return
		}
	}
	e.say(t.ID, "anywhere", "check", "Merging the worker branches")
	if err := e.Teams.Merge(t.AgentID); err != nil {
		again("Couldn't merge the worker branches: " + err.Error())
	}
}

// integrated records how the merge of tm ended for its Ticket at Checks.
func (e *Engine) integrated(tm proto.Team) {
	for _, t := range e.atChecks() {
		switch {
		case t.AgentID != tm.Lead:
		case tm.State == "conflict" && len(tm.Conflict) > 0:
			e.fault(t, "Conflict in "+strings.Join(tm.Conflict, ", "))
		case tm.State == "review" && tm.Error != "":
			e.fault(t, "Couldn't merge the worker branches: "+tm.Error)
		}
	}
}

// openPR pushes t's branch and opens its draft PR, or only pushes when the PR exists.
func (e *Engine) openPR(t proto.Ticket) {
	if e.flight[t.ID] {
		return
	}
	if e.GitHub == nil {
		e.fault(t, "GitHub is not set up")
		e.post(event{ticket: t.ID})
		return
	}
	e.flight[t.ID] = true
	spec := readSpec(e.Home, t.ID)
	body := PRBody(spec, Key(e.project(t.Folder), t.Number))
	run := e.ctx
	go func() {
		ctx, cancel := context.WithTimeout(run, 5*time.Minute)
		defer cancel()
		head, _ := worktree.Head(t.Worktree)
		number, err := t.PR, error(nil)
		if number == 0 {
			number, _, err = e.GitHub.Create(ctx, t.Worktree, t.Title, body, t.Base, true)
			if err != nil {
				// The branch may have a PR that Anywhere did not open.
				if pr, _ := e.GitHub.PR(ctx, t.Worktree); pr != nil && pr.State == "OPEN" {
					number, err = pr.Number, e.GitHub.Push(ctx, t.Worktree)
				}
			}
		} else {
			err = e.GitHub.Push(ctx, t.Worktree)
		}
		e.later(func() { e.pushed(t, head, number, err) })
	}()
}

// pushed takes the result of openPR for the commit head.
func (e *Engine) pushed(t proto.Ticket, head string, number int, err error) {
	delete(e.flight, t.ID)
	rec, fresh := e.current(t)
	switch {
	case err != nil:
		rec.Error = "Couldn't open the pull request: " + err.Error()
		if t.PR != 0 {
			rec.Error = "Couldn't push the branch: " + err.Error()
		}
	case rec.Head != head:
		// The checkout moved while the push ran. The next step pushes again.
	case t.PR == 0:
		rec.Pushed = e.now().UnixMilli()
		if e.prs[t.Worktree] == nil {
			e.prs[t.Worktree] = &proto.PR{Number: number, State: "OPEN", Draft: true}
		}
		e.say(t.ID, "anywhere", "check", fmt.Sprintf("Opened draft PR #%d", number))
	default:
		rec.Pushed = e.now().UnixMilli()
		e.say(t.ID, "anywhere", "check", "Pushed "+head[:min(7, len(head))])
	}
	e.save(t.ID, rec, fresh)
	e.post(event{ticket: t.ID})
}
