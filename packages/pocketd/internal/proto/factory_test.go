package proto

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"
)

func factoryGolden(t *testing.T, side, name string) []byte {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join("testdata", "factory", side, name))
	if err != nil {
		t.Fatal(err)
	}
	return bytes.TrimSpace(raw)
}

var factoryServerGolden = map[string]any{
	"board": NewBoard([]Ticket{
		{ID: "t1", Title: "Fold hunks", Prompt: "Fold unchanged hunks", Provider: "claude", Folder: "/app", Base: "main", State: TicketLive, Order: 3, Status: TicketNeedsYou,
			AgentID: "a1", TerminalID: "tm1", Worktree: "/wt/fold", CreatedAt: 1000, ChangedAt: 2000, Number: 7,
			Station: StationChecks, Gate: GateAsk, PR: 12, Fix: 1, Hint: true},
		{ID: "t2", Title: "Reveal in Finder", Prompt: "Add it", Provider: "codex", Folder: "/app", State: TicketTodo, Order: 1, CreatedAt: 1000, ChangedAt: 1000},
	}, 4),
	"factory_config": NewFactoryConfig("factory-1", "/app", FactoryConfig{Enabled: true, Spec: true, Reviewer: true, Lead: "claude-opus", Workers: "claude-sonnet", Review: "codex",
		Slots: 2, Fix: 1, Checks: []string{"cargo test", "cargo clippy --all-targets"}}, ""),
	"factory_config_problem":    NewFactoryConfig("factory-2", "/app", DefaultFactoryConfig(), "Install GitHub CLI (gh) to use the Factory"),
	"error_factory_unavailable": NewErrorCode("factory-3", CodeFactoryUnavailable, "This project has no GitHub remote"),
	"factory_ticket": NewFactoryTicket("factory-4", FactoryTicket{
		TicketID: "t1",
		Spec:     "## Brief\nSends wait in a queue.\n## Acceptance criteria\n- [ ] A send survives a restart\n## Risks\n- None",
		Activity: []FactoryEvent{
			{At: 1790000000000, Station: StationSpec, Actor: "anywhere", Kind: "moved"},
			{At: 1790000060000, Station: StationPlan, Actor: "anywhere", Kind: "moved"},
			{At: 1790000120000, Station: StationPlan, Actor: "you", Kind: "approved", Text: "2 workers"},
		},
	}),
	"factory_ticket_checks": NewFactoryTicket("factory-9", FactoryTicket{
		TicketID: "t1",
		Activity: []FactoryEvent{{At: 1762900000000, Station: StationChecks, Actor: "anywhere", Kind: "check", Text: "Failed (exit 101): cargo test"}},
		Checks: []FactoryCheck{
			{Command: "cargo fmt --check", Exit: 0, Output: "", Attempt: 1, At: 1762900000000},
			{Command: "cargo test", Exit: 101, Output: "test result: FAILED. 41 passed; 1 failed\n", Attempt: 1, At: 1762900030000},
		},
	}),
	"factory_update": NewFactoryUpdate("t1"),
}

func TestAFactoryTicketCarriesItsStationOnTheBoard(t *testing.T) {
	var snap Board
	dec := json.NewDecoder(bytes.NewReader(factoryGolden(t, "server", "board.json")))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&snap); err != nil {
		t.Fatal(err)
	}
	if f := snap.Tickets[0]; f.Station != StationChecks || f.Gate != GateAsk || f.PR != 12 || f.Fix != 1 || !f.Hint {
		t.Fatalf("got %+v", f)
	}
	if plain := snap.Tickets[1]; plain.Station != "" || plain.Gate != "" || plain.PR != 0 || plain.Fix != 0 || plain.Hint {
		t.Fatalf("got %+v", plain)
	}
}

func TestATicketWithoutAStationSendsNoFactoryField(t *testing.T) {
	raw, _ := json.Marshal(Ticket{ID: "t", Title: "T", Prompt: "P", Provider: "claude", Folder: "/app", State: TicketTodo})
	for _, key := range []string{`"station"`, `"gate"`, `"pr"`, `"fix"`, `"hint"`} {
		if strings.Contains(string(raw), key) {
			t.Errorf("%s in %s", key, raw)
		}
	}
}

