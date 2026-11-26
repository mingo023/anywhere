//go:build unix

package e2e

import (
	"pocketd/internal/proto"
	"slices"
	"strings"
	"testing"
	"time"
)

// chatClaude creates a claude through Pocket, which runs it for the chat
// behind `pocketd pipe`, and waits until it is ready.
func chatClaude(t *testing.T, opts ...func(*Harness)) (h *Harness, phone *Phone, term, agent string) {
	t.Helper()
	h = Start(t, append([]func(*Harness){launchReady("")}, opts...)...)
	phone = h.Owner()
	create(phone, "r1", inRepo(h, map[string]any{"chat": true}))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	term, agent = done.TerminalID, done.AgentID
	h.WaitScreen(term, "fake claude ready")
	return h, phone, term, agent
}

func asked(m Message) bool { return m.Type == "permission.request" }

func resolve(p *Phone, req Message, fields map[string]any) {
	m := map[string]any{"type": "permission.resolve", "id": "res-" + req.Request.RequestID, "requestId": req.Request.RequestID}
	for k, v := range fields {
		m[k] = v
	}
	p.Send(m)
}

func TestAChatAgentAnswersInItsTranscriptAndEndsItsTurn(t *testing.T) {
	_, phone, _, id := chatClaude(t)
	phone.Prompt(id, "hello")
	phone.WaitAll("the turn",
		func(m Message) bool { return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.ChatOnly },
		streamOf(id, "user", "hello"),
		streamOf(id, "assistant", "echo: hello"),
		func(m Message) bool {
			return m.Type == "agent.stream" && m.AgentID == id && m.Item.Kind == "result" && m.Item.OK
		},
		statusOf(id, "done"),
	)
}

func TestAChatsModelSwitchShowsNoPromptInIt(t *testing.T) {
	_, phone, _, id := chatClaude(t)
	phone.Send(map[string]any{"type": "agent.settings", "id": "s1", "agentId": id, "model": "fake-claude-opus"})
	phone.WaitFor("ack", func(m Message) bool { return m.Type == "ack" && m.ID == "s1" })
	phone.Prompt(id, "hello")
	var prompts []string
	phone.WaitFor("the reply", func(m Message) bool {
		if m.Type == "agent.stream" && m.AgentID == id && m.Item.Kind == "user" {
			prompts = append(prompts, m.Item.Text)
		}
		return streamOf(id, "assistant", "echo: hello")(m)
	})
	if !slices.Equal(prompts, []string{"hello"}) {
		t.Fatalf("chat shows prompts %q", prompts)
	}
}

func TestAPromptStartingWithACommandClaudeDoesNotKnowShowsAsSent(t *testing.T) {
	_, phone, _, id := chatClaude(t)
	phone.Prompt(id, "/gial fix the composer")
	phone.WaitAll("the turn",
		streamOf(id, "user", "/gial fix the composer"),
		streamOf(id, "assistant", "echo: /gial fix the composer"),
		statusOf(id, "done"),
	)
}

func TestAForkedChatShowsTheConversationItForkedBeforeItsFirstPrompt(t *testing.T) {
	h, phone, _, parent := chatClaude(t)
	phone.Prompt(parent, "hello")
	var session string
	phone.WaitAll("the parent's turn",
		streamOf(parent, "assistant", "echo: hello"),
		func(m Message) bool {
			if m.Type == "agent.update" && m.Agent.ID == parent && m.Agent.ProviderSessionID != "" {
				session = m.Agent.ProviderSessionID
			}
			return session != ""
		},
		statusOf(parent, "done"),
	)
	create(phone, "r2", inRepo(h, map[string]any{"chat": true, "fork": session}))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	fork := done.AgentID
	h.WaitScreen(done.TerminalID, "fake claude ready")
	h.eventually("the inherited conversation", func() bool {
		got := timelineOf(phone, fork)
		return slices.Contains(got, "user hello") && slices.Contains(got, "assistant echo: hello")
	})
	phone.Prompt(fork, "again")
	phone.WaitAll("the fork's own turn", streamOf(fork, "assistant", "echo: again"), statusOf(fork, "done"))
	h.eventually("both conversations", func() bool {
		got := timelineOf(phone, fork)
		return slices.Contains(got, "user hello") && slices.Contains(got, "user again") && slices.Contains(got, "assistant echo: again")
	})
}

