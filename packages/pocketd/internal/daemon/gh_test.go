package daemon

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"pocketd/internal/registry"
	"pocketd/internal/shellenv"
)

func TestATerminalInAProjectGetsTheTokenOfTheAccountItPicked(t *testing.T) {
	d := newDaemon(t)
	bin, project := t.TempDir(), t.TempDir()
	// Prints "tok-<account>" for `gh auth token --hostname github.com --user <account>`.
	os.WriteFile(filepath.Join(bin, "gh"), []byte("#!/bin/sh\n[ -z \"$GH_TOKEN\" ] && echo \"tok-$6\"\n"), 0o755)
	d.env = &shellenv.Result{Env: []string{"PATH=" + bin + ":/usr/bin:/bin", "GH_TOKEN=leaked"}}
	path := filepath.Join(t.TempDir(), "desktop.json")
	d.Registry = registry.New(path)
	os.WriteFile(path, []byte(`{"projects":["`+project+`"],"repos":{"`+project+`":{"gh_account":"work"}}}`), 0o600)

	got := d.GhEnv([]string{"A=1", "GH_TOKEN=old"}, filepath.Join(project, "src"))
	if want := []string{"A=1", "GH_TOKEN=tok-work"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("in the project: got %v", got)
	}
	if got := d.GhEnv([]string{"A=1"}, t.TempDir()); !reflect.DeepEqual(got, []string{"A=1"}) {
		t.Fatalf("outside it: got %v", got)
	}
}
