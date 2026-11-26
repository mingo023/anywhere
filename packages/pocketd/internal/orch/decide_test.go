package orch

import (
	"encoding/json"
	"slices"
	"strings"
	"testing"
	"unicode/utf8"
)

func closedState() State {
	st := parentState("idle", 0)
	st.Session.ClosedAt = 5
	return st
}

func TestSessionCreateStepsTheChildsAccessDownAndLaunchesOnItsOwnLane(t *testing.T) {
	st := parentState("working", 0)
	d, err := decide(st, NewCommand("c1", CmdSessionCreate, "p", LaunchPayload{Agent: "codex", Prompt: "go"}))
	if err != nil {
		t.Fatal(err)
	}
	if len(d.Effects) != 1 || d.Effects[0].Kind != EffLaunch || d.Effects[0].Lane != "launch:c1" || d.Effects[0].Then != CmdSessionLaunched {
		t.Fatalf("effects = %+v", d.Effects)
	}
	var p LaunchPayload
	json.Unmarshal(d.Effects[0].Payload, &p)
	if p.Access != "edits" || p.SteppedDown != "edits" {
		t.Errorf("access %q stepped %q, want codex's edits", p.Access, p.SteppedDown)
	}
}

func TestSessionCreateWithNoCallerKeepsTheAccessAsked(t *testing.T) {
	st := State{Limits: DefaultLimits, Target: agents("codex")}
	d, err := decide(st, NewCommand("c1", CmdSessionCreate, "", LaunchPayload{Agent: "codex", Access: "auto"}))
	if err != nil {
		t.Fatal(err)
	}
	var p LaunchPayload
	json.Unmarshal(d.Effects[0].Payload, &p)
	if p.Access != "auto" || p.SteppedDown != "" {
		t.Errorf("access %q stepped %q, want auto as asked", p.Access, p.SteppedDown)
	}
}

func TestSessionCreateRefusesWhenAgentsFillTheMac(t *testing.T) {
	st := parentState("working", 0)
	st.Running = st.Limits.PerMac
	_, err := decide(st, NewCommand("c1", CmdSessionCreate, "p", LaunchPayload{Agent: "codex"}))
	if err == nil || AsFailure(err).Code != CodeConflict {
		t.Fatalf("err = %v, want conflict", err)
	}
}

func TestSessionCreateRefusesAnAgentThatAlwaysRunsFullForANarrowerCaller(t *testing.T) {
	st := parentState("working", 0)
	st.Target = agents("amp")
	_, err := decide(st, NewCommand("c1", CmdSessionCreate, "p", LaunchPayload{Agent: "amp"}))
	if err == nil || AsFailure(err).Code != CodeForbidden {
		t.Fatalf("err = %v, want forbidden", err)
	}
}

func TestSessionCloseRunsTheCloseEffectAndRefusesAnUnknownSession(t *testing.T) {
	d, err := decide(parentState("idle", 0), NewCommand("c", CmdSessionClose, "p", nil))
	if err != nil || len(d.Effects) != 1 || d.Effects[0].Kind != EffClose || d.Effects[0].Lane != "p" {
		t.Fatalf("close = %+v, %v", d, err)
	}
	if _, err := decide(State{}, NewCommand("c", CmdSessionClose, "nope", nil)); AsFailure(err).Code != CodeNotFound || err == nil {
		t.Fatalf("err = %v, want not_found", err)
	}
}

func TestSessionRenameChangesOnlyTheTitleAndRefusesBlankOrClosed(t *testing.T) {
	d, err := decide(parentState("idle", 0), NewCommand("c", CmdSessionRename, "p", TitlePayload{Title: "New"}))
	if err != nil || len(d.Effects) != 1 || d.Effects[0].Kind != EffRename || string(d.Effects[0].Payload) != `{"title":"New"}` {
		t.Fatalf("rename = %+v, %v", d, err)
	}
	if _, err := decide(parentState("idle", 0), NewCommand("c", CmdSessionRename, "p", TitlePayload{Title: "  "})); err == nil || AsFailure(err).Code != CodeInvalid {
		t.Errorf("blank title: %v, want invalid_request", err)
	}
	if _, err := decide(closedState(), NewCommand("c", CmdSessionRename, "p", TitlePayload{Title: "New"})); err == nil || AsFailure(err).Code != CodeConflict {
		t.Errorf("closed session: %v, want conflict", err)
	}
}

