package orch

import (
	"encoding/json"
	"testing"
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
