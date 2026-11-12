//go:build unix

package e2e

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"pocketd/internal/proto"
)

// buildFakePi builds the fake pi in dir, and as omp, the fork that speaks the same RPC.
func buildFakePi(dir string) {
	if out, err := exec.Command("go", "build", "-o", filepath.Join(dir, "pi"), "./fakepi").CombinedOutput(); err != nil {
		fmt.Fprintf(os.Stderr, "build ./fakepi: %v\n%s", err, out)
		os.Exit(1)
	}
	if err := os.Symlink("pi", filepath.Join(dir, "omp")); err != nil {
		panic(err)
	}
}

// chatPi creates provider's agent, pi or omp, through Pocket, which runs it
// with `--mode rpc` for the chat behind `pocketd pipe`, and waits until it is up.
func chatPi(t *testing.T, provider string, extra map[string]any, opts ...func(*Harness)) (h *Harness, phone *Phone, term, agent string) {
	t.Helper()
	h = Start(t, append([]func(*Harness){launchReady("")}, opts...)...)
	phone = h.Owner()
	spec := map[string]any{"provider": provider, "chat": true}
	for k, v := range extra {
		spec[k] = v
	}
	create(phone, "r1", inRepo(h, spec))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	term, agent = done.TerminalID, done.AgentID
	h.WaitScreen(term, "fake "+provider+" ready --mode rpc")
	h.WaitScreen(term, "session: ")
	return h, phone, term, agent
}

func acked(id string) func(Message) bool {
	return func(m Message) bool { return m.Type == "ack" && m.ID == id }
}

// history is the agent's timeline as a phone that just connected reads it, an item a line.
func history(phone *Phone, id string) []string {
	return historyUnless(phone, id, func(Message) bool { return false })
}

// historyUnless is history, failing if what the phone hears before it is bad.
func historyUnless(phone *Phone, id string, bad func(Message) bool) []string {
	phone.t.Helper()
	phone.Send(map[string]any{"type": "agent.timeline", "id": "t1", "agentId": id})
	page := phone.WaitFor("the timeline", func(m Message) bool {
		if bad(m) {
			phone.t.Fatalf("got %s", m.Raw)
		}
		return m.Type == "agent.timeline" && m.ID == "t1"
	})
	var p struct {
		Items []acpItem `json:"items"`
	}
	json.Unmarshal([]byte(page.Raw), &p)
	var got []string
	for _, it := range p.Items {
		got = append(got, strings.Join(strings.Fields(it.Kind+" "+it.Text+it.Call.Detail.Command+" "+it.Call.Status+it.Error), " "))
	}
	return got
}

// withSubAgents is the agent's timeline as a phone that takes sub-agents reads
// it, an item a line, a sub-agent's marked ">".
func withSubAgents(phone *Phone, id string) []string {
	phone.t.Helper()
	phone.Send(map[string]any{"type": "agent.timeline", "id": "t1", "agentId": id})
	page := phone.WaitFor("the timeline", func(m Message) bool { return m.Type == "agent.timeline" && m.ID == "t1" })
	var p struct {
		Items []struct {
			acpItem
			Parent string `json:"parentToolUseId"`
		} `json:"items"`
	}
	json.Unmarshal([]byte(page.Raw), &p)
	var got []string
	for _, it := range p.Items {
		line := strings.Join(strings.Fields(it.Kind+" "+it.Text+it.Call.Detail.Kind+" "+it.Call.Status+" "+it.Call.Output), " ")
		if it.Parent != "" {
			line = "> " + line
		}
		got = append(got, line)
	}
	return got
}

func TestAChatPiStreamsItsReplyAndEndsItsTurn(t *testing.T) {
	_, phone, _, id := chatPi(t, "pi", nil)
	phone.Prompt(id, "hello")
	working := false
	phone.WaitAll("the turn",
		func(m Message) bool { return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.ChatOnly },
		streamOf(id, "user", "hello"),
		func(m Message) bool {
			working = working || statusOf(id, "working")(m)
			return working
		},
		// The reply drafts a delta at a time before it is whole.
		streamOf(id, "assistant", "echo: "),
		streamOf(id, "assistant", "echo: hello"),
		itemOf(id, func(it acpItem) bool { return it.Kind == "result" && it.OK }),
		func(m Message) bool { return working && statusOf(id, "done")(m) },
		// pi tells its model, and what its context holds once the turn ends.
		func(m Message) bool {
			return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.Model == "fake/model" && strings.Contains(m.Raw, `"tokensUsed":1200`)
		},
	)
	if got, want := history(phone, id), []string{"user hello", "assistant echo: hello", "result"}; strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("got %q", got)
	}
}

