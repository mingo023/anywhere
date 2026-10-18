// Package agent tracks the sessions the phone can see and drive.
package agent

import (
	"cmp"
	"errors"
	"fmt"
	"sort"
	"strings"
	"sync"
	"time"

	"pocketd/internal/hub"
	"pocketd/internal/proto"
	"pocketd/internal/timeline"
)

type Driver interface {
	Prompt(text string) error
	Interrupt() error
	Compact() error
	// Settings switches the model, the effort or both; "" keeps one as it is.
	Settings(model, effort string) error
	Close()
}

type Agent struct {
	id, provider string
	hub          *hub.Hub
	reg          *Registry
	Timeline     *timeline.Timeline

	mu              sync.Mutex
	driver          Driver
	cwd             string
	terminal        string
	conversation    string
	title           string
	fallback        string
	named           string // from naming; outranks title until the Conversation changes
	naming          bool   // naming has started on this Conversation
	conversations   int    // Conversations so far, so a late name for an older one is dropped
	model           string
	effort          string
	phase           string // idle, working or needsYou
	unseenEnd       bool
	failed          bool
	compacting      bool
	pinned          bool
	account         string
	compactFromIdle bool
	attached        bool
	restore         string
	seen            bool
	closed          bool
	createdAt       int64
	updatedAt       int64
	project         string
	worktree        string
	branch          string
	mainWorktree    bool
	tokensUsed      int64
	contextWindow   int64
	origin          string
	chatOnly        bool
	activity        string // the provider's live line; each report replaces it
	stopped         bool   // the user stopped the turn; the next one forgets it
	refilled        bool   // stopped since the last typed prompt: Claude may have put it back in its input
}

type Registry struct {
	mu     sync.Mutex
	agents map[string]*Agent
	views  map[string][]string
	hub    *hub.Hub

	// OnStatus runs under the agent's lock on each status change, so it must
	// stay short and not call back into the registry. Set it before adding agents.
	OnStatus func(id, provider, from, to string)
}

func NewRegistry(h *hub.Hub) *Registry {
	return &Registry{agents: map[string]*Agent{}, views: map[string][]string{}, hub: h}
}

func now() int64 { return time.Now().UnixMilli() }

func (r *Registry) Add(id, cwd, provider string, d Driver) *Agent {
	return r.AddFunc(id, cwd, provider, func(*Agent) Driver { return d })
}

// AddFunc is Add for a driver that needs its agent before any phone can reach it.
func (r *Registry) AddFunc(id, cwd, provider string, newDriver func(*Agent) Driver) *Agent {
	t := now()
	a := &Agent{id: id, cwd: cwd, provider: provider, hub: r.hub, reg: r, Timeline: timeline.New(), phase: "idle", attached: true, origin: "desktop", createdAt: t, updatedAt: t}
	a.driver = newDriver(a)
	r.mu.Lock()
	r.agents[id] = a
	r.mu.Unlock()
	r.hub.Publish(proto.NewAgentUpdate(a.Summary()))
	return a
}

func (r *Registry) Get(id string) (*Agent, error) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if a := r.agents[id]; a != nil {
		return a, nil
	}
	return nil, fmt.Errorf("Unknown agent: %s", id)
}

// ErrResuming refuses a prompt, interrupt or compact while a restored Agent's
// provider is still coming back.
var ErrResuming = errors.New("This session is still resuming")

// Restored is an Agent a pocketd restart brings back under its old id.
type Restored struct {
	ID, Cwd, Provider, Terminal, Conversation string
	Origin                                    string
	Fallback                                  string // title until the provider names the Conversation
	CreatedAt                                 int64
	Done, Failed, Pinned                      bool
	Account                                   string
	ChatOnly                                  bool
	// Named, Phase, Epoch and Seq carry an Agent across an upgrade's exec.
	Named, Phase string
	Epoch, Seq   int64
}

