package proto

import (
	"encoding/json"
	"slices"
	"strings"
	"unicode/utf8"
)

const CapAutomations = "automations.v1"

const (
	CodeInvalidAutomation = "invalid_automation"
	CodeUnknownAutomation = "unknown_automation"
	CodeUnknownProject    = "unknown_project"
	CodeAutomationBusy    = "automation_busy"
)

const (
	MinEveryMin = 5
	MaxEveryMin = 10080
	MaxName     = 80
)

type Schedule struct {
	Kind     string `json:"kind"`               // "days" | "interval"
	Days     []int  `json:"days,omitempty"`     // days only: 0=Sun..6=Sat, non-empty, unique
	Time     string `json:"time,omitempty"`     // days only: "HH:MM" 24h, daemon local time
	EveryMin int    `json:"everyMin,omitempty"` // interval only: 5..10080
}

// Valid: a days schedule has days and a time but no everyMin; an interval has only everyMin.
func (s Schedule) Valid() bool {
	switch s.Kind {
	case "days":
		if len(s.Days) == 0 || s.EveryMin != 0 {
			return false
		}
		for i, d := range s.Days {
			if d < 0 || d > 6 || slices.Contains(s.Days[:i], d) {
				return false
			}
		}
		_, _, ok := ParseClock(s.Time)
		return ok
	case "interval":
		return len(s.Days) == 0 && s.Time == "" && s.EveryMin >= MinEveryMin && s.EveryMin <= MaxEveryMin
	}
	return false
}

// ParseClock reads a 24h "HH:MM".
func ParseClock(s string) (hour, minute int, ok bool) {
	if len(s) != 5 || s[2] != ':' {
		return 0, 0, false
	}
	for _, i := range []int{0, 1, 3, 4} {
		if s[i] < '0' || s[i] > '9' {
			return 0, 0, false
		}
	}
	hour, minute = int(s[0]-'0')*10+int(s[1]-'0'), int(s[3]-'0')*10+int(s[4]-'0')
	return hour, minute, hour < 24 && minute < 60
}

type Automation struct {
	ID        string   `json:"id"`
	Name      string   `json:"name"`
	Prompt    string   `json:"prompt"`
	Provider  string   `json:"provider"`
	Folder    string   `json:"folder"`
	Schedule  Schedule `json:"schedule"`
	Enabled   bool     `json:"enabled"`
	NextRunAt int64    `json:"nextRunAt,omitempty"` // unix ms; omitted when disabled
}

// Valid is what a client's automation.save must satisfy.
func (a Automation) Valid() bool {
	name := utf8.RuneCountInString(strings.TrimSpace(a.Name))
	return name >= 1 && name <= MaxName && a.Prompt != "" && len(a.Prompt) <= MaxPrompt &&
		(a.Provider == "claude" || a.Provider == "codex") && a.Folder != "" && a.Schedule.Valid()
}

type Run struct {
	ID           string `json:"id"`
	AutomationID string `json:"automationId"`
	Status       string `json:"status"`  // pending running waiting succeeded failed skipped cancelled
	Trigger      string `json:"trigger"` // "schedule" | "manual"
	Why          string `json:"why,omitempty"`
	Summary      string `json:"summary,omitempty"`
	StartedAt    int64  `json:"startedAt"` // unix ms; for skipped, the due time
	FinishedAt   int64  `json:"finishedAt,omitempty"`
	AgentID      string `json:"agentId,omitempty"`
	TerminalID   string `json:"terminalId,omitempty"`
}

// Automations is the full snapshot, runs newest first. It is never a delta.
type Automations struct {
	Type        string       `json:"type"`
	Automations []Automation `json:"automations"`
	Runs        []Run        `json:"runs"`
}

func NewAutomations(autos []Automation, runs []Run) Automations {
	if autos == nil {
		autos = []Automation{}
	}
	if runs == nil {
		runs = []Run{}
	}
	return Automations{"automations", autos, runs}
}

func decodeAutomation(raw json.RawMessage, dst **Automation) bool {
	f, ok := strictObject(raw, "id", "name", "prompt", "provider", "folder", "schedule", "enabled")
	if !ok {
		return false
	}
	for _, k := range []string{"name", "prompt", "provider", "folder", "schedule", "enabled"} {
		if _, has := f[k]; !has {
			return false
		}
	}
	if _, ok := strictObject(f["schedule"], "kind", "days", "time", "everyMin"); !ok {
		return false
	}
	var a Automation
	if json.Unmarshal(raw, &a) != nil || !a.Valid() {
		return false
	}
	*dst = &a
	return true
}
