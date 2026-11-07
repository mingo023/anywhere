//go:build unix

package e2e

import (
	"syscall"
	"testing"

	"pocketd/internal/mailbox"
)

// mail queues text for agent from the test's own process: nothing on the wire
// sends mail until team mode, so it goes at the agent's next status change or
// once a restored session settles. Opening the mailbox puts shown mail back in
// the queue, so call it only while no mail is shown.
func mail(t *testing.T, h *Harness, agent, text string) {
	t.Helper()
	box, err := mailbox.Open(h.Home)
	if err != nil {
		t.Fatal(err)
	}
	defer box.Close()
	if _, err := box.Send(agent, text); err != nil {
		t.Fatal(err)
	}
}

// unacked is the text of agent's mail no turn acked. Call it with pocketd
// stopped: opening the mailbox puts shown mail back in the queue.
func unacked(t *testing.T, h *Harness, agent string) []string {
	t.Helper()
	box, err := mailbox.Open(h.Home)
	if err != nil {
		t.Fatal(err)
	}
	defer box.Close()
	q, err := box.Queued(agent)
	if err != nil {
		t.Fatal(err)
	}
	var out []string
	for _, m := range q {
		out = append(out, m.Text)
	}
	return out
}

// statusIs matches agent's status in an update, or in the list a phone gets on hello.
func statusIs(agent, status string) func(Message) bool {
	return func(m Message) bool {
		if m.Type == "agent.list" {
			for _, a := range m.Agents {
				if a.ID == agent && a.Status == status {
					return true
				}
			}
			return false
		}
		return statusOf(agent, status)(m)
	}
}

func stop(t *testing.T, h *Harness) {
	t.Helper()
	if code := h.Stop(syscall.SIGTERM); code != 0 {
		t.Fatalf("pocketd exited %d", code)
	}
}

// Proves: mail survives a restart, goes to the chat once it resumes, and its
// completed turn acks it.
func TestMailWaitingWhilePocketdIsDownReachesTheResumedChatAndIsAcked(t *testing.T) {
	h, phone, term, id := chatClaude(t, func(h *Harness) { h.resume = true })
	phone.Prompt(id, "before")
	phone.WaitStatus(id, "done")
	phone.Send(map[string]any{"type": "agent.seen", "id": "seen", "agentIds": []string{id}})
	phone.WaitStatus(id, "idle")
	stop(t, h)

	mail(t, h, id, "from the mailbox")
	h.up()

	again := h.Owner()
	again.WaitFor("the mail's turn to end", statusIs(id, "done"))
	h.WaitScreen(term, "completed: from the mailbox")
	stop(t, h)
	if got := unacked(t, h, id); got != nil {
		t.Fatalf("unacked: %q", got)
	}
}

// Proves: mail a stop cancels goes to the chat again.
func TestMailAStopCancelsGoesToTheChatAgain(t *testing.T) {
	h, phone, term, id := chatClaude(t)
	mail(t, h, id, "from the mailbox")
	phone.Prompt(id, "hang")
	h.WaitScreen(term, "queued: from the mailbox")

	phone.Send(map[string]any{"type": "agent.interrupt", "id": "i1", "agentId": id})
	phone.WaitFor("the stop", func(m Message) bool { return m.Type == "ack" && m.ID == "i1" })

	h.WaitScreen(term, "cancelled: from the mailbox")
	phone.WaitStream(id, "assistant", "echo: from the mailbox")
	h.WaitScreen(term, "completed: from the mailbox")
}

// Proves: mail sent mid-turn steers into the running codex turn, and that
// turn completing acks it.
func TestMailSteeredIntoAChatCodexTurnIsAckedWhenTheTurnCompletes(t *testing.T) {
	h, phone, term, id := chatCodex(t, nil)
	mail(t, h, id, "from the mailbox")
	phone.Prompt(id, "slow first")
	h.WaitScreen(term, "steer: from the mailbox")
	phone.WaitStream(id, "assistant", "echo: first + from the mailbox")
	phone.WaitStatus(id, "done")

	phone.Prompt(id, "after")
	phone.WaitStream(id, "assistant", "echo: after")
	stop(t, h)
	if got := unacked(t, h, id); got != nil {
		t.Fatalf("unacked: %q", got)
	}
}

// Proves: mail in an interrupted codex turn goes in again, as its own turn.
func TestMailInAnInterruptedChatCodexTurnGoesInAgain(t *testing.T) {
	h, phone, term, id := chatCodex(t, nil)
	mail(t, h, id, "from the mailbox")
	phone.Prompt(id, "slow never")
	h.WaitScreen(term, "steer: from the mailbox")

	phone.Send(map[string]any{"type": "agent.interrupt", "id": "i1", "agentId": id})
	phone.WaitFor("the stop", func(m Message) bool { return m.Type == "ack" && m.ID == "i1" })

	phone.WaitStream(id, "assistant", "echo: from the mailbox")
	h.WaitScreen(term, "prompt: from the mailbox")
}
