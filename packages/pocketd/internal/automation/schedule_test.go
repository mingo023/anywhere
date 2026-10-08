package automation

import (
	"testing"
	"time"

	"pocketd/internal/proto"
)

var zone = time.FixedZone("test", 2*3600)

func at(day, hour, minute int) time.Time {
	return time.Date(2026, time.October, day, hour, minute, 0, 0, zone)
}

func TestWeekdaysAtNineSkipTheWeekend(t *testing.T) {
	s := proto.Schedule{Kind: "days", Days: []int{1, 2, 3, 4, 5}, Time: "09:00"}
	// 2026-10-09 is a Friday.
	for from, want := range map[time.Time]time.Time{
		at(9, 8, 59):  at(9, 9, 0),
		at(9, 9, 0):   at(12, 9, 0),
		at(9, 17, 30): at(12, 9, 0),
		at(10, 12, 0): at(12, 9, 0),
		at(11, 0, 0):  at(12, 9, 0),
		at(12, 9, 1):  at(13, 9, 0),
	} {
		if got := nextAfter(s, from); !got.Equal(want) {
			t.Errorf("from %v: got %v, want %v", from, got, want)
		}
	}
}

func TestADaysScheduleOnOneWeekdayWaitsAWeekWhenItJustFired(t *testing.T) {
	s := proto.Schedule{Kind: "days", Days: []int{5}, Time: "09:00"}
	if got, want := nextAfter(s, at(9, 9, 0)), at(16, 9, 0); !got.Equal(want) {
		t.Fatalf("got %v, want %v", got, want)
	}
}

func TestAnIntervalCountsFromTheGivenTime(t *testing.T) {
	s := proto.Schedule{Kind: "interval", EveryMin: 90}
	if got, want := nextAfter(s, at(9, 8, 15)), at(9, 9, 45); !got.Equal(want) {
		t.Fatalf("got %v, want %v", got, want)
	}
}

func TestNextIsStrictlyAfterFrom(t *testing.T) {
	for _, s := range []proto.Schedule{
		{Kind: "days", Days: []int{0, 1, 2, 3, 4, 5, 6}, Time: "09:00"},
		{Kind: "interval", EveryMin: proto.MinEveryMin},
	} {
		from := at(9, 9, 0)
		if got := nextAfter(s, from); !got.After(from) {
			t.Errorf("%+v: %v is not after %v", s, got, from)
		}
	}
}

func TestInvalidSchedulesAreRefused(t *testing.T) {
	for _, s := range []proto.Schedule{
		{},
		{Kind: "days", Time: "09:00"},
		{Kind: "days", Days: []int{1}, Time: "25:00"},
		{Kind: "interval", EveryMin: 1},
	} {
		if got := nextAfter(s, at(9, 9, 0)); !got.IsZero() {
			t.Errorf("%+v: got %v", s, got)
		}
	}
}

func TestARunIsLateOnlyBeyondTheGraceWindow(t *testing.T) {
	due := at(9, 9, 0)
	if late(due, due.Add(Grace)) || !late(due, due.Add(Grace+time.Second)) {
		t.Fatal("grace boundary")
	}
}

func firesFrom(s proto.Schedule, from time.Time, days int) []time.Time {
	var fires []time.Time
	until := from.AddDate(0, 0, days)
	for at := nextAfter(s, from); at.Before(until); at = nextAfter(s, at) {
		fires = append(fires, at)
	}
	return fires
}

func TestAClockTimeDaysFireOncePerDayAcrossDaylightSaving(t *testing.T) {
	ny, err := time.LoadLocation("America/New_York")
	if err != nil {
		t.Skip(err)
	}
	every := []int{0, 1, 2, 3, 4, 5, 6}
	for _, c := range []struct {
		name, clock string
		start       time.Time
		days        int
	}{
		{"a wall time that spring forward skips", "02:30", time.Date(2026, time.March, 6, 12, 0, 0, 0, ny), 5},
		{"a wall time that fall back repeats", "01:30", time.Date(2026, time.October, 30, 12, 0, 0, 0, ny), 5},
		{"a wall time outside the shift", "09:00", time.Date(2026, time.March, 6, 12, 0, 0, 0, ny), 5},
	} {
		fires := firesFrom(proto.Schedule{Kind: "days", Days: every, Time: c.clock}, c.start, c.days)
		if len(fires) != c.days {
			t.Fatalf("%s: %d fires: %v", c.name, len(fires), fires)
		}
		seen := map[string]bool{}
		for i, at := range fires {
			day := at.Format("2006-01-02")
			if seen[day] {
				t.Errorf("%s: %s fires twice: %v", c.name, day, fires)
			}
			seen[day] = true
			if i > 0 && !at.After(fires[i-1]) {
				t.Errorf("%s: %v does not follow %v", c.name, at, fires[i-1])
			}
		}
	}
}

func TestAWallTimeThatDoesNotExistFiresWhenTheClockJumpsOver(t *testing.T) {
	ny, err := time.LoadLocation("America/New_York")
	if err != nil {
		t.Skip(err)
	}
	s := proto.Schedule{Kind: "days", Days: []int{0}, Time: "02:30"}
	got := nextAfter(s, time.Date(2026, time.March, 7, 12, 0, 0, 0, ny))
	if want := time.Date(2026, time.March, 8, 3, 30, 0, 0, ny); !got.Equal(want) {
		t.Fatalf("got %v, want %v", got, want)
	}
}

func TestAWallTimeThatRepeatsFiresAtItsFirstPass(t *testing.T) {
	ny, err := time.LoadLocation("America/New_York")
	if err != nil {
		t.Skip(err)
	}
	s := nextAfter(proto.Schedule{Kind: "days", Days: []int{0}, Time: "01:30"}, time.Date(2026, time.October, 31, 12, 0, 0, 0, ny))
	if want := time.Date(2026, time.November, 1, 5, 30, 0, 0, time.UTC); !s.Equal(want) {
		t.Fatalf("got %v, want %v", s.UTC(), want)
	}
	if again := nextAfter(proto.Schedule{Kind: "days", Days: []int{0}, Time: "01:30"}, s); again.Before(time.Date(2026, time.November, 8, 0, 0, 0, 0, ny)) {
		t.Fatalf("fires again at %v", again)
	}
}
