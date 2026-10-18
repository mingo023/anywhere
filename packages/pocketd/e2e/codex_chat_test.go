//go:build unix

package e2e

import (
	"strings"
	"testing"
	"time"
)

// chatCodex creates a codex through Pocket, which runs its app-server for
// the chat behind `pocketd pipe`, and waits until it is ready.
func chatCodex(t *testing.T, extra map[string]any, opts ...func(*Harness)) (h *Harness, phone *Phone, term, agent string) {
	t.Helper()
	h = Start(t, append([]func(*Harness){launchReady("")}, opts...)...)
	phone = h.Owner()
	spec := map[string]any{"provider": "codex"}
	for k, v := range extra {
		spec[k] = v
	}
	create(phone, "r1", inRepo(h, spec))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	term, agent = done.TerminalID, done.AgentID
	h.WaitScreen(term, "fake codex ready app-server")
	h.WaitScreen(term, "thread: ")
	return h, phone, term, agent
}

func TestAChatCodexAnswersInItsTranscriptAndEndsItsTurn(t *testing.T) {
	_, phone, _, id := chatCodex(t, nil)
	phone.Prompt(id, "hello")
	phone.WaitAll("the turn",
		func(m Message) bool { return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.ChatOnly },
		streamOf(id, "user", "hello"),
		streamOf(id, "assistant", "echo: hello"),
		statusOf(id, "done"),
	)
}

func TestACreatedChatCodexGetsItsPromptAsTheFirstMessage(t *testing.T) {
	h := Start(t, launchReady(""))
	phone := h.Owner()
	create(phone, "r1", inRepo(h, map[string]any{"provider": "codex", "prompt": "hello"}))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	phone.WaitAll("the first turn", streamOf(done.AgentID, "assistant", "echo: hello"), statusOf(done.AgentID, "done"))
}

func TestAChatCodexRunsWhatTheChatAllows(t *testing.T) {
	h, phone, term, id := chatCodex(t, nil)
	phone.Prompt(id, "run ls")
	req := phone.WaitFor("permission.request", asked)
	if req.Request.AgentID != id || req.Request.Detail.Command != "ls" {
		t.Fatalf("got %s", req.Raw)
	}
	resolve(phone, req, map[string]any{"decision": "allow"})
	phone.WaitAll("the turn", streamOf(id, "assistant", "echo: ran ls"), statusOf(id, "done"))
	h.WaitScreen(term, "approval: accept")
}

func TestAChatCodexSkipsWhatTheChatDenies(t *testing.T) {
	h, phone, term, id := chatCodex(t, nil)
	phone.Prompt(id, "run rm -rf build")
	resolve(phone, phone.WaitFor("permission.request", asked), map[string]any{"decision": "deny"})
	h.WaitScreen(term, "approval: decline")
	phone.WaitStatus(id, "done")

	phone.Prompt(id, "next")
	phone.WaitStream(id, "assistant", "echo: next")
}

func TestAStoppedChatCodexTurnEndsAndTheNextPromptIsAnswered(t *testing.T) {
	h, phone, term, id := chatCodex(t, nil)
	phone.Prompt(id, "slow never")
	phone.WaitStatus(id, "working")
	phone.Send(map[string]any{"type": "agent.interrupt", "id": "i1", "agentId": id})
	phone.WaitAll("the stop", func(m Message) bool { return m.Type == "ack" && m.ID == "i1" }, statusOf(id, "idle"))
	h.WaitScreen(term, "interrupt")

	phone.Prompt(id, "after the stop")
	phone.WaitAll("the next turn", streamOf(id, "assistant", "echo: after the stop"), statusOf(id, "done"))
	time.Sleep(3 * time.Second)
	if strings.Contains(h.Screen(term), "echo: never") {
		t.Fatal("the stopped turn went on")
	}
}

func TestAPromptSentMidTurnSteersTheChatCodexTurn(t *testing.T) {
	h, phone, term, id := chatCodex(t, nil)
	phone.Prompt(id, "slow first")
	phone.WaitStatus(id, "working")
	phone.Prompt(id, "second")
	h.WaitScreen(term, "steer: second")
	ends := 0
	phone.WaitAll("one turn with both",
		streamOf(id, "user", "second"),
		streamOf(id, "assistant", "echo: first + second"),
		func(m Message) bool {
			if statusOf(id, "done")(m) {
				ends++
			}
			return ends == 1
		},
	)
}

func TestAChatCodexSwitchesModelForItsNextTurn(t *testing.T) {
	h, phone, term, id := chatCodex(t, nil)
	phone.Send(map[string]any{"type": "agent.settings", "id": "s1", "agentId": id, "model": "gpt-6-sol", "effort": "high"})
	phone.WaitFor("the switch", func(m Message) bool { return m.Type == "ack" && m.ID == "s1" })
	h.WaitScreen(term, "set_model: gpt-6-sol")
	h.WaitScreen(term, "effort: high")
	phone.Prompt(id, "after the switch")
	phone.WaitAll("the turn on the new model",
		func(m Message) bool {
			return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.Model == "gpt-6-sol"
		},
		streamOf(id, "assistant", "echo: after the switch"),
		statusOf(id, "done"),
	)
}

func TestAChatCodexCompacts(t *testing.T) {
	h, phone, term, id := chatCodex(t, nil)
	phone.Send(map[string]any{"type": "agent.compact", "id": "c1", "agentId": id})
	phone.WaitFor("the compact", func(m Message) bool { return m.Type == "ack" && m.ID == "c1" })
	h.WaitScreen(term, "compacted")
	phone.WaitStatus(id, "done")
}

func TestABogusModelFailsTheChatCodexTurn(t *testing.T) {
	_, phone, _, id := chatCodex(t, map[string]any{"model": "bogus"})
	phone.Prompt(id, "hi")
	phone.WaitFor("the failed turn", func(m Message) bool { return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.Failed })
}

func TestAnUpgradeKeepsTheChatCodexAndReasksItsOpenApproval(t *testing.T) {
	h, phone, term, id := chatCodex(t, nil)
	phone.Prompt(id, "run ls")
	first := phone.WaitFor("permission.request", asked)

	h.Upgrade()

	again := h.Owner()
	req := again.WaitFor("the request asked again", func(m Message) bool {
		return asked(m) && m.Request.AgentID == id && m.Request.RequestID != first.Request.RequestID
	})
	resolve(again, req, map[string]any{"decision": "allow"})
	h.WaitScreen(term, "approval: accept")
	again.WaitAll("the turn", streamOf(id, "assistant", "echo: ran ls"), statusOf(id, "done"))

	again.Prompt(id, "still here")
	again.WaitStream(id, "assistant", "echo: still here")
	if n := strings.Count(h.Screen(term), "fake codex ready"); n != 1 {
		t.Fatalf("codex started %d times", n)
	}
}

func TestARestartResumesTheChatCodexThread(t *testing.T) {
	h, phone, term, id := chatCodex(t, nil, func(h *Harness) { h.resume = true })
	phone.Prompt(id, "before")
	phone.WaitStatus(id, "done")

	h.Restart()

	again := h.Owner()
	again.WaitFor("the agent resumed for the chat", func(m Message) bool {
		if m.Type == "agent.list" && len(m.Agents) == 1 {
			return m.Agents[0].ID == id && m.Agents[0].Restore == "resumed" && m.Agents[0].ChatOnly
		}
		return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.Restore == "resumed" && m.Agent.ChatOnly
	})
	again.Prompt(id, "after the restart")
	again.WaitStream(id, "assistant", "echo: after the restart")
	h.WaitScreen(term, "resume: ")
}
