package orch

import (
	"cmp"
	"encoding/json"
	"fmt"
	"slices"
	"strconv"
	"strings"

	"pocketd/internal/proto"
)

// AgentInfo is what the rules need to know about an agent kind.
type AgentInfo struct {
	Known     bool
	Access    []string // the access levels it offers
	NeverAsks bool     // it always runs Full
	CanSteer  bool     // it takes a message mid-turn without cutting its tools
}

// State is everything decide reads, loaded by the engine before it runs.
type State struct {
	Now      int64
	Session  *Session
	Queue    []QueueEntry // the session's entries not sent or cancelled, by order
	Children []Delegation // delegations the session started
	Task     *Delegation  // the delegation the payload names
	Subtree  []Delegation // Task and every delegation under it, deepest first
	Parent   *Session     // Task's parent, for a settle
	Request  *Request
	Other    *Session // the payload's target session
	Related  bool     // Other is the session's parent, child, fork or source
	Running  int      // sessions agents started on this Mac: launching, or running and not yet settled
	Limits   Limits
	Self     AgentInfo // the session's own agent
	Target   AgentInfo // the payload's agent
	Held     []string  // task ids the session holds
	Starting bool      // a turn start the session was sent is still out
}

// Decision is what a command does: facts, row writes, effects and the
// commands it sets off in other sessions.
type Decision struct {
	Events      []Event
	Effects     []Effect
	Delegations []Delegation
	Queue       []QueueEntry
	Requests    []Request
	Lineage     []Session // parent_id, relationship, depth, created_by
	Transfers   []Transfer
	Hold        []string
	Unhold      []string
	Archive     []string
	Follow      []Command
	Result      any
}

type (
	SendPayload struct {
		Text   string   `json:"text"`
		Mode   string   `json:"mode,omitempty"` // auto, queue, steer, restart
		Origin string   `json:"origin,omitempty"`
		Refs   []string `json:"refs,omitempty"`
	}
	EntryPayload struct {
		EntryID string `json:"entryId"`
		Text    string `json:"text,omitempty"`
		Before  string `json:"before,omitempty"` // reorder: the entry it moves ahead of; "" moves it last
		Ref     string `json:"ref,omitempty"`
	}
	RequestPayload struct {
		RequestID string            `json:"requestId"`
		Kind      string            `json:"kind,omitempty"`
		Payload   json.RawMessage   `json:"payload,omitempty"`
		Decision  string            `json:"decision,omitempty"`
		Option    string            `json:"option,omitempty"`
		Message   string            `json:"message,omitempty"`
		Answers   map[string]string `json:"answers,omitempty"`
		// User is the user's own answer from a client: it answers a permission
		// ask too, and only the broker says whether the ask is still open.
		User bool `json:"user,omitempty"`
	}
	DelegatePayload struct {
		TaskID          string `json:"taskId,omitempty"`
		Agent           string `json:"agent"`
		Model           string `json:"model,omitempty"`
		Effort          string `json:"effort,omitempty"`
		Title           string `json:"title"`
		Brief           string `json:"brief"`
		Role            string `json:"role,omitempty"`
		Isolation       string `json:"isolation,omitempty"`
		Mode            string `json:"mode,omitempty"`
		CompletionWake  string `json:"completionWake,omitempty"`
		ClientRequestID string `json:"clientRequestId,omitempty"`
		Environment     string `json:"environmentId,omitempty"`
		HostProject     string `json:"hostProject,omitempty"`
		// OutsideGit is the caller's Project not being a git repository, which the tool finds.
		OutsideGit bool `json:"outsideGit,omitempty"`
	}
	TaskPayload struct {
		TaskID         string `json:"taskId"`
		ChildID        string `json:"childId,omitempty"`
		Status         string `json:"status,omitempty"`
		Summary        string `json:"summary,omitempty"`
		Error          string `json:"error,omitempty"`
		CompletionWake string `json:"completionWake,omitempty"`
		WorkState      string `json:"workState,omitempty"`
		Stopped        bool   `json:"stopped,omitempty"`
	}
	// StartPayload is the user's agent.create from a client, and who asked: Key is the requester.
	StartPayload struct {
		Owner bool             `json:"owner,omitempty"`
		Key   string           `json:"key"`
		Spec  proto.LaunchSpec `json:"spec"`
	}
	// LaunchPayload is an agent.launch effect: what the launcher needs.
	LaunchPayload struct {
		TaskID      string `json:"taskId,omitempty"`
		TransferID  string `json:"transferId,omitempty"`
		Parent      string `json:"parent,omitempty"`
		Agent       string `json:"agent"`
		Model       string `json:"model,omitempty"`
		Effort      string `json:"effort,omitempty"`
		Access      string `json:"access,omitempty"`
		Prompt      string `json:"prompt,omitempty"`
		Title       string `json:"title,omitempty"`
		Project     string `json:"project,omitempty"`
		Cwd         string `json:"cwd,omitempty"`
		Checkout    string `json:"checkout,omitempty"` // root, worktree, existing_worktree, shared
		Worktree    string `json:"worktree,omitempty"`
		Branch      string `json:"branch,omitempty"`
		Fork        string `json:"fork,omitempty"` // native fork of this session
		Context     string `json:"context,omitempty"`
		Depth       int    `json:"depth,omitempty"`
		CreatedBy   string `json:"createdBy,omitempty"`
		Rel         string `json:"relationship,omitempty"`
		Chat        bool   `json:"chat,omitempty"`
		Env         string `json:"environmentId,omitempty"`
		HostProject string `json:"hostProject,omitempty"`
		SteppedDown string `json:"steppedDown,omitempty"`
	}
	TurnPayload struct {
		Text   string `json:"text"`
		Key    string `json:"key"`
		Origin string `json:"origin,omitempty"`
	}
	DeliveryPayload struct {
		TaskIDs  []string `json:"taskIds"`
		ChildIDs []string `json:"childIds"`
		Outcome  string   `json:"outcome"`
		Question string   `json:"question,omitempty"`
		Titles   []string `json:"titles,omitempty"`
		Agents   []string `json:"agents,omitempty"`
	}
	ConfigPayload struct {
		Agent   string            `json:"agent,omitempty"`
		Model   string            `json:"model,omitempty"`
		Effort  string            `json:"effort,omitempty"`
		Options map[string]string `json:"options,omitempty"`
	}
	// SettingsPayload is the user's own switch from a client; unlike session.configure, it sets the access too.
	SettingsPayload struct {
		Model   string            `json:"model,omitempty"`
		Effort  string            `json:"effort,omitempty"`
		Options map[string]string `json:"options,omitempty"`
		Access  string            `json:"access,omitempty"`
	}
	TitlePayload struct {
		Title string `json:"title,omitempty"`
	}
	OrganizePayload struct {
		Action string `json:"action"`
	}
	HoldPayload struct {
		TaskIDs []string `json:"taskIds"`
		Waiting bool     `json:"waiting"`
	}
	StopTaskPayload struct {
		TaskID string `json:"taskId"`
	}
	TransferPayload struct {
		TransferID  string `json:"transferId,omitempty"`
		TargetID    string `json:"targetId,omitempty"`
		Agent       string `json:"agent,omitempty"`
		Model       string `json:"model,omitempty"`
		Isolation   string `json:"isolation,omitempty"`
		Title       string `json:"title,omitempty"`
		Prompt      string `json:"prompt,omitempty"`
		SourcePoint int64  `json:"sourcePoint,omitempty"`
		Context     string `json:"context,omitempty"`
		Native      bool   `json:"native,omitempty"`
	}
)

