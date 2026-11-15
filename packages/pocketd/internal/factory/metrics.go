package factory

import (
	"slices"
	"time"

	"pocketd/internal/proto"
)

// MetricsDays is how many days the metrics look back.
const MetricsDays = 14

type eventMatch func(proto.FactoryEvent) bool

func isKind(k string) eventMatch { return func(e proto.FactoryEvent) bool { return e.Kind == k } }

func isAt(k, station string) eventMatch {
	return func(e proto.FactoryEvent) bool { return e.Kind == k && e.Station == station }
}

func isBy(actor string, m eventMatch) eventMatch {
	return func(e proto.FactoryEvent) bool { return e.Actor == actor && m(e) }
}

func anyOf(ms ...eventMatch) eventMatch {
	return func(e proto.FactoryEvent) bool {
		return slices.ContainsFunc(ms, func(m eventMatch) bool { return m(e) })
	}
}

// nextOf is the index of the first event from i on that m matches, or -1.
func nextOf(log []proto.FactoryEvent, i int, m eventMatch) int {
	for ; i < len(log); i++ {
		if m(log[i]) {
			return i
		}
	}
	return -1
}

func secondsBetween(a, b proto.FactoryEvent) int64 { return (b.At - a.At) / 1000 }

// Compute is a project's numbers from the activity logs of its Tickets, over the MetricsDays before now.
func Compute(logs [][]proto.FactoryEvent, now time.Time) proto.Metrics {
	since, until := now.AddDate(0, 0, -MetricsDays).UnixMilli(), now.UnixMilli()
	in := func(e proto.FactoryEvent) bool { return e.At >= since && e.At <= until }
	var merged, decided, repaired, tries int
	var ready, plan, pr, ask []int64
	for _, log := range logs {
		if i := nextOf(log, 0, isKind("merged")); i >= 0 && in(log[i]) {
			merged++
		}
		if s := nextOf(log, 0, isKind("moved")); s >= 0 {
			if r := nextOf(log, s+1, isAt("gate", proto.StationPR)); r >= 0 && in(log[r]) {
				ready = append(ready, secondsBetween(log[s], log[r]))
			}
		}
		plan = append(plan, gateWaits(log, in, isAt("gate", proto.StationPlan), anyOf(isAt("approved", proto.StationPlan), isAt("changes", proto.StationPlan)), anyOf(isKind("moved"), isKind("failed")))...)
		pr = append(pr, gateWaits(log, in, isAt("gate", proto.StationPR), anyOf(isKind("merged"), isAt("changes", proto.StationPR), isAt("moved", proto.StationMerged)), anyOf(isKind("moved"), isKind("failed")))...)
		ask = append(ask, askWaits(log, in)...)
		if fixed, n, ok := firstFix(log, in); ok {
			decided, tries = decided+1, tries+n
			if fixed {
				repaired++
			}
		}
	}
	m := proto.Metrics{
		MergedPerDay: float64(merged) / MetricsDays,
		StartToReady: medianOf(ready),
		WaitPlan:     medianOf(plan),
		WaitPR:       medianOf(pr),
		WaitAsk:      medianOf(ask),
		Days:         MetricsDays,
	}
	if decided > 0 {
		m.FixRate, m.FixMean = float64(repaired)/float64(decided), float64(tries)/float64(decided)
	}
	return m
}

// gateWaits is how long each wait lasted, from an event that opens it to one that closes it. An event that drops
// it, or opens another, coming first ends the wait with no sample.
func gateWaits(log []proto.FactoryEvent, in func(proto.FactoryEvent) bool, opens, closes, drops eventMatch) []int64 {
	var out []int64
	for i, e := range log {
		if !opens(e) {
			continue
		}
		if j := nextOf(log, i+1, anyOf(closes, drops, opens)); j >= 0 && closes(log[j]) && in(log[j]) {
			out = append(out, secondsBetween(e, log[j]))
		}
	}
	return out
}

// askWaits pairs each answer of yours with the oldest Ask still open.
func askWaits(log []proto.FactoryEvent, in func(proto.FactoryEvent) bool) []int64 {
	var open []proto.FactoryEvent
	var out []int64
	answer := isBy("you", anyOf(isKind("allowed"), isKind("denied")))
	for _, e := range log {
		switch {
		case e.Kind == "asked":
			open = append(open, e)
		case answer(e) && len(open) > 0:
			if in(e) {
				out = append(out, secondsBetween(open[0], e))
			}
			open = open[1:]
		}
	}
	return out
}

// firstFix is how the first auto-fix of a Ticket ended: whether it repaired the Ticket, and after how many
// attempts. ok is false while it is under way, or when it ended before the window.
func firstFix(log []proto.FactoryEvent, in func(proto.FactoryEvent) bool) (repaired bool, attempts int, ok bool) {
	i := nextOf(log, 0, isAt("fix", proto.StationChecks))
	if i < 0 {
		return false, 0, false
	}
	attempts = 1
	for _, e := range log[i+1:] {
		switch {
		case e.Kind == "fix":
			attempts++
		case e.Kind == "moved" && e.Station == proto.StationPR:
			return true, attempts, in(e)
		case e.Kind == "failed", e.Kind == "hint":
			return false, attempts, in(e)
		}
	}
	return false, 0, false
}

func medianOf(samples []int64) int64 {
	if len(samples) == 0 {
		return 0
	}
	s := slices.Sorted(slices.Values(samples))
	mid := len(s) / 2
	if len(s)%2 == 1 {
		return s[mid]
	}
	return (s[mid-1] + s[mid]) / 2
}

// Metrics is project's numbers over the last MetricsDays, from the activity of its Tickets. A log that can't be
// read counts as empty.
func (e *Engine) Metrics(project string) proto.Metrics {
	tickets, _ := e.Board.Snapshot()
	var logs [][]proto.FactoryEvent
	for _, t := range tickets {
		if t.Folder != project {
			continue
		}
		if log, err := ReadActivity(e.Home, t.ID); err == nil && len(log) > 0 {
			logs = append(logs, log)
		}
	}
	return Compute(logs, e.now())
}