func (r *Registry) Restore(x Restored, d Driver) *Agent {
	a := &Agent{id: x.ID, cwd: x.Cwd, provider: x.Provider, hub: r.hub, reg: r, Timeline: timeline.New(), driver: d, terminal: x.Terminal,
		conversation: x.Conversation, named: x.Named, fallback: x.Fallback, phase: cmp.Or(x.Phase, "idle"), unseenEnd: x.Done, failed: x.Failed, pinned: x.Pinned, account: x.Account, chatOnly: x.ChatOnly, attached: true,
		origin: cmp.Or(x.Origin, "desktop"), createdAt: x.CreatedAt, updatedAt: now()}
	if x.Seq > 0 {
		a.Timeline.Continue(x.Epoch, x.Seq)
	}
	r.mu.Lock()
	r.agents[x.ID] = a
	r.mu.Unlock()
	r.hub.Publish(proto.NewAgentUpdate(a.Summary()))
	return a
}

// Remove marks the agent closed for every phone, then forgets it.
func (r *Registry) Remove(id string) {
	r.mu.Lock()
	a := r.agents[id]
	delete(r.agents, id)
	r.mu.Unlock()
	if a != nil {
		a.update(true, func() { a.closed = true })
	}
}

func (r *Registry) List() []proto.AgentSummary {
	r.mu.Lock()
	all := make([]*Agent, 0, len(r.agents))
	for _, a := range r.agents {
		all = append(all, a)
	}
	r.mu.Unlock()
	out := make([]proto.AgentSummary, len(all))
	for n, a := range all {
		out[n] = a.Summary()
	}
	sort.Slice(out, func(i, j int) bool { return out[i].CreatedAt < out[j].CreatedAt })
	return out
}

// SetView replaces the agents one connection shows. An agent is seen while any connection shows it.
func (r *Registry) SetView(conn string, ids []string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.views[conn] = ids
	r.refreshSeen()
}

func (r *Registry) DropView(conn string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	delete(r.views, conn)
	r.refreshSeen()
}

// refreshSeen runs under r.mu, so two views changing at once can't leave an agent's seen stale.
func (r *Registry) refreshSeen() {
	shown := map[string]bool{}
	for _, ids := range r.views {
		for _, id := range ids {
			shown[id] = true
		}
	}
	for id, a := range r.agents {
		a.setSeen(shown[id])
	}
}

// MarkSeen clears Done once, whether or not anything shows the agents.
func (r *Registry) MarkSeen(ids []string) {
	for _, id := range ids {
		if a, err := r.Get(id); err == nil {
			a.update(false, func() { a.unseenEnd = false })
		}
	}
}

func (a *Agent) ID() string       { return a.id }
func (a *Agent) Provider() string { return a.provider }

func (a *Agent) Named() string {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.named
}

func (a *Agent) Driver() Driver {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.driver
}

func (a *Agent) SetDriver(d Driver) {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.driver = d
}

func (a *Agent) Summary() proto.AgentSummary {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.summary()
}

func (a *Agent) summary() proto.AgentSummary {
	epoch, maxSeq := a.Timeline.State()
	status := a.status()
	return proto.AgentSummary{
		ID: a.id, TerminalID: a.terminal, Title: cmp.Or(a.named, a.title, a.fallback), Cwd: a.cwd, Provider: a.provider, Model: a.model, Effort: a.effort,
		Status: status, Failed: status == "done" && a.failed, Attached: a.attached, Restore: a.restore, Compacting: a.compacting, Pinned: a.pinned, Account: a.account,
		Epoch: epoch, MaxSeq: maxSeq, Resets: a.Timeline.Resets(), ProviderSessionID: a.conversation, CreatedAt: a.createdAt, UpdatedAt: a.updatedAt,
		Project: a.project, Worktree: a.worktree, MainWorktree: a.mainWorktree, Branch: a.branch,
		TokensUsed: a.tokensUsed, ContextWindow: a.contextWindow, Origin: a.origin, Activity: a.activity, ChatOnly: a.chatOnly,
	}
}

func (a *Agent) status() string {
	switch {
	case a.closed:
		return "closed"
	case a.phase != "idle":
		return a.phase
	case a.unseenEnd:
		return "done"
	}
	return "idle"
}

