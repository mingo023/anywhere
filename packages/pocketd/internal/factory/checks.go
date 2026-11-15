package factory

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"time"
	"unicode/utf8"

	"pocketd/internal/atomicfile"
	"pocketd/internal/github"
	"pocketd/internal/hostop"
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
	ActIntegrate  = "integrate"
	ActOpenPR     = "openPR"
	ActRunChecks  = "runChecks"
	ActSendFix    = "sendFix"
	ActCommitPush = "commitPush"
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
	for _, stage := range []func(*Facts) ([]Action, bool){fixing, faulted, integration, pullRequest, commands} {
		if acts, done := stage(&f); done {
			return f.Ticket, acts
		}
	}
	acts, _ := remote(&f)
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

// commands is rows 11 to 13.
func commands(f *Facts) ([]Action, bool) {
	switch f.Local.State {
	case "":
		return work(f, fmt.Sprintf("Checks · 0/%d", len(f.Config.Checks)), Action{Kind: ActRunChecks})
	case LocalRunning:
		return work(f, fmt.Sprintf("Checks · %d/%d", f.Local.Done, f.Local.Total))
	case LocalFailed:
		return broken(f, f.Local.Failed)
	}
	return nil, false
}

// remote is rows 14 to 21.
func remote(f *Facts) ([]Action, bool) {
	pr := f.PR
	switch {
	case pr == nil:
		return work(f, "Asking GitHub")
	case pr.State == "CLOSED":
		return fail(f, "The pull request was closed")
	case pr.State == "OPEN" && pr.Head != f.Local.Head:
		return work(f, "Asking GitHub")
	case pr.Mergeable == "CONFLICTING":
		return broken(f, "Conflicts with "+pr.Base)
	case pr.Checks.Failed > 0:
		return broken(f, failedRun(pr))
	case pr.Checks.Pending > 0:
		return work(f, fmt.Sprintf("GitHub checks · %d/%d", pr.Checks.Passed, pr.Checks.Passed+pr.Checks.Failed+pr.Checks.Pending))
	case len(pr.Runs) == 0 && !f.Local.Quiet:
		return work(f, "Looking for GitHub checks")
	}
	f.Ticket.Station, f.Ticket.Hint = proto.StationPR, false
	return work(f, "")
}

// failedRun is the name of pr's first failed check.
func failedRun(pr *proto.PR) string {
	for _, run := range pr.Runs {
		if run.Outcome == "failed" {
			return run.Name
		}
	}
	return "GitHub checks"
}

// broken is a failed check named what: the fix rows.
func broken(f *Facts, what string) ([]Action, bool) {
	limit := f.Config.Fix
	switch {
	case f.Local.Same:
		return fail(f, what+" · The lead changed nothing")
	case limit == 0:
		return fail(f, what+" · Auto-fix is off")
	case f.Ticket.Fix >= limit:
		// A cap lowered after attempts were used reads as all of it used.
		return fail(f, fmt.Sprintf("%s · Auto-fix %d of %d used", what, min(f.Ticket.Fix, limit), limit))
	}
	f.Ticket.Fix++
	return work(f, fmt.Sprintf("Fixing · attempt %d", f.Ticket.Fix), Action{Kind: ActSendFix})
}

// fixing is rows 1 to 5.
func fixing(f *Facts) ([]Action, bool) {
	switch {
	case f.Local.Fix == FixMailed && f.Lead.Turn == "needsYou":
		f.Ticket.Status, f.Ticket.Gate = proto.TicketNeedsYou, proto.GateAsk
		return nil, true
	case f.Local.Fix == FixMailed:
		return work(f, f.Ticket.Why)
	case f.Local.Fix == FixEnded:
		return work(f, "Committing the fix", Action{Kind: ActCommitPush})
	case f.Local.Hint:
		f.Ticket.Hint = true
		return work(f, "Fixing · your hint", Action{Kind: ActSendFix, Text: "you"})
	case f.Local.Fix == FixStopped:
		return fail(f, "The lead's fix turn failed or was stopped")
	}
	return nil, false
}

// OutputLimit is how much of a check command's output is kept.
const OutputLimit = 64 << 10

// CheckLimit is how long one check command may run.
const CheckLimit = 30 * time.Minute

// Tail is the last limit bytes of out, from a line start when one is inside them.
func Tail(out []byte, limit int) string {
	if len(out) <= limit {
		return string(out)
	}
	out = out[len(out)-limit:]
	if i := bytes.IndexByte(out, '\n'); i >= 0 && i < len(out)-1 {
		return string(out[i+1:])
	}
	for len(out) > 0 && !utf8.RuneStart(out[0]) {
		out = out[1:]
	}
	return string(out)
}