func TestACreatedChatPiGetsItsPromptAsTheFirstMessage(t *testing.T) {
	h := Start(t, launchReady(""))
	phone := h.Owner()
	create(phone, "r1", inRepo(h, map[string]any{"provider": "pi", "prompt": "hello", "chat": true, "model": "fake/other", "effort": "high"}))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	phone.WaitAll("the first turn", streamOf(done.AgentID, "assistant", "echo: hello"), statusOf(done.AgentID, "done"))
	h.WaitScreen(done.TerminalID, "fake pi ready --mode rpc --model fake/other --thinking high")
}

func TestAChatPiShowsItsSubAgentsWorkUnderTheCallThatRanItToAPhoneThatTakesIt(t *testing.T) {
	h, phone, _, id := chatPi(t, "pi", nil)
	phone.Prompt(id, "delegate ls")
	phone.WaitAll("the turn", streamOf(id, "assistant", "echo: delegated ls"), statusOf(id, "done"))
	if got, want := withSubAgents(h.Owner(proto.CapSubagents), id), []string{"user delegate ls", "tool task ok did ls", "> user ls", "> assistant did ls", "assistant echo: delegated ls", "result"}; strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("got %q", got)
	}
	if got := history(phone, id); slices.Contains(got, "assistant did ls") {
		t.Fatalf("a phone that doesn't take sub-agents got %q", got)
	}
}

func TestAChatPiRunsItsToolsWithoutAsking(t *testing.T) {
	_, phone, _, id := chatPi(t, "pi", nil)
	phone.Prompt(id, "run ls")
	phone.WaitFor("the turn", func() func(Message) bool {
		want := []func(Message) bool{
			itemOf(id, shellCall("running", "ls")),
			itemOf(id, func(it acpItem) bool { return shellCall("ok", "ls")(it) && it.Call.Output == "ran: ls" }),
			streamOf(id, "assistant", "echo: ran ls"),
			statusOf(id, "done"),
		}
		return func(m Message) bool {
			if asked(m) || statusOf(id, "needsYou")(m) {
				t.Fatalf("pi asked: %s", m.Raw)
			}
			if len(want) > 0 && want[0](m) {
				want = want[1:]
			}
			return len(want) == 0
		}
	}())
}

// The error is the one a real pi on a DeepSeek key without credit ended its reply with.
func TestAProviderErrorFailsTheChatPiTurnWithItsMessage(t *testing.T) {
	const failure = `402: {"message":"Insufficient Balance (request_id: 00a9f247-b478-4352-8991-a6bd9eb3db6f)","type":"insufficient_balance_error","param":null,"code":"invalid_request_error"}`
	_, phone, _, id := chatPi(t, "pi", nil)
	phone.Prompt(id, "fail "+failure)
	phone.WaitAll("the failed turn",
		itemOf(id, func(it acpItem) bool { return it.Kind == "result" && !it.OK && it.Error == failure }),
		func(m Message) bool {
			return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.Status == "done" && m.Agent.Failed
		},
	)

	phone.Prompt(id, "next")
	phone.WaitAll("the next turn", streamOf(id, "assistant", "echo: next"), func(m Message) bool {
		return statusOf(id, "done")(m) && !m.Agent.Failed
	})
}

func TestAnErrorPiTriesAgainAfterIsANoticeNotAFailedTurn(t *testing.T) {
	_, phone, _, id := chatPi(t, "pi", nil)
	phone.Prompt(id, "retry twice")
	phone.WaitFor("the turn", func(m Message) bool {
		if m.Type == "agent.update" && m.Agent.ID == id && m.Agent.Failed {
			t.Fatalf("the turn failed: %s", m.Raw)
		}
		return statusOf(id, "done")(m)
	})
	want := []string{"user retry twice", "notice Pi is trying again (1/3): 529 overloaded", "assistant echo: twice", "result"}
	if got := history(phone, id); strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("got %q", got)
	}
}

