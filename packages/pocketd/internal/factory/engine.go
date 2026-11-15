package factory

import (
	"cmp"
	"context"
	"errors"
	"log"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/board"
	"pocketd/internal/github"
	"pocketd/internal/launch"
	"pocketd/internal/proto"
	"pocketd/internal/team"
)

const (
	// reads is how often a turn's end is looked for before the turn is taken as it stands.
	reads = 5
	// turnItems bounds the items a turn is read from.
	turnItems = 500
)

// ErrOff refuses every verb of a nil Engine.
var ErrOff = errors.New("The Factory is off")

// Engine moves every Factory ticket through its stations. Hooks only queue what happened. Run handles the queue
// in order on one goroutine, and step is the one place a Ticket is decided. A nil Engine ignores every hook.
type Engine struct {
	Home    string
	Board   *board.Store
	Configs *Configs
	Teams   *team.Teams
	Agents  *agent.Registry
	Now     func() time.Time
	// Poll is the wait between reads of a turn whose end isn't in the timeline yet. Zero is a second.
	Poll time.Duration
	// Launch starts a session as the owner.
	Launch func(key, requestID string, s proto.LaunchSpec, created func(cwd string)) launch.Result
	// Mail queues text for an agent's chat (daemon.Mail).
	Mail func(agentID, text string) error
	// Profiles are the profiles a session may run with.
	Profiles *Profiles
	// GitHub opens the draft PR and reports its checks.
	GitHub *github.Service
	// Env is the login environment a check command runs with: the function github.New gets.
	Env func(dir string) []string

	// mu guards specs and changed.
	mu sync.Mutex
	// specs is how far each Ticket at Spec got, and where its lead's timeline stood at the prompt.
	specs    map[string]asked
	reviewMu sync.Mutex
	// reviewers is each open reviewer's agent id and its Ticket: whose asks Deny answers.
	reviewers map[string]string
	// advance holds the Tickets the user moved on from Implement, until Next has moved them. mu guards it.
	advance map[string]bool
	changed chan string
	// recovered is set once the restored sessions are back, so a lead still resuming isn't taken for gone.
	recovered atomic.Bool

	qmu   sync.Mutex
	queue []event
	wake  chan struct{}

	// Only the goroutine that drains the queue reads and writes the fields below.
	ctx context.Context
	// flight holds the Tickets with a push or a check command on another goroutine.
	flight map[string]bool
	// prs is the newest PR of each worktree.
	prs map[string]*proto.PR
	// watched is the worktrees the PR cache polls for the engine.
	watched []string
	// armed is, per Ticket, the commit whose wait for GitHub's checks is armed.
	armed map[string]string
}

type asked struct {
	state SpecState
	since int64
}

// event is something that happened, queued by a hook that may run under an agent's lock.
type event struct {
	ticket string // the Ticket to step, or
	agent  string // the agent whose Ticket to step
	turn   bool   // the agent's turn ended
	ok     bool   // and ended normally
	do     func() // or a function to run, for work that was done on another goroutine
}

// Owns is whether t is the engine's to move: it has a station, or it has no session yet and its project's
// Factory is on.
func (e *Engine) Owns(t proto.Ticket) bool {
	return e != nil && (t.Station != "" || t.AgentID == "" && e.Configs.Get(t.Folder).Enabled)
}

