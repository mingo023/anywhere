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
	id, cwd, provider string
	driver            Driver
	hub               *hub.Hub
	Timeline          *timeline.Timeline

	mu        sync.Mutex
	title     string
	model     string
	status    string
	createdAt int64
	updatedAt int64
}

type Registry struct {
	mu     sync.Mutex
	agents map[string]*Agent
	hub    *hub.Hub
}

func NewRegistry(h *hub.Hub) *Registry {
	return &Registry{agents: map[string]*Agent{}, hub: h}
}

func now() int64 { return time.Now().UnixMilli() }

func (r *Registry) Add(id, cwd, provider string, d Driver) *Agent {
	return r.AddFunc(id, cwd, provider, func(*Agent) Driver { return d })
}

// AddFunc is Add for a driver that needs its agent before any phone can reach it.
func (r *Registry) AddFunc(id, cwd, provider string, newDriver func(*Agent) Driver) *Agent {
	t := now()
	a := &Agent{id: id, cwd: cwd, provider: provider, hub: r.hub, Timeline: timeline.New(), status: "idle", createdAt: t, updatedAt: t}
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
		a.setStatus("closed")
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

func (a *Agent) ID() string     { return a.id }
func (a *Agent) Driver() Driver { return a.driver }

func (a *Agent) Summary() proto.AgentSummary {
	epoch, maxSeq := a.Timeline.State()
	a.mu.Lock()
	defer a.mu.Unlock()
	return proto.AgentSummary{
		ID: a.id, Title: a.title, Cwd: a.cwd, Provider: a.provider, Model: a.model, Status: a.status,
		Epoch: epoch, MaxSeq: maxSeq, ProviderSessionID: a.id, CreatedAt: a.createdAt, UpdatedAt: a.updatedAt,
	}
}

func (a *Agent) setStatus(status string) {
	a.mu.Lock()
	a.status, a.updatedAt = status, now()
	a.mu.Unlock()
	a.hub.Publish(proto.NewAgentUpdate(a.Summary()))
}

func (a *Agent) SetCompacting() { a.setStatus("compacting") }

// SetTitle takes the provider's title; it outranks the one taken from the first prompt.
func (a *Agent) SetTitle(title string) {
	a.mu.Lock()
	a.title = title
	a.mu.Unlock()
	a.hub.Publish(proto.NewAgentUpdate(a.Summary()))
}

func (a *Agent) SetModel(model string) {
	a.mu.Lock()
	changed := a.model != model
	a.model = model
	a.mu.Unlock()
	if changed {
		a.hub.Publish(proto.NewAgentUpdate(a.Summary()))
	}
}

func firstLine(text string) string {
	line, _, _ := strings.Cut(strings.TrimSpace(text), "\n")
	if r := []rune(line); len(r) > 80 {
		return string(r[:80])
	}
	return line
}

// Apply records one provider event and tells every phone.
func (a *Agent) Apply(e timeline.Event) {
	if e.Kind == "user" {
		a.Timeline.StartEpoch()
		a.mu.Lock()
		if a.title == "" {
			a.title = firstLine(e.Text)
		}
		a.mu.Unlock()
	}
	item, ok := a.Timeline.Apply(e, now())
	if ok {
		epoch, _ := a.Timeline.State()
		a.hub.Publish(proto.NewAgentStream(a.id, epoch, item))
	}
	switch e.Kind {
	case "user":
		a.setStatus("running")
	case "result", "compacted", "error":
		a.setStatus("idle")
	}
}
