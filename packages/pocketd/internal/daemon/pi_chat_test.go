//go:build unix

package daemon

import (
	"testing"

	"pocketd/internal/state"
	"pocketd/internal/terminal"
)

func TestAPiThatLeftTheForegroundIsNotRestartedToLeavePlan(t *testing.T) {
	d := newDaemon(t)
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: "/bin/sh"})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	pr := &presence{t: term, provider: "pi", launch: state.Launch{Access: "full", Plan: true}}
	c := piDriver{termDriver{t: term, pr: pr}, &piChat{d: d, pr: pr}}
	if err := c.Access("full"); err != errNotForeground {
		t.Fatalf("got %v", err)
	}
}
