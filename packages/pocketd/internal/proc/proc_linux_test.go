package proc

import (
	"slices"
	"testing"
)

func TestParseStatReadsPastAParenthesisedName(t *testing.T) {
	s, err := parseStat([]byte("42 (a) (b c) S 7 42 40 34816 42 4194304"))
	if err != nil || s != (stat{comm: "a) (b c", state: 'S', ppid: 7, pgrp: 42, session: 40}) {
		t.Fatalf("parseStat = %+v, %v", s, err)
	}
}

func TestSplitDropsTitleFill(t *testing.T) {
	if got := split([]byte("claude\x00-p\x00\x00\x00")); !slices.Equal(got, []string{"claude", "-p"}) {
		t.Fatalf("split = %q", got)
	}
	if got := split(nil); got != nil {
		t.Fatalf("split(nil) = %q", got)
	}
}
