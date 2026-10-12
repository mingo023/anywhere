package main

import (
	"encoding/json"
	"runtime"
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

func TestVersionJSONNamesThisBuildsOSAndArch(t *testing.T) {
	var got map[string]string
	if err := json.Unmarshal(versionJSON(), &got); err != nil {
		t.Fatal(err)
	}
	if got["os"] != runtime.GOOS || got["arch"] != runtime.GOARCH || got["version"] == "" {
		t.Fatalf("got %v", got)
	}
}
