package proto

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"
)

func TestTheBoardGoldenIsWhatTheServerWrites(t *testing.T) {
	raw := golden(t, "server", "board.json")
	var snap Board
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&snap); err != nil {
		t.Fatal(err)
	}
	got, _ := json.Marshal(snap)
	if !bytes.Equal(got, raw) {
		t.Fatalf("got  %s\nwant %s", got, raw)
	}
}

func TestAnEmptyBoardHasAnEmptyList(t *testing.T) {
	got, _ := json.Marshal(NewBoard(nil, DefaultBoardLimit))
	if string(got) != `{"type":"board","tickets":[],"limit":4}` {
		t.Fatalf("%s", got)
	}
}

func TestATicketNeedsATitleAPromptAKnownAgentAndAFolder(t *testing.T) {
	ok := Ticket{Title: "T", Prompt: "P", Provider: "claude", Folder: "/app"}
	if !ok.Valid() {
		t.Fatal("rejected a good ticket")
	}
	for name, change := range map[string]func(*Ticket){
		"blank title":  func(x *Ticket) { x.Title = "  " },
		"long title":   func(x *Ticket) { x.Title = strings.Repeat("é", MaxTicketTitle+1) },
		"blank prompt": func(x *Ticket) { x.Prompt = " \n" },
		"long prompt":  func(x *Ticket) { x.Prompt = strings.Repeat("a", MaxPrompt+1) },
		"agent":        func(x *Ticket) { x.Provider = "nope" },
		"folder":       func(x *Ticket) { x.Folder = "" },
		"priority":     func(x *Ticket) { x.Priority = MaxTicketPriority + 1 },
		"negative":     func(x *Ticket) { x.Priority = -1 },
		"blank label":    func(x *Ticket) { x.Labels = []string{" "} },
		"unknown label":  func(x *Ticket) { x.Labels = []string{"Perf"} },
		"spaced label":   func(x *Ticket) { x.Labels = []string{" Bug "} },
		"repeated label": func(x *Ticket) { x.Labels = []string{"Bug", "Bug"} },
		"base flag":    func(x *Ticket) { x.Base = "--force" },
	} {
		bad := ok
		change(&bad)
		if bad.Valid() {
			t.Errorf("%s: accepted", name)
		}
	}
}

func TestOnlyTicketsWithWorkInHandHoldASlot(t *testing.T) {
	for status, want := range map[string]bool{TicketStarting: true, TicketWorking: true, TicketNeedsYou: true, TicketDone: false, TicketIdle: false, TicketFailed: false, TicketStopped: false} {
		if got := (Ticket{State: TicketLive, Status: status}).HoldsSlot(); got != want {
			t.Errorf("%s: %v", status, got)
		}
	}
	if (Ticket{State: TicketClosed, Status: TicketWorking}).HoldsSlot() {
		t.Error("a closed ticket held a slot")
	}
}

func TestDecodeTicketRefusesFieldsAClientMayNotSet(t *testing.T) {
	var dst *Ticket
	if !decodeTicket(json.RawMessage(`{"title":"T","prompt":"P","provider":"claude","folder":"/app"}`), &dst) || dst.Title != "T" {
		t.Fatal("refused a good draft")
	}
	for _, raw := range []string{
		`{"title":"T","prompt":"P","provider":"claude","folder":"/app","state":"live"}`,
		`{"title":"T","prompt":"P","provider":"claude","folder":"/app","agentId":"a"}`,
		`{"prompt":"P","provider":"claude","folder":"/app"}`,
		`not json`,
	} {
		var d *Ticket
		if decodeTicket(json.RawMessage(raw), &d) {
			t.Errorf("accepted %s", raw)
		}
	}
}