func TestAChatClaudesNotesToTheUserShowAsItsWords(t *testing.T) {
	_, phone, _, id := chatClaude(t)
	phone.Prompt(id, "narrate Found the cause.")
	shown := []func(Message) bool{streamOf(id, "assistant", "Found the cause."), streamOf(id, "assistant", "echo: done"), statusOf(id, "done")}
	phone.WaitFor("the turn", func(m Message) bool {
		if m.Type == "agent.stream" && m.AgentID == id && m.Item.Kind == "thinking" {
			t.Fatalf("a note to the user came as thinking: %s", m.Raw)
		}
		shown = slices.DeleteFunc(shown, func(ok func(Message) bool) bool { return ok(m) })
		return len(shown) == 0
	})
}

func TestAChatAgentHoldsItsFirstTurnForMCPServersAtMostASecond(t *testing.T) {
	_, phone, _, id := chatClaude(t)
	phone.Prompt(id, "env CLAUDE_CODE_MCP_STARTUP_WAIT_MS")
	phone.WaitFor("the reply", streamOf(id, "assistant", "echo: 1000"))
}

func TestAChatAgentHasClaudesTaskTools(t *testing.T) {
	_, phone, _, id := chatClaude(t)
	phone.Prompt(id, "env CLAUDE_CODE_ENABLE_TODO_TOOLS")
	phone.WaitFor("the reply", streamOf(id, "assistant", "echo: 1"))
}

func TestAChatAgentsFailedTurnEndsWithItsError(t *testing.T) {
	_, phone, _, id := chatClaude(t)
	phone.Send(map[string]any{"type": "agent.settings", "id": "s1", "agentId": id, "model": "bogus"})
	phone.WaitFor("the switch", func(m Message) bool { return m.Type == "ack" && m.ID == "s1" })
	phone.Prompt(id, "hi")
	phone.WaitFor("the failed end", func(m Message) bool {
		return m.Type == "agent.stream" && m.AgentID == id && m.Item.Kind == "result" && !m.Item.OK
	})
}

func TestACreatedChatAgentGetsItsPromptAsTheFirstMessage(t *testing.T) {
	h := Start(t, launchReady(""))
	phone := h.Owner()
	create(phone, "r1", inRepo(h, map[string]any{"prompt": "hello", "chat": true}))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	phone.WaitAll("the first turn", streamOf(done.AgentID, "assistant", "echo: hello"), statusOf(done.AgentID, "done"))
}

func TestAPromptAfterAModelSwitchIsAnswered(t *testing.T) {
	h, phone, term, id := chatClaude(t)
	phone.Send(map[string]any{"type": "agent.settings", "id": "s1", "agentId": id, "model": "opus", "effort": "high"})
	phone.WaitFor("the switch", func(m Message) bool { return m.Type == "ack" && m.ID == "s1" })
	h.WaitScreen(term, "set_model: opus")
	h.WaitScreen(term, "effort: high")

	phone.Prompt(id, "after the switch")

	phone.WaitAll("the turn on opus",
		func(m Message) bool { return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.Model == "opus" },
		streamOf(id, "assistant", "echo: after the switch"),
		statusOf(id, "done"),
	)
}

func TestAToolTheChatAllowsRunsAndIsAskedOnce(t *testing.T) {
	h, phone, term, id := chatClaude(t)
	phone.Prompt(id, "run ls")
	req := phone.WaitFor("permission.request", asked)
	if req.Request.AgentID != id || req.Request.Detail.Command != "ls" {
		t.Fatalf("got %s", req.Raw)
	}
	resolve(phone, req, map[string]any{"decision": "allow"})
	phone.WaitAll("the turn",
		streamOf(id, "assistant", "echo: ran ls"),
		func(m Message) bool {
			if asked(m) {
				t.Fatalf("asked twice: %s", m.Raw)
			}
			return statusOf(id, "done")(m)
		},
	)
	if got := h.Screen(term); !strings.Contains(got, "can_use_tool: allow") || strings.Contains(got, "hook replied") {
		t.Fatalf("screen:\n%s", got)
	}
}

func TestAToolAHookAllowsWhileTheChatAsksGoesBackToWorking(t *testing.T) {
	_, phone, _, id := chatClaude(t)
	phone.Prompt(id, "settled ls")
	phone.WaitFor("permission.request", asked)
	phone.WaitFor("working again before the turn ends", func(m Message) bool {
		if statusOf(id, "done")(m) {
			t.Fatalf("the turn ended still waiting on the user: %s", m.Raw)
		}
		return statusOf(id, "working")(m)
	})
	phone.WaitAll("the turn", streamOf(id, "assistant", "echo: ran ls"), statusOf(id, "done"))
}

