//go:build unix

package daemon

import (
	"encoding/json"
	"fmt"
	"reflect"
	"testing"

	"pocketd/internal/agent"
	"pocketd/internal/mailbox"
	"pocketd/internal/terminal"
)

// chatIn is a chat claude behind a pipe nothing reads: what pocketd sends it
// waits in its stream's inbox.
func chatIn(t *testing.T, d *Daemon) *presence {
	t.Helper()
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "pocketd"), Args: []string{"pipe", "claude", "-p"}, Env: []string{"PATH=/bin:/usr/bin"}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	waitAgent(t, d, term)
	return d.presentIn(term.Info().ID)
}

// sentLine is a user line pocketd sent claude.
type sentLine struct{ UUID, Text string }

func sentToClaude(pr *presence) []sentLine {
	pr.stream.mu.Lock()
	defer pr.stream.mu.Unlock()
	var out []sentLine
	for _, raw := range pr.stream.inbox {
		var l struct {
			Type    string `json:"type"`
			UUID    string `json:"uuid"`
			Message struct {
				Content string `json:"content"`
			} `json:"message"`
		}
		if json.Unmarshal([]byte(raw), &l) == nil && l.Type == "user" {
			out = append(out, sentLine{l.UUID, l.Message.Content})
		}
	}
	return out
}

// sentN waits until pocketd has sent pr's claude n user lines.
func sentN(t *testing.T, pr *presence, n int) []sentLine {
	t.Helper()
	var got []sentLine
	eventually(t, fmt.Sprintf("%d user lines", n), func() bool {
		got = sentToClaude(pr)
		return len(got) == n
	})
	return got
}

// Proves: Deliver sends a prompt under a fresh uuid it returns; a chat prompt has none.
func TestDeliverSendsClaudeAPromptUnderTheUUIDItReturns(t *testing.T) {
	d := newDaemon(t)
	pr := chatIn(t, d)
	if err := pr.a.Driver().Prompt("typed"); err != nil {
		t.Fatal(err)
	}

	ref, err := pr.a.Driver().(agent.Deliverer).Deliver("mailed")
	if err != nil {
		t.Fatal(err)
	}

	want := []sentLine{{"", "typed"}, {ref, "mailed"}}
	if got := sentToClaude(pr); !reflect.DeepEqual(got, want) || len(ref) != 36 {
		t.Fatalf("got %+v (ref %q), want %+v with a uuid", got, ref, want)
	}
}

// withMailbox gives d a mailbox pumped on each status change, as serve wires it.
func withMailbox(t *testing.T, d *Daemon) {
	t.Helper()
	box, err := mailbox.Open(d.Home)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { box.Close() })
	d.Mailbox = box
	d.Agents.OnStatus = func(id, _, _, _ string) { d.Pump(id) }
}

func queuedTexts(t *testing.T, d *Daemon, agentID string) []string {
	t.Helper()
	q, err := d.Mailbox.Queued(agentID)
	if err != nil {
		t.Fatal(err)
	}
	var out []string
	for _, m := range q {
		out = append(out, m.Text)
	}
	return out
}

// Proves: mail to an idle chat goes to claude under a uuid and leaves the queue.
func TestMailToAnIdleChatGoesToClaudeAndLeavesTheQueue(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	pr := chatIn(t, d)
	id := pr.a.ID()

	if err := d.Mail(id, "hello"); err != nil {
		t.Fatal(err)
	}

	if got := sentN(t, pr, 1); got[0].Text != "hello" || got[0].UUID == "" {
		t.Fatalf("got %+v", got)
	}
	eventually(t, "the mail shown", func() bool { return queuedTexts(t, d, id) == nil })
}