func TestSessionConfigureTakesModelAndEffortButNotAnotherAgent(t *testing.T) {
	d, err := decide(parentState("idle", 0), NewCommand("c", CmdSessionConfig, "p", ConfigPayload{Agent: "claude", Model: "opus", Effort: "high"}))
	if err != nil || len(d.Effects) != 1 || d.Effects[0].Kind != EffConfigure {
		t.Fatalf("configure = %+v, %v", d, err)
	}
	var p ConfigPayload
	json.Unmarshal(d.Effects[0].Payload, &p)
	if p.Model != "opus" || p.Effort != "high" {
		t.Errorf("payload = %+v", p)
	}
	if _, err := decide(parentState("idle", 0), NewCommand("c", CmdSessionConfig, "p", ConfigPayload{Agent: "codex"})); err == nil || AsFailure(err).Code != CodeUnsupported {
		t.Errorf("another agent: %v, want unsupported", err)
	}
	if _, err := decide(closedState(), NewCommand("c", CmdSessionConfig, "p", ConfigPayload{Model: "opus"})); err == nil || AsFailure(err).Code != CodeConflict {
		t.Errorf("closed session: %v, want conflict", err)
	}
}

func TestSessionSettingsTakesTheAccessTooOnALiveSessionOnly(t *testing.T) {
	d, err := decide(parentState("idle", 0), NewCommand("c", CmdSessionSettings, "p", SettingsPayload{Model: "opus", Access: "full"}))
	if err != nil || len(d.Effects) != 1 || d.Effects[0].Kind != EffSettings || d.Effects[0].Lane != "p" {
		t.Fatalf("settings = %+v, %v", d, err)
	}
	var p SettingsPayload
	json.Unmarshal(d.Effects[0].Payload, &p)
	if p.Model != "opus" || p.Access != "full" {
		t.Errorf("payload = %+v", p)
	}
	if _, err := decide(State{}, NewCommand("c", CmdSessionSettings, "nope", SettingsPayload{Access: "full"})); err == nil || AsFailure(err).Code != CodeNotFound {
		t.Errorf("unknown session: %v, want not_found", err)
	}
	if _, err := decide(closedState(), NewCommand("c", CmdSessionSettings, "p", SettingsPayload{Access: "full"})); err == nil || AsFailure(err).Code != CodeConflict {
		t.Errorf("closed session: %v, want conflict", err)
	}
}

func TestTurnInterruptRunsTheInterruptEffectOnALiveSessionOnly(t *testing.T) {
	d, err := decide(parentState("working", 0), NewCommand("c", CmdTurnInterrupt, "p", nil))
	if err != nil || len(d.Effects) != 1 || d.Effects[0].Kind != EffInterrupt || d.Effects[0].Lane != "p" {
		t.Fatalf("interrupt = %+v, %v", d, err)
	}
	if _, err := decide(State{}, NewCommand("c", CmdTurnInterrupt, "nope", nil)); err == nil || AsFailure(err).Code != CodeNotFound {
		t.Errorf("unknown session: %v, want not_found", err)
	}
	if _, err := decide(closedState(), NewCommand("c", CmdTurnInterrupt, "p", nil)); err == nil || AsFailure(err).Code != CodeConflict {
		t.Errorf("closed session: %v, want conflict", err)
	}
}

