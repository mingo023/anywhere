package push

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net/http"
	"sync"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/broker"
	"pocketd/internal/events"
	"pocketd/internal/proto"
)

// Timing is every delay the notifier keeps; tests shrink them.
type Timing struct {
	// NeedsYou and Done wait before an alert, as auto-allow and the desktop often answer first.
	NeedsYou, Done time.Duration
	// Coalesce spaces priority-5 activity updates; Slow when the phone turned frequent updates off.
	Coalesce, Slow time.Duration
	// Heartbeat moves the stale date on while anything works.
	Heartbeat time.Duration
	// EndAfter ends the activity once nothing has worked or needed the user for it.
	EndAfter time.Duration
	// Stale is the activity's stale date and its pushes' expiry; Dismiss how long an ended one lingers.
	Stale, Dismiss time.Duration
	// Restart spaces push-to-starts, which always buzz.
	Restart time.Duration
	// Retry is the first wait after the relay says it is busy; it doubles, three times.
	Retry time.Duration
}

var DefaultTiming = Timing{
	NeedsYou: 1500 * time.Millisecond, Done: 3 * time.Second,
	Coalesce: 15 * time.Second, Slow: time.Minute,
	Heartbeat: 4 * time.Minute, EndAfter: 10 * time.Minute,
	Stale: 5 * time.Minute, Dismiss: 15 * time.Minute,
	Restart: 10 * time.Minute, Retry: 2 * time.Second,
}

const retries = 3

// Alerts expire from APNs' store unsent: a Needs you after an hour, an ending after six.
var expiry = map[string]time.Duration{NeedsYou: time.Hour, Done: 6 * time.Hour, Failed: 6 * time.Hour, Automation: 6 * time.Hour, Test: time.Hour}

// NewHTTP is the client the relay is reached with.
func NewHTTP() *http.Client { return &http.Client{Timeout: 20 * time.Second} }

// Notifier turns status changes, asks and automation runs into pushes. Its
// inputs only enqueue, since OnStatus runs under the agent's lock; one loop
// decides, and each phone has its own sender so its pushes stay in order.
type Notifier struct {
	Store  *Store
	Relay  *Client
	Agents *agent.Registry
	Broker *broker.Broker
	// Mac is the hello.ok serverId alerts are sealed for; MacName names the activity.
	Mac, MacName string
	Events       *events.Log
	// Foreground says whether the phone has the app in front on a connection now; it shows alerts itself then.
	Foreground func(device string) bool
	Timing     Timing
	Now        func() time.Time

	once sync.Once
	in   chan func(*loop)
	ctx  context.Context
}

func (n *Notifier) init() {
	n.once.Do(func() {
		n.in = make(chan func(*loop), 1024)
		if n.Timing == (Timing{}) {
			n.Timing = DefaultTiming
		}
		if n.Now == nil {
			n.Now = time.Now
		}
	})
}

// post hands f to the loop. A full queue drops it rather than block an agent's lock.
func (n *Notifier) post(f func(*loop)) {
	n.init()
	select {
	case n.in <- f:
	default:
		log.Print("push: queue full; dropped an update")
	}
}

// wait hands f to the loop from a timer or sender, which may block until the loop takes it.
func (n *Notifier) wait(f func(*loop)) {
	select {
	case n.in <- f:
	case <-n.ctx.Done():
	}
}

// Run is the loop; it returns when ctx ends.
func (n *Notifier) Run(ctx context.Context) {
	n.init()
	n.ctx = ctx
	l := &loop{n: n, alerts: map[string]*pending{}, sent: map[string]map[string]time.Time{}, lives: map[string]*live{}, senders: map[string]chan job{}}
	for {
		select {
		case <-ctx.Done():
			return
		case f := <-n.in:
			f(l)
		}
	}
}

// Kick takes a status change; it is OnStatus's, so it only enqueues.
func (n *Notifier) Kick(id, from, to string) {
	n.post(func(l *loop) { l.status(id, from, to) })
}

