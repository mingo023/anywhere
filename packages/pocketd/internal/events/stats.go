package events

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"maps"
	"slices"
	"strings"
	"time"
)

type Percentiles struct {
	N   int   `json:"n"`
	P50 int64 `json:"p50Ms"`
	P90 int64 `json:"p90Ms"`
}

type Ratio struct {
	N  int `json:"n"`
	Of int `json:"of"`
}

// Report is PRD §9 over one window of events.
type Report struct {
	Since              string                 `json:"since"`
	Events             int                    `json:"events"`
	Skipped            int                    `json:"skipped"`
	Answer             map[string]Percentiles `json:"answer"`   // Needs you → answer, by principal
	Seen               Percentiles            `json:"seen"`     // Done → Seen
	Restores           Ratio                  `json:"restores"` // ok within 30 s
	Creates            int                    `json:"creates"`
	CreateFailures     map[string]int         `json:"createFailures"` // "origin code"
	AckErrors          map[string]Ratio       `json:"ackErrors"`      // by provider, of prompts
	Pushes             Ratio                  `json:"pushes"`         // acted on, of sent
	Refusals           map[string]int         `json:"refusals"`       // "surface code"
	UnreachableMinutes float64                `json:"unreachableMinutes"`
}

// Stats reads events.jsonl lines oldest first. Lines it can't parse are counted, not fatal.
func Stats(r io.Reader, since time.Time) Report {
	rep := Report{Since: since.UTC().Format(Layout), CreateFailures: map[string]int{}, AckErrors: map[string]Ratio{}, Refusals: map[string]int{}}
	answers := map[string][]time.Duration{}
	var seen []time.Duration
	needs, done := map[string]time.Time{}, map[string]time.Time{}
	working := map[string]bool{}
	var beat time.Time
	busyAtBeat := false
	var unreachable time.Duration
	sc := bufio.NewScanner(r)
	sc.Buffer(make([]byte, 64<<10), 1<<20)
	for sc.Scan() {
		var e Event
		if json.Unmarshal(sc.Bytes(), &e) != nil || e.Kind == "" {
			rep.Skipped++
			continue
		}
		ts, err := time.Parse(Layout, e.TS)
		if err != nil {
			rep.Skipped++
			continue
		}
		if ts.Before(since) {
			continue
		}
		rep.Events++
		switch e.Kind {
		case "start":
			if !beat.IsZero() && busyAtBeat {
				unreachable += ts.Sub(beat)
			}
			beat, busyAtBeat = time.Time{}, false
			clear(working)
			clear(needs)
			clear(done)
		case "stop":
			beat, busyAtBeat = ts, len(working) > 0
		case "tick":
			if gap := ts.Sub(beat); !beat.IsZero() && busyAtBeat && gap > 90*time.Second {
				unreachable += gap - time.Minute
			}
			beat, busyAtBeat = ts, len(working) > 0
		case "status":
			if t0, ok := needs[e.Agent]; ok && e.From == "needsYou" {
				answers["other"] = append(answers["other"], ts.Sub(t0))
				delete(needs, e.Agent)
			}
			if t0, ok := done[e.Agent]; ok && e.From == "done" {
				if e.To == "idle" {
					seen = append(seen, ts.Sub(t0))
				}
				delete(done, e.Agent)
			}
			switch e.To {
			case "needsYou":
				needs[e.Agent] = ts
			case "done":
				done[e.Agent] = ts
			}
			delete(working, e.Agent)
			if e.To == "working" {
				working[e.Agent] = true
			}
		case "answer":
			if t0, ok := needs[e.Agent]; ok {
				answers[e.Principal] = append(answers[e.Principal], ts.Sub(t0))
				delete(needs, e.Agent)
			}
		case "seen":
			if t0, ok := done[e.Agent]; ok {
				seen = append(seen, ts.Sub(t0))
				delete(done, e.Agent)
			}
		case "restore":
			rep.Restores.Of++
			if e.OK != nil && *e.OK && e.MS <= 30_000 {
				rep.Restores.N++
			}
		case "create":
			rep.Creates++
			if e.OK != nil && !*e.OK {
				rep.CreateFailures[e.Origin+" "+e.Code]++
			}
		case "prompt":
			r := rep.AckErrors[e.Provider]
			r.Of++
			if e.Ack != "ok" {
				r.N++
			}
			rep.AckErrors[e.Provider] = r
		case "push":
			rep.Pushes.Of++
			if e.Acted != nil && *e.Acted {
				rep.Pushes.N++
			}
		case "refusal":
			rep.Refusals[e.Surface+" "+e.Code]++
		}
	}
	rep.Answer = map[string]Percentiles{}
	for p, ds := range answers {
		rep.Answer[p] = percentiles(ds)
	}
	rep.Seen = percentiles(seen)
	rep.UnreachableMinutes = unreachable.Minutes()
	return rep
}