func TestTheStationsAndGatesHaveTheirWireNames(t *testing.T) {
	stations := []string{StationSpec, StationPlan, StationImplement, StationChecks, StationPR, StationMerged}
	gates := []string{GatePlan, GatePR, GateAsk}
	if got := strings.Join(stations, " ") + " | " + strings.Join(gates, " "); got != "spec plan implement checks pr merged | plan pr ask" {
		t.Fatalf("got %s", got)
	}
	if MaxFix != 2 || CapFactory != "factory.v1" {
		t.Fatalf("MaxFix %d, CapFactory %s", MaxFix, CapFactory)
	}
}

func TestTheFactoryStartsOffWithItsSpecAndReviewerOn(t *testing.T) {
	want := FactoryConfig{Spec: true, Reviewer: true, Lead: "claude-opus", Workers: "claude-sonnet", Review: "codex", Fix: MaxFix, Checks: []string{}}
	if got := DefaultFactoryConfig(); !reflect.DeepEqual(got, want) || !got.Valid() {
		t.Fatalf("got %+v", got)
	}
}

func TestAFactoryConfigKeepsItsLimitsAndCommandsInRange(t *testing.T) {
	for name, change := range map[string]func(*FactoryConfig){
		"negative slots":         func(c *FactoryConfig) { c.Slots = -1 },
		"slots over the Board's": func(c *FactoryConfig) { c.Slots = MaxBoardLimit + 1 },
		"negative fix":           func(c *FactoryConfig) { c.Fix = -1 },
		"fix over the cap":       func(c *FactoryConfig) { c.Fix = MaxFix + 1 },
		"blank command":          func(c *FactoryConfig) { c.Checks = []string{"go test ./...", "  "} },
		"long command":           func(c *FactoryConfig) { c.Checks = []string{strings.Repeat("é", MaxFactoryCheck+1)} },
		"too many commands":      func(c *FactoryConfig) { c.Checks = slices.Repeat([]string{"true"}, MaxFactoryChecks+1) },
	} {
		bad := DefaultFactoryConfig()
		change(&bad)
		if bad.Valid() {
			t.Errorf("%s: accepted", name)
		}
	}
	most := DefaultFactoryConfig()
	most.Slots, most.Fix, most.Checks = MaxBoardLimit, 0, slices.Repeat([]string{strings.Repeat("é", MaxFactoryCheck)}, MaxFactoryChecks)
	if !most.Valid() {
		t.Fatal("refused a config at its limits")
	}
}

func TestTheFactoryConfigGoldensDecodeToTheirFields(t *testing.T) {
	m, err := DecodeClient(factoryGolden(t, "client", "factory_config_get.json"))
	if err != nil || m.Type != "factory.config.get" || m.ID != "factory-1" || m.Project != "/app" || m.Config != nil {
		t.Fatalf("%+v %v", m, err)
	}
	m, err = DecodeClient(factoryGolden(t, "client", "factory_config_save.json"))
	want := FactoryConfig{Enabled: true, Spec: true, Reviewer: true, Lead: "claude-opus", Workers: "claude-sonnet", Review: "codex", Slots: 2, Fix: 1, Checks: []string{"cargo test", "cargo clippy --all-targets"}}
	if err != nil || m.Project != "/app" || m.Config == nil || !reflect.DeepEqual(*m.Config, want) {
		t.Fatalf("%+v %v", m.Config, err)
	}
}

func TestAFactoryConfigOffTheContractIsMalformed(t *testing.T) {
	full := `"enabled":false,"spec":true,"reviewer":true,"lead":"claude-opus","workers":"claude-sonnet","review":"codex","slots":0,"fix":2,"checks":[]`
	save := func(config string) string {
		return `{"type":"factory.config.save","id":"1","project":"/app","config":` + config + `}`
	}
	for _, raw := range []string{
		`{"type":"factory.config.get","id":"1"}`,
		`{"type":"factory.config.get","id":"1","project":""}`,
		`{"type":"factory.config.save","id":"1","project":"/app"}`,
		`{"type":"factory.config.save","id":"1","config":{` + full + `}}`,
		save(`"on"`),
		save(`{` + strings.Replace(full, `,"checks":[]`, ``, 1) + `}`),
		save(`{` + full + `,"station":"plan"}`),
		save(`{` + strings.Replace(full, `"slots":0`, `"slots":1.5`, 1) + `}`),
		save(`{` + strings.Replace(full, `"checks":[]`, `"checks":null`, 1) + `}`),
		save(`{` + strings.Replace(full, `"enabled":false`, `"Enabled":false`, 1) + `}`),
	} {
		if _, err := DecodeClient([]byte(raw)); err != ErrMalformed {
			t.Errorf("%s: got %v, want ErrMalformed", raw, err)
		}
	}
}