func TestAToolTheChatDeniesStopsTheTurn(t *testing.T) {
	h, phone, term, id := chatClaude(t)
	phone.Prompt(id, "run rm -rf build")
	resolve(phone, phone.WaitFor("permission.request", asked), map[string]any{"decision": "deny"})
	h.WaitScreen(term, "can_use_tool: deny")
	phone.WaitStatus(id, "idle")

	phone.Prompt(id, "next")
	phone.WaitAll("the next turn", streamOf(id, "assistant", "echo: next"), statusOf(id, "done"))
}

func TestAQuestionTheChatAnswersReachesClaude(t *testing.T) {
	_, phone, _, id := chatClaude(t)
	phone.Prompt(id, "ask Ship it?")
	req := phone.WaitFor("the question", asked)
	if req.Request.ToolName != "AskUserQuestion" {
		t.Fatalf("got %s", req.Raw)
	}
	resolve(phone, req, map[string]any{"decision": "allow", "answers": map[string]string{"Ship it?": "Yes"}})
	phone.WaitStream(id, "assistant", `echo: answers {"Ship it?":"Yes"}`)
}

func TestAStopEndsTheTurnAndTheNextPromptIsAnswered(t *testing.T) {
	h, phone, term, id := chatClaude(t)
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

func TestAStopEndsOnlyTheTurnAndLeavesBackgroundCommandsRunning(t *testing.T) {
	h, phone, term, id := chatClaude(t)
	phone.Prompt(id, "bg npm run dev")
	phone.WaitStream(id, "assistant", "echo: started npm run dev")
	phone.Send(map[string]any{"type": "agent.interrupt", "id": "i1", "agentId": id})
	phone.WaitAll("the stop", func(m Message) bool { return m.Type == "ack" && m.ID == "i1" }, statusOf(id, "idle"))
	h.WaitScreen(term, "interrupt")
	if strings.Contains(h.Screen(term), "stop_task") {
		t.Fatalf("the stop stopped the background command:\n%s", h.Screen(term))
	}
}

func TestAStopOnATurnHeldForBackgroundAgentsStopsThemButNotItsCommands(t *testing.T) {
	h, phone, term, id := chatClaude(t)
	phone.Prompt(id, "bg npm run dev")
	phone.WaitStatus(id, "done")
	phone.Prompt(id, "agent Look")
	phone.WaitAll("the held turn", streamOf(id, "assistant", "echo: started Lookecho: done"), func(m Message) bool {
		return m.Type == "agent.stream" && m.AgentID == id && m.Item.Kind == "result"
	})
	// The result item comes from the transcript; the held turn starts when pocketd reads claude's stream result, which phones can't see.
	time.Sleep(time.Second)
	phone.Send(map[string]any{"type": "agent.interrupt", "id": "i1", "agentId": id})
	phone.WaitAll("the stop", func(m Message) bool { return m.Type == "ack" && m.ID == "i1" }, statusOf(id, "idle"))
	h.WaitScreen(term, "stop_task: agent_")
	if s := h.Screen(term); strings.Contains(s, "stop_task: bg_") || strings.Contains(s, "interrupt") {
		t.Fatalf("the stop reached past the background agent:\n%s", s)
	}
}

func TestAWorkflowsAgentsReachThePhoneUnderItsCallAndAStopStopsIt(t *testing.T) {
	h, phone, term, id := chatClaude(t)
	desk := h.Owner(proto.CapSubagents)
	phone.Prompt(id, "workflow probe:one")
	m := desk.WaitFor("the workflow's agent", func(m Message) bool {
		return m.Type == "agent.stream" && m.AgentID == id && m.Item.Call.Detail.Phase == "Probe"
	})
	if c := m.Item.Call; m.Item.ParentToolUseID == "" || c.Detail.Description != "probe:one" || c.Status != "running" {
		t.Fatalf("the agent's call: %+v under %q", c, m.Item.ParentToolUseID)
	}
	phone.WaitStatus(id, "done")
	phone.Send(map[string]any{"type": "agent.interrupt", "id": "i1", "agentId": id})
	phone.WaitAll("the stop", func(m Message) bool { return m.Type == "ack" && m.ID == "i1" })
	h.WaitScreen(term, "stop_task: wf_")
}

func TestAStopOnAnIdleChatAgentSendsClaudeNothing(t *testing.T) {
	h, phone, term, id := chatClaude(t)
	phone.Prompt(id, "hello")
	phone.WaitStatus(id, "done")
	phone.Send(map[string]any{"type": "agent.interrupt", "id": "i1", "agentId": id})
	phone.WaitFor("the stop", func(m Message) bool { return m.Type == "ack" && m.ID == "i1" })
	phone.Prompt(id, "again")
	phone.WaitStream(id, "assistant", "echo: again")
	if strings.Contains(h.Screen(term), "interrupt") {
		t.Fatalf("an idle stop reached claude:\n%s", h.Screen(term))
	}
}

func TestAPromptSentMidTurnIsQueuedForTheNext(t *testing.T) {
	_, phone, _, id := chatClaude(t)
	phone.Prompt(id, "slow first")
	phone.WaitStatus(id, "working")
	phone.Prompt(id, "second")
	// Statuses come from the stream and items from the transcript file, so only each's own order holds.
	ends := 0
	phone.WaitAll("both turns",
		streamOf(id, "assistant", "echo: first"),
		streamOf(id, "assistant", "echo: second"),
		func(m Message) bool {
			if statusOf(id, "done")(m) {
				ends++
			}
			return ends == 2
		},
	)
}

func TestACompactRunsAsATurn(t *testing.T) {
	h, phone, term, id := chatClaude(t)
	phone.Send(map[string]any{"type": "agent.compact", "id": "c1", "agentId": id})
	phone.WaitFor("the compact", func(m Message) bool { return m.Type == "ack" && m.ID == "c1" })
	h.WaitScreen(term, "compacted")
	phone.WaitStatus(id, "done")
}

func TestRemoteControlTurnsOnWithTheSessionsLinkAndOffWithout(t *testing.T) {
	h, phone, term, id := chatClaude(t)
	phone.Send(map[string]any{"type": "agent.remote_control", "id": "rc1", "agentId": id, "enabled": true})
	on := phone.WaitFor("remote control on", func(m Message) bool { return m.ID == "rc1" })
	if on.Type != "agent.remote_control" || !strings.Contains(on.Raw, `"url":"https://claude.ai/code/session_fake"`) {
		t.Fatalf("on: %s", on.Raw)
	}
	h.WaitScreen(term, "remote_control: true")
	phone.Send(map[string]any{"type": "agent.remote_control", "id": "rc2", "agentId": id, "enabled": false})
	off := phone.WaitFor("remote control off", func(m Message) bool { return m.ID == "rc2" })
	if off.Type != "agent.remote_control" || strings.Contains(off.Raw, "url") {
		t.Fatalf("off: %s", off.Raw)
	}
	h.WaitScreen(term, "remote_control: false")
}

func TestAnUpgradeKeepsTheChatAgentAndReasksItsOpenTool(t *testing.T) {
	h, phone, term, id := chatClaude(t)
	phone.Prompt(id, "run ls")
	first := phone.WaitFor("permission.request", asked)

	h.Upgrade()

	again := h.Owner()
	req := again.WaitFor("the request asked again", func(m Message) bool {
		return asked(m) && m.Request.AgentID == id && m.Request.RequestID != first.Request.RequestID
	})
	resolve(again, req, map[string]any{"decision": "allow"})
	h.WaitScreen(term, "can_use_tool: allow")
	again.WaitAll("the turn", streamOf(id, "assistant", "echo: ran ls"), statusOf(id, "done"))

	again.Prompt(id, "still here")
	again.WaitStream(id, "assistant", "echo: still here")
}

func TestARestartResumesTheChatAgentForTheChat(t *testing.T) {
	h, phone, term, id := chatClaude(t, func(h *Harness) { h.resume = true })
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
	h.WaitScreen(term, "prompt: after the restart")
}

func TestAChatClaudesTurnIsStoredAsItsTimelineShowsIt(t *testing.T) {
	h, phone, _, id := chatClaude(t)
	phone.Prompt(id, "run ls")
	resolve(phone, phone.WaitFor("permission.request", asked), map[string]any{"decision": "allow"})
	phone.WaitAll("the turn", streamOf(id, "assistant", "echo: ran ls"), statusOf(id, "done"))
	storedAsLive(h, id)
}
