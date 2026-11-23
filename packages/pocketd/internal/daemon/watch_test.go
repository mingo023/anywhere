//go:build unix

package daemon

import (
	"context"
	"os"
	"os/exec"
	"slices"
	"strconv"
	"strings"
	"syscall"
	"testing"
	"time"

	"pocketd/internal/holder"
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
	text, procs, _ := foreground(term)
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

// held runs d's Terminals under holders, as pocketd does on a Mac.
func held(t *testing.T, d *Daemon) {
	t.Helper()
	exe, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	dir, err := os.MkdirTemp("/tmp", "held")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		holder.EndAll(dir, 0)
		os.RemoveAll(dir)
	})
	d.Terminals.Holder = &holder.Starter{Exe: exe, Dir: dir}
}

func TestAHolderTooBusyToAnswerLeavesItsAgentListed(t *testing.T) {
	d := newDaemon(t)
	held(t, d)
	term, pr := claudeIn(t, d)
	out, err := exec.Command("ps", "-o", "ppid=", "-p", strconv.Itoa(pr.pid)).Output()
	if err != nil {
		t.Fatal(err)
	}
	holderPid, _ := strconv.Atoi(strings.TrimSpace(string(out)))
	syscall.Kill(holderPid, syscall.SIGSTOP)
	defer syscall.Kill(holderPid, syscall.SIGCONT)

	d.poll()

	if d.presentIn(term.Info().ID) != pr {
		t.Fatal("a foreground the holder didn't answer for ended the agent")
	}
	if _, err := d.Agents.Get(pr.a.ID()); err != nil {
		t.Fatal(err)
	}
}
