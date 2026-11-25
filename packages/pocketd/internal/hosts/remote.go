package hosts

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"slices"
	"strings"
	"sync"
	"time"

	"pocketd/internal/proto"
)

type Mirror interface {
	Summary(proto.AgentSummary)
	Item(session string, epoch int64, it proto.Item)
	// Asked is a mirrored session's question or permission ask, under tagged ids; Answered is it closing.
	Asked(req proto.PermissionRequest)
	Answered(session, request string)
	// Live is host's sessions mirrored here that haven't closed, as last stored; Open is a session's asks still
	// open here, by tagged id.
	Live(host string) []proto.AgentSummary
	Open(session string) []string
	// Removed is host forgotten, with name as it was shown.
	Removed(host, name string)
	// Linked is host's link up, with what its hello sent mirrored.
	Linked(host string)
	// Gone is a mirrored session its host no longer has: its pocketd came back without it. name is the host's.
	Gone(session, name string)
}

var errInvalidID = errors.New("The host sent an id this Mac doesn't take")

// safe refuses an id from a host that could climb out of a folder, were it ever joined to a path.
func safe(id string) bool { return id != "" && !strings.Contains(id, "..") }

// mirrored is a host's session as this Mac keeps it: under its tag, with no terminal here.
func mirrored(host string, s proto.AgentSummary) (proto.AgentSummary, bool) {
	if !safe(s.ID) || s.Parent != "" && !safe(s.Parent) {
		return s, false
	}
	s.ID = Tag(host, s.ID)
	if s.Parent != "" {
		s.Parent = Tag(host, s.Parent)
	}
	s.TerminalID, s.Delegation = "", nil
	return s, true
}

func busy(status string) bool { return status == "working" || status == "needsYou" }

type remote struct {
	m    *Manager
	host string
	// ready closes once client is set, or the dial failed and it stays nil.
	ready  chan struct{}
	client *Client
	// prev is the host's remote before this one, whose cursors this one takes up once finished closes.
	prev     *remote
	finished chan struct{}

	mu    sync.Mutex
	queue []func()
	wake  chan struct{}

	// The rest belongs to run's goroutine. latest is every session on the host as last heard.
	latest  map[string]proto.AgentSummary
	tracked map[string]bool
	asks    map[string]string
	// early is the open asks of sessions not tracked yet, by request id: a session Launch started can ask
	// before agent.created returns. kept is the asks still open here from before this link.
	early map[string]proto.PermissionRequest
	kept  map[string]bool
	seq   map[string]int64
	// started is a turn the host showed busy; ended is its result stored.
	started map[string]bool
	ended   map[string]bool
}

func newRemote(m *Manager, host string, prev *remote) *remote {
	return &remote{m: m, host: host, ready: make(chan struct{}), prev: prev, finished: make(chan struct{}), wake: make(chan struct{}, 1), latest: map[string]proto.AgentSummary{},
		tracked: map[string]bool{}, asks: map[string]string{}, early: map[string]proto.PermissionRequest{}, kept: map[string]bool{}, seq: map[string]int64{}, started: map[string]bool{}, ended: map[string]bool{}}
}

func (r *remote) open(ctx context.Context) {
	socket, token, _ := r.m.Endpoint(r.host)
	dial, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	c, err := Dial(dial, socket, token, func(kind string, raw []byte) { r.push(func() { r.frame(kind, raw) }) }, proto.CapLaunch, proto.CapWorktrees)
	if err != nil {
		r.inherit()
		r.m.drop(r)
		close(r.ready)
		close(r.finished)
		return
	}
	r.client = c
	close(r.ready)
	go r.run()
	// hello sends the host's open asks with nothing to mark the last; the reply to a call comes after them all.
	if _, err := c.Call(dial, "agent.list", map[string]any{"type": "agent.list"}); err == nil {
		r.push(r.answeredMeanwhile)
		r.m.Mirror.Linked(r.host)
	}
}

// answeredMeanwhile closes the asks kept open here that the host didn't send again: answered there while the link
// was down.
func (r *remote) answeredMeanwhile() {
	for req := range r.kept {
		r.m.Mirror.Answered(Tag(r.host, r.asks[req]), Tag(r.host, req))
		delete(r.asks, req)
	}
	clear(r.kept)
}

// inherit takes up where the last remote left off, so a reconnect pages only what's new.
func (r *remote) inherit() {
	if r.prev == nil {
		return
	}
	<-r.prev.finished
	r.seq, r.started, r.ended = r.prev.seq, r.prev.started, r.prev.ended
	r.prev = nil
}