// update runs f and publishes one agent.update if phones would see a change.
// A status machine step (touch) also moves updatedAt; being seen or a setter does not.
func (a *Agent) update(touch bool, f func()) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.closed {
		return
	}
	before := a.summary()
	f()
	after := a.summary()
	if after == before {
		return
	}
	if touch {
		a.updatedAt = now()
		after.UpdatedAt = a.updatedAt
	}
	a.hub.Publish(proto.NewAgentUpdate(after))
	if after.Status != before.Status && a.reg.OnStatus != nil {
		a.reg.OnStatus(a.id, a.provider, before.Status, after.Status)
	}
}

// Busy is whether any agent is working or waiting on the user.
func Busy(agents []proto.AgentSummary) bool {
	for _, a := range agents {
		if a.Status == "working" || a.Status == "needsYou" {
			return true
		}
	}
	return false
}

func (a *Agent) Working() {
	a.update(true, func() {
		a.enter("working")
		a.unseenEnd, a.failed, a.restore = false, false, ""
		a.stopped = false
	})
}

// enter moves to phase. The activity belonged to the phase left, so it goes too.
func (a *Agent) enter(phase string) {
	if a.phase != phase {
		a.phase, a.activity = phase, ""
	}
}

func (a *Agent) NeedsYou() {
	a.update(true, func() { a.enter("needsYou") })
}

// TurnEnded is Done unless a phone or the desktop shows the agent right now.
func (a *Agent) TurnEnded(failed bool) {
	a.update(true, func() { a.turnEnded(failed) })
}

func (a *Agent) turnEnded(failed bool) {
	if a.phase != "idle" {
		a.enter("idle")
		a.unseenEnd, a.failed = !a.seen, failed
		a.endCompactFromIdle()
	}
}

// Clear ends a turn the user stopped, so it is not Done.
func (a *Agent) Clear() {
	a.update(true, func() {
		if a.phase != "idle" {
			a.enter("idle")
			a.unseenEnd = false
			a.endCompactFromIdle()
		}
	})
}

// Stop notes a stop asked for from a client.
func (a *Agent) Stop() {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.stopped, a.refilled = true, true
}

// Escaped notes an Esc or Ctrl+C typed to the agent, a stop only while it works.
func (a *Agent) Escaped() {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.phase == "working" {
		a.stopped, a.refilled = true, true
	}
}

// Sent notes the user sent what the agent's input held, a stopped prompt it
// put back included.
func (a *Agent) Sent() {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.refilled = false
}

// Refilled says whether a stop came since the last call, so the agent's input
// may hold the stopped prompt.
func (a *Agent) Refilled() bool {
	a.mu.Lock()
	defer a.mu.Unlock()
	r := a.refilled
	a.refilled = false
	return r
}

// EndStopped ends a turn the user stopped before any reply as interrupted,
// if nothing reached the timeline past seq since: Claude marks no such stop.
func (a *Agent) EndStopped(seq int64) {
	a.mu.Lock()
	defer a.mu.Unlock()
	_, now := a.Timeline.State()
	if a.stopped && a.phase == "idle" && now == seq && a.Timeline.Unanswered() {
		a.stopped = false
		a.record(timeline.Event{Kind: "result", Error: "interrupted"})
	}
}

func (a *Agent) endCompactFromIdle() {
	if a.compactFromIdle {
		a.compacting, a.compactFromIdle = false, false
	}
}

func (a *Agent) SetCompacting() {
	a.update(true, func() {
		a.compacting = true
		if a.phase == "idle" {
			a.enter("working")
			a.compactFromIdle = true
		}
	})
}

// Compacted ends a compaction; one the user started from idle ends like a turn.
func (a *Agent) Compacted() {
	a.update(true, func() {
		a.compacting = false
		if a.compactFromIdle {
			a.compactFromIdle = false
			a.turnEnded(false)
		}
	})
}

func (a *Agent) setSeen(seen bool) {
	a.update(false, func() {
		a.seen = seen
		if seen {
			a.unseenEnd = false
		}
	})
}

func (a *Agent) SetTerminal(id string) {
	a.update(false, func() { a.terminal = id })
}

