// Package agent tracks the sessions the phone can see and drive.
package agent

import (
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
	Close()
}

type Agent struct {
	id, provider string
	driver       Driver
	hub          *hub.Hub
	reg          *Registry
	Timeline     *timeline.Timeline

	mu              sync.Mutex
	cwd             string
	terminal        string
	conversation    string
	title           string
	model           string
	phase           string // idle, working or needsYou
	unseenEnd       bool
	failed          bool
	compacting      bool
	compactFromIdle bool
	attached        bool
	seen            bool
	closed          bool
	createdAt       int64
	updatedAt       int64
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
	a := &Agent{id: id, cwd: cwd, provider: provider, hub: r.hub, reg: r, Timeline: timeline.New(), phase: "idle", attached: true, createdAt: t, updatedAt: t}
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
func (a *Agent) Driver() Driver   { return a.driver }

func (a *Agent) Summary() proto.AgentSummary {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.summary()
}

func (a *Agent) summary() proto.AgentSummary {
	epoch, maxSeq := a.Timeline.State()
	status := a.status()
	return proto.AgentSummary{
		ID: a.id, TerminalID: a.terminal, Title: a.title, Cwd: a.cwd, Provider: a.provider, Model: a.model,
		Status: status, Failed: status == "done" && a.failed, Attached: a.attached, Compacting: a.compacting,
		Epoch: epoch, MaxSeq: maxSeq, ProviderSessionID: a.conversation, CreatedAt: a.createdAt, UpdatedAt: a.updatedAt,
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
	a.update(true, func() { a.phase, a.unseenEnd, a.failed = "working", false, false })
}

func (a *Agent) NeedsYou() {
	a.update(true, func() { a.phase = "needsYou" })
}

// TurnEnded is Done unless a phone or the desktop shows the agent right now.
func (a *Agent) TurnEnded(failed bool) {
	a.update(true, func() { a.turnEnded(failed) })
}

func (a *Agent) turnEnded(failed bool) {
	if a.phase != "idle" {
		a.phase, a.unseenEnd, a.failed = "idle", !a.seen, failed
		a.endCompactFromIdle()
	}
}

// Clear ends a turn the user stopped, so it is not Done.
func (a *Agent) Clear() {
	a.update(true, func() {
		if a.phase != "idle" {
			a.phase, a.unseenEnd = "idle", false
			a.endCompactFromIdle()
		}
	})
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
			a.phase, a.compactFromIdle = "working", true
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
		}
		a.conversation = id
	})
}

// SetTitle takes the provider's title; it outranks the one taken from the first prompt.
func (a *Agent) SetTitle(title string) {
	a.update(false, func() { a.title = title })
}

func (a *Agent) SetModel(model string) {
	a.update(false, func() { a.model = model })
}

func firstLine(text string) string {
	line, _, _ := strings.Cut(strings.TrimSpace(text), "\n")
	if r := []rune(line); len(r) > 80 {
		return string(r[:80])
	}
	return line
}

// Record adds one provider event to the timeline and tells every phone. It leaves the status alone.
func (a *Agent) Record(e timeline.Event) {
	if e.Kind == "user" {
		a.Timeline.StartEpoch()
		a.update(false, func() {
			if a.title == "" {
				a.title = firstLine(e.Text)
			}
		})
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.closed {
		return
	}
	if item, ok := a.Timeline.Apply(e, now()); ok {
		epoch, _ := a.Timeline.State()
		a.hub.Publish(proto.NewAgentStream(a.id, epoch, item))
	}
}