func TestAStoppedChatPiTurnEndsIdleAndTheNextPromptIsAnswered(t *testing.T) {
	h, phone, term, id := chatPi(t, "pi", nil)
	phone.Prompt(id, "slow never")
	phone.WaitStatus(id, "working")
	phone.Send(map[string]any{"type": "agent.interrupt", "id": "i1", "agentId": id})
	phone.WaitAll("the stop",
		acked("i1"),
		statusOf(id, "idle"),
		itemOf(id, func(it acpItem) bool { return it.Kind == "result" && it.Error == "interrupted" }),
	)
	h.WaitScreen(term, "abort")

	phone.Prompt(id, "after the stop")
	phone.WaitAll("the next turn", streamOf(id, "assistant", "echo: after the stop"), statusOf(id, "done"))
	time.Sleep(3 * time.Second)
	if strings.Contains(h.Screen(term), "echo: never") {
		t.Fatal("the stopped turn went on")
	}
}

func TestAPromptSentMidTurnSteersTheChatPi(t *testing.T) {
	h, phone, term, id := chatPi(t, "pi", nil)
	phone.Prompt(id, "slow first")
	phone.WaitStatus(id, "working")
	phone.Prompt(id, "second")
	h.WaitScreen(term, "steer: second")
	phone.WaitAll("both answers in the one turn",
		streamOf(id, "assistant", "echo: first"),
		streamOf(id, "user", "second"),
		streamOf(id, "assistant", "echo: second"),
		statusOf(id, "done"),
	)
	want := []string{"user slow first", "assistant echo: first", "user second", "assistant echo: second", "result"}
	if got := history(phone, id); strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("got %q", got)
	}
	if strings.Contains(h.Screen(term), "busy") {
		t.Fatalf("pi refused the second prompt:\n%s", h.Screen(term))
	}
}

func TestAChatPiCompactsItsContext(t *testing.T) {
	h, phone, term, id := chatPi(t, "pi", nil)
	phone.Send(map[string]any{"type": "agent.compact", "id": "c1", "agentId": id})
	phone.WaitAll("pi's reason it can't",
		acked("c1"),
		streamOf(id, "notice", "Pi couldn't compact: Nothing to compact (session too small)"),
	)

	phone.Prompt(id, "hello")
	phone.WaitStream(id, "assistant", "echo: hello")
	phone.WaitStatus(id, "done")
	phone.Send(map[string]any{"type": "agent.compact", "id": "c2", "agentId": id})
	compacting := false
	phone.WaitAll("the compaction",
		acked("c2"),
		func(m Message) bool {
			compacting = compacting || statusOf(id, "working")(m) && strings.Contains(m.Raw, `"compacting":true`)
			return compacting
		},
		itemOf(id, func(it acpItem) bool { return it.Kind == "compact" }),
		func(m Message) bool {
			return compacting && statusOf(id, "done")(m) && !strings.Contains(m.Raw, `"compacting":true`) && strings.Contains(m.Raw, `"tokensUsed":300`)
		},
	)
	h.WaitScreen(term, "compact")
}

func TestAChatPiSwitchesModelAndThinkingForItsNextTurn(t *testing.T) {
	h, phone, term, id := chatPi(t, "pi", nil)
	phone.Send(map[string]any{"type": "agent.settings", "id": "s1", "agentId": id, "model": "fake/other", "effort": "high"})
	phone.WaitAll("the switch",
		acked("s1"),
		func(m Message) bool {
			return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.Model == "fake/other" && m.Agent.Effort == "high"
		},
	)
	h.WaitScreen(term, "set_model: fake/other")
	h.WaitScreen(term, "set_thinking_level: high")
	phone.Prompt(id, "after the switch")
	phone.WaitAll("the turn on the new model", streamOf(id, "assistant", "echo: after the switch"), statusOf(id, "done"))

	for model, reason := range map[string]string{"fake/bogus": "Model not found: fake/bogus", "bogus": "Pi names a model as provider/id"} {
		phone.Send(map[string]any{"type": "agent.settings", "id": "s-" + model, "agentId": id, "model": model})
		refused := phone.WaitFor("the refusal", func(m Message) bool { return m.Type == "error" && m.ID == "s-"+model })
		if !strings.Contains(refused.Raw, reason) {
			t.Fatalf("got %s", refused.Raw)
		}
	}
}

