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
	CodeInvalidProfile     = "invalid_profile"
	CodeUnknownProfile     = "unknown_profile"
	CodeProfileInUse       = "profile_in_use"
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
	Kind    string `json:"kind"`  // moved | gate | approved | changes | asked | allowed | denied | autoAllowed | handback | check | fix | hint | finding | skill | merged | failed
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

// FactoryTicketReply answers factory.ticket.
type FactoryTicketReply struct {
	Type   string        `json:"type"`
	ID     string        `json:"id"`
	Ticket FactoryTicket `json:"ticket"`
}

func NewFactoryTicket(id string, t FactoryTicket) FactoryTicketReply {
	return FactoryTicketReply{Type: "factory.ticket", ID: id, Ticket: t}
}

// FactoryUpdate says a Ticket's detail changed. The desktop asks again with factory.ticket.
type FactoryUpdate struct {
	Type     string `json:"type"`
	TicketID string `json:"ticketId"`
}

func NewFactoryUpdate(ticketID string) FactoryUpdate {
	return FactoryUpdate{Type: "factory.update", TicketID: ticketID}
}

const (
	// MaxProfileText bounds a profile's soul plus its instructions, in bytes.
	MaxProfileText = 64 << 10
	// MaxProfileBytes bounds a whole profile: soul, instructions and every skill file.
	MaxProfileBytes = 2 << 20
)

// ProfileInfo is a profile as a list shows it.
type ProfileInfo struct {
	ID       string `json:"id"`
	Name     string `json:"name"`
	Provider string `json:"provider"`
	Model    string `json:"model,omitempty"`
	Skills   int    `json:"skills"`
	BuiltIn  bool   `json:"builtIn,omitempty"`
}

// Profile is a whole profile: who the agent is, how it works, and its skills' files.
type Profile struct {
	ProfileInfo
	Soul         string        `json:"soul,omitempty"`
	Instructions string        `json:"instructions,omitempty"`
	Files        []ProfileFile `json:"files,omitempty"`
}

// ProfileFile is one file under skills/, by its path from the profile's folder. Text is base64 when Binary.
type ProfileFile struct {
	Path   string `json:"path"`
	Text   string `json:"text"`
	Binary bool   `json:"binary,omitempty"`
}

// FactoryProfiles answers factory.profile.list, .save and .delete, and with no id tells of a change.
type FactoryProfiles struct {
	Type     string        `json:"type"`
	ID       string        `json:"id,omitempty"`
	Profiles []ProfileInfo `json:"profiles"`
}

func NewFactoryProfiles(id string, list []ProfileInfo) FactoryProfiles {
	return FactoryProfiles{"factory.profiles", id, orEmpty(list)}
}

// FactoryProfile answers factory.profile.get.
type FactoryProfile struct {
	Type    string  `json:"type"`
	ID      string  `json:"id"`
	Profile Profile `json:"profile"`
}

func NewFactoryProfile(id string, p Profile) FactoryProfile {
	return FactoryProfile{"factory.profile", id, p}
}

// decodeProfile takes factory.profile.save's profile. skills and builtIn are
// pocketd's to say, so they may come and are ignored.
func decodeProfile(raw json.RawMessage, dst **Profile) bool {
	o, ok := strictObject(raw, "id", "name", "provider", "model", "skills", "builtIn", "soul", "instructions", "files")
	if !ok {
		return false
	}
	var p Profile
	var files []json.RawMessage
	f := fields(o)
	if !f.optionalString("id", &p.ID) || !f.get("name", &p.Name) || !f.get("provider", &p.Provider) || !f.optionalString("model", &p.Model) ||
		!f.optionalString("soul", &p.Soul) || !f.optionalString("instructions", &p.Instructions) || f.has("files") && !f.get("files", &files) {
		return false
	}
	for _, item := range files {
		o, ok := strictObject(item, "path", "text", "binary")
		var file ProfileFile
		if !ok || !fields(o).get("path", &file.Path) || !fields(o).get("text", &file.Text) || !fields(o).optionalBool("binary", &file.Binary) {
			return false
		}
		p.Files = append(p.Files, file)
	}
	*dst = &p
	return true
}

// Metrics is one project's numbers over the last Days days. Durations are seconds, and 0 means no data.
type Metrics struct {
	MergedPerDay float64 `json:"mergedPerDay"`
	StartToReady int64   `json:"startToReady"` // median
	WaitPlan     int64   `json:"waitPlan"`     // median
	WaitPR       int64   `json:"waitPr"`
	WaitAsk      int64   `json:"waitAsk"`
	FixRate      float64 `json:"fixRate"` // share of failed Tickets repaired within the cap
	FixMean      float64 `json:"fixMean"` // mean attempts
	Days         int     `json:"days"`
}

// FactoryMetrics answers factory.metrics.
type FactoryMetrics struct {
	Type    string  `json:"type"`
	ID      string  `json:"id"`
	Project string  `json:"project"`
	Metrics Metrics `json:"metrics"`
}

func NewFactoryMetrics(id, project string, m Metrics) FactoryMetrics {
	return FactoryMetrics{"factory.metrics", id, project, m}
}