// decide is pure: it reads st and cmd and returns what the command does.
func decide(st State, cmd Command) (Decision, error) {
	ids := idSeq{base: cmd.ID}
	switch cmd.Kind {
	case CmdMessageSend:
		var p SendPayload
		if err := decode(cmd, &p); err != nil {
			return Decision{}, err
		}
		return send(st, cmd, p, &ids)
	case CmdQueueDrain:
		return drain(st, cmd)
	case CmdQueueSent, CmdQueueUnsent:
		var p TurnPayload
		if err := decode(cmd, &p); err != nil {
			return Decision{}, err
		}
		return delivered(st, cmd, p.Key)
	case CmdQueueEdit, CmdQueueCancel, CmdQueueReorder, CmdQueueSendNow, CmdQueueCancelRefs:
		var p EntryPayload
		if err := decode(cmd, &p); err != nil {
			return Decision{}, err
		}
		return queueEdit(st, cmd, p)
	case CmdTurnInterrupt:
		if err := live(st); err != nil {
			return Decision{}, err
		}
		return Decision{Effects: []Effect{effect(cmd.Session, EffInterrupt, nil)}, Events: stopEvents(st)}, nil
	case CmdTurnSendNow:
		if err := live(st); err != nil {
			return Decision{}, err
		}
		return Decision{Effects: []Effect{effect(cmd.Session, EffSendNow, nil)}}, nil
	case CmdSessionStop:
		return stop(st, cmd)
	case CmdSessionClose:
		if st.Session == nil {
			return Decision{}, refuse(CodeNotFound, "Unknown session "+cmd.Session)
		}
		return Decision{Effects: []Effect{effect(cmd.Session, EffClose, nil)}}, nil
	case CmdRequestOpen, CmdRequestClose, CmdRequestAnswer:
		var p RequestPayload
		if err := decode(cmd, &p); err != nil {
			return Decision{}, err
		}
		return request(st, cmd, p)
	case CmdDelegate:
		var p DelegatePayload
		if err := decode(cmd, &p); err != nil {
			return Decision{}, err
		}
		return delegate(st, cmd, p, &ids)
	case CmdDelegateStarted, CmdDelegateFailed, CmdDelegateSettle, CmdDelegateCancel, CmdDelegateWake, CmdDelegateWaited:
		var p TaskPayload
		if err := decode(cmd, &p); err != nil {
			return Decision{}, err
		}
		return task(st, cmd, p)
	case CmdDelegatePump:
		return pump(st, cmd)
	case CmdWakeDeliver:
		return deliver(st, cmd, &ids)
	case CmdParentIdle:
		return parentIdle(st, cmd)
	case CmdSessionCreate:
		var p LaunchPayload
		if err := decode(cmd, &p); err != nil {
			return Decision{}, err
		}
		if st.Session != nil {
			access, stepped, err := Access(st.Session.Access, st.Target)
			if err != nil {
				return Decision{}, err
			}
			p.Access, p.SteppedDown = access, stepped
		}
		if err := macFull(st); err != nil {
			return Decision{}, err
		}
		e := effect("launch:"+cmd.ID, EffLaunch, p)
		e.Then = CmdSessionLaunched
		return Decision{Effects: []Effect{e}, Result: map[string]string{"status": "starting", "access": p.Access, "steppedDown": p.SteppedDown}}, nil
	case CmdSessionStart:
		return Decision{Effects: []Effect{{Lane: cmd.ID, Kind: EffCreate, Payload: cmd.Payload}}}, nil
	case CmdSessionLaunched:
		var p struct {
			AgentID string `json:"childId"`
			LaunchPayload
		}
		if err := decode(cmd, &p); err != nil {
			return Decision{}, err
		}
		if p.AgentID == "" {
			return Decision{}, nil
		}
		createdBy := p.CreatedBy
		if createdBy == "" {
			createdBy = "agent"
		}
		return Decision{Lineage: []Session{{ID: p.AgentID, CreatedBy: createdBy, ParentID: p.Parent, Relationship: p.Rel, Depth: p.Depth}}}, nil
	case CmdSessionRename:
		var p TitlePayload
		if err := decode(cmd, &p); err != nil {
			return Decision{}, err
		}
		if err := live(st); err != nil {
			return Decision{}, err
		}
		if strings.TrimSpace(p.Title) == "" {
			return Decision{}, refuse(CodeInvalid, "A title can't be empty")
		}
		return Decision{Effects: []Effect{effect(cmd.Session, EffRename, p)}}, nil
	case CmdSessionRetitle:
		if err := live(st); err != nil {
			return Decision{}, err
		}
		return Decision{Effects: []Effect{effect("naming:"+cmd.Session, EffNaming, nil)}}, nil
	case CmdSessionConfig:
		var p ConfigPayload
		if err := decode(cmd, &p); err != nil {
			return Decision{}, err
		}
		if err := live(st); err != nil {
			return Decision{}, err
		}
		if p.Agent != "" && p.Agent != st.Session.Agent {
			return Decision{}, refuse(CodeUnsupported, "A session stays one agent. Fork it with anywhere_thread_transfer to continue on "+p.Agent)
		}
		return Decision{Effects: []Effect{effect(cmd.Session, EffConfigure, p)}}, nil
	case CmdSessionSettings:
		if err := live(st); err != nil {
			return Decision{}, err
		}
		return Decision{Effects: []Effect{{Lane: cmd.Session, Kind: EffSettings, Payload: cmd.Payload}}}, nil
	case CmdSessionOrganize:
		var p OrganizePayload
		if err := decode(cmd, &p); err != nil {
			return Decision{}, err
		}
		return organize(st, cmd, p)
	case CmdBackgroundHold:
		var p HoldPayload
		if err := decode(cmd, &p); err != nil {
			return Decision{}, err
		}
		return hold(st, cmd, p)
	case CmdWorkflowStop:
		var p StopTaskPayload
		if err := decode(cmd, &p); err != nil {
			return Decision{}, err
		}
		if err := live(st); err != nil {
			return Decision{}, err
		}
		if st.Session.Agent != "claude" {
			return Decision{}, refuse(CodeUnsupported, "Only Claude runs workflows a tool can stop")
		}
		return Decision{Effects: []Effect{effect(cmd.Session, EffStopTask, p)}}, nil
	case CmdTransferFork, CmdTransferMerge, CmdTransferStarted:
		var p TransferPayload
		if err := decode(cmd, &p); err != nil {
			return Decision{}, err
		}
		return transfer(st, cmd, p, &ids)
	}
	return Decision{}, refuse(CodeInvalid, "Unknown command "+cmd.Kind)
}

