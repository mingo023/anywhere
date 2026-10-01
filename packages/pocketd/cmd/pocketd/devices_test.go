package main

import (
	"bytes"
	"testing"
	"time"

	"pocketd/internal/devices"
)

func TestTheDevicesTableShowsShortIDsAndAges(t *testing.T) {
	now := time.UnixMilli(1_000_000_000_000)
	var out bytes.Buffer
	printDevices(&out, []devices.Device{
		{ID: "legacy", Name: "Shared token (legacy)", LastSeenAt: now.Add(-2 * time.Minute).UnixMilli()},
		{ID: "3fa9c1d2e5f60718293a4b5c6d7e8f90", Name: "iPhone", Platform: "ios", LastSeenAt: now.UnixMilli()},
		{ID: "77aa00bb11cc22dd33ee44ff55006611", Name: "Pixel", Platform: "android"},
	}, now)
	want := "" +
		"ID        NAME                   PLATFORM  LAST SEEN\n" +
		"legacy    Shared token (legacy)  -         2 min ago\n" +
		"3fa9c1d2  iPhone                 ios       now\n" +
		"77aa00bb  Pixel                  android   never\n"
	if out.String() != want {
		t.Fatalf("got\n%s\nwant\n%s", out.String(), want)
	}
}

func TestLastSeenReadsAsAnAge(t *testing.T) {
	now := time.UnixMilli(1_000_000_000_000)
	for d, want := range map[time.Duration]string{
		30 * time.Second: "now",
		59 * time.Minute: "59 min ago",
		5 * time.Hour:    "5 h ago",
		49 * time.Hour:   "2 days ago",
	} {
		if got := ago(now.Add(-d).UnixMilli(), now); got != want {
			t.Errorf("%v: got %q, want %q", d, got, want)
		}
	}
}

func TestTheLegacyRowShowsWhenItsGraceEnds(t *testing.T) {
	end := time.Date(2026, 10, 8, 12, 0, 0, 0, time.Local).UnixMilli()
	for _, c := range []struct {
		d    devices.Device
		want string
	}{
		{devices.Device{ID: devices.LegacyID, Legacy: true, GraceEndsAt: end}, " · grace ends 2026-10-08"},
		{devices.Device{ID: devices.LegacyID, Legacy: true}, ""},
		{devices.Device{ID: "d1", GraceEndsAt: end}, ""},
	} {
		if got := graceNote(c.d); got != c.want {
			t.Errorf("%+v: %q, want %q", c.d, got, c.want)
		}
	}
}