// Start is the board.Starter for a Ticket the engine owns: it puts t at its first station and launches its lead.
func (e *Engine) Start(requestID string, t proto.Ticket, created func(cwd string)) launch.Result {
	cfg := e.Configs.Get(t.Folder)
	station := cmp.Or(t.Station, First(cfg))
	at := e.now().UnixMilli()
	profile, known := e.Profiles.Get(cfg.Lead)
	e.update(t.ID, func(x *proto.Ticket) {
		x.Station, x.Gate, x.Status, x.Why = station, "", proto.TicketStarting, ""
		if known {
			x.Provider = profile.Provider
		}
	})
	if t.Station == "" {
		e.note(t.ID, proto.FactoryEvent{At: at, Station: station, Actor: "anywhere", Kind: "moved"})
	}
	res := launch.Result{Err: &launch.Failure{Code: proto.CodeUnknownProfile, Message: "The lead's profile is gone. Pick one in Settings › Factory"}}
	if known {
		res = e.Launch("factory:"+t.ID, requestID, leadSpec(t, requestID, profile), created)
	}
	if res.Err != nil {
		e.note(t.ID, proto.FactoryEvent{At: e.now().UnixMilli(), Station: station, Actor: "anywhere", Kind: "failed", Text: res.Err.Message})
		return res
	}
	for _, skill := range res.Skipped {
		e.note(t.ID, proto.FactoryEvent{At: e.now().UnixMilli(), Station: station, Actor: "lead", Kind: "skill", Text: skippedSkill(skill)})
	}
	// Claimed before the first prompt, so the spec request doesn't become the session's title.
	e.rename(t, res.AgentID)
	e.mu.Lock()
	delete(e.specs, t.ID)
	e.mu.Unlock()
	// The scheduler records the same ids once Start returns. They are written here first, so the step queued below
	// finds the lead.
	e.update(t.ID, func(x *proto.Ticket) {
		x.AgentID, x.TerminalID = res.AgentID, res.TerminalID
		if res.Cwd != "" {
			x.Worktree = res.Cwd
		}
	})
	e.post(event{ticket: t.ID})
	return res
}

// leadSpec is the lead's session: a chat that may edit, in the Ticket's own worktree, with no first prompt.
func leadSpec(t proto.Ticket, requestID string, p proto.Profile) proto.LaunchSpec {
	s := proto.LaunchSpec{Project: t.Folder, Provider: p.Provider, Model: p.Model, Profile: p.ID, Access: "edits", Chat: true}
	if t.Worktree != "" {
		s.Checkout = proto.Checkout{Worktree: t.Worktree}
		return s
	}
	// The branch is pushed and shown on the PR, so it reads as the Ticket's title. The request id keeps it unique.
	name := cmp.Or(launch.Slug(t.Title), "ticket") + "-" + requestID[:min(4, len(requestID))]
	s.Checkout = proto.Checkout{New: &proto.NewWorktree{Name: name, Base: t.Base}}
	return s
}

// skippedSkill is the activity line for a profile's skill the checkout kept its own folder for.
func skippedSkill(name string) string {
	return "Skill " + name + " was not copied: the checkout has its own"
}

// Status is Registry.OnStatus's. It runs under the agent's lock, so it only queues.
func (e *Engine) Status(id, from, to string) {
	if e != nil {
		e.post(event{agent: id})
	}
}

// TurnEnded is Registry.OnTurnEnd's. It only queues.
func (e *Engine) TurnEnded(id string, ok bool) {
	if e != nil {
		e.later(func() { e.fixEnded(id, ok) })
		e.post(event{agent: id, turn: true, ok: ok})
	}
}

// TeamChanged is Teams.OnChange's. It runs under Teams.mu, so it only queues.
func (e *Engine) TeamChanged(t proto.Team) {
	if e != nil {
		e.later(func() { e.integrated(t) })
		e.post(event{agent: t.Lead})
	}
}

func (e *Engine) post(ev event) {
	e.qmu.Lock()
	e.queue = append(e.queue, ev)
	e.qmu.Unlock()
	select {
	case e.wakeup() <- struct{}{}:
	default:
	}
}

func (e *Engine) wakeup() chan struct{} {
	e.qmu.Lock()
	defer e.qmu.Unlock()
	if e.wake == nil {
		e.wake = make(chan struct{}, 1)
	}
	return e.wake
}

// Run handles what happened, in order, until ctx ends.
func (e *Engine) Run(ctx context.Context) {
	if e == nil {
		return
	}
	wake := e.wakeup()
	for {
		select {
		case <-ctx.Done():
			return
		case <-wake:
		}
		e.drain(ctx)
	}
}