func decode(cmd Command, v any) error {
	if len(cmd.Payload) == 0 {
		return nil
	}
	if err := json.Unmarshal(cmd.Payload, v); err != nil {
		return refuse(CodeInvalid, "Bad "+cmd.Kind+": "+err.Error())
	}
	return nil
}

// idSeq names the rows a command creates from its id, so a replay names them the same.
type idSeq struct {
	base string
	n    int
}

func (s *idSeq) next(prefix string) string {
	s.n++
	return prefix + "_" + s.base + "_" + strconv.Itoa(s.n)
}

func live(st State) error {
	if st.Session == nil {
		return refuse(CodeNotFound, "Unknown session")
	}
	if st.Session.ClosedAt > 0 || st.Session.Status == "closed" {
		return refuse(CodeConflict, "This session is closed")
	}
	return nil
}

func send(st State, cmd Command, p SendPayload, ids *idSeq) (Decision, error) {
	if err := live(st); err != nil {
		return Decision{}, err
	}
	if strings.TrimSpace(p.Text) == "" {
		return Decision{}, refuse(CodeInvalid, "A message can't be empty")
	}
	origin := p.Origin
	if origin == "" {
		origin = OriginUser
	}
	var d Decision
	if origin == OriginUser {
		// A send releases what a Stop held.
		for _, e := range st.Queue {
			if e.State == EntryHeld {
				e.State = EntryPending
				d.Queue = append(d.Queue, e)
			}
		}
	}
	mode := p.Mode
	if mode == "" {
		mode = "auto"
	}
	if !slices.Contains([]string{"auto", "queue", "steer", "restart"}, mode) {
		return Decision{}, refuse(CodeInvalid, "mode is auto, queue, steer or restart")
	}
	busy := st.Session.Busy() || st.Starting || pendingAhead(st.Queue)
	queueIt := mode == "queue" || mode == "auto" && busy && !(origin == OriginWake && st.Self.CanSteer)
	if origin == OriginQuestion {
		queueIt = busy
	}
	if queueIt {
		e := QueueEntry{ID: ids.next("q"), Session: st.Session.ID, Text: p.Text, Origin: origin, Ord: nextOrd(st.Queue), State: EntryPending, Refs: p.Refs, CreatedAt: st.Now}
		d.Queue = append(d.Queue, e)
		d.Events = append(d.Events, Event{st.Session.ID, "queue.added", e})
		if !st.Session.Busy() {
			d.Follow = append(d.Follow, Command{Kind: CmdQueueDrain, Session: st.Session.ID})
		}
		d.Result = map[string]any{"queued": true, "entryId": e.ID}
		return d, nil
	}
	if mode == "restart" && st.Session.Busy() {
		d.Effects = append(d.Effects, effect(st.Session.ID, EffInterrupt, nil))
		d.Events = append(d.Events, Event{st.Session.ID, "turn.restarted", nil})
	}
	eff := effect(st.Session.ID, EffTurnStart, TurnPayload{Text: p.Text, Key: cmd.ID, Origin: origin})
	// What queued behind this start goes out if no turn came of it, as no turn's end will send it.
	eff.Then, eff.Else = CmdQueueDrain, CmdQueueDrain
	d.Effects = append(d.Effects, eff)
	d.Events = append(d.Events, Event{st.Session.ID, "message.sent", map[string]string{"origin": origin}})
	d.Result = map[string]any{"queued": false}
	return d, nil
}