func (a *Agent) SetAttached(attached bool) {
	a.update(false, func() { a.attached = attached })
}

// SetRestore shows how the Agent came back from a restart, until its next turn.
func (a *Agent) SetRestore(outcome string) {
	a.update(false, func() { a.restore = outcome })
}

func (a *Agent) SetCwd(cwd string) {
	a.update(false, func() { a.cwd = cwd })
}

// SetConversation tracks the provider's conversation id. A switch to another
// conversation (/clear, /resume, /new) starts the timeline and title over.
func (a *Agent) SetConversation(id string) {
	a.update(false, func() {
		if a.conversation != "" && a.conversation != id {
			a.Timeline.Clear()
			a.title = ""
			a.named = ""
			a.naming = false
			a.conversations++
			a.tokensUsed = 0
		}
		a.conversation = id
	})
}

// ClearTimeline starts the timeline over in the same conversation and tells clients.
func (a *Agent) ClearTimeline() {
	a.update(false, a.Timeline.Clear)
}

// SetTitle takes the provider's title; it outranks the one taken from the first prompt.
func (a *Agent) SetTitle(title string) {
	a.update(false, func() { a.title = title })
}

// ClaimNaming starts naming the Conversation unless it has a name or naming
// already started on it. The claim is what SetNamed takes.
func (a *Agent) ClaimNaming() (claim int, ok bool) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.named != "" || a.naming {
		return 0, false
	}
	a.naming = true
	return a.conversations, true
}

// SetNamed takes naming's title for the session, unless the Conversation it
// was claimed on has since changed.
func (a *Agent) SetNamed(claim int, title string) {
	a.update(false, func() {
		if claim == a.conversations {
			a.named = title
		}
	})
}

func (a *Agent) SetModel(model string) {
	a.update(false, func() { a.model = model })
}

func (a *Agent) SetEffort(effort string) {
	a.update(false, func() { a.effort = effort })
}

// SetAccount names the managed account the agent runs on; "" is the built-in login.
func (a *Agent) SetAccount(id string) {
	a.update(false, func() { a.account = id })
}

func (a *Agent) SetPinned(pinned bool) {
	a.update(false, func() { a.pinned = pinned })
}

// SetLocation places the agent in a registered Project; all "" when it is in none.
func (a *Agent) SetLocation(project, worktree, branch string, main bool) {
	a.update(false, func() { a.project, a.worktree, a.branch, a.mainWorktree = project, worktree, branch, main })
}

// SetTokens takes the context in use; a window of 0 keeps the one already known.
func (a *Agent) SetTokens(used, window int64) {
	a.update(false, func() {
		a.tokensUsed = used
		if window > 0 {
			a.contextWindow = window
		}
	})
}

func (a *Agent) SetOrigin(origin string) {
	a.update(false, func() { a.origin = origin })
}

func (a *Agent) SetChatOnly() { a.update(false, func() { a.chatOnly = true }) }

func (a *Agent) SetActivity(text string) { a.update(false, func() { a.activity = text }) }

func firstLine(text string) string {
	line, _, _ := strings.Cut(strings.TrimSpace(text), "\n")
	if r := []rune(line); len(r) > 80 {
		return string(r[:80])
	}
	return line
}

// Record adds one provider event to the timeline and tells every phone. It leaves the status alone.
func (a *Agent) Record(e timeline.Event) {
	sub := e.ParentToolUseID != ""
	if e.Kind == "user" && !sub {
		a.Timeline.StartEpoch()
		a.update(false, func() {
			if a.title == "" {
				a.title = firstLine(e.Text)
			}
		})
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	a.record(e)
}

func (a *Agent) record(e timeline.Event) {
	sub := e.ParentToolUseID != ""
	if a.closed {
		return
	}
	if item, ok := a.Timeline.Apply(e, now()); ok {
		epoch, _ := a.Timeline.State()
		if sub {
			a.hub.PublishSubagent(proto.NewAgentStream(a.id, epoch, item))
		} else {
			a.hub.Publish(proto.NewAgentStream(a.id, epoch, item))
		}
	}
}
