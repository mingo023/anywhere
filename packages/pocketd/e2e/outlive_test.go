//go:build unix

package e2e

import (
	"encoding/json"
	"os"
	"slices"
	"syscall"
	"testing"

	"pocketd/internal/holder"
	"pocketd/internal/ops"
	"pocketd/internal/state"
)

// heldPids is each Terminal's process, by Terminal id, as its holder reports
// it. Only while pocketd is down: attaching takes the holder from pocketd.
func heldPids(h *Harness) map[string]int {
	h.t.Helper()
	pids := map[string]int{}
	for _, c := range holder.Find(holder.Dir(h.Home)) {
		pids[c.ID] = c.Pid()
		c.Close()
	}
	return pids
}

// gapless fails unless counts run on by one from at most last.
func gapless(t *testing.T, counts []int, last int) {
	t.Helper()
	if counts[0] > last {
		t.Fatalf("the screen before pocketd stopped is gone: %v, last was %d", counts, last)
	}
	for i := 1; i < len(counts); i++ {
		if counts[i] != counts[i-1]+1 {
			t.Fatalf("output was lost: %v", counts)
		}
	}
}

func keepsTheShellRunningWithEveryLine(t *testing.T, sig os.Signal) {
	h := Start(t)
	id, shell := counting(h)
	before := h.countsPast(id, 3)

	h.Stop(sig)
	if syscall.Kill(shell, 0) != nil {
		t.Fatal("the shell died with pocketd")
	}
	h.up()

	c := h.Ops()
	c.Send(ops.Msg{Op: "list"})
	if m, err := c.Recv(); err != nil || len(m.Items) != 1 || m.Items[0].ID != id || m.Items[0].Cmd != "sh" {
		t.Fatalf("list = %+v, %v", m, err)
	}
	last := before[len(before)-1]
	gapless(t, h.countsPast(id, last+5), last)
}

func TestASigtermRestartKeepsTheShellRunningWithEveryLine(t *testing.T) {
	keepsTheShellRunningWithEveryLine(t, syscall.SIGTERM)
}

func TestACrashKeepsTheShellRunningWithEveryLine(t *testing.T) {
	keepsTheShellRunningWithEveryLine(t, syscall.SIGKILL)
}

func TestARestartKeepsEveryAgentRunningWithoutARestore(t *testing.T) {
	for _, sig := range []os.Signal{syscall.SIGTERM, syscall.SIGKILL} {
		t.Run(sig.String(), func(t *testing.T) {
			h, phone, chatTerm, chat := chatClaude(t, func(h *Harness) { h.resume = true })
			create(phone, "r2", inRepo(h, map[string]any{}))
			created := reply(phone, "agent.created", "error")
			if created.Type != "agent.created" {
				t.Fatalf("got %s", created.Raw)
			}
			inTerm, term := created.AgentID, created.TerminalID
			h.WaitScreen(term, "fake claude ready")
			for _, id := range []string{chat, inTerm} {
				phone.Prompt(id, "before")
				h.eventually("the reply before the restart", func() bool { return hasReply(phone, id, "before") })
			}

			// A crash keeps what pocketd last saved, up to a second old.
			h.eventually("both sessions saved", func() bool {
				f, _ := state.Load(state.Path(h.Home))
				return slices.ContainsFunc(f.Terminals, func(e state.Terminal) bool { return e.AgentID == chat }) &&
					slices.ContainsFunc(f.Terminals, func(e state.Terminal) bool { return e.AgentID == inTerm })
			})
			h.Stop(sig)
			pids := heldPids(h)
			if len(pids) != 2 || pids[chatTerm] == 0 || pids[term] == 0 {
				t.Fatalf("held %v", pids)
			}
			h.up()

			again := h.Owner()
			sessionsListed(h, again, chat, inTerm)
			for _, id := range []string{chat, inTerm} {
				h.eventually("the session takes a prompt", func() bool {
					again.Prompt(id, "after")
					return again.WaitFor("the prompt's answer", func(m Message) bool {
						return m.ID == "prompt-after" && (m.Type == "ack" || m.Type == "error")
					}).Type == "ack"
				})
				h.eventually("the reply after the restart", func() bool { return hasReply(again, id, "after") })
				if n := countOf(timelineOf(again, id), "assistant echo: before"); n != 1 {
					t.Fatalf("%s shows the reply before the restart %d times", id, n)
				}
			}
			for _, pid := range pids {
				if syscall.Kill(pid, 0) != nil {
					t.Fatalf("agent %d died", pid)
				}
			}
			for _, a := range again.agents() {
				if a.Restore != "" {
					t.Fatalf("agent %s was restored: %s", a.ID, a.Restore)
				}
			}
		})
	}
}

func countOf(items []string, want string) int {
	n := 0
	for _, it := range items {
		if it == want {
			n++
		}
	}
	return n
}

type listedAgent struct {
	ID      string `json:"id"`
	Restore string `json:"restore"`
}

func (p *Phone) agents() []listedAgent {
	p.t.Helper()
	p.Send(map[string]any{"type": "agent.list", "id": "agents"})
	var page struct {
		Agents []listedAgent `json:"agents"`
	}
	json.Unmarshal([]byte(p.WaitFor("agent.list", func(m Message) bool { return m.Type == "agent.list" && m.ID == "agents" }).Raw), &page)
	return page.Agents
}