func pendingAhead(q []QueueEntry) bool {
	return slices.ContainsFunc(q, func(e QueueEntry) bool { return e.State == EntryPending || e.State == EntrySending })
}

func nextOrd(q []QueueEntry) float64 {
	top := 0.0
	for _, e := range q {
		top = max(top, e.Ord)
	}
	return top + 1
}

// drain sends the next pending entry once the session is idle.
func drain(st State, cmd Command) (Decision, error) {
	if st.Session == nil || st.Session.Busy() || st.Starting || st.Session.ClosedAt > 0 {
		return Decision{}, nil
	}
	if slices.ContainsFunc(st.Queue, func(e QueueEntry) bool { return e.State == EntrySending }) {
		return Decision{}, nil
	}
	for _, e := range st.Queue {
		if e.State != EntryPending {
			continue
		}
		return sendEntry(st, e), nil
	}
	return Decision{}, nil
}

// sendEntry starts a turn with e; e counts as sent only once the agent takes it.
func sendEntry(st State, e QueueEntry) Decision {
	e.State = EntrySending
	eff := effect(st.Session.ID, EffTurnStart, TurnPayload{Text: e.Text, Key: e.ID, Origin: e.Origin})
	eff.Then, eff.Else = CmdQueueSent, CmdQueueUnsent
	return Decision{Queue: []QueueEntry{e}, Effects: []Effect{eff}}
}

// delivered settles a sending entry: sent, or pending again for the next idle.
func delivered(st State, cmd Command, id string) (Decision, error) {
	i := slices.IndexFunc(st.Queue, func(e QueueEntry) bool { return e.ID == id && e.State == EntrySending })
	if i < 0 {
		return Decision{}, nil
	}
	e := st.Queue[i]
	if cmd.Kind == CmdQueueUnsent {
		e.State = EntryPending
		return Decision{Queue: []QueueEntry{e}}, nil
	}
	e.State = EntrySent
	return Decision{Queue: []QueueEntry{e}, Events: []Event{{e.Session, "queue.sent", e.ID}}}, nil
}

func queueEdit(st State, cmd Command, p EntryPayload) (Decision, error) {
	if cmd.Kind == CmdQueueCancelRefs {
		var d Decision
		for _, e := range st.Queue {
			if e.State == EntryPending && slices.Contains(e.Refs, p.Ref) {
				e.State = EntryCancelled
				d.Queue = append(d.Queue, e)
			}
		}
		return d, nil
	}
	i := slices.IndexFunc(st.Queue, func(e QueueEntry) bool { return e.ID == p.EntryID })
	if i < 0 {
		return Decision{}, refuse(CodeNotFound, "That queued message was sent or removed")
	}
	e := st.Queue[i]
	if e.State != EntryPending && e.State != EntryHeld {
		return Decision{}, refuse(CodeConflict, "That queued message is already on its way")
	}
	switch cmd.Kind {
	case CmdQueueEdit:
		if strings.TrimSpace(p.Text) == "" {
			return Decision{}, refuse(CodeInvalid, "A message can't be empty")
		}
		e.Text = p.Text
	case CmdQueueCancel:
		e.State = EntryCancelled
	case CmdQueueReorder:
		e.Ord = reorder(st.Queue, e.ID, p.Before)
	case CmdQueueSendNow:
		return sendEntry(st, e), nil
	}
	return Decision{Queue: []QueueEntry{e}}, nil
}

// reorder is the order that puts id just ahead of before, or last.
func reorder(q []QueueEntry, id, before string) float64 {
	var rest []QueueEntry
	for _, e := range q {
		if e.ID != id && (e.State == EntryPending || e.State == EntryHeld) {
			rest = append(rest, e)
		}
	}
	if before == "" {
		return nextOrd(q)
	}
	for i, e := range rest {
		if e.ID == before {
			if i == 0 {
				return e.Ord - 1
			}
			return (rest[i-1].Ord + e.Ord) / 2
		}
	}
	return nextOrd(q)
}

// stopEvents notes that the session's running turn was asked to stop, so the
// turn that ends after it reads as interrupted whatever the agent reports.
func stopEvents(st State) []Event {
	if !st.Session.Busy() {
		return nil
	}
	return []Event{{st.Session.ID, "turn.interrupted", nil}}
}

// stop is the user's Stop: the turn stops, wake and question notices go for
// good, the user's own queued messages wait for their next send, and wakes
// still due from the session's delegations are disposed.
func stop(st State, cmd Command) (Decision, error) {
	if err := live(st); err != nil {
		return Decision{}, err
	}
	d := Decision{Effects: []Effect{effect(cmd.Session, EffInterrupt, nil)}, Events: stopEvents(st)}
	for _, e := range st.Queue {
		switch {
		case e.State != EntryPending:
		case e.Origin == OriginUser:
			e.State = EntryHeld
			d.Queue = append(d.Queue, e)
		default:
			e.State = EntryCancelled
			d.Queue = append(d.Queue, e)
		}
	}
	for _, t := range st.Children {
		if t.Delivery == DeliverPending || t.Delivery == DeliverQueued || t.Delivery == DeliverWaiting {
			t.Delivery = DeliverDisposed
			d.Delegations = append(d.Delegations, t)
		}
	}
	d.Events = append(d.Events, Event{cmd.Session, "session.stopped", nil})
	return d, nil
}