// percentiles uses nearest rank.
func percentiles(ds []time.Duration) Percentiles {
	if len(ds) == 0 {
		return Percentiles{}
	}
	slices.Sort(ds)
	rank := func(p int) int64 { return ds[(p*len(ds)+99)/100-1].Milliseconds() }
	return Percentiles{N: len(ds), P50: rank(50), P90: rank(90)}
}

func (p Percentiles) text() string {
	if p.N == 0 {
		return "—"
	}
	ms := func(v int64) time.Duration { return (time.Duration(v) * time.Millisecond).Round(time.Second) }
	return fmt.Sprintf("p50 %s, p90 %s (n %d)", ms(p.P50), ms(p.P90), p.N)
}

func (r Ratio) text(what string) string {
	if r.Of == 0 {
		return "—"
	}
	return fmt.Sprintf("%d of %d %s (%d%%)", r.N, r.Of, what, r.N*100/r.Of)
}

func counts(m map[string]int) string {
	if len(m) == 0 {
		return "—"
	}
	var parts []string
	for _, k := range slices.Sorted(maps.Keys(m)) {
		parts = append(parts, fmt.Sprintf("%s %d", k, m[k]))
	}
	return strings.Join(parts, ", ")
}

func (r Report) Text() string {
	var b strings.Builder
	fmt.Fprintf(&b, "since %s: %d events", r.Since, r.Events)
	if r.Skipped > 0 {
		fmt.Fprintf(&b, ", %d unreadable lines skipped", r.Skipped)
	}
	b.WriteString("\n")
	row := func(name, value string) { fmt.Fprintf(&b, "%-22s%s\n", name, value) }
	if len(r.Answer) == 0 {
		row("Needs you → answer", "—")
	}
	for _, p := range slices.Sorted(maps.Keys(r.Answer)) {
		row("Needs you → answer", p+" "+r.Answer[p].text())
	}
	row("Done → Seen", r.Seen.text())
	row("Restore ≤ 30 s", r.Restores.text("restored"))
	failed := 0
	for _, n := range r.CreateFailures {
		failed += n
	}
	if r.Creates == 0 {
		row("Create failures", "—")
	} else {
		row("Create failures", fmt.Sprintf("%d of %d: %s", failed, r.Creates, counts(r.CreateFailures)))
	}
	if len(r.AckErrors) == 0 {
		row("Ack errors", "—")
	}
	for _, p := range slices.Sorted(maps.Keys(r.AckErrors)) {
		a := r.AckErrors[p]
		row("Ack errors", fmt.Sprintf("%s %.1f per 100 prompts (%d of %d)", p, float64(a.N)*100/float64(a.Of), a.N, a.Of))
	}
	row("Push precision", r.Pushes.text("acted on"))
	row("Refusals", counts(r.Refusals))
	row("Timeline use", "n/a")
	row("Unreachable", fmt.Sprintf("%.1f min while an agent was working", r.UnreachableMinutes))
	return b.String()
}