func (r *remote) close() {
	<-r.ready
	if r.client != nil {
		r.client.Close()
	}
}

func (r *remote) push(f func()) {
	r.mu.Lock()
	r.queue = append(r.queue, f)
	r.mu.Unlock()
	select {
	case r.wake <- struct{}{}:
	default:
	}
}

func (r *remote) run() {
	defer close(r.finished)
	defer r.m.drop(r)
	r.inherit()
	for {
		r.mu.Lock()
		queue := r.queue
		r.queue = nil
		r.mu.Unlock()
		for _, f := range queue {
			f()
		}
		select {
		case <-r.wake:
		case <-r.client.Done():
			return
		}
	}
}

func (r *remote) frame(kind string, raw []byte) {
	switch kind {
	case "agent.list":
		// The one hello sends: what was mirrored before this link picks up from here.
		var l proto.AgentList
		json.Unmarshal(raw, &l)
		for _, a := range l.Agents {
			r.latest[a.ID] = a
		}
		for _, s := range r.m.Mirror.Live(r.host) {
			_, theirs := Untag(s.ID)
			a, ok := r.latest[theirs]
			if !ok {
				h, _ := r.m.host(r.host)
				r.m.Mirror.Gone(s.ID, h.Name)
				s.Status = "closed"
				r.m.Mirror.Summary(s)
				continue
			}
			r.tracked[theirs] = true
			for _, tagged := range r.m.Mirror.Open(s.ID) {
				_, req := Untag(tagged)
				r.asks[req], r.kept[req] = theirs, true
			}
			if busy(s.Status) {
				// Stored mid-turn, so whatever the host shows now is past that turn.
				r.started[theirs] = true
			}
			r.sync(a)
		}
	case "agent.update":
		var u proto.AgentUpdate
		json.Unmarshal(raw, &u)
		r.latest[u.Agent.ID] = u.Agent
		if r.tracked[u.Agent.ID] {
			r.sync(u.Agent)
		}
	case "permission.request":
		var p proto.PermissionRequested
		json.Unmarshal(raw, &p)
		req := p.Request
		switch {
		case !safe(req.RequestID):
		case r.tracked[req.AgentID]:
			r.ask(req)
		default:
			r.early[req.RequestID] = req
		}
	case "permission.resolved":
		var p proto.PermissionResolved
		json.Unmarshal(raw, &p)
		delete(r.early, p.RequestID)
		delete(r.kept, p.RequestID)
		if agent, ok := r.asks[p.RequestID]; ok {
			delete(r.asks, p.RequestID)
			r.m.Mirror.Answered(Tag(r.host, agent), Tag(r.host, p.RequestID))
		}
	}
}

// track mirrors a session just started there from its first turn, which may already have ended: start stands in
// until the host has said anything of it.
func (r *remote) track(theirs string, start proto.AgentSummary) {
	r.tracked[theirs] = true
	a, heard := r.latest[theirs]
	if heard {
		start = a
	}
	start.Status = "working"
	r.feed(start)
	if heard {
		r.sync(a)
	}
	for id, req := range r.early {
		if req.AgentID == theirs {
			delete(r.early, id)
			r.ask(req)
		}
	}
}

func (r *remote) ask(req proto.PermissionRequest) {
	if r.kept[req.RequestID] {
		// Its parent heard of it before this link.
		delete(r.kept, req.RequestID)
		return
	}
	r.asks[req.RequestID] = req.AgentID
	req.AgentID, req.RequestID = Tag(r.host, req.AgentID), Tag(r.host, req.RequestID)
	r.m.Mirror.Asked(req)
}

// sync copies a tracked session's state; one whose turn ended gets its items first, so its reply is stored
// before the turn end settles its task.
func (r *remote) sync(a proto.AgentSummary) {
	switch {
	case busy(a.Status):
		r.started[a.ID], r.ended[a.ID] = true, false
	case a.Status == "closed":
		r.catchUp(a.ID, false)
	case r.started[a.ID]:
		r.catchUp(a.ID, true)
	default:
		// A session is idle before its first turn shows, unless that turn came and went before this Mac heard:
		// it left a result, or nobody saw it end.
		r.catchUp(a.ID, false)
		if !r.ended[a.ID] && a.Status != "done" {
			return
		}
	}
	r.feed(a)
	if a.Status == "closed" {
		delete(r.tracked, a.ID)
	}
}