// RunCheck runs command with sh in dir and returns its exit code and the end of its output. The code is -1 when
// the command could not start, ran longer than limit, or ctx ended it.
func RunCheck(ctx context.Context, env []string, dir, command string, limit time.Duration) (int, string) {
	code, out, _, err := hostop.Run(ctx, env, dir, []string{"sh", "-c", "exec 2>&1\n" + command}, nil, limit)
	switch {
	case err != nil:
		return -1, "Couldn't run the command: " + err.Error() + "\n"
	case code == -1 && ctx.Err() == nil:
		return -1, Tail(out, OutputLimit) + fmt.Sprintf("Stopped after %s.\n", limit)
	}
	return code, Tail(out, OutputLimit)
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
	if l.State == LocalRunning && !e.flight[t.ID] {
		// pocketd stopped while a command ran: run them again.
		l.State, l.Done = "", 0
	}
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
	if err == nil {
		e.PRChanged(t.Worktree)
	}
}

// runChecks runs the project's commands for the commit in t's checkout, one after another, and stops at the first
// failure.
func (e *Engine) runChecks(t proto.Ticket) {
	if e.flight[t.ID] {
		return
	}
	rec, fresh := e.current(t)
	cmds := e.Configs.Get(t.Folder).Checks
	rec.State, rec.Total, rec.Checks = LocalRunning, len(cmds), nil
	if len(cmds) == 0 {
		rec.State = LocalPassed
		e.save(t.ID, rec, fresh)
		e.post(event{ticket: t.ID})
		return
	}
	e.save(t.ID, rec, fresh)
	e.flight[t.ID] = true
	head, run := rec.Head, e.ctx
	go func() {
		env := e.Env(t.Worktree)
		for _, command := range cmds {
			exit, output := RunCheck(run, env, t.Worktree, command, CheckLimit)
			if run.Err() != nil {
				// A result that arrives after pocketd began to stop is dropped: the record stays running, and the
				// next start reads that as nothing having run.
				return
			}
			check := proto.FactoryCheck{Command: command, Exit: exit, Output: output, Attempt: t.Fix, At: e.now().UnixMilli()}
			e.later(func() { e.checked(t.ID, head, check) })
			if exit != 0 {
				break
			}
		}
		e.later(func() {
			delete(e.flight, t.ID)
			e.post(event{ticket: t.ID})
		})
	}()
}

// checked takes one command's result for the commit head.
func (e *Engine) checked(ticket, head string, c proto.FactoryCheck) {
	rec, ok := ReadRecord(e.Home, ticket)
	if !ok || rec.Head != head || rec.State != LocalRunning {
		return
	}
	rec.Checks = append(rec.Checks, c)
	text := "Passed: " + c.Command
	switch {
	case c.Exit != 0:
		rec.State, text = LocalFailed, fmt.Sprintf("Failed (exit %d): %s", c.Exit, c.Command)
	case len(rec.Checks) == rec.Total:
		rec.State = LocalPassed
	}
	e.save(ticket, rec, false)
	e.say(ticket, "anywhere", "check", text)
	e.post(event{ticket: ticket})
}

// quietAfter is how long after a push GitHub has to report a check before the commit counts as having none.
// GitHub creates the checks of a push within seconds, and the PR cache asks every 30 seconds: three polls.
var quietAfter = 3 * github.WatchedEvery

// PRChanged is github.Service.Listen's callback: the PR of tree changed. It reads the PR off the engine's
// goroutine, under the engine's context, and queues it.
func (e *Engine) PRChanged(tree string) {
	if e == nil || e.GitHub == nil {
		return
	}
	e.later(func() {
		run := e.ctx
		go func() {
			ctx, cancel := context.WithTimeout(run, time.Minute)
			defer cancel()
			pr, problem := e.GitHub.PR(ctx, tree)
			e.later(func() { e.prRead(tree, pr, problem) })
		}()
	})
}

// prRead takes what gh said of tree's PR. A problem is noted once for each Ticket there, until gh reads again.
func (e *Engine) prRead(tree string, pr *proto.PR, problem github.Problem) {
	if pr == nil {
		if problem == "" || e.problems[tree] == problem {
			return
		}
		if e.problems == nil {
			e.problems = map[string]github.Problem{}
		}
		e.problems[tree] = problem
	} else {
		delete(e.problems, tree)
		e.prs[tree] = pr
	}
	all, _ := e.Board.Snapshot()
	for _, t := range all {
		if t.State != proto.TicketLive || t.Station == "" || t.Worktree != tree {
			continue
		}
		if pr == nil {
			e.note(t.ID, proto.FactoryEvent{At: e.now().UnixMilli(), Station: t.Station, Actor: "anywhere", Kind: "check", Text: "Couldn't read the pull request (" + string(problem) + ")"})
			continue
		}
		e.post(event{ticket: t.ID})
	}
}