// pi asks before no tool, so there is no access to launch it on but full,
// and none to switch it to.
func TestAChatPiRunsOnFullAccessWhateverItIsLaunchedOn(t *testing.T) {
	h := Start(t, launchReady(""))
	phone := h.Owner()
	create(phone, "r1", inRepo(h, map[string]any{"provider": "pi", "chat": true, "access": "ask"}))
	var id string
	phone.WaitFor("the agent on full access", func(m Message) bool {
		if m.Type == "agent.created" {
			id = m.AgentID
		}
		return m.Type == "agent.update" && m.Agent.ChatOnly && strings.Contains(m.Raw, `"access":"full"`)
	})
	phone.WaitFor("the agent created", func(m Message) bool {
		if m.Type == "agent.created" {
			id = m.AgentID
		}
		return id != ""
	})
	phone.Send(map[string]any{"type": "agent.settings", "id": "s1", "agentId": id, "access": "ask"})
	refused := phone.WaitFor("the refusal", func(m Message) bool { return m.Type == "error" && m.ID == "s1" })
	if !strings.Contains(refused.Raw, "Pi runs every tool without asking, so its access is always full") {
		t.Fatalf("got %s", refused.Raw)
	}
}

// access asks for the agent's access to switch, and returns pocketd's answer.
func access(phone *Phone, id, msgID, to string) Message {
	phone.Send(map[string]any{"type": "agent.settings", "id": msgID, "agentId": id, "access": to})
	return phone.WaitFor("the answer", func(m Message) bool { return (m.Type == "error" || m.Type == "ack") && m.ID == msgID })
}

func TestAChatPiStartedPlanningLeavesPlanByRestartingOnItsFullToolsInTheSameSession(t *testing.T) {
	for _, c := range []struct{ provider, tools, resume string }{
		{"pi", "--tools read,grep,find,ls", "--session-id"},
		{"omp", "--tools read,grep,glob,lsp", "--resume"},
	} {
		t.Run(c.provider, func(t *testing.T) {
			h, phone, term, id := chatPi(t, c.provider, map[string]any{"plan": true}, func(h *Harness) { h.resume = true })
			h.WaitScreen(term, "fake "+c.provider+" ready --mode rpc "+c.tools)
			phone.Prompt(id, "plan it")
			session := phone.WaitFor("the planned turn", statusOf(id, "done")).Agent.ProviderSessionID

			if got := access(phone, id, "s1", "full"); got.Type != "ack" {
				t.Fatalf("got %s", got.Raw)
			}

			resumed := "fake " + c.provider + " ready " + c.resume + " " + session + " --mode rpc"
			h.WaitScreen(term, resumed)
			if screen := h.Screen(term); strings.Contains(screen, "--tools") {
				t.Fatalf("%s restarted planning:\n%s", c.provider, screen)
			}
			// It refuses prompts until its new process is on the session.
			for try := 0; ; try++ {
				phone.Send(map[string]any{"type": "agent.prompt", "id": fmt.Sprint("p", try), "agentId": id, "text": "build it"})
				got := phone.WaitFor("the prompt taken", func(m Message) bool { return (m.Type == "error" || m.Type == "ack") && m.ID == fmt.Sprint("p", try) })
				if got.Type == "ack" {
					break
				}
				if got.Code != "agent_resuming" || try == 50 {
					t.Fatalf("got %s", got.Raw)
				}
				time.Sleep(100 * time.Millisecond)
			}
			phone.WaitAll("the turn on its full tools", streamOf(id, "assistant", "echo: build it"), statusOf(id, "done"))
			want := []string{"user plan it", "assistant echo: plan it", "result", "user build it", "assistant echo: build it", "result"}
			if got := history(phone, id); strings.Join(got, "|") != strings.Join(want, "|") {
				t.Fatalf("got %q", got)
			}
			if strings.Contains(h.Screen(term), "get_messages") {
				t.Fatalf("the restarted %s's history was read again into the timeline it kept", c.provider)
			}

			// It has left plan for good: the pocketd after this one resumes it on its full tools too.
			h.Restart()

			h.WaitScreen(term, "get_messages")
			if screen := h.Screen(term); !strings.Contains(screen, resumed) || strings.Contains(screen, "--tools") {
				t.Fatalf("%s came back planning:\n%s", c.provider, screen)
			}
		})
	}
}