func TestTurnSendNowRunsTheSendNowEffectOnALiveSessionOnly(t *testing.T) {
	d, err := decide(parentState("working", 0), NewCommand("c", CmdTurnSendNow, "p", nil))
	if err != nil || len(d.Effects) != 1 || d.Effects[0].Kind != EffSendNow || d.Effects[0].Lane != "p" {
		t.Fatalf("send_now = %+v, %v", d, err)
	}
	if _, err := decide(State{}, NewCommand("c", CmdTurnSendNow, "nope", nil)); err == nil || AsFailure(err).Code != CodeNotFound {
		t.Errorf("unknown session: %v, want not_found", err)
	}
	if _, err := decide(closedState(), NewCommand("c", CmdTurnSendNow, "p", nil)); err == nil || AsFailure(err).Code != CodeConflict {
		t.Errorf("closed session: %v, want conflict", err)
	}
}

func TestMessageSendRestartCutsTheRunningTurnThenStartsTheMessage(t *testing.T) {
	d, err := decide(parentState("working", 0), NewCommand("c", CmdMessageSend, "p", SendPayload{Text: "new plan", Mode: "restart"}))
	if err != nil {
		t.Fatal(err)
	}
	if len(d.Effects) != 2 || d.Effects[0].Kind != EffInterrupt || d.Effects[1].Kind != EffTurnStart || len(d.Queue) != 0 {
		t.Fatalf("restart on a working session = effects %+v queue %+v, want interrupt then turn.start and nothing queued", d.Effects, d.Queue)
	}
}

func TestMessageSendRestartOnAnIdleSessionJustStartsTheTurn(t *testing.T) {
	d, err := decide(parentState("idle", 0), NewCommand("c", CmdMessageSend, "p", SendPayload{Text: "go", Mode: "restart"}))
	if err != nil {
		t.Fatal(err)
	}
	if len(d.Effects) != 1 || d.Effects[0].Kind != EffTurnStart {
		t.Fatalf("effects = %+v, want one turn.start", d.Effects)
	}
}

func TestMessageSendRefusesAModeItDoesNotKnow(t *testing.T) {
	_, err := decide(parentState("idle", 0), NewCommand("c", CmdMessageSend, "p", SendPayload{Text: "go", Mode: "later"}))
	if err == nil || AsFailure(err).Code != CodeInvalid {
		t.Fatalf("err = %v, want invalid_request", err)
	}
}

func TestASettleForATurnOlderThanTheChildsLatestIsDropped(t *testing.T) {
	task := Delegation{TaskID: "t1", ParentID: "p", ChildID: "c", Status: TaskWorking}
	st := State{Task: &task, ChildTurn: 2}
	d, err := decide(st, NewCommand("", CmdDelegateSettle, "p", TaskPayload{TaskID: "t1", Status: TaskCompleted, Turn: 1}))
	if err != nil || len(d.Delegations) != 0 {
		t.Fatalf("settled anyway: %+v %v", d.Delegations, err)
	}
	d, _ = decide(st, NewCommand("", CmdDelegateSettle, "p", TaskPayload{TaskID: "t1", Status: TaskCompleted, Turn: 2}))
	if len(d.Delegations) != 1 || d.Delegations[0].Status != TaskCompleted {
		t.Fatalf("the latest turn's settle: %+v", d.Delegations)
	}
}

func TestAFollowUpIsANewTaskAndTheSettledOneKeepsItsResult(t *testing.T) {
	task := Delegation{TaskID: "t1", ParentID: "p", ChildID: "c", Title: "Fix redirect", Mode: "wait", Status: TaskCompleted, Summary: "Fixed", SettledAt: 5, Delivery: DeliverDelivered}
	d, err := decide(State{Task: &task, Now: 9}, NewCommand("f1", CmdDelegateFollow, "p", TaskPayload{TaskID: "t1", Brief: "Add a test"}))
	if err != nil || len(d.Delegations) != 1 {
		t.Fatalf("decision %+v %v", d, err)
	}
	f := d.Delegations[0]
	if f.TaskID == "t1" || f.ParentID != "p" || f.ChildID != "c" || f.Title != "Fix redirect" || f.Brief != "Add a test" || f.Mode != "async" ||
		f.Status != TaskWorking || f.Summary != "" || f.SettledAt != 0 || f.Delivery != DeliverNone || f.CreatedAt != 9 {
		t.Fatalf("follow-up %+v", f)
	}
	working := Delegation{TaskID: "t1", Status: TaskWorking}
	if d, _ := decide(State{Task: &working}, NewCommand("f2", CmdDelegateFollow, "p", TaskPayload{TaskID: "t1"})); len(d.Delegations) != 0 {
		t.Fatalf("a working task took a follow-up: %+v", d.Delegations)
	}
}