func request(st State, cmd Command, p RequestPayload) (Decision, error) {
	switch cmd.Kind {
	case CmdRequestOpen:
		r := Request{ID: p.RequestID, Session: cmd.Session, Kind: p.Kind, Payload: p.Payload, State: "open", CreatedAt: st.Now}
		d := Decision{Requests: []Request{r}, Events: []Event{{cmd.Session, "request.opened", map[string]string{"id": r.ID, "kind": r.Kind}}}}
		if p.Kind != "question" || st.Task == nil || st.Task.Terminal() {
			return d, nil
		}
		a := blocked(*st.Task, p.RequestID, p.Payload)
		d.Delegations, d.Follow, d.Effects = a.Delegations, a.Follow, a.Effects
		return d, nil
	case CmdRequestClose:
		d := Decision{Events: []Event{{cmd.Session, "request.closed", p.RequestID}}}
		if st.Request != nil {
			r := *st.Request
			r.State = "closed"
			d.Requests = append(d.Requests, r)
		}
		if st.Task != nil && st.Task.PendingQuestion == p.RequestID {
			t := *st.Task
			t.PendingQuestion = ""
			d.Delegations = append(d.Delegations, t)
			d.Follow = append(d.Follow, NewCommand("", CmdQueueCancelRefs, t.ParentID, EntryPayload{Ref: p.RequestID}))
		}
		return d, nil
	}
	if p.User {
		return Decision{Effects: []Effect{effect(cmd.Session, EffAnswer, p)}}, nil
	}
	if st.Request == nil || st.Request.State != "open" || st.Request.Session != cmd.Session {
		return Decision{}, refuse(CodeNotFound, "That question is no longer open")
	}
	if st.Request.Kind != "question" {
		return Decision{}, refuse(CodeForbidden, "Permission asks are answered by the user")
	}
	return Decision{Effects: []Effect{effect(st.Request.Session, EffAnswer, p)}}, nil
}

// blocked is a child's question reaching its task's parent.
func blocked(t Delegation, requestID string, payload json.RawMessage) Decision {
	t.PendingQuestion = requestID
	d := Decision{Delegations: []Delegation{t}}
	if t.Mode == "wait" && t.Delivery == DeliverWaiting {
		// The parent's wait reports hasPendingQuestion instead.
		return d
	}
	text := fmt.Sprintf("Delegated task %s (%q) is blocked on a question. Read it with anywhere_pending_request (taskId %q) and answer it when the answer follows from the task or the user's instructions; otherwise ask the user.", t.TaskID, t.Title, t.TaskID)
	d.Follow = append(d.Follow, NewCommand("", CmdMessageSend, t.ParentID, SendPayload{Text: text, Origin: OriginQuestion, Refs: []string{requestID}}))
	d.Effects = append(d.Effects, effect(t.ParentID, EffDelivery, DeliveryPayload{TaskIDs: []string{t.TaskID}, ChildIDs: []string{t.ChildID}, Outcome: "asks", Question: QuestionText(payload), Titles: []string{t.Title}, Agents: []string{t.Agent}}))
	return d
}

// QuestionText is the first question an AskUserQuestion-shaped payload asks.
func QuestionText(raw json.RawMessage) string {
	var q struct {
		Questions []struct {
			Question string `json:"question"`
		} `json:"questions"`
		Question string `json:"question"`
	}
	json.Unmarshal(raw, &q)
	if len(q.Questions) > 0 {
		return q.Questions[0].Question
	}
	return q.Question
}

// Access is what a child of a parent at access runs at on target: the
// parent's, or the broadest narrower level the target offers.
func Access(parent string, target AgentInfo) (access, steppedDown string, err error) {
	if target.NeverAsks {
		if parent != "full" {
			return "", "", refuse(CodeForbidden, "That agent always runs with Full access; only a Full session can delegate to it")
		}
		return "full", "", nil
	}
	if len(target.Access) == 0 || slices.Contains(target.Access, parent) || parent == "settings" && slices.Contains(target.Access, "ask") {
		return parent, "", nil
	}
	best, rank := "", -1
	for _, a := range target.Access {
		if r := accessRank[a]; r <= accessRank[parent] && r > rank {
			best, rank = a, r
		}
	}
	if best == "" {
		return "", "", refuse(CodeForbidden, "That agent offers no access as narrow as this session's")
	}
	return best, best, nil
}