func TestAChatPiLeavesPlanOnlyBetweenTurnsOfASessionItHas(t *testing.T) {
	h, phone, term, id := chatPi(t, "pi", map[string]any{"plan": true}, func(h *Harness) { h.resume = true })
	unsaved := "Pi has no session to resume yet, so start a new session to build"
	if got := access(phone, id, "s1", "full"); !strings.Contains(got.Raw, unsaved) {
		t.Fatalf("got %s", got.Raw)
	}
	// pi saves no session for what it answers without a turn, though the chat shows it.
	phone.Send(map[string]any{"type": "agent.compact", "id": "c1", "agentId": id})
	phone.WaitAll("pi's reason it can't", acked("c1"), streamOf(id, "notice", "Pi couldn't compact: Nothing to compact (session too small)"))
	phone.Prompt(id, "/reload")
	phone.WaitAll("the command, which runs no agent", streamOf(id, "user", "/reload"), statusOf(id, "done"))
	if got := access(phone, id, "s2", "full"); !strings.Contains(got.Raw, unsaved) {
		t.Fatalf("got %s", got.Raw)
	}
	phone.Prompt(id, "slow one")
	phone.WaitStatus(id, "working")
	if got := access(phone, id, "s3", "full"); !strings.Contains(got.Raw, "Pi can leave plan once its turn ends") {
		t.Fatalf("got %s", got.Raw)
	}
	if got := access(phone, id, "s4", "ask"); !strings.Contains(got.Raw, "Pi runs every tool without asking, so its access is always full") {
		t.Fatalf("got %s", got.Raw)
	}
	phone.WaitStatus(id, "done")
	if n := strings.Count(h.Screen(term), "fake pi ready"); n != 1 {
		t.Fatalf("pi started %d times", n)
	}
}

func TestAChatPiThatCantBeResumedStaysPlanningAndSaysWhy(t *testing.T) {
	stays := func(t *testing.T, why string, resume bool, breakIt func(*Harness)) {
		h, phone, term, id := chatPi(t, "pi", map[string]any{"plan": true}, func(h *Harness) { h.resume = resume })
		phone.Prompt(id, "plan it")
		phone.WaitStatus(id, "done")
		breakIt(h)
		if got := access(phone, id, "s1", "full"); !strings.Contains(got.Raw, why) {
			t.Fatalf("got %s", got.Raw)
		}
		phone.Prompt(id, "still planning")
		phone.WaitAll("the next turn", streamOf(id, "assistant", "echo: still planning"), statusOf(id, "done"))
		if n := strings.Count(h.Screen(term), "fake pi ready"); n != 1 {
			t.Fatalf("pi started %d times", n)
		}
	}
	t.Run("resume off", func(t *testing.T) {
		stays(t, "Pi leaves plan by resuming its session, which Settings has off", false, func(*Harness) {})
	})
	t.Run("its command gone", func(t *testing.T) {
		stays(t, "Pi couldn't leave plan", true, func(h *Harness) {
			gone := filepath.Join(h.Home, "pi-gone")
			if err := os.WriteFile(gone, []byte("#!/bin/sh\n"), 0o755); err != nil {
				t.Fatal(err)
			}
			if out, code := h.Pocketd("config", "set", "pi.command", gone); code != 0 {
				t.Fatalf("exit %d: %q", code, out)
			}
			os.Remove(gone)
		})
	})
}

