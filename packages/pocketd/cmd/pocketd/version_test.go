package main

import (
	"runtime/debug"
	"testing"
)

func TestVersionIsTheShortRevisionMarkedDirty(t *testing.T) {
	rev := debug.BuildSetting{Key: "vcs.revision", Value: "5091a01c0ffee5091a01c0ffee"}
	for _, c := range []struct {
		settings []debug.BuildSetting
		want     string
	}{
		{nil, "dev"},
		{[]debug.BuildSetting{rev}, "5091a01c0ffe"},
		{[]debug.BuildSetting{rev, {Key: "vcs.modified", Value: "true"}}, "5091a01c0ffe-dirty"},
	} {
		if got := fromSettings(c.settings); got != c.want {
			t.Errorf("%v: %q, want %q", c.settings, got, c.want)
		}
	}
}
