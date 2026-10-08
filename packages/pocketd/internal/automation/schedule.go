// Package automation keeps scheduled prompts and runs them as Sessions. All
// state is on disk, so a restart or an upgrade neither loses nor repeats a run.
package automation

import (
	"slices"
	"time"

	"pocketd/internal/proto"
)

// Grace is how late a due run may still start; a later one is skipped.
const Grace = 12 * time.Hour

// nextAfter is the first time s matches strictly after from, in from's zone;
// zero when s is invalid. A clock time the zone skips fires when the clock
// jumps over it, and one it repeats fires on its first pass, so each day fires
// once.
func nextAfter(s proto.Schedule, from time.Time) time.Time {
	if !s.Valid() {
		return time.Time{}
	}
	if s.Kind == "interval" {
		return from.Add(time.Duration(s.EveryMin) * time.Minute)
	}
	hour, minute, _ := proto.ParseClock(s.Time)
	y, m, d := from.Date()
	for i := 0; i <= 7; i++ {
		at := clockTime(y, m, d+i, hour, minute, from.Location())
		if at.After(from) && slices.Contains(s.Days, int(at.Weekday())) {
			return at
		}
	}
	return time.Time{}
}

// clockTime is hour:minute on a day. time.Date picks either side of a gap in
// the clock, so a wall time that came out different is moved past the gap.
func clockTime(y int, m time.Month, d, hour, minute int, loc *time.Location) time.Time {
	at := time.Date(y, m, d, hour, minute, 0, 0, loc)
	if at.Hour() != hour || at.Minute() != minute {
		_, before := at.Zone()
		_, after := at.Add(12 * time.Hour).Zone()
		at = at.Add(time.Duration(after-before) * time.Second)
	}
	return at
}

// late is whether due is further past than Grace allows.
func late(due, now time.Time) bool { return now.Sub(due) > Grace }
