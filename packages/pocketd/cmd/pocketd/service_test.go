package main

import (
	"io"
	"testing"

	"pocketd/internal/config"
)

func TestServiceTakesAnActionThenFlags(t *testing.T) {
	got, ok := parseService([]string{"status", "--json", "--label", "x.pocketd"}, io.Discard)
	if !ok || got != (serviceArgs{action: "status", label: "x.pocketd", json: true}) {
		t.Fatalf("got %+v %v", got, ok)
	}
	if got, ok := parseService([]string{"install"}, io.Discard); !ok || got.label != config.HostLabel() || got.json {
		t.Fatalf("defaults: got %+v %v", got, ok)
	}
	for _, args := range [][]string{nil, {"restart"}, {"--json", "status"}, {"install", "extra"}, {"uninstall", "--bogus"}} {
		if _, ok := parseService(args, io.Discard); ok {
			t.Errorf("%q parsed", args)
		}
	}
}

func TestTheHostLabelLeavesTheAppsServiceAlone(t *testing.T) {
	if config.HostLabel() == config.Label() {
		t.Fatal("host service would replace the app's")
	}
}