func TestAnExtensionsDialogAsksTheChatPi(t *testing.T) {
	h, phone, term, id := chatPi(t, "pi", nil)
	phone.Prompt(id, "confirm Clear the session?")
	var req Message
	phone.WaitAll("the question", statusOf(id, "needsYou"), func(m Message) bool {
		if asked(m) {
			req = m
		}
		return asked(m)
	})
	if req.Request.AgentID != id || req.Request.ToolName != "Clear the session?" || !strings.Contains(req.Raw, `"input":{"message":"It can't be undone."}`) {
		t.Fatalf("got %s", req.Raw)
	}
	resolve(phone, req, map[string]any{"decision": "deny"})
	phone.WaitAll("the turn", statusOf(id, "working"), streamOf(id, "assistant", "echo: no"), statusOf(id, "done"))

	phone.Prompt(id, "select red, green")
	req = phone.WaitFor("the choice", asked)
	if req.Request.ToolName != "AskUserQuestion" || !strings.Contains(req.Raw, `{"multiSelect":false,"options":[{"description":"","label":"red"},{"description":"","label":"green"}],"question":"Pick one"}`) {
		t.Fatalf("got %s", req.Raw)
	}
	resolve(phone, req, map[string]any{"decision": "allow", "answers": map[string]string{"Pick one": "green"}})
	phone.WaitAll("the turn", streamOf(id, "assistant", "echo: picked green"), statusOf(id, "done"))

	// A stop answers the dialog cancelled, so pi's extension stops waiting.
	phone.Prompt(id, "confirm Go on?")
	req = phone.WaitFor("the question", asked)
	phone.Send(map[string]any{"type": "agent.interrupt", "id": "i1", "agentId": id})
	phone.WaitAll("the stop",
		func(m Message) bool { return m.Type == "permission.resolved" && m.RequestID == req.Request.RequestID },
		statusOf(id, "idle"),
	)
	h.WaitScreen(term, "abort")
}

func TestAChatPiListsItsSkillsAndRunsACommandOfItsOwnAsATurn(t *testing.T) {
	h, phone, term, id := chatPi(t, "pi", nil)
	phone.Send(map[string]any{"type": "agent.commands", "id": "c1", "agentId": id})
	got := phone.WaitFor("the commands", func(m Message) bool { return m.Type == "agent.commands" && m.ID == "c1" })
	if !strings.Contains(got.Raw, `"commands":[{"name":"skill:review","description":"Review the changes"}]`) {
		t.Fatalf("got %s", got.Raw)
	}

	phone.Prompt(id, "/reload")
	phone.WaitAll("the command, which runs no agent", streamOf(id, "user", "/reload"), statusOf(id, "done"))
	h.WaitScreen(term, "command: /reload")
}

func TestARestartResumesTheChatPiSessionAndShowsItsHistory(t *testing.T) {
	h, phone, term, id := chatPi(t, "pi", nil, func(h *Harness) { h.resume = true })
	phone.Prompt(id, "run ls")
	phone.WaitStatus(id, "done")
	phone.Prompt(id, "fail No credits left")
	session := phone.WaitFor("the failed turn", func(m Message) bool { return statusOf(id, "done")(m) && m.Agent.Failed }).Agent.ProviderSessionID

	h.Restart()

	again := h.Owner()
	again.WaitFor("the agent resumed for the chat", func(m Message) bool {
		if m.Type == "agent.list" && len(m.Agents) == 1 {
			return m.Agents[0].ID == id && m.Agents[0].Restore == "resumed" && m.Agents[0].ChatOnly
		}
		return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.Restore == "resumed" && m.Agent.ChatOnly
	})
	h.WaitScreen(term, "fake pi ready --session-id "+session+" --mode rpc")
	h.WaitScreen(term, "get_messages")
	// The history may be whole before this phone connects, so its stream is not waited on.
	want := []string{"user run ls", "tool ls ok", "assistant echo: ran ls", "result", "user fail No credits left", "result No credits left"}
	var got []string
	for range 50 {
		if got = historyUnless(again, id, statusOf(id, "working")); len(got) == len(want) {
			break
		}
		time.Sleep(100 * time.Millisecond)
	}
	if strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("got %q", got)
	}

	again.Prompt(id, "after the restart")
	again.WaitAll("the next turn", streamOf(id, "assistant", "echo: after the restart"), statusOf(id, "done"))
}

// pi writes no session before its first prompt, so the restart has it create the one it names.
func TestARestartBeforeTheFirstPromptBringsTheChatPiBackOnTheSameSession(t *testing.T) {
	h, _, term, id := chatPi(t, "pi", nil, func(h *Harness) { h.resume = true })
	_, session, _ := strings.Cut(h.Screen(term), "session: ")
	session, _, _ = strings.Cut(session, "\n")
	session = strings.TrimSpace(session)

	h.Restart()

	again := h.Owner()
	h.WaitScreen(term, "fake pi ready --session-id "+session+" --mode rpc")
	again.Prompt(id, "after the restart")
	again.WaitAll("the first turn", streamOf(id, "assistant", "echo: after the restart"), statusOf(id, "done"))
}

