package proto

import (
	"encoding/json"
	"strings"
	"unicode/utf8"
)

// CapFactory: factory.* verbs, and a Ticket's station and gate.
const CapFactory = "factory.v1"

const (
	// CodeFactoryOff: this pocketd serves no Factory to the connection.
	CodeFactoryOff = "factory_off"
	// CodeFactoryUnavailable: the Factory can't be turned on for the project; the message says why.
	CodeFactoryUnavailable = "factory_unavailable"
	CodeInvalidConfig      = "invalid_config"
)

// A Factory ticket's Station.
const (
	StationSpec      = "spec"
	StationPlan      = "plan"
	StationImplement = "implement"
	StationChecks    = "checks"
	StationPR        = "pr"
	StationMerged    = "merged"
)

// What a Factory ticket's Gate says it waits on the user for.
const (
	GatePlan = "plan"
	GatePR   = "pr"
	GateAsk  = "ask"
)

const (
	// MaxFix is the most auto-fix attempts a project can allow a ticket.
	MaxFix = 2
	// MaxFactoryChecks is how many check commands a project can list, and
	// MaxFactoryCheck how long one can be, in runes.
	MaxFactoryChecks = 16
	MaxFactoryCheck  = 400
)

// FactoryConfig is one project's Factory settings.
type FactoryConfig struct {
	Enabled  bool `json:"enabled"`
	Spec     bool `json:"spec"`
	Reviewer bool `json:"reviewer"`
	// Lead, Workers and Review are profile ids. Workers is the default for each task.
	Lead    string `json:"lead"`
	Workers string `json:"workers"`
	Review  string `json:"review"`
	// Slots is the project's own slot limit, 1 to MaxBoardLimit; 0 is the Board's.
	Slots int `json:"slots"`
	// Fix is how many auto-fix attempts a ticket gets, 0 to MaxFix.
	Fix int `json:"fix"`
	// Checks are the local commands the Checks station runs, in order.
	Checks []string `json:"checks"`
}

func DefaultFactoryConfig() FactoryConfig {
	return FactoryConfig{Spec: true, Reviewer: true, Lead: "claude-opus", Workers: "claude-sonnet", Review: "codex", Fix: MaxFix, Checks: []string{}}
}

// Valid is what a client's factory.config.save must satisfy. Whether its
// profiles exist is the profile store's to say.
func (c FactoryConfig) Valid() bool {
	if c.Slots < 0 || c.Slots > MaxBoardLimit || c.Fix < 0 || c.Fix > MaxFix || len(c.Checks) > MaxFactoryChecks {
		return false
	}
	for _, command := range c.Checks {
		if strings.TrimSpace(command) == "" || utf8.RuneCountInString(command) > MaxFactoryCheck {
			return false
		}
	}
	return true
}

// decodeFactoryConfig takes a config only whole: a key left out would switch
// its setting off without the user having chosen that.
func decodeFactoryConfig(raw json.RawMessage, dst **FactoryConfig) bool {
	var c FactoryConfig
	if !strictInto(raw, &c, []string{"enabled", "spec", "reviewer", "lead", "workers", "review", "slots", "fix", "checks"}) {
		return false
	}
	*dst = &c
	return true
}

// FactoryConfigReply answers factory.config.get and factory.config.save.
type FactoryConfigReply struct {
	Type    string        `json:"type"`
	ID      string        `json:"id"`
	Project string        `json:"project"`
	Config  FactoryConfig `json:"config"`
	// Problem says why the Factory can't be turned on for the project.
	Problem string `json:"problem,omitempty"`
}

func NewFactoryConfig(id, project string, config FactoryConfig, problem string) FactoryConfigReply {
	config.Checks = orEmpty(config.Checks)
	return FactoryConfigReply{"factory.config", id, project, config, problem}
}

// FactoryTicket is what factory.ticket answers with: one Ticket's spec and what happened to it.
type FactoryTicket struct {
	TicketID string         `json:"ticketId"`
	Spec     string         `json:"spec,omitempty"`
	Activity []FactoryEvent `json:"activity"`
	Checks   []FactoryCheck `json:"checks,omitempty"`
	Findings []Finding      `json:"findings,omitempty"`
	NoChecks bool           `json:"noChecks,omitempty"`
}

// FactoryEvent is one line of a Ticket's activity.
type FactoryEvent struct {
	At      int64  `json:"at"` // Unix ms
	Station string `json:"station"`
	Actor   string `json:"actor"` // you | lead | worker:<task> | reviewer | anywhere
	Kind    string `json:"kind"`  // moved | approved | changes | asked | allowed | denied | autoAllowed | handback | check | fix | hint | finding | skill | merged | failed
	Text    string `json:"text,omitempty"`
}

type FactoryCheck struct {
	Command string `json:"command"`
	Exit    int    `json:"exit"`
	Output  string `json:"output"`
	Attempt int    `json:"attempt"`
	At      int64  `json:"at"`
}

type Finding struct {
	Severity string `json:"severity"` // critical | minor | nit
	File     string `json:"file,omitempty"`
	Line     int    `json:"line,omitempty"`
	Message  string `json:"message"`
}