// drain handles the queue until it is empty. Handling an event can queue more.
func (e *Engine) drain(ctx context.Context) {
	e.ctx = ctx
	if e.flight == nil {
		e.flight, e.prs, e.armed = map[string]bool{}, map[string]*proto.PR{}, map[string]string{}
	}
	for {
		e.qmu.Lock()
		queue := e.queue
		e.queue = nil
		e.qmu.Unlock()
		if len(queue) == 0 {
			return
		}
		for _, ev := range queue {
			// Shutting down closes every session. Those closes aren't leads failing.
			if ctx.Err() != nil {
				return
			}
			e.handle(ev)
		}
		e.watch()
	}
}

func (e *Engine) handle(ev event) {
	if ev.do != nil {
		ev.do()
		return
	}
	if ev.ticket != "" {
		e.step(ev.ticket, ev)
		return
	}
	tickets, _ := e.Board.Snapshot()
	reviewed, _ := e.reviewerOf(ev.agent)
	for _, t := range tickets {
		if (t.AgentID == ev.agent || reviewed == t.ID) && t.Station != "" && t.State == proto.TicketLive {
			e.step(t.ID, ev)
		}
	}
}

// step decides one Ticket: it gathers the facts, asks Next, writes the Ticket, notes what changed, and does the
// actions.
func (e *Engine) step(id string, ev event) {
	t, ok := e.ticket(id)
	if !ok || t.State != proto.TicketLive || t.Station == "" || t.AgentID == "" {
		return
	}
	f := e.facts(t, ev)
	if !f.Lead.Present && !e.recovered.Load() {
		return
	}
	next, acts := Next(f)
	if !same(t, next) {
		at := e.now().UnixMilli()
		ok := e.update(id, func(x *proto.Ticket) {
			// The user may have closed it since the snapshot.
			if x.State != proto.TicketLive {
				return
			}
			x.Station, x.Gate, x.Status, x.Why = next.Station, next.Gate, next.Status, next.Why
			x.AgentID, x.TerminalID, x.ChangedAt = next.AgentID, next.TerminalID, at
			x.PR, x.Fix, x.Hint = next.PR, next.Fix, next.Hint
		})
		if !ok {
			return
		}
		for _, n := range Notes(t, next, f.Team, at) {
			e.note(id, n)
		}
		e.tell(id)
	}
	for _, a := range acts {
		if !e.act(next, a) {
			return
		}
	}
}

// same compares the fields Next may change.
func same(a, b proto.Ticket) bool {
	return a.Station == b.Station && a.Gate == b.Gate && a.Status == b.Status && a.Why == b.Why && a.AgentID == b.AgentID && a.TerminalID == b.TerminalID && a.PR == b.PR && a.Fix == b.Fix && a.Hint == b.Hint
}

func (e *Engine) facts(t proto.Ticket, ev event) Facts {
	f := Facts{Ticket: t, Config: e.Configs.Get(t.Folder)}
	lead, ok := e.lead(t.AgentID)
	if !ok {
		if t.Station == proto.StationPR {
			e.prFacts(&f, ev)
		}
		return f
	}
	f.Lead = Session{Present: true, Turn: lead.Turn()}
	f.Advance = e.advancing(t)
	tm, found, err := e.Teams.Store.Get(t.AgentID)
	if err != nil {
		log.Printf("factory: ticket %s: %v", t.ID, err)
	}
	if found {
		f.Team = &tm
	}
	var since int64
	f.Spec, since = e.spec(t)
	if f.Spec == SpecAsked || f.Spec == SpecRepaired {
		f.Reply = e.read(lead, since, ev)
	}
	switch t.Station {
	case proto.StationChecks:
		f.Local = e.local(t)
		f.PR = e.prs[t.Worktree]
	case proto.StationPR:
		e.prFacts(&f, ev)
	}
	return f
}

