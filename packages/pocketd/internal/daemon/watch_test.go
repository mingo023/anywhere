package daemon

import (
	"context"
	"slices"
	"strings"
	"testing"
	"time"

	"pocketd/internal/proc"
	"pocketd/internal/terminal"
)

func shell(t *testing.T, d *Daemon) *terminal.Terminal {
	t.Helper()
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: "sh", Env: []string{"PATH=/bin:/usr/bin", "PS1=ready$ "}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	eventually(t, "prompt", func() bool { return strings.Contains(term.Screen(), "ready$") })
	return term
}

func TestForegroundAtThePromptIsEmptyButListsTheShell(t *testing.T) {
	term := shell(t, newDaemon(t))
	text, procs := foreground(term)
	if text != "" || !slices.ContainsFunc(procs, func(p proc.Proc) bool { return p.Pid == term.Pid() }) {
		t.Fatalf("foreground = %q, %+v", text, procs)
	}
}

func TestWatchFollowsTheForegroundJob(t *testing.T) {
	defer func(e time.Duration) { WatchEvery = e }(WatchEvery)
	WatchEvery = 10 * time.Millisecond
	d := newDaemon(t)
	term := shell(t, d)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go d.Watch(ctx)
	term.Write([]byte("sleep 30\r"))
	eventually(t, "sleep in the foreground", func() bool { return term.Info().Foreground == "sleep 30" })
	term.Write([]byte{0x03})
	eventually(t, "back at the prompt", func() bool { return term.Info().Foreground == "" })
	term.Write([]byte("sleep 0.1 | sleep 30\r"))
	eventually(t, "the pipeline's last member", func() bool { return term.Info().Foreground == "sleep 30" })
	term.Write([]byte{0x03})
	eventually(t, "back at the prompt", func() bool { return term.Info().Foreground == "" })
}
