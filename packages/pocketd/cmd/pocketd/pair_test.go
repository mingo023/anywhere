package main

import (
	"bytes"
	"testing"
	"time"

	"pocketd/internal/ops"
	"pocketd/internal/pairing"
)

func TestPairNeedsATerminal(t *testing.T) {
	var out, errOut bytes.Buffer
	code, err := pairCmd("/nonexistent", nil, false, &out, &errOut)
	if code != 2 || err != nil || errOut.String() != "Run pocketd pair in a terminal.\n" {
		t.Fatalf("%d %v %q", code, err, errOut.String())
	}
}

func TestTheOfferLineNamesHostCodeAndExpiry(t *testing.T) {
	expires := time.Date(2026, 9, 30, 14, 5, 0, 0, time.Local)
	got := offerLine(pairing.Offer{URL: "codingpocket://pair?v=1&h=100.77.122.82%3A4517&c=q3xY&n=Mac", Code: "q3xY", ExpiresAt: expires.UnixMilli()})
	if want := "Scan with the iPhone Camera app, or enter  100.77.122.82:4517  q3xY  Expires at 14:05.\n"; got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
}

func TestEveryWayAPairingEndsHasALine(t *testing.T) {
	for _, c := range []struct {
		m    ops.Msg
		want string
		code int
	}{
		{ops.Msg{Ev: "pair.ok", ID: "d1", Text: "iPhone"}, "Paired iPhone.", 0},
		{ops.Msg{Ev: "pair.expired", ErrorCode: "pair_expired"}, "Code expired. Run pocketd pair again.", 1},
		{ops.Msg{Ev: "pair.expired", ErrorCode: "pair_locked"}, "Too many wrong codes. Run pocketd pair again in a minute.", 1},
		{ops.Msg{Ev: "pair.expired", ErrorCode: "pair_failed"}, "Pairing failed. Run pocketd pair again.", 1},
	} {
		if got, code := pairEnd(c.m); got != c.want || code != c.code {
			t.Errorf("%+v: got %q %d", c.m, got, code)
		}
	}
}