// Ask takes a new permission ask.
func (n *Notifier) Ask(req proto.PermissionRequest) {
	n.post(func(l *loop) {
		l.schedule(req.AgentID, "needsYou", n.Timing.NeedsYou)
		l.activities()
	})
}

// Automation takes a run that finished; name is its automation's.
func (n *Notifier) Automation(run proto.Run, name string) {
	n.post(func(l *loop) { l.automation(run, name) })
}

func (n *Notifier) Register(device string, r Registration) error {
	if err := n.Store.Register(device, r, n.now()); err != nil {
		return err
	}
	n.post(func(l *loop) { l.activity(device, l.inputs()) })
	return nil
}

func (n *Notifier) Activity(device, id string, h *proto.PushHandle, ended bool) error {
	if err := n.Store.SetActivity(device, id, h, ended, n.now()); err != nil {
		return err
	}
	n.post(func(l *loop) { l.activity(device, l.inputs()) })
	return nil
}

func (n *Notifier) Prefs(device string, p proto.PushPrefs) error {
	if err := n.Store.SetPrefs(device, p); err != nil {
		return err
	}
	n.post(func(l *loop) { l.activity(device, l.inputs()) })
	return nil
}

// Forget drops device's registration: it unregistered or was revoked.
func (n *Notifier) Forget(device string) error {
	err := n.Store.Remove(device)
	n.post(func(l *loop) { l.forget(device) })
	return err
}

// Test sends a test alert to device now and says how it went.
func (n *Notifier) Test(ctx context.Context, device string) error {
	r, ok := n.Store.Get(device)
	if !ok {
		return ErrNotRegistered
	}
	if r.Alert == nil {
		return errors.New("The relay no longer knows this phone; open Anywhere to register again")
	}
	notice := Notice{Mac: n.Mac, Kind: Test, Title: "Anywhere", Place: n.MacName, Line: "Notifications from " + n.MacName + " work.", Badge: Badge(n.Agents.List())}
	payload, err := AlertPayload(r.Key, notice, "", Level(Test, false))
	if err != nil {
		return err
	}
	now := n.now()
	_, err = n.Relay.Send(ctx, *r.Alert, Send{Priority: 10, Expiration: now.Add(expiry[Test]).Unix(), Payload: payload})
	n.Store.Note(Sent{At: now.UnixMilli(), Device: device, Kind: Test, Priority: 10, Outcome: outcome(err)})
	if errors.Is(err, ErrGone) {
		n.Store.Drop(device, AlertHandle, r.Alert.Handle)
		return errors.New("The relay no longer knows this phone; open Anywhere to register again")
	}
	if err != nil {
		return fmt.Errorf("The test push didn't go out: %v", err)
	}
	return nil
}

func (n *Notifier) now() time.Time {
	n.init()
	return n.Now()
}

func outcome(err error) string {
	switch {
	case err == nil:
		return "sent"
	case errors.Is(err, ErrGone):
		return "gone"
	case errors.Is(err, ErrLimited):
		return "limited"
	}
	return err.Error()
}

// job is one send for one phone.
type job struct {
	which  string
	handle proto.PushHandle
	send   Send
	kind   string // the alert kind or the activity event
	agent  string
}

// pending is an alert waiting out its debounce.
type pending struct {
	status string
	timer  *time.Timer
	gen    int
}

// live is what the loop knows of one phone's activity.
type live struct {
	activity string
	last     []byte
	lastSent time.Time
	needs    bool
	failed   int
	idle     time.Time
	wake     *time.Timer
	started  time.Time
	wasNeeds bool
}

type loop struct {
	n       *Notifier
	alerts  map[string]*pending
	gen     int
	sent    map[string]map[string]time.Time
	lives   map[string]*live
	senders map[string]chan job
}

