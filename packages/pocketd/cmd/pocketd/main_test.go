package main

import (
	"path/filepath"
	"runtime"
	"testing"
)

func TestTheAnywhereCommandIsKnownWhereverItIsInstalled(t *testing.T) {
	argv0s := []string{"anywhere", filepath.FromSlash("/usr/local/bin/anywhere")}
	if runtime.GOOS == "windows" {
		argv0s = append(argv0s, `C:\Program Files\Anywhere\anywhere.exe`, "Anywhere.EXE")
	}
	for _, argv0 := range argv0s {
		if got := command(argv0); got != "anywhere" {
			t.Errorf("%s runs as %q", argv0, got)
		}
	}
	if got := command(filepath.FromSlash("/usr/local/bin/pocketd")); got != "pocketd" {
		t.Errorf("pocketd runs as %q", got)
	}
}