func TestAConfigOutOfRangeStillDecodesSoItsRefusalCanSayWhy(t *testing.T) {
	m, err := DecodeClient([]byte(`{"type":"factory.config.save","id":"1","project":"/app","config":{"enabled":false,"spec":true,"reviewer":true,"lead":"claude-opus","workers":"claude-sonnet","review":"codex","slots":99,"fix":2,"checks":[]}}`))
	if err != nil || m.Config == nil || m.Config.Valid() {
		t.Fatalf("%+v %v", m.Config, err)
	}
}

func TestTheServerOffersTheFactoryCap(t *testing.T) {
	_, caps, code := Negotiate(Range{Min: 3, Max: 3}, []string{CapFactory}, ServerCaps)
	if code != "" || !slices.Equal(caps, []string{CapFactory}) {
		t.Fatalf("caps %v, code %q", caps, code)
	}
}

func TestAFactoryConfigReplyAlwaysListsItsCommands(t *testing.T) {
	got, _ := json.Marshal(NewFactoryConfig("g", "/app", FactoryConfig{}, ""))
	want := `{"type":"factory.config","id":"g","project":"/app","config":{"enabled":false,"spec":false,"reviewer":false,"lead":"","workers":"","review":"","slots":0,"fix":0,"checks":[]}}`
	if string(got) != want {
		t.Fatalf("got  %s\nwant %s", got, want)
	}
	if CodeFactoryOff != "factory_off" || CodeFactoryUnavailable != "factory_unavailable" || CodeInvalidConfig != "invalid_config" {
		t.Fatal("a Factory error code changed its wire name")
	}
}

func TestFactoryTicketDecodesWithItsTicketID(t *testing.T) {
	m, err := DecodeClient(factoryGolden(t, "client", "factory_ticket.json"))
	if err != nil || m.Type != "factory.ticket" || m.ID != "factory-4" || m.TicketID != "t1" {
		t.Fatalf("got %+v, %v", m, err)
	}
	for _, raw := range []string{
		`{"type":"factory.ticket","id":"1"}`,
		`{"type":"factory.ticket","id":"1","ticketId":""}`,
		`{"type":"factory.ticket","id":"1","ticketId":7}`,
	} {
		if _, err := DecodeClient([]byte(raw)); err == nil {
			t.Errorf("accepted %s", raw)
		}
	}
}

func TestATicketWithNoActivitySendsAnEmptyList(t *testing.T) {
	got, _ := json.Marshal(NewFactoryTicket("f1", FactoryTicket{TicketID: "t1", Activity: []FactoryEvent{}}))
	if string(got) != `{"type":"factory.ticket","id":"f1","ticket":{"ticketId":"t1","activity":[]}}` {
		t.Fatalf("%s", got)
	}
	got, _ = json.Marshal(NewFactoryUpdate("t1"))
	if string(got) != `{"type":"factory.update","ticketId":"t1"}` {
		t.Fatalf("%s", got)
	}
}

func TestTheChecksOfAFactoryTicketAreWhatTheServerWrites(t *testing.T) {
	var frame struct {
		Type   string        `json:"type"`
		ID     string        `json:"id"`
		Ticket FactoryTicket `json:"ticket"`
	}
	dec := json.NewDecoder(bytes.NewReader(factoryGolden(t, "server", "factory_ticket_checks.json")))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&frame); err != nil {
		t.Fatal(err)
	}
	if len(frame.Ticket.Checks) != 2 || frame.Ticket.Checks[1].Exit != 101 || frame.Ticket.Checks[1].Attempt != 1 || frame.Ticket.NoChecks {
		t.Fatalf("got %+v", frame.Ticket)
	}
}