func TestARestartMidTurnLeavesTheChatPiTurnInterrupted(t *testing.T) {
	h, phone, term, id := chatPi(t, "pi", nil, func(h *Harness) { h.resume = true })
	phone.Prompt(id, "slow one")
	phone.WaitStream(id, "user", "slow one")

	h.Restart()

	again := h.Owner()
	h.WaitScreen(term, "get_messages")
	want := []string{"user slow one", "result interrupted"}
	var got []string
	for range 50 {
		if got = history(again, id); len(got) == len(want) {
			break
		}
		time.Sleep(100 * time.Millisecond)
	}
	if strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("got %q", got)
	}
	again.Prompt(id, "after the restart")
	again.WaitAll("the next turn", streamOf(id, "assistant", "echo: after the restart"), statusOf(id, "done"))
}

func TestAChatPiThatExitsAsItStartsShowsWhatItSaidAndThePromptItNeverGot(t *testing.T) {
	h := Start(t, launchReady(""), func(h *Harness) {
		os.MkdirAll(filepath.Join(h.Home, ".pi-fake"), 0o700)
		os.WriteFile(filepath.Join(h.Home, ".pi-fake", "start"), []byte("exit No API key found"), 0o600)
	})
	phone := h.Owner()
	create(phone, "r1", inRepo(h, map[string]any{"provider": "pi", "chat": true, "prompt": "do the important thing"}))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	phone.WaitAll("the failure",
		streamOf(done.AgentID, "user", "do the important thing"),
		itemOf(done.AgentID, func(it acpItem) bool {
			return it.Kind == "result" && !it.OK && strings.HasPrefix(it.Error, "Pi didn't start, so your prompt was not sent: it exited") && strings.HasSuffix(it.Error, ": No API key found")
		}),
		func(m Message) bool { return statusOf(done.AgentID, "done")(m) && m.Agent.Failed },
	)
}

func TestAChatOmpEndsTheTurnOfACommandThatRanNoAgentOrFailedAfterItWasTaken(t *testing.T) {
	_, phone, _, id := chatPi(t, "omp", nil)
	phone.Prompt(id, "/reviewer")
	phone.WaitAll("the command that ran no agent", streamOf(id, "notice", "Selected reviewer: careful"), streamOf(id, "user", "/reviewer"), statusOf(id, "done"))
	phone.Prompt(id, "/workflow")
	phone.WaitAll("the command that failed late",
		itemOf(id, func(it acpItem) bool { return it.Kind == "result" && it.Error == "Workflow failed" }),
		func(m Message) bool { return statusOf(id, "done")(m) && m.Agent.Failed },
	)
}

func TestAnAutomationAndATicketOfPiRunAsAChatAndReachTheirEnd(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapAutomations, proto.CapBoard)
	au := draft(h, "review the diff")
	au["provider"] = "pi"
	if a := o.Automation("save", "s1", map[string]any{"automation": au}); a.Type != "ack" {
		t.Fatalf("got %s", a.Raw)
	}
	id := o.WaitFor("the saved automation", func(m Message) bool { return m.Type == "automations" && len(m.Automations) == 1 }).Automations[0].ID
	if a := o.Automation("run", "r1", map[string]any{"automationId": id}); a.Type != "ack" {
		t.Fatalf("got %s", a.Raw)
	}
	if run := o.WaitRun("a succeeded run", func(r proto.Run) bool { return r.Status == "succeeded" }); run.Summary != "echo: review the diff" {
		t.Fatalf("got %+v", run)
	}

	ticket := ticketDraft(h, "review the diff")
	ticket["provider"] = "pi"
	if a := o.Board("save", "s2", map[string]any{"ticket": ticket, "start": true}); a.Type != "ack" {
		t.Fatalf("got %s", a.Raw)
	}
	done := o.WaitTicket("a finished ticket", func(x proto.Ticket) bool { return x.Status == proto.TicketDone || x.Status == proto.TicketIdle })
	h.WaitScreen(done.TerminalID, "fake pi ready --mode rpc")
}