// Proves: mail goes mid-turn too, for claude to take at a tool boundary or next.
func TestMailSentMidTurnGoesToClaudeAtOnce(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	pr := chatIn(t, d)
	pr.working("")

	if err := d.Mail(pr.a.ID(), "hello"); err != nil {
		t.Fatal(err)
	}

	if got := sentN(t, pr, 1); got[0].Text != "hello" {
		t.Fatalf("got %+v", got)
	}
}

// Proves: needsYou holds mail, and the next status change sends it.
func TestMailWaitsWhileTheChatNeedsYouAndGoesOnceItWorksAgain(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	pr := chatIn(t, d)
	id := pr.a.ID()
	pr.a.NeedsYou()
	if _, err := d.Mailbox.Send(id, "hello"); err != nil {
		t.Fatal(err)
	}

	d.deliver(id)
	if got := sentToClaude(pr); got != nil {
		t.Fatalf("sent while it needs you: %+v", got)
	}

	pr.a.Working()
	if got := sentN(t, pr, 1); got[0].Text != "hello" {
		t.Fatalf("got %+v", got)
	}
}

// Proves: an agent in its TUI is not a chat, so its mail stays queued.
func TestMailToATerminalAgentStaysQueued(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	_, pr := claudeIn(t, d)
	id := pr.a.ID()
	if _, err := d.Mailbox.Send(id, "hello"); err != nil {
		t.Fatal(err)
	}

	d.deliver(id)

	if got := queuedTexts(t, d, id); !reflect.DeepEqual(got, []string{"hello"}) {
		t.Fatalf("queued %q", got)
	}
}

func lifecycle(uuid, state string) string {
	return `{"type":"command_lifecycle","command_uuid":"` + uuid + `","state":"` + state + `"}`
}

// Proves: completed acks the mail, so the session's exit doesn't put it back.
func TestMailClaudeCompletesIsAckedAndOutlivesTheSession(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	pr := chatIn(t, d)
	id := pr.a.ID()
	if err := d.Mail(id, "hello"); err != nil {
		t.Fatal(err)
	}
	sent := sentN(t, pr, 1)[0]

	d.streamLine(pr, lifecycle(sent.UUID, "completed"))
	d.endAgent(pr)

	if got := queuedTexts(t, d, id); got != nil {
		t.Fatalf("queued after the exit: %q", got)
	}
}

// Proves: a session that exits puts the mail it was shown back in the queue.
func TestMailAChatWasShownGoesBackToTheQueueWhenItExits(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	pr := chatIn(t, d)
	id := pr.a.ID()
	if err := d.Mail(id, "hello"); err != nil {
		t.Fatal(err)
	}
	sentN(t, pr, 1)

	d.endAgent(pr)

	if got := queuedTexts(t, d, id); !reflect.DeepEqual(got, []string{"hello"}) {
		t.Fatalf("queued %q", got)
	}
}

// Proves: cancelled mail goes to claude again, under a new uuid.
func TestMailClaudeCancelsGoesToItAgainUnderANewUUID(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	pr := chatIn(t, d)
	if err := d.Mail(pr.a.ID(), "hello"); err != nil {
		t.Fatal(err)
	}
	first := sentN(t, pr, 1)[0]

	d.streamLine(pr, lifecycle(first.UUID, "cancelled"))

	again := sentN(t, pr, 2)[1]
	if again.Text != "hello" || again.UUID == "" || again.UUID == first.UUID {
		t.Fatalf("first %+v, again %+v", first, again)
	}
}

// Proves: a lifecycle line naming no uuid settles nothing.
func TestMailIsNotAckedByALifecycleLineWithNoUUID(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	pr := chatIn(t, d)
	id := pr.a.ID()
	if err := d.Mail(id, "hello"); err != nil {
		t.Fatal(err)
	}
	sentN(t, pr, 1)

	d.streamLine(pr, lifecycle("", "completed"))
	d.endAgent(pr)

	if got := queuedTexts(t, d, id); !reflect.DeepEqual(got, []string{"hello"}) {
		t.Fatalf("queued %q", got)
	}
}