// lead is the session with this id, and whether it is still there: not closed, and back after a restart.
func (e *Engine) lead(id string) (*agent.Agent, bool) {
	a, err := e.Agents.Get(id)
	if err != nil {
		return nil, false
	}
	s := a.Summary()
	return a, s.Status != "closed" && s.Restore != proto.RestoreFailed
}

// spec is how far t's Spec station got, and where the lead's timeline stood at the last prompt.
func (e *Engine) spec(t proto.Ticket) (SpecState, int64) {
	if _, err := os.Stat(specPath(e.Home, t.ID)); err == nil {
		return SpecSaved, 0
	}
	if t.Station != proto.StationSpec {
		return SpecNone, 0
	}
	if t.Status == proto.TicketFailed {
		return SpecFailed, 0
	}
	e.mu.Lock()
	defer e.mu.Unlock()
	a, ok := e.specs[t.ID]
	if !ok {
		return SpecNone, 0
	}
	return a.state, a.since
}

// read is the lead's turn after since. When ev says the turn ended, it looks again while the end hasn't reached
// the timeline. An end that never shows is taken as it stands, unless the lead works on a newer turn.
func (e *Engine) read(lead *agent.Agent, since int64, ev event) Turn {
	var r Turn
	for i := range reads {
		if i > 0 {
			time.Sleep(cmp.Or(e.Poll, time.Second))
		}
		items, _ := lead.Timeline.Page(since, turnItems)
		r.Text, r.Ended, r.Failed = team.Reply(items)
		if r.Ended || !ev.turn {
			return r
		}
	}
	if lead.Turn() != "working" {
		r.Ended, r.Failed = true, !ev.ok
	}
	return r
}

// act does one action for t. False stops the actions after it.
func (e *Engine) act(t proto.Ticket, a Action) bool {
	// These need no lead: it may be gone, and the PR is still yours to merge.
	switch a.Kind {
	case ActCloseLead:
		e.closeLead(a.Text)
		return true
	case ActCloseReviewer:
		e.closeReviewer(t)
		return true
	case ActMarkReady:
		e.markReady(t)
		return true
	case ActClose:
		e.merged(t)
		return true
	}
	lead, ok := e.lead(t.AgentID)
	if !ok {
		return false
	}
	switch a.Kind {
	case ActAskSpec:
		return e.ask(t, lead, SpecAsked, SpecRequest(t))
	case ActRepairSpec:
		return e.ask(t, lead, SpecRepaired, a.Text)
	case ActSaveSpec:
		if err := saveSpec(e.Home, t.ID, a.Text); err != nil {
			e.fail(t.ID, "The spec couldn't be saved: "+err.Error())
			return false
		}
		e.tell(t.ID)
	case ActAskPlan:
		text := PlanRequest(t, readSpec(e.Home, t.ID)) + ProfileHint(e.Profiles.List())
		if err := e.Teams.Prompt(lead, text); err != nil {
			e.fail(t.ID, "The lead couldn't be asked for a plan: "+err.Error())
			return false
		}
	case ActAwait:
		lead.SetAwaiting(true)
	case ActIntegrate:
		e.integrate(t)
	case ActOpenPR:
		e.openPR(t)
	case ActRunChecks:
		e.runChecks(t)
	case ActSendFix:
		e.sendFix(t, a.Text)
	case ActCommitPush:
		e.commitPush(t)
	case ActStartReview:
		if pr := e.prs[t.Worktree]; pr != nil {
			e.startReview(t, e.Configs.Get(t.Folder), pr)
		}
	case ActRepairReview:
		if pr := e.prs[t.Worktree]; pr != nil {
			e.repairReview(t, pr)
		}
	}
	return true
}

// ask prompts the lead for its spec, noting where its timeline stood so only the answer is read.
func (e *Engine) ask(t proto.Ticket, lead *agent.Agent, state SpecState, text string) bool {
	_, seq := lead.Timeline.State()
	e.mu.Lock()
	if e.specs == nil {
		e.specs = map[string]asked{}
	}
	e.specs[t.ID] = asked{state: state, since: seq}
	e.mu.Unlock()
	d := lead.Driver()
	if d == nil {
		e.fail(t.ID, "The lead couldn't be asked: its session is not a chat")
		return false
	}
	if err := d.Prompt(text); err != nil {
		e.fail(t.ID, "The lead couldn't be asked: "+err.Error())
		return false
	}
	return true
}

