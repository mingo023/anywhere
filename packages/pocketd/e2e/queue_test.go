//go:build unix

package e2e

import (
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func queueOf(id string, ok func([]proto.QueueEntry) bool) func(Message) bool {
	return func(m Message) bool {
		if m.Type != "queue.update" || m.AgentID != id {
			return false
		}
		var q proto.QueueList
		json.Unmarshal([]byte(m.Raw), &q)
		return ok(q.Entries)
	}
}

// waitQueue reads until a queue.update of agent id satisfies ok and gives its entries.
func waitQueue(p *Phone, id, what string, ok func([]proto.QueueEntry) bool) []proto.QueueEntry {
	p.t.Helper()
	var entries []proto.QueueEntry
	p.WaitFor(what, queueOf(id, func(got []proto.QueueEntry) bool {
		entries = got
		return ok(got)
	}))
	return entries
}

func allIn(state string) func([]proto.QueueEntry) bool {
	return func(entries []proto.QueueEntry) bool {
		return len(entries) == 2 && entries[0].State == state && entries[1].State == state
	}
}

func queueTexts(entries []proto.QueueEntry) []string {
	var out []string
	for _, e := range entries {
		out = append(out, e.Text)
	}
	return out
}

func queueIs(want ...string) func([]proto.QueueEntry) bool {
	return func(entries []proto.QueueEntry) bool { return slices.Equal(queueTexts(entries), want) }
}

func entryOf(entries []proto.QueueEntry, text string) string {
	for _, e := range entries {
		if e.Text == text {
			return e.ID
		}
	}
	return ""
}

func queueMessage(p *Phone, id, text string) {
	p.Send(map[string]any{"type": "agent.prompt", "id": "q-" + text, "agentId": id, "text": text, "queue": true})
}

func openGate(t *testing.T, h *Harness, gate string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(h.Home, gate), nil, 0o600); err != nil {
		t.Fatal(err)
	}
}

var promptNote = regexp.MustCompile(`(?m)^prompt: (.*?)\s*$`)

// promptsTaken is what the fake agent was sent, in the order it began each turn.
func promptsTaken(h *Harness, term string) []string {
	var out []string
	for _, m := range promptNote.FindAllStringSubmatch(h.Screen(term), -1) {
		out = append(out, m[1])
	}
	return out
}

func TestQueuedMessagesReachTheAgentOnePerTurnEndInTheReorderedOrderWithTheEditedText(t *testing.T) {
	h, phone, term, id := chatClaude(t)
	phone.Prompt(id, "await g0")
	phone.WaitStatus(id, "working")

	for _, text := range []string{"await g1", "await g2", "await g3"} {
		queueMessage(phone, id, text)
	}
	entries := waitQueue(phone, id, "three queued messages", queueIs("await g1", "await g2", "await g3"))
	phone.Send(map[string]any{"type": "queue.reorder", "id": "r1", "agentId": id, "entryId": entryOf(entries, "await g3"), "before": entryOf(entries, "await g1")})
	waitQueue(phone, id, "the third message ahead of the first", queueIs("await g3", "await g1", "await g2"))
	phone.Send(map[string]any{"type": "queue.edit", "id": "e1", "agentId": id, "entryId": entryOf(entries, "await g2"), "text": "await g2b"})
	waitQueue(phone, id, "the second message edited", queueIs("await g3", "await g1", "await g2b"))
	if got := promptsTaken(h, term); !slices.Equal(got, []string{"await g0"}) {
		t.Fatalf("the agent took %q while its turn ran, want only the first", got)
	}

	openGate(t, h, "g0")
	waitQueue(phone, id, "the third message sent", queueIs("await g1", "await g2b"))
	openGate(t, h, "g3")
	waitQueue(phone, id, "the first message sent", queueIs("await g2b"))
	openGate(t, h, "g1")
	waitQueue(phone, id, "the second message sent", queueIs())
	h.eventually("the last message taken", func() bool { return len(promptsTaken(h, term)) == 4 })

	want := []string{"await g0", "await g3", "await g1", "await g2b"}
	if got := promptsTaken(h, term); !slices.Equal(got, want) {
		t.Fatalf("the agent took %q, want %q", got, want)
	}
	openGate(t, h, "g2b")
}

func TestAUserStopHoldsTheQueuedMessagesAndTheNextSendReleasesThemInOrder(t *testing.T) {
	h, phone, term, id := chatClaude(t)
	phone.Prompt(id, "await g0")
	phone.WaitStatus(id, "working")
	queueMessage(phone, id, "await h1")
	queueMessage(phone, id, "await h2")
	waitQueue(phone, id, "two queued messages", queueIs("await h1", "await h2"))

	phone.Send(map[string]any{"type": "agent.interrupt", "id": "stop", "agentId": id})
	phone.WaitAll("both messages held and the turn stopped", queueOf(id, allIn("held")), statusOf(id, "idle"))
	if got := promptsTaken(h, term); !slices.Equal(got, []string{"await g0"}) {
		t.Fatalf("the agent took %q after the Stop, want nothing more", got)
	}

	phone.Prompt(id, "await g4")
	waitQueue(phone, id, "both messages released", allIn("pending"))
	openGate(t, h, "g4")
	waitQueue(phone, id, "the first message sent", queueIs("await h2"))
	openGate(t, h, "h1")
	waitQueue(phone, id, "the second message sent", queueIs())
	h.eventually("the last message taken", func() bool { return len(promptsTaken(h, term)) == 4 })

	want := []string{"await g0", "await g4", "await h1", "await h2"}
	if got := promptsTaken(h, term); !slices.Equal(got, want) {
		t.Fatalf("the agent took %q, want %q", got, strings.Join(want, ", "))
	}
	openGate(t, h, "h2")
}