func (r *remote) feed(a proto.AgentSummary) {
	s, ok := mirrored(r.host, a)
	if !ok {
		return
	}
	r.m.mu.Lock()
	r.m.summaries[s.ID] = s
	r.m.mu.Unlock()
	r.m.Mirror.Summary(s)
}

// catchUp stores the items theirs has past those already stored; with wait it pages again, for up to 3s, until
// the turn's result is there, since a reply can land after the turn ended.
func (r *remote) catchUp(theirs string, wait bool) {
	deadline := time.Now().Add(3 * time.Second)
	for {
		for r.page(theirs) {
		}
		if !wait || r.ended[theirs] || time.Now().After(deadline) {
			return
		}
		time.Sleep(100 * time.Millisecond)
	}
}

const pageSize = 500

// page stores one page of theirs' items past the stored ones, and says whether it held any.
func (r *remote) page(theirs string) bool {
	since := r.seq[theirs]
	t, err := r.timeline(theirs, "sinceSeq", since)
	if err != nil {
		return false
	}
	items := t.Items
	// A full page is the newest past since, so what lies between is read backwards.
	for older := items; len(older) == pageSize; {
		o, err := r.timeline(theirs, "beforeSeq", older[0].Seq)
		if err != nil {
			return false
		}
		older = slices.DeleteFunc(o.Items, func(it proto.Item) bool { return it.Seq <= since })
		items = append(older, items...)
	}
	newer := false
	for _, it := range items {
		if it.Seq <= r.seq[theirs] {
			continue
		}
		newer = true
		r.seq[theirs] = it.Seq
		// An item of a kind this pocketd can't write would fail the store's whole batch.
		if _, err := json.Marshal(it); err != nil {
			continue
		}
		r.m.Mirror.Item(Tag(r.host, theirs), t.Epoch, it)
		if it.Kind == "result" && it.ParentToolUseID == "" {
			r.ended[theirs] = true
		}
	}
	return newer
}

func (r *remote) timeline(theirs, from string, seq int64) (proto.AgentTimeline, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	var t proto.AgentTimeline
	raw, err := r.client.Call(ctx, "agent.timeline", map[string]any{"type": "agent.timeline", "agentId": theirs, from: seq, "limit": pageSize})
	if err == nil {
		json.Unmarshal(raw, &t)
	}
	return t, err
}

func (m *Manager) drop(r *remote) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.remotes[r.host] == r {
		delete(m.remotes, r.host)
	}
}

// follow keeps a conn to the host's pocketd open while its link is online; m.mu is held.
func (m *Manager) follow(id string, l Link) {
	r := m.remotes[id]
	switch {
	case l.State == StateOnline && r == nil && m.Mirror != nil:
		r = newRemote(m, id, m.last[id])
		m.remotes[id], m.last[id] = r, r
		go r.open(m.ctx)
	case l.State != StateOnline && r != nil:
		delete(m.remotes, id)
		go r.close()
	}
}

func (m *Manager) connected(ctx context.Context, id string) (*remote, error) {
	m.mu.Lock()
	h, ok := m.hostLocked(id)
	r := m.remotes[id]
	m.mu.Unlock()
	if !ok {
		return nil, ErrUnknown
	}
	offline := Offline{h.Name}
	if r == nil {
		return nil, offline
	}
	select {
	case <-r.ready:
	case <-ctx.Done():
		return nil, ctx.Err()
	}
	if r.client == nil {
		return nil, offline
	}
	return r, nil
}

func (m *Manager) Launch(ctx context.Context, id string, spec proto.LaunchSpec, key string) (string, error) {
	r, err := m.connected(ctx, id)
	if err != nil {
		return "", err
	}
	if !slices.Contains(r.client.Caps, proto.CapLaunch) {
		h, _ := m.host(id)
		return "", fmt.Errorf("Update Anywhere on %s", h.Name)
	}
	if n := spec.Checkout.New; n != nil && !slices.Contains(r.client.Caps, proto.CapWorktrees) {
		// An older host refuses a spec with forAgent; its worktree stays listed there.
		n := *n
		n.ForAgent = false
		spec.Checkout.New = &n
	}
	ctx, cancel := context.WithTimeout(ctx, time.Minute)
	defer cancel()
	raw, err := r.client.Call(ctx, "agent.created", map[string]any{"type": "agent.create", "requestId": "orch-" + key, "spec": spec})
	if err != nil {
		return "", err
	}
	var created proto.AgentCreated
	json.Unmarshal(raw, &created)
	if !safe(created.AgentID) {
		return "", errInvalidID
	}
	done := make(chan struct{})
	r.push(func() {
		r.track(created.AgentID, proto.AgentSummary{ID: created.AgentID, Provider: spec.Provider, Cwd: spec.Project, ChatOnly: spec.Chat})
		close(done)
	})
	select {
	case <-done:
	case <-r.client.Done():
	}
	return Tag(id, created.AgentID), nil
}