func delegate(st State, cmd Command, p DelegatePayload, ids *idSeq) (Decision, error) {
	if err := live(st); err != nil {
		return Decision{}, err
	}
	parent := *st.Session
	if !parent.Busy() {
		return Decision{}, refuse(CodeInvalid, "Delegate from inside a turn of the calling session")
	}
	if strings.TrimSpace(p.Brief) == "" {
		return Decision{}, refuse(CodeInvalid, "A delegated task needs a brief")
	}
	if !st.Target.Known {
		return Decision{}, refuse(CodeInvalid, "Unknown agent "+p.Agent)
	}
	if parent.Depth+1 > st.Limits.Depth {
		return Decision{}, refuse(CodeDepthExceeded, fmt.Sprintf("Delegation goes at most %d levels below a session you opened", st.Limits.Depth))
	}
	access, stepped, err := Access(parent.Access, st.Target)
	if err != nil {
		return Decision{}, err
	}
	isolation := p.Isolation
	if isolation == "" {
		isolation = "worktree"
	}
	if isolation != "worktree" && isolation != "shared" {
		return Decision{}, refuse(CodeInvalid, "isolation is worktree or shared")
	}
	if (parent.Project == "" || p.OutsideGit && p.Environment == "") && isolation == "worktree" {
		isolation = "shared"
	}
	mode := p.Mode
	if mode == "" {
		mode = "async"
	}
	if mode != "async" && mode != "wait" {
		return Decision{}, refuse(CodeInvalid, "mode is async or wait")
	}
	wake := cmp.Or(p.CompletionWake, "settled_only")
	if wake != "settled_only" && wake != "always" {
		return Decision{}, refuse(CodeInvalid, "completionWake is settled_only or always")
	}
	title := strings.TrimSpace(p.Title)
	if title == "" {
		title = firstWords(p.Brief, 8)
	}
	t := Delegation{TaskID: p.TaskID, ParentID: parent.ID, Title: title, Role: p.Role, Agent: p.Agent, Model: p.Model, Effort: p.Effort, Brief: p.Brief,
		Isolation: isolation, Mode: mode, Status: TaskQueued, Access: access, SteppedDown: stepped, CompletionWake: wake, Delivery: DeliverNone,
		ClientRequestID: p.ClientRequestID, Environment: p.Environment, HostProject: p.HostProject, CreatedAt: st.Now}
	if t.TaskID == "" {
		t.TaskID = ids.next("task")
	}
	d := Decision{Events: []Event{{parent.ID, "delegation.requested", t.TaskID}}}
	running := 0
	for _, c := range st.Children {
		if c.Running() {
			running++
		}
	}
	if running < st.Limits.PerSession && st.Running < st.Limits.PerMac {
		t.Status = TaskStarting
		d.Effects = append(d.Effects, launchEffect(t, parent))
	}
	d.Delegations = append(d.Delegations, t)
	d.Result = t
	return d, nil
}

func launchEffect(t Delegation, parent Session) Effect {
	checkout := "shared"
	if t.Isolation == "worktree" {
		checkout = "worktree"
	}
	e := effect("launch:"+t.TaskID, EffLaunch, LaunchPayload{TaskID: t.TaskID, Parent: parent.ID, Agent: t.Agent, Model: t.Model, Effort: t.Effort, Access: t.Access,
		Prompt: t.Brief, Title: t.Title, Project: parent.Project, Cwd: parent.Cwd, Checkout: checkout, Depth: parent.Depth + 1, CreatedBy: "agent", Rel: "subagent", Chat: true,
		Env: t.Environment, HostProject: t.HostProject})
	e.Then, e.Else = CmdDelegateStarted, CmdDelegateFailed
	return e
}

func firstWords(s string, n int) string {
	w := strings.Fields(s)
	if len(w) > n {
		w = w[:n]
	}
	return strings.Join(w, " ")
}

func task(st State, cmd Command, p TaskPayload) (Decision, error) {
	if st.Task == nil {
		return Decision{}, refuse(CodeNotFound, "Unknown task "+p.TaskID)
	}
	t := *st.Task
	switch cmd.Kind {
	case CmdDelegateStarted:
		depth := 1
		if st.Parent != nil {
			depth = st.Parent.Depth + 1
		}
		lineage := []Session{{ID: p.ChildID, ParentID: t.ParentID, Relationship: "subagent", Depth: depth, CreatedBy: "agent"}}
		if t.Terminal() {
			// Cancelled while it launched: the child it got is closed.
			return Decision{Lineage: lineage, Effects: []Effect{effect(p.ChildID, EffClose, nil)}}, nil
		}
		t.ChildID, t.Status = p.ChildID, TaskWorking
		d := Decision{Delegations: []Delegation{t}, Lineage: lineage, Events: []Event{{t.ParentID, "delegation.started", t.TaskID}}}
		if st.Request != nil {
			// The launcher prompts the child before it returns, so it may have asked while no task named it.
			a := blocked(t, st.Request.ID, st.Request.Payload)
			d.Delegations, d.Follow, d.Effects = a.Delegations, a.Follow, a.Effects
		}
		return d, nil
	case CmdDelegateFailed, CmdDelegateSettle:
		if t.Terminal() {
			// A published result never changes.
			return Decision{}, nil
		}
		if cmd.Kind == CmdDelegateFailed {
			return settle(st, t, TaskFailed, "", cmp.Or(p.Error, "The session couldn't start"), false)
		}
		return settle(st, t, p.Status, p.Summary, p.Error, p.Stopped)
	case CmdDelegateWake:
		if p.CompletionWake != "settled_only" && p.CompletionWake != "always" {
			return Decision{}, refuse(CodeInvalid, "completionWake is settled_only or always")
		}
		t.CompletionWake = p.CompletionWake
		return Decision{Delegations: []Delegation{t}}, nil
	case CmdDelegateWaited:
		if t.Terminal() && (t.Delivery == DeliverWaiting || t.Delivery == DeliverPending) {
			t.Delivery = DeliverReturned
			return Decision{Delegations: []Delegation{t}}, nil
		}
		return Decision{}, nil
	case CmdDelegateCancel:
		var d Decision
		for _, x := range st.Subtree {
			if x.Terminal() {
				continue
			}
			wasRunning := x.ChildID != "" && x.Running()
			x.Status, x.SettledAt = TaskCancelled, st.Now
			x.Delivery = DeliverDisposed
			if x.PendingQuestion != "" {
				d.Follow = append(d.Follow, NewCommand("", CmdQueueCancelRefs, x.ParentID, EntryPayload{Ref: x.PendingQuestion}))
			}
			x.PendingQuestion = ""
			d.Delegations = append(d.Delegations, x)
			if wasRunning {
				d.Effects = append(d.Effects, effect(x.ChildID, EffInterrupt, nil))
			}
			d.Events = append(d.Events, Event{x.ParentID, "delegation.cancelled", x.TaskID})
		}
		d.Follow = append(d.Follow, Command{Kind: CmdDelegatePump, Session: t.ParentID})
		d.Result = map[string]any{"cancelled": len(d.Delegations)}
		return d, nil
	}
	return Decision{}, nil
}