func (l *loop) status(id, from, to string) {
	switch {
	case to == "needsYou":
		l.schedule(id, to, l.n.Timing.NeedsYou)
	case to == "done" && (from == "working" || from == "needsYou"):
		l.schedule(id, to, l.n.Timing.Done)
	default:
		if p := l.alerts[id]; p != nil {
			p.timer.Stop()
			delete(l.alerts, id)
		}
	}
	l.activities()
}

// schedule (re)starts agent's alert debounce for status.
func (l *loop) schedule(id, status string, after time.Duration) {
	if p := l.alerts[id]; p != nil {
		p.timer.Stop()
	}
	l.gen++
	gen := l.gen
	l.alerts[id] = &pending{status: status, gen: gen, timer: time.AfterFunc(after, func() {
		l.n.wait(func(l *loop) { l.fire(id, gen) })
	})}
}

// fire sends agent's alert if it still stands: same status, not seen, not sent before.
func (l *loop) fire(id string, gen int) {
	p := l.alerts[id]
	if p == nil || p.gen != gen {
		return
	}
	delete(l.alerts, id)
	a, err := l.n.Agents.Get(id)
	if err != nil {
		return
	}
	s := a.Summary()
	if s.Status != p.status || !s.Attached || a.Seen() {
		return
	}
	kind := s.Status
	if kind == Done && s.Failed {
		kind = Failed
	}
	var ask *proto.PermissionRequest
	if kind == NeedsYou {
		for _, r := range l.n.Broker.Open() {
			if r.AgentID == id {
				ask = &r
				break
			}
		}
	}
	reply := ""
	if kind == Done {
		reply = lastReply(a)
	}
	notice := Notice{Mac: l.n.Mac, Agent: id, Kind: kind, Title: Title(s), Place: Place(s), Line: Line(kind, ask, reply)}
	key := fmt.Sprintf("%s|%s|%d", id, s.Status, s.UpdatedAt)
	if ask != nil {
		notice.Request = ask.RequestID
		key += "|" + ask.RequestID
	}
	l.alert(kind, key, notice, Category(kind, ask), Level(kind, false))
}

// lastReply is the agent's last reply text, not a sub-agent's.
func lastReply(a *agent.Agent) string {
	items, _ := a.Timeline.Before(1<<53, 64, false)
	for i := len(items) - 1; i >= 0; i-- {
		if it := items[i]; it.Kind == "assistant" && it.Text != "" {
			return it.Text
		}
	}
	return ""
}

func (l *loop) automation(run proto.Run, name string) {
	if run.Status != "succeeded" && run.Status != "failed" {
		return
	}
	failed := run.Status == "failed"
	notice := Notice{Mac: l.n.Mac, Agent: run.AgentID, Kind: Automation, Title: name}
	if notice.Title == "" {
		notice.Title = "Automation"
	}
	if a, err := l.n.Agents.Get(run.AgentID); err == nil {
		notice.Place = Place(a.Summary())
	}
	line := FirstParagraph(run.Summary)
	if failed {
		line = FirstParagraph(run.Why)
	}
	if line == "" {
		line = map[bool]string{true: "Failed", false: "Succeeded"}[failed]
	}
	notice.Line = Cut(line)
	l.alert(Automation, "run|"+run.ID, notice, Category(Automation, nil), Level(Automation, failed))
}

func wants(p proto.PushPrefs, kind string) bool {
	switch kind {
	case NeedsYou:
		return p.NeedsYou
	case Done:
		return p.Done
	case Failed:
		return p.Failed
	case Automation:
		return p.Automations
	}
	return true
}