func TestAPhoneStartsPiOnlyWithTheFullAccessItRunsWith(t *testing.T) {
	h := Start(t, launchReady(""))
	p := h.Paired("launch.v1")
	create(p, "r1", inRepo(h, map[string]any{"provider": "pi", "chat": true, "access": "ask"}))
	if e := reply(p, "agent.created", "error"); e.Type != "error" || e.Code != "access_not_allowed" || e.Message != "Pi runs every tool without asking. Start it with Full access." {
		t.Fatalf("got %s", e.Raw)
	}
	create(p, "r2", inRepo(h, map[string]any{"provider": "pi", "chat": true, "access": "full"}))
	if done := reply(p, "agent.created", "error"); done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
}

func TestAnUpgradeKeepsTheChatPiAndTheTurnItWasIn(t *testing.T) {
	h, phone, term, id := chatPi(t, "pi", nil)
	phone.Prompt(id, "hello")
	phone.WaitStatus(id, "done")
	phone.Prompt(id, "slow one")
	phone.WaitStatus(id, "working")

	h.Upgrade()

	again := h.Owner()
	again.WaitAll("the turn the old pocketd started", streamOf(id, "assistant", "echo: one"), statusOf(id, "done"))
	want := []string{"user hello", "assistant echo: hello", "result", "user slow one", "assistant echo: one", "result"}
	if got := history(again, id); strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("got %q", got)
	}
	again.Prompt(id, "still here")
	again.WaitStream(id, "assistant", "echo: still here")
	if n := strings.Count(h.Screen(term), "fake pi ready"); n != 1 {
		t.Fatalf("pi started %d times", n)
	}
}

func TestAChatOmpIsDrivenAsPiIsWithItsOwnFlagsAndEvents(t *testing.T) {
	h, phone, term, id := chatPi(t, "omp", map[string]any{"effort": "high"}, func(h *Harness) { h.resume = true })
	// omp has no flag for its thinking level, so it is set once it is up.
	h.WaitScreen(term, "set_thinking_level: high")
	phone.Prompt(id, "hello")
	session := phone.WaitFor("the turn", statusOf(id, "done")).Agent.ProviderSessionID

	// omp tells that a command of its own ran no agent after it took the prompt.
	phone.Prompt(id, "/reload")
	phone.WaitAll("the command", streamOf(id, "user", "/reload"), statusOf(id, "done"))

	phone.Send(map[string]any{"type": "agent.commands", "id": "c1", "agentId": id})
	got := phone.WaitFor("the commands", func(m Message) bool { return m.Type == "agent.commands" && m.ID == "c1" })
	if !strings.Contains(got.Raw, `{"name":"plan","description":"Plan before building","argumentHint":"goal"}`) {
		t.Fatalf("got %s", got.Raw)
	}

	h.Restart()

	again := h.Owner()
	h.WaitScreen(term, "fake omp ready --resume "+session+" --mode rpc")
	h.WaitScreen(term, "get_messages")
	replied := func(item string) bool { return strings.Contains(item, "echo: hello") }
	var items []string
	for range 50 {
		if items = history(again, id); slices.ContainsFunc(items, replied) {
			break
		}
		time.Sleep(100 * time.Millisecond)
	}
	if !slices.ContainsFunc(items, replied) {
		t.Fatalf("got %q", items)
	}
	again.Prompt(id, "after the restart")
	again.WaitAll("the next turn", streamOf(id, "assistant", "echo: after the restart"), statusOf(id, "done"))
}

func TestAChatOmpSwitchesFastForItsNextTurn(t *testing.T) {
	h, phone, term, id := chatPi(t, "omp", nil)
	phone.Send(map[string]any{"type": "agent.settings", "id": "s1", "agentId": id, "options": map[string]string{"fast": "true"}})
	phone.WaitAll("the switch",
		acked("s1"),
		func(m Message) bool {
			return m.Type == "agent.update" && m.Agent.ID == id && strings.Contains(m.Raw, `"options":{"fast":"true"}`)
		},
	)
	h.WaitScreen(term, "set_fast_mode: true")
	phone.Prompt(id, "after the switch")
	phone.WaitAll("the turn on fast", streamOf(id, "assistant", "echo: after the switch"), statusOf(id, "done"))

	phone.Send(map[string]any{"type": "agent.settings", "id": "s2", "agentId": id, "options": map[string]string{"context": "1m"}})
	refused := phone.WaitFor("the refusal", func(m Message) bool { return m.Type == "error" && m.ID == "s2" })
	if !strings.Contains(refused.Raw, "omp has no setting context") {
		t.Fatalf("got %s", refused.Raw)
	}
}