// settle publishes t's result once and decides how the parent hears of it. A
// cancelled task wakes it only when the child was stopped, or for an "always" wake.
func settle(st State, t Delegation, status, summary, errText string, stopped bool) (Decision, error) {
	if status != TaskCompleted && status != TaskFailed && status != TaskCancelled {
		return Decision{}, refuse(CodeInvalid, "Bad settle status "+status)
	}
	t.Status, t.Summary, t.Error, t.SettledAt, t.PendingQuestion = status, summary, errText, st.Now, ""
	d := Decision{Events: []Event{{t.ParentID, "delegation.settled", map[string]string{"taskId": t.TaskID, "status": status}}}}
	wakes := status != TaskCancelled || t.CompletionWake == "always" || stopped
	switch {
	case t.Delivery == DeliverDisposed || !wakes:
	case t.Mode == "wait" && st.Parent != nil && st.Parent.Busy():
		t.Delivery = DeliverWaiting
	default:
		t.Delivery = DeliverPending
		d.Follow = append(d.Follow, Command{Kind: CmdWakeDeliver, Session: t.ParentID})
	}
	d.Delegations = append(d.Delegations, t)
	d.Follow = append(d.Follow, Command{Kind: CmdDelegatePump, Session: t.ParentID})
	return d, nil
}

// macFull refuses a session an agent starts once agents run the Mac's limit.
func macFull(st State) error {
	if st.Running >= st.Limits.PerMac {
		return refuse(CodeConflict, fmt.Sprintf("Agents already run %d sessions on this Mac; start more once some settle", st.Limits.PerMac))
	}
	return nil
}

// pump starts the session's queued delegations as far as the limits allow.
func pump(st State, cmd Command) (Decision, error) {
	if st.Session == nil {
		return Decision{}, nil
	}
	running := 0
	for _, c := range st.Children {
		if c.Running() {
			running++
		}
	}
	var d Decision
	mac := st.Running
	for _, c := range st.Children {
		if c.Status != TaskQueued || running >= st.Limits.PerSession || mac >= st.Limits.PerMac {
			continue
		}
		c.Status = TaskStarting
		running++
		mac++
		d.Delegations = append(d.Delegations, c)
		d.Effects = append(d.Effects, launchEffect(c, *st.Session))
	}
	return d, nil
}

// WakeText is the message that wakes a parent; it carries no result.
func WakeText(taskIDs []string) string {
	if len(taskIDs) == 1 {
		return fmt.Sprintf("Delegated task %s reached a terminal state. Use task_status with taskIds [%q] to read its result.", taskIDs[0], taskIDs[0])
	}
	quoted := make([]string, len(taskIDs))
	for i, id := range taskIDs {
		quoted[i] = strconv.Quote(id)
	}
	return fmt.Sprintf("Delegated tasks %s reached a terminal state. Use task_status with taskIds [%s] to read their results.", strings.Join(taskIDs, ", "), strings.Join(quoted, ", "))
}

// deliver wakes the parent for every delegation whose wake is due. A wake
// still queued takes the newcomers, so siblings that settle before it is sent
// share one.
func deliver(st State, cmd Command, ids *idSeq) (Decision, error) {
	if st.Session == nil || st.Session.ClosedAt > 0 {
		return Decision{}, nil
	}
	var due []Delegation
	for _, t := range st.Children {
		if t.Delivery == DeliverPending {
			due = append(due, t)
		}
	}
	if len(due) == 0 {
		return Decision{}, nil
	}
	var d Decision
	var taskIDs, childIDs, titles, agents []string
	outcome := "done"
	for _, t := range due {
		taskIDs, childIDs, titles, agents = append(taskIDs, t.TaskID), append(childIDs, t.ChildID), append(titles, t.Title), append(agents, t.Agent)
		if t.Status == TaskFailed {
			outcome = "failed"
		}
	}
	d.Effects = append(d.Effects, effect(st.Session.ID, EffDelivery, DeliveryPayload{TaskIDs: taskIDs, ChildIDs: childIDs, Outcome: outcome, Titles: titles, Agents: agents}))
	for _, e := range st.Queue {
		if e.State == EntryPending && e.Origin == OriginWake {
			all := append(slices.Clone(e.Refs), taskIDs...)
			e.Refs, e.Text = all, WakeText(all)
			d.Queue = append(d.Queue, e)
			for _, t := range due {
				t.Delivery = DeliverQueued
				d.Delegations = append(d.Delegations, t)
			}
			return d, nil
		}
	}
	sent, err := send(st, Command{ID: cmd.ID, Kind: CmdMessageSend, Session: st.Session.ID}, SendPayload{Text: WakeText(taskIDs), Origin: OriginWake, Refs: taskIDs}, ids)
	if err != nil {
		return Decision{}, err
	}
	state := DeliverDelivered
	if q, _ := sent.Result.(map[string]any); q["queued"] == true {
		state = DeliverQueued
	}
	for _, t := range due {
		t.Delivery = state
		d.Delegations = append(d.Delegations, t)
	}
	d.Queue = append(d.Queue, sent.Queue...)
	d.Effects = append(d.Effects, sent.Effects...)
	d.Follow = append(d.Follow, sent.Follow...)
	d.Events = append(d.Events, sent.Events...)
	return d, nil
}