// alert seals notice for each phone that wants kind, isn't in front and hasn't had key.
func (l *loop) alert(kind, key string, notice Notice, category, level string) {
	now := l.n.now()
	notice.Badge = Badge(l.n.Agents.List())
	priority := 10
	if level == "passive" {
		priority = 5
	}
	for _, r := range l.n.Store.Records() {
		if r.Alert == nil || !wants(r.Prefs, kind) || l.n.Foreground != nil && l.n.Foreground(r.Device) || l.seen(r.Device, key, now) {
			continue
		}
		payload, err := AlertPayload(r.Key, notice, category, level)
		if err != nil {
			log.Printf("push: sealing for %s: %v", r.Device, err)
			continue
		}
		collapse := ""
		if notice.Agent != "" {
			collapse = Opaque(r.Key, notice.Agent)
		}
		l.enqueue(r.Device, job{which: AlertHandle, handle: *r.Alert, kind: kind, agent: notice.Agent,
			send: Send{Priority: priority, Expiration: now.Add(expiry[kind]).Unix(), CollapseID: collapse, Payload: payload}})
	}
}

// seen notes key as sent to device, and says whether it already was.
func (l *loop) seen(device, key string, now time.Time) bool {
	keys := l.sent[device]
	if keys == nil {
		keys = map[string]time.Time{}
		l.sent[device] = keys
	}
	if _, ok := keys[key]; ok {
		return true
	}
	if len(keys) > 512 {
		for k, at := range keys {
			if now.Sub(at) > 24*time.Hour {
				delete(keys, k)
			}
		}
	}
	keys[key] = now
	return false
}

type inputs struct {
	agents []proto.AgentSummary
	asks   []proto.PermissionRequest
}

func (l *loop) inputs() inputs {
	return inputs{l.n.Agents.List(), l.n.Broker.Open()}
}

func (l *loop) activities() {
	in := l.inputs()
	for _, r := range l.n.Store.Records() {
		l.activity(r.Device, in)
	}
}

// activity brings device's Live Activity up to date: priority 10 at once on
// entering or leaving Needs you or on a new failure, else priority 5 at most
// once per Coalesce, a heartbeat for as long as it lives, and an end once
// nothing has worked or needed the user for EndAfter. With no activity, entering Needs you starts one
// by push when the phone gave a start handle.
func (l *loop) activity(device string, in inputs) {
	n := l.n
	r, ok := n.Store.Get(device)
	if !ok {
		l.forget(device)
		return
	}
	lv := l.lives[device]
	if lv == nil {
		lv = &live{}
		l.lives[device] = lv
	}
	now := n.now()
	st := Build(in.agents, in.asks, r.Prefs.Details)
	needs := st.NeedsYou > 0
	entered := needs && !lv.wasNeeds
	lv.wasNeeds = needs
	if r.Activity == nil || r.Activity.Handle == nil {
		if r.Activity == nil && entered && r.Start != nil && (lv.started.IsZero() || now.Sub(lv.started) >= n.Timing.Restart) {
			lv.started = now
			l.enqueue(device, job{which: StartHandle, handle: *r.Start, kind: "start",
				send: Send{Priority: 10, Expiration: now.Add(n.Timing.Stale).Unix(), Payload: StartPayload(st, n.Mac, n.MacName, now, n.Timing.Stale)}})
		}
		return
	}
	if lv.activity != r.Activity.ID {
		*lv = live{activity: r.Activity.ID, started: lv.started, wasNeeds: needs, wake: lv.wake}
	}
	if st.Busy() {
		lv.idle = time.Time{}
	} else if lv.idle.IsZero() {
		lv.idle = now
	}
	coalesce := n.Timing.Coalesce
	if !r.Frequent {
		coalesce = n.Timing.Slow
	}
	if !st.Busy() && !lv.lastSent.IsZero() && now.Sub(lv.idle) >= n.Timing.EndAfter {
		l.enqueue(device, job{which: ActivityHandle, handle: *r.Activity.Handle, kind: "end",
			send: Send{Priority: 10, Expiration: now.Add(n.Timing.Stale).Unix(), Payload: UpdatePayload(st, now, n.Timing.Stale, n.Timing.Dismiss, true)}})
		n.Store.SetActivity(device, r.Activity.ID, nil, true, now)
		l.stopWake(lv)
		*lv = live{started: lv.started, wasNeeds: needs}
		return
	}
	raw, _ := json.Marshal(st)
	fails := failures(in.agents)
	changed := !bytes.Equal(raw, lv.last)
	urgent := needs != lv.needs || fails > lv.failed
	// Every live state beats, not only working: a long wait on the user or the idle minutes before the end would
	// otherwise pass the stale date and show the Mac as offline.
	heartbeat := now.Sub(lv.lastSent) >= n.Timing.Heartbeat
	due := lv.lastSent.Add(coalesce)
	priority := 0
	switch {
	case changed && urgent:
		priority = 10
	case (changed || heartbeat) && !now.Before(due):
		priority = 5
	}
	if priority > 0 {
		l.enqueue(device, job{which: ActivityHandle, handle: *r.Activity.Handle, kind: "update",
			send: Send{Priority: priority, Expiration: now.Add(n.Timing.Stale).Unix(), Payload: UpdatePayload(st, now, n.Timing.Stale, 0, false)}})
		lv.last, lv.lastSent, lv.needs, lv.failed = raw, now, needs, fails
		changed = false
	}
	var next time.Time
	soonest := func(t time.Time) {
		if next.IsZero() || t.Before(next) {
			next = t
		}
	}
	if changed {
		soonest(due)
	}
	soonest(lv.lastSent.Add(n.Timing.Heartbeat))
	if !st.Busy() {
		soonest(lv.idle.Add(n.Timing.EndAfter))
	}
	l.stopWake(lv)
	if !next.IsZero() {
		lv.wake = time.AfterFunc(max(0, next.Sub(now)), func() {
			n.wait(func(l *loop) { l.activity(device, l.inputs()) })
		})
	}
}

