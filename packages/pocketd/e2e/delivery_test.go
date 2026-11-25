//go:build unix

package e2e

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func deliveryOf(id, state string) func(Message) bool {
	return func(m Message) bool {
		return m.Type == "agent.delivery" && m.ID == id && strings.Contains(m.Raw, `"state":"`+state+`"`)
	}
}

func sendsOnce(p *Phone, id, agent, text string) {
	p.Send(map[string]any{"type": "agent.prompt", "id": id, "agentId": agent, "text": text})
}

const secret = "secret-words-42"

func TestAPromptSentAgainUnderTheSameIdAfterAReconnectReachesTheAgentOnce(t *testing.T) {
	h, _, _, agent := chatClaude(t)
	first := h.Owner("delivery.v1")
	sendsOnce(first, "p-1", agent, secret)
	first.WaitAll("the ack and the start",
		func(m Message) bool { return m.Type == "ack" && m.ID == "p-1" },
		deliveryOf("p-1", "started"))
	first.ws.CloseNow()
	h.eventually("the closed conn in pocketd.log", func() bool {
		log, _ := os.ReadFile(filepath.Join(h.Home, "logs", "pocketd.log"))
		return strings.Contains(string(log), `ws closed: client="e2e-desktop"`)
	})

	again := h.Owner("delivery.v1")
	sendsOnce(again, "p-1", agent, secret)
	again.WaitAll("the ack and the start again",
		func(m Message) bool { return m.Type == "ack" && m.ID == "p-1" },
		deliveryOf("p-1", "started"))
	h.eventually("the reply", func() bool { return hasReply(again, agent, secret) })
	time.Sleep(500 * time.Millisecond)
	if log, _ := os.ReadFile(filepath.Join(h.Home, "logs", "pocketd.log")); strings.Contains(string(log), secret) {
		t.Fatalf("pocketd.log holds the prompt:\n%s", log)
	}
	got := timelineOf(again, agent)
	if asked, answered := count(got, "user "+secret), count(got, "assistant echo: "+secret); asked != 1 || answered != 1 {
		t.Fatalf("the agent saw the prompt %d times and answered %d: %q", asked, answered, got)
	}
}

func count(items []string, want string) int {
	n := 0
	for _, it := range items {
		if it == want {
			n++
		}
	}
	return n
}

func TestAPromptToAFrozenAgentIsAckedAtOnceAndItsOutcomeFollowsWithAReasonIfItFailed(t *testing.T) {
	h, _, _, agent := chatClaude(t)
	owner := h.Owner("delivery.v1")
	freeze(t, holdersOf(t, h.cmd.Process.Pid), 3*time.Second)
	sendsOnce(owner, "p-1", agent, "hello")
	owner.WaitWithin(2*time.Second, "the ack while the agent is frozen", func(m Message) bool { return m.Type == "ack" && m.ID == "p-1" })
	end := owner.WaitFor("the outcome", func(m Message) bool {
		return deliveryOf("p-1", "started")(m) || deliveryOf("p-1", "failed")(m)
	})
	if strings.Contains(end.Raw, `"state":"failed"`) && !strings.Contains(end.Raw, `"error":"`) {
		t.Fatalf("failed without a reason: %s", end.Raw)
	}
}