// parentIdle switches a wait whose parent turn ended to a wake, and marks
// queued wakes that went out as delivered.
func parentIdle(st State, cmd Command) (Decision, error) {
	var d Decision
	wake := false
	for _, t := range st.Children {
		if t.Terminal() && t.Delivery == DeliverWaiting {
			t.Delivery = DeliverPending
			d.Delegations = append(d.Delegations, t)
			wake = true
		}
	}
	if wake {
		d.Follow = append(d.Follow, Command{Kind: CmdWakeDeliver, Session: cmd.Session})
	}
	d.Follow = append(d.Follow, Command{Kind: CmdQueueDrain, Session: cmd.Session})
	return d, nil
}

func organize(st State, cmd Command, p OrganizePayload) (Decision, error) {
	if st.Session == nil {
		return Decision{}, refuse(CodeNotFound, "Unknown session")
	}
	switch p.Action {
	case "pin", "unpin":
		return Decision{Effects: []Effect{effect(cmd.Session, EffPin, map[string]bool{"pinned": p.Action == "pin"})}}, nil
	case "mark_unread":
		return Decision{Effects: []Effect{effect(cmd.Session, EffMarkUnread, nil)}}, nil
	case "archive":
		d := Decision{Archive: []string{cmd.Session}}
		if st.Session.ClosedAt == 0 {
			d.Effects = append(d.Effects, effect(cmd.Session, EffClose, nil))
		}
		return d, nil
	case "snooze", "settle":
		return Decision{}, refuse(CodeUnsupported, p.Action+" isn't supported in Anywhere")
	}
	return Decision{}, refuse(CodeInvalid, "action is pin, unpin, mark_unread or archive")
}

// HoldCap is how long an agent-declared background task keeps its session Working at most.
const HoldCap = 6 * 60 * 60 * 1000

func hold(st State, cmd Command, p HoldPayload) (Decision, error) {
	if err := live(st); err != nil {
		return Decision{}, err
	}
	if st.Session.Agent != "claude" && st.Session.Agent != "opencode" {
		return Decision{}, refuse(CodeUnsupported, "Only Claude and OpenCode declare background work")
	}
	d := Decision{}
	if !p.Waiting {
		d.Unhold = append(d.Unhold, st.Held...)
		d.Effects = append(d.Effects, effect(cmd.Session, EffHold, HoldPayload{Waiting: false}))
		d.Result = map[string]any{"matched": st.Held}
		return d, nil
	}
	d.Hold = p.TaskIDs
	d.Effects = append(d.Effects, effect(cmd.Session, EffHold, p))
	d.Result = map[string]any{"matched": p.TaskIDs}
	return d, nil
}

func transfer(st State, cmd Command, p TransferPayload, ids *idSeq) (Decision, error) {
	switch cmd.Kind {
	case CmdTransferStarted:
		if st.Session == nil {
			return Decision{}, nil
		}
		return Decision{Transfers: []Transfer{{ID: p.TransferID, Source: cmd.Session, Target: p.TargetID, Kind: "fork", SourcePoint: p.SourcePoint, State: "done", CreatedAt: st.Now}},
			Lineage: []Session{{ID: p.TargetID, ParentID: cmd.Session, Relationship: "fork", Depth: st.Session.Depth, CreatedBy: "agent"}}}, nil
	case CmdTransferFork:
		if err := live(st); err != nil {
			return Decision{}, err
		}
		s := *st.Session
		agent := cmp.Or(p.Agent, s.Agent)
		if !st.Target.Known {
			return Decision{}, refuse(CodeInvalid, "Unknown agent "+agent)
		}
		access, _, err := Access(s.Access, st.Target)
		if err != nil {
			return Decision{}, err
		}
		if err := macFull(st); err != nil {
			return Decision{}, err
		}
		id := ids.next("xfer")
		checkout := "shared"
		if p.Isolation == "worktree" && s.Project != "" {
			checkout = "worktree"
		}
		lp := LaunchPayload{TransferID: id, Parent: s.ID, Agent: agent, Model: p.Model, Access: access, Prompt: p.Prompt, Title: cmp.Or(p.Title, s.Title), Project: s.Project, Cwd: s.Cwd,
			Checkout: checkout, Depth: s.Depth, CreatedBy: "agent", Rel: "fork", Chat: true}
		if p.Native && agent == s.Agent && s.Conversation != "" {
			lp.Fork = s.ID
		} else {
			lp.Context = p.Context
		}
		e := effect("launch:"+id, EffForkLaunch, lp)
		e.Then = CmdTransferStarted
		return Decision{Transfers: []Transfer{{ID: id, Source: s.ID, Kind: "fork", SourcePoint: p.SourcePoint, State: "starting", CreatedAt: st.Now}}, Effects: []Effect{e},
			Result: map[string]any{"transferId": id, "status": "starting", "native": lp.Fork != ""}}, nil
	case CmdTransferMerge:
		if err := live(st); err != nil {
			return Decision{}, err
		}
		if st.Other == nil || st.Other.ClosedAt > 0 {
			return Decision{}, refuse(CodeNotFound, "Unknown or closed target session")
		}
		if !st.Related || st.Other.Project != st.Session.Project {
			return Decision{}, refuse(CodeForbidden, "merge_back goes only to a related session (parent, child, fork or source) in the same project")
		}
		if strings.TrimSpace(p.Context) == "" {
			return Decision{}, refuse(CodeInvalid, "Nothing to merge back since the split")
		}
		id := ids.next("xfer")
		d := Decision{Transfers: []Transfer{{ID: id, Source: st.Session.ID, Target: st.Other.ID, Kind: "merge_back", State: "done", CreatedAt: st.Now}},
			Result: map[string]any{"transferId": id, "status": "sent"}}
		d.Follow = append(d.Follow, NewCommand("", CmdMessageSend, st.Other.ID, SendPayload{Text: p.Context, Origin: OriginContext}))
		return d, nil
	}
	return Decision{}, nil
}
