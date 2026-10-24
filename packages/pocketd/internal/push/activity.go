package push

import (
	"cmp"
	"encoding/json"
	"slices"
	"time"

	"pocketd/internal/proto"
)

// State is the Live Activity's content-state (MacActivity.ContentState on the phone).
// It carries counts, statuses and agent ids; titles stay on the phone.
type State struct {
	Working  int  `json:"working"`
	NeedsYou int  `json:"needsYou"`
	Unseen   int  `json:"unseen"`
	Top      *Top `json:"top,omitempty"`
}

// Top is the one session the activity shows.
type Top struct {
	Slot   string `json:"slot"`
	Status string `json:"status"`
	// SinceUnix is Unix seconds: ActivityKit decodes a Date from 2001.
	SinceUnix  float64 `json:"sinceUnix"`
	Verb       string  `json:"verb,omitempty"`
	Subject    string  `json:"subject,omitempty"`
	Answerable bool    `json:"answerable"`
	Request    string  `json:"request,omitempty"`
}

const subjectMax = 60

var verbs = map[string]string{"run": "Run", "edit": "Edit", "write": "Write", "read": "Read", "search": "Search", "start": "Start", "use": "Use"}

// status is the session's status as the phone shows it: an agent not attached shows none.
func status(a proto.AgentSummary) string {
	switch {
	case !a.Attached:
		return ""
	case a.Status == "done" && a.Failed:
		return Failed
	case a.Status == "needsYou", a.Status == "working", a.Status == "done":
		return a.Status
	}
	return ""
}

// Build is the state of agents and their open asks. Top is the oldest Needs
// you, else the longest working, else the oldest Failed, else the oldest
// Done. With details the ask's verb and subject show; never otherwise.
func Build(agents []proto.AgentSummary, asks []proto.PermissionRequest, details bool) State {
	var st State
	rank := map[string]int{NeedsYou: 0, "working": 1, Failed: 2, Done: 3}
	var top *proto.AgentSummary
	for i, a := range agents {
		s := status(a)
		switch s {
		case "":
			continue
		case NeedsYou:
			st.NeedsYou++
		case "working":
			st.Working++
		default:
			st.Unseen++
		}
		if top == nil || cmp.Or(cmp.Compare(rank[s], rank[status(*top)]), cmp.Compare(a.UpdatedAt, top.UpdatedAt), cmp.Compare(a.ID, top.ID)) < 0 {
			top = &agents[i]
		}
	}
	if top == nil {
		return st
	}
	st.Top = &Top{Slot: top.ID, Status: status(*top), SinceUnix: float64(top.UpdatedAt) / 1000}
	if st.Top.Status != NeedsYou {
		return st
	}
	i := slices.IndexFunc(asks, func(r proto.PermissionRequest) bool { return r.AgentID == top.ID })
	if i < 0 {
		return st
	}
	ask := asks[i]
	if details {
		verb, what := Parts(ask)
		st.Top.Verb, st.Top.Subject = verbs[verb], cutTo(what, subjectMax)
	}
	if Answerable(ask) {
		st.Top.Answerable, st.Top.Request = true, ask.RequestID
	}
	return st
}

func cutTo(s string, n int) string {
	r := []rune(s)
	if len(r) <= n {
		return s
	}
	return string(r[:n-1]) + "…"
}

// Relevance ranks this Mac's activity against others' for the Dynamic Island.
func Relevance(s State) int {
	switch {
	case s.NeedsYou > 0:
		return 100
	case s.Top != nil && s.Top.Status == Failed:
		return 75
	case s.Working > 0:
		return 50
	}
	return 10
}

// Busy is anything working or waiting on the user, which keeps an activity going.
func (s State) Busy() bool { return s.Working > 0 || s.NeedsYou > 0 }

// failures counts the sessions that failed and wait to be seen.
func failures(agents []proto.AgentSummary) int {
	n := 0
	for _, a := range agents {
		if status(a) == Failed {
			n++
		}
	}
	return n
}

// Badge counts the sessions wanting a look: Needs you, Failed and Done.
func Badge(agents []proto.AgentSummary) int {
	n := 0
	for _, a := range agents {
		if s := status(a); s == NeedsYou || s == Failed || s == Done {
			n++
		}
	}
	return n
}

// UpdatePayload is an "update" (end false) or "end" (end true) of the activity.
func UpdatePayload(s State, now time.Time, stale, dismiss time.Duration, end bool) json.RawMessage {
	aps := map[string]any{"timestamp": now.Unix(), "event": "update", "content-state": s, "relevance-score": Relevance(s),
		"stale-date": now.Add(stale).Unix()}
	if end {
		aps = map[string]any{"timestamp": now.Unix(), "event": "end", "content-state": s, "dismissal-date": now.Add(dismiss).Unix()}
	}
	raw, _ := json.Marshal(map[string]any{"aps": aps})
	return raw
}

// StartPayload starts the activity by push-to-start. Apple requires an alert;
// it has no sound, since the sealed alert of the same Needs you carries it.
func StartPayload(s State, mac, macName string, now time.Time, stale time.Duration) json.RawMessage {
	raw, _ := json.Marshal(map[string]any{"aps": map[string]any{
		"timestamp": now.Unix(), "event": "start", "input-push-token": 1,
		"attributes-type": "MacActivity", "attributes": map[string]string{"mac": mac, "macName": macName},
		"content-state": s, "alert": map[string]string{"title": "Anywhere", "body": generic[NeedsYou]},
		"relevance-score": Relevance(s), "stale-date": now.Add(stale).Unix(),
	}})
	return raw
}
