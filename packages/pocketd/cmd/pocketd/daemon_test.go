package main

import (
	"strings"
	"testing"
)

func TestInstallRefusesWhatTheServiceCouldNotRun(t *testing.T) {
	ok := installCheck{Exe: "/Users/me/bin/pocketd", Temp: []string{"/private/var/folders/x/T", "/private/tmp", "/Users/me/Library/Caches/go-build"}, DefaultHome: "/Users/me/.coding-pocket"}
	for _, c := range []struct {
		name string
		edit func(*installCheck)
		want string
	}{
		{"a built binary", func(*installCheck) {}, ""},
		{"the default home spelled out", func(c *installCheck) {
			c.Home, c.Sock = "/Users/me/.coding-pocket/", "/Users/me/.coding-pocket/pocketd.sock"
		}, ""},
		{"go run", func(c *installCheck) { c.Exe = "/private/var/folders/x/T/go-build123/b001/exe/pocketd" }, "temporary build"},
		{"the go build cache", func(c *installCheck) { c.Exe = "/Users/me/Library/Caches/go-build/ab/pocketd" }, "temporary build"},
		{"a build in /tmp", func(c *installCheck) { c.Exe = "/private/tmp/pk-stats/pocketd" }, "temporary build"},
		{"inside Anywhere.app", func(c *installCheck) {
			c.Exe = "/Applications/Anywhere.app/Contents/Library/Helpers/AnywhereDaemon.app/Contents/MacOS/pocketd"
		}, "Login Item"},
		{"a scratch home", func(c *installCheck) { c.Home = "/tmp/pk1" }, "POCKET_HOME is set to /tmp/pk1"},
		{"a scratch socket", func(c *installCheck) { c.Sock = "/tmp/pk1/s.sock" }, "POCKETD_SOCK is set to /tmp/pk1/s.sock"},
		{"inside a Pocket Terminal", func(c *installCheck) { c.InTerminal = true }, "outside Pocket"},
	} {
		check := ok
		c.edit(&check)
		got := check.refusal()
		if c.want == "" && got != "" || !strings.Contains(got, c.want) {
			t.Errorf("%s: %q, want %q", c.name, got, c.want)
		}
	}
}