func (l *loop) stopWake(lv *live) {
	if lv.wake != nil {
		lv.wake.Stop()
		lv.wake = nil
	}
}

func (l *loop) forget(device string) {
	if lv := l.lives[device]; lv != nil {
		l.stopWake(lv)
	}
	delete(l.lives, device)
	delete(l.sent, device)
	if ch := l.senders[device]; ch != nil {
		close(ch)
		delete(l.senders, device)
	}
}

func (l *loop) enqueue(device string, j job) {
	ch := l.senders[device]
	if ch == nil {
		ch = make(chan job, 64)
		l.senders[device] = ch
		go l.n.deliverAll(device, ch)
	}
	select {
	case ch <- j:
	default:
		log.Printf("push: %s's queue is full; dropped a %s", device, j.kind)
	}
}

func (n *Notifier) deliverAll(device string, jobs <-chan job) {
	for j := range jobs {
		n.deliver(device, j)
	}
}

// deliver sends j, retrying a busy relay three times with doubling waits.
func (n *Notifier) deliver(device string, j job) {
	var err error
	for attempt := 0; ; attempt++ {
		ctx, cancel := context.WithTimeout(n.ctx, 20*time.Second)
		_, err = n.Relay.Send(ctx, j.handle, j.send)
		cancel()
		var busy RetryError
		if !errors.As(err, &busy) || attempt == retries {
			break
		}
		select {
		case <-time.After(max(n.Timing.Retry<<attempt, busy.After)):
		case <-n.ctx.Done():
			return
		}
	}
	if errors.Is(err, ErrGone) {
		n.Store.Drop(device, j.which, j.handle.Handle)
		if j.which == ActivityHandle {
			n.wait(func(l *loop) {
				if lv := l.lives[device]; lv != nil {
					l.stopWake(lv)
					*lv = live{started: lv.started, wasNeeds: lv.wasNeeds}
				}
			})
		}
	}
	if err == nil && j.which == AlertHandle {
		n.Events.Emit(events.Event{Kind: "push", Agent: j.agent, Alert: j.kind})
	}
	n.Store.Note(Sent{At: n.now().UnixMilli(), Device: device, Kind: j.kind, Agent: j.agent, Priority: j.send.Priority, Outcome: outcome(err)})
}