func (m *Manager) Send(ctx context.Context, id string, msg map[string]any) error {
	r, err := m.connected(ctx, id)
	if err != nil {
		return err
	}
	_, err = r.client.Call(ctx, "ack", msg)
	return err
}

// Offline is a linked host whose link is down.
type Offline struct{ Name string }

func (o Offline) Error() string { return o.Name + " isn't online" }

// Deletion is how a worktree.delete deletes: its branches too, even commits nowhere else, to the Trash, and with
// its teardown script bounded to TeardownSecs, or skipped.
type Deletion struct {
	DeleteBranch, Force, Trash bool
	TeardownSecs               int
	Teardown                   bool
}

// DeleteWorktree deletes the worktree at path on host id as a phone's worktree.delete does, which takes its
// subagents' there too, for the subagents named. One already gone, made since they closed, or that someone else
// works in counts as deleted.
func (m *Manager) DeleteWorktree(ctx context.Context, id, path string, subagents []string, d Deletion) error {
	c, err := m.worktrees(ctx, id)
	if err != nil {
		return err
	}
	defer c.Close()
	raw, err := c.Call(ctx, "ack", map[string]any{"type": "worktree.delete", "path": path, "deleteBranch": d.DeleteBranch, "force": d.Force, "subagents": subagents,
		"trash": d.Trash, "teardownSecs": d.TeardownSecs, "teardown": d.Teardown})
	var e proto.Error
	if err == nil || json.Unmarshal(raw, &e) != nil || e.Code == "" {
		return err
	}
	if e.Code == "unknown_worktree" || e.Code == "worktree_in_use" {
		return nil
	}
	return Refusal{e}
}

// InspectWorktree is what deleting the worktree at path on host id would take there.
func (m *Manager) InspectWorktree(ctx context.Context, id, path string) (proto.WorktreeInspect, error) {
	var in proto.WorktreeInspect
	c, err := m.worktrees(ctx, id)
	if err != nil {
		return in, err
	}
	defer c.Close()
	raw, err := c.Call(ctx, "worktree.inspect", map[string]any{"type": "worktree.inspect", "path": path})
	var e proto.Error
	if json.Unmarshal(raw, &e) == nil && e.Code != "" {
		return in, Refusal{e}
	}
	if err != nil {
		return in, err
	}
	return in, json.Unmarshal(raw, &in)
}

// worktrees is a conn to host id's pocketd for its worktree verbs, over a conn of its own: workspace.v1 would also
// subscribe the link to every project.list. A pocketd without worktrees.v1 is refused: it would delete without the
// teardown.
func (m *Manager) worktrees(ctx context.Context, id string) (*Client, error) {
	if _, err := m.connected(ctx, id); err != nil {
		return nil, err
	}
	socket, token, _ := m.Endpoint(id)
	c, err := Dial(ctx, socket, token, func(string, []byte) {}, proto.CapWorkspace, proto.CapWorktrees)
	if err != nil {
		return nil, err
	}
	if !slices.Contains(c.Caps, proto.CapWorktrees) {
		c.Close()
		h, _ := m.host(id)
		return nil, Refusal{proto.NewErrorCode("", proto.CodeHostOutdated, "Can't delete worktrees on "+h.Name+": Anywhere there is out of date. Update it on "+h.Name+", then try again.")}
	}
	return c, nil
}

// Refusal is a host's coded error, kept whole so a phone can act on it.
type Refusal struct{ Reply proto.Error }

func (r Refusal) Error() string { return r.Reply.Message }

func (m *Manager) Projects(ctx context.Context, id string) ([]proto.Project, error) {
	r, err := m.connected(ctx, id)
	if err != nil {
		return nil, err
	}
	raw, err := r.client.Call(ctx, "project.list", map[string]any{"type": "project.list"})
	if err != nil {
		return nil, err
	}
	var l proto.ProjectList
	json.Unmarshal(raw, &l)
	return l.Projects, nil
}

func (m *Manager) Summary(tagged string) (proto.AgentSummary, bool) {
	m.mu.Lock()
	defer m.mu.Unlock()
	s, ok := m.summaries[tagged]
	return s, ok
}