// fail records an action that could not be done. A Ticket at Spec or Plan loses its lead, so it can Start again.
func (e *Engine) fail(id, why string) {
	var station, lead string
	at := e.now().UnixMilli()
	ok := e.update(id, func(x *proto.Ticket) {
		x.Status, x.Why, x.Gate, x.ChangedAt = proto.TicketFailed, why, "", at
		station = x.Station
		if station == proto.StationSpec || station == proto.StationPlan {
			lead, x.AgentID, x.TerminalID = x.AgentID, "", ""
		}
	})
	if ok {
		e.note(id, proto.FactoryEvent{At: at, Station: station, Actor: "anywhere", Kind: "failed", Text: why})
		e.closeLead(lead)
	}
}

// closeLead closes the session with this id, if it is still there.
func (e *Engine) closeLead(id string) {
	if a, err := e.Agents.Get(id); err == nil {
		if d := a.Driver(); d != nil {
			d.Close()
		}
	}
}

func (e *Engine) update(id string, change func(*proto.Ticket)) bool {
	if err := e.Board.Update(id, change); err != nil {
		log.Printf("factory: ticket %s: %v", id, err)
		return false
	}
	return true
}

func (e *Engine) ticket(id string) (proto.Ticket, bool) {
	tickets, _ := e.Board.Snapshot()
	i := slices.IndexFunc(tickets, func(t proto.Ticket) bool { return t.ID == id })
	if i < 0 {
		return proto.Ticket{}, false
	}
	return tickets[i], true
}

func (e *Engine) now() time.Time {
	if e.Now != nil {
		return e.Now()
	}
	return time.Now()
}

// Changed carries the id of each Ticket the engine wrote, for the factory.update feed.
func (e *Engine) Changed() <-chan string {
	if e == nil {
		return nil
	}
	return e.changes()
}

func (e *Engine) changes() chan string {
	e.mu.Lock()
	defer e.mu.Unlock()
	if e.changed == nil {
		e.changed = make(chan string, 64)
	}
	return e.changed
}

// tell never blocks: an update is a hint, and a full channel drops it.
func (e *Engine) tell(ticket string) {
	select {
	case e.changes() <- ticket:
	default:
	}
}

// Detail is what factory.ticket answers with.
func (e *Engine) Detail(ticket string) (proto.FactoryTicket, error) {
	out := proto.FactoryTicket{TicketID: ticket, Activity: []proto.FactoryEvent{}}
	if e == nil {
		return out, ErrOff
	}
	// The id names a folder, so only an id the Board holds is read.
	if _, ok := e.ticket(ticket); !ok {
		return out, board.ErrUnknown
	}
	out.Spec = readSpec(e.Home, ticket)
	acts, err := ReadActivity(e.Home, ticket)
	if err != nil {
		return out, err
	}
	out.Activity = append(out.Activity, acts...)
	out.Findings = readReview(e.Home, ticket).Findings
	if rec, ok := ReadRecord(e.Home, ticket); ok {
		out.Checks = rec.Checks
		out.NoChecks = rec.State == LocalPassed && rec.Total == 0 && rec.Quiet
	}
	return out, nil
}

func specPath(home, ticket string) string {
	return filepath.Join(dir(home, ticket), "spec.md")
}

func saveSpec(home, ticket, text string) error {
	if err := os.MkdirAll(dir(home, ticket), 0o700); err != nil {
		return err
	}
	return os.WriteFile(specPath(home, ticket), []byte(text+"\n"), 0o600)
}

// readSpec is the saved spec, or "" when there is none.
func readSpec(home, ticket string) string {
	raw, _ := os.ReadFile(specPath(home, ticket))
	return strings.TrimSpace(string(raw))
}