// watch keeps the PR cache polling the worktrees of the Tickets at Checks and PR review, and arms the wait for
// GitHub's checks of each pushed commit.
func (e *Engine) watch() {
	if e.GitHub == nil {
		return
	}
	all, _ := e.Board.Snapshot()
	var trees []string
	for _, t := range all {
		if t.State != proto.TicketLive || t.Worktree == "" || (t.Station != proto.StationChecks && t.Station != proto.StationPR) {
			continue
		}
		trees = append(trees, t.Worktree)
		if rec, ok := ReadRecord(e.Home, t.ID); ok && t.Station == proto.StationChecks {
			e.settle(t.ID, rec)
		}
	}
	if slices.Equal(trees, e.watched) {
		return
	}
	added := slices.DeleteFunc(slices.Clone(trees), func(tree string) bool { return slices.Contains(e.watched, tree) })
	e.watched = trees
	if len(trees) == 0 {
		e.GitHub.Leave("factory")
		return
	}
	e.GitHub.Listen("factory", e.PRChanged)
	e.GitHub.Watch("factory", trees)
	for _, tree := range added {
		e.PRChanged(tree)
	}
}

// settle arms the wait after which rec's commit, with no GitHub check, counts as having none.
func (e *Engine) settle(ticket string, rec Record) {
	if rec.Pushed == 0 || rec.Quiet || e.armed[ticket] == rec.Head {
		return
	}
	e.armed[ticket] = rec.Head
	head := rec.Head
	wait := max(0, quietAfter-e.now().Sub(time.UnixMilli(rec.Pushed)))
	time.AfterFunc(wait, func() { e.later(func() { e.quiet(ticket, head) }) })
}

// quiet marks the wait for GitHub's checks of the commit head as over.
func (e *Engine) quiet(ticket, head string) {
	rec, ok := ReadRecord(e.Home, ticket)
	if !ok || rec.Head != head || rec.Quiet {
		return
	}
	rec.Quiet = true
	e.save(ticket, rec, false)
	e.post(event{ticket: ticket})
}

// sendFix mails the lead the fix note for t's newest record. by is "you" for the note of a hint.
func (e *Engine) sendFix(t proto.Ticket, by string) {
	rec, fresh := e.current(t)
	limit := e.Configs.Get(t.Folder).Fix
	attempt, hint := t.Fix, ""
	if by == "you" {
		attempt, hint = t.Fix+1, rec.Hint
	}
	checks, pr := rec.Checks, e.prs[t.Worktree]
	if rec.State != LocalFailed {
		checks = nil
	}
	if rec.State != LocalPassed {
		pr = nil
	}
	if err := e.Mail(t.AgentID, FixNote(attempt, limit, hint, checks, pr)); err != nil {
		rec.Hint, rec.Error = "", "Couldn't send the fix note: "+err.Error()
		e.save(t.ID, rec, fresh)
		e.post(event{ticket: t.ID})
		return
	}
	rec.Fix, rec.Hint = FixMailed, ""
	e.save(t.ID, rec, fresh)
	if by == "you" {
		e.say(t.ID, "you", "hint", "Sent your hint to the lead")
		return
	}
	e.say(t.ID, "anywhere", "fix", fmt.Sprintf("Sent the failure to the lead: attempt %d of %d", attempt, limit))
}

// fixEnded records how lead's turn ended while its fix note was out. The next normal end after a stopped fix turn
// counts too, because the mailbox shows a stopped turn's note again.
func (e *Engine) fixEnded(lead string, ok bool) {
	for _, t := range e.atChecks() {
		rec, has := ReadRecord(e.Home, t.ID)
		if t.AgentID != lead || !has || (rec.Fix != FixMailed && rec.Fix != FixStopped) || (rec.Fix == FixStopped && !ok) {
			continue
		}
		rec.Fix = FixStopped
		if ok {
			rec.Fix = FixEnded
			e.say(t.ID, "lead", "fix", "Finished the fix")
		}
		e.save(t.ID, rec, false)
	}
}

// commitPush commits what the fix turn changed and starts the record of the commit. The next step pushes it and
// runs the commands again, also on the same commit, once.
func (e *Engine) commitPush(t proto.Ticket) {
	old, _ := ReadRecord(e.Home, t.ID)
	if _, err := worktree.Commit(t.Worktree, "Fix the failed checks"); err != nil {
		old.Fix, old.Error = "", "Couldn't commit the fix: "+err.Error()
		e.save(t.ID, old, false)
		e.post(event{ticket: t.ID})
		return
	}
	head, _ := worktree.Head(t.Worktree)
	same := head == old.Head
	e.save(t.ID, Record{Head: head, Same: same}, true)
	text := "Committed the fix " + head[:min(7, len(head))]
	if same {
		text = "The lead changed nothing: checking the same commit once more"
	}
	e.say(t.ID, "anywhere", "fix", text)
	e.post(event{ticket: t.ID})
}