func TestReadingAResultTakesItsTaskOutOfTheWakeStillQueued(t *testing.T) {
	t1 := Delegation{TaskID: "t1", ParentID: "p", Status: TaskCompleted, Summary: "one", Delivery: DeliverQueued}
	t2 := Delegation{TaskID: "t2", ParentID: "p", Status: TaskCompleted, Summary: "two", Delivery: DeliverQueued}
	queue := []QueueEntry{
		{ID: "w1", Origin: OriginWake, State: EntryPending, Refs: []string{"t1", "t2"}, Text: WakeText([]Delegation{t1, t2})},
		{ID: "w2", Origin: OriginWake, State: EntryPending, Refs: []string{"t1"}, Text: WakeText([]Delegation{t1})},
	}
	d, err := decide(State{Task: &t1, Queue: queue, Children: []Delegation{t1, t2}}, NewCommand("", CmdDelegateWaited, "p", TaskPayload{TaskID: "t1"}))
	if err != nil || len(d.Queue) != 2 || len(d.Delegations) != 1 || d.Delegations[0].Delivery != DeliverReturned {
		t.Fatalf("decision %+v %v", d, err)
	}
	if w := d.Queue[0]; w.State != EntryPending || !slices.Equal(w.Refs, []string{"t2"}) || w.Text != WakeText([]Delegation{t2}) {
		t.Fatalf("shared wake %+v", w)
	}
	if d.Queue[1].State != EntryCancelled {
		t.Fatalf("its own wake %+v", d.Queue[1])
	}
	delivered := t1
	delivered.Delivery = DeliverDelivered
	if d, _ := decide(State{Task: &delivered}, NewCommand("", CmdDelegateWaited, "p", TaskPayload{TaskID: "t1"})); len(d.Delegations) != 0 {
		t.Fatalf("a delivered wake was read again: %+v", d.Delegations)
	}
}

func TestAWakeCarriesEachReportAndCutsOneTooLong(t *testing.T) {
	done := Delegation{TaskID: "t1", Title: "Fix redirect", Status: TaskCompleted, Summary: "Fixed in auth.ts\nTests pass"}
	failed := Delegation{TaskID: "t2", Title: "Bump deps", Status: TaskFailed, Error: "rate limited"}
	text := WakeText([]Delegation{done, failed})
	for _, want := range []string{"Delegated tasks t1, t2 reached a terminal state.", "t1 (\"Fix redirect\") completed:\nFixed in auth.ts\nTests pass", "t2 (\"Bump deps\") failed (rate limited), with no report."} {
		if !strings.Contains(text, want) {
			t.Fatalf("wake lacks %q:\n%s", want, text)
		}
	}
	long := done
	long.Summary = strings.Repeat("é", wakeReports)
	text = WakeText([]Delegation{long})
	if len(text) > wakeReports+300 || !utf8.ValidString(text) || !strings.HasSuffix(text, "[Cut short; task_status has the whole report.]") {
		t.Fatalf("a long report: %d bytes, valid %v, ends %q", len(text), utf8.ValidString(text), text[len(text)-60:])
	}
	edge := done
	edge.Summary = strings.Repeat("a", wakeReports)
	if strings.Contains(WakeText([]Delegation{edge}), "Cut short") {
		t.Fatal("a report exactly at the bound was cut")
	}
}
