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
	"pocketd/internal/providers"
)

// buildFakeACP builds the fake ACP agent in dir under each command an ACP
// provider's chat runs as.
func buildFakeACP(dir string) {
	var names []string
	for _, p := range providers.All {
		if p.Transport == providers.ACP {
			names = append(names, p.Cmd())
		}
	}
	if out, err := exec.Command("go", "build", "-o", filepath.Join(dir, names[0]), "./fakeacp").CombinedOutput(); err != nil {
		fmt.Fprintf(os.Stderr, "build ./fakeacp: %v\n%s", err, out)
		os.Exit(1)
	}
	for _, name := range names[1:] {
		if err := os.Symlink(names[0], filepath.Join(dir, name)); err != nil {
			panic(err)
		}
	}
}

// chatACP creates provider's agent through Pocket, which runs its ACP server
// for the chat behind `pocketd pipe`, and waits until its session is open.
func chatACP(t *testing.T, provider string, extra map[string]any, opts ...func(*Harness)) (h *Harness, phone *Phone, term, agent string) {
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
	h.WaitScreen(term, "fake "+providers.Get(provider).Cmd()+" ready")
	h.WaitScreen(term, "session: ")
	return h, phone, term, agent
}

// acpItem is a timeline item with what the ACP tests read of its tool call.
type acpItem struct {
	Kind  string `json:"kind"`
	Text  string `json:"text"`
	OK    bool   `json:"ok"`
	Error string `json:"error"`
	Tasks []struct {
		Text, Status string
	} `json:"items"`
	Call struct {
		Name   string `json:"name"`
		Status string `json:"status"`
		Output string `json:"output"`
		Detail struct {
			Kind    string `json:"kind"`
			Command string `json:"command"`
		} `json:"detail"`
	} `json:"call"`
}

// itemOf matches an agent.stream of id whose item ok takes.
func itemOf(id string, ok func(acpItem) bool) func(Message) bool {
	return func(m Message) bool {
		var s struct {
			Item acpItem `json:"item"`
		}
		return m.Type == "agent.stream" && m.AgentID == id && json.Unmarshal([]byte(m.Raw), &s) == nil && ok(s.Item)
	}
}

func shellCall(status, command string) func(acpItem) bool {
	return func(it acpItem) bool {
		return it.Kind == "tool" && it.Call.Status == status && it.Call.Detail.Kind == "shell" && it.Call.Detail.Command == command
	}
}

func TestAChatHermesStreamsItsReplyAndEndsItsTurn(t *testing.T) {
	_, phone, _, id := chatACP(t, "hermes", nil)
	phone.Prompt(id, "hello")
	working := false
	phone.WaitAll("the turn",
		func(m Message) bool { return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.ChatOnly },
		streamOf(id, "user", "hello"),
		func(m Message) bool {
			working = working || statusOf(id, "working")(m)
			return working
		},
		// The reply drafts a chunk at a time before it is whole.
		streamOf(id, "assistant", "echo: "),
		streamOf(id, "assistant", "echo: hello"),
		itemOf(id, func(it acpItem) bool { return it.Kind == "result" && it.OK }),
		func(m Message) bool { return working && statusOf(id, "done")(m) },
		func(m Message) bool { return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.Title == "hello" },
	)
}

func TestACreatedChatHermesGetsItsPromptAsTheFirstMessage(t *testing.T) {
	h := Start(t, launchReady(""))
	phone := h.Owner()
	create(phone, "r1", inRepo(h, map[string]any{"provider": "hermes", "prompt": "hello", "chat": true}))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	phone.WaitAll("the first turn", streamOf(done.AgentID, "assistant", "echo: hello"), statusOf(done.AgentID, "done"))
}

func TestAChatHermesRunsWhatTheChatAllows(t *testing.T) {
	h, phone, term, id := chatACP(t, "hermes", nil)
	phone.Prompt(id, "run ls")
	var req Message
	phone.WaitAll("the call asked about", itemOf(id, shellCall("running", "ls")), statusOf(id, "needsYou"), func(m Message) bool {
		if asked(m) {
			req = m
		}
		return asked(m)
	})
	if req.Request.AgentID != id || req.Request.ToolName != "shell" || req.Request.Detail.Command != "ls" {
		t.Fatalf("got %s", req.Raw)
	}
	resolve(phone, req, map[string]any{"decision": "allow"})
	phone.WaitAll("the turn",
		itemOf(id, func(it acpItem) bool { return shellCall("ok", "ls")(it) && it.Call.Output == "ran: ls" }),
		streamOf(id, "assistant", "echo: ran ls"),
		statusOf(id, "done"),
	)
	h.WaitScreen(term, "permission: allow_once")
}

func TestAChatHermesAllowsForTheSessionWhenTheChatPicksThatOption(t *testing.T) {
	h, phone, term, id := chatACP(t, "hermes", nil)
	phone.Prompt(id, "run ls")
	req := phone.WaitFor("permission.request", asked)
	if !strings.Contains(req.Raw, `"options":[{"id":"allow_session","label":"Allow for session"},{"id":"allow_always","label":"Allow always"}]`) {
		t.Fatalf("got %s", req.Raw)
	}
	resolve(phone, req, map[string]any{"decision": "allow", "option": "allow_always"})
	h.WaitScreen(term, "permission: allow_always")
	phone.WaitStatus(id, "done")
}

func TestAChatHermesSkipsWhatTheChatDeniesAndGoesOn(t *testing.T) {
	h, phone, term, id := chatACP(t, "hermes", nil)
	phone.Prompt(id, "run rm -rf build")
	resolve(phone, phone.WaitFor("permission.request", asked), map[string]any{"decision": "deny"})
	phone.WaitAll("the turn",
		itemOf(id, shellCall("error", "rm -rf build")),
		streamOf(id, "assistant", "echo: skipped rm -rf build"),
		statusOf(id, "done"),
	)
	h.WaitScreen(term, "permission: deny")

	phone.Prompt(id, "next")
	phone.WaitStream(id, "assistant", "echo: next")
}

func TestAChatHermesOnFullAccessIsNotAsked(t *testing.T) {
	h, phone, term, id := chatACP(t, "hermes", map[string]any{"access": "full"})
	h.WaitScreen(term, "set_mode: dont_ask")
	phone.Prompt(id, "run ls")
	phone.WaitFor("the turn", func(m Message) bool {
		if asked(m) {
			t.Fatalf("asked %s", m.Raw)
		}
		return statusOf(id, "done")(m)
	})
	// Never an "always": Hermes would keep it past this session's full access.
	h.WaitScreen(term, "permission: allow_once")
}

func TestAnAccessSwitchChangesHermesModeAndWhatTheChatIsAsked(t *testing.T) {
	h, phone, term, id := chatACP(t, "hermes", nil)
	h.WaitScreen(term, "set_mode: default")
	phone.Send(map[string]any{"type": "agent.settings", "id": "s1", "agentId": id, "access": "auto"})
	phone.WaitFor("the switch", func(m Message) bool { return m.Type == "ack" && m.ID == "s1" })
	h.WaitScreen(term, "set_mode: dont_ask")
	phone.Prompt(id, "run ls")
	phone.WaitStatus(id, "done")
	h.WaitScreen(term, "permission: allow_once")
}

func TestAStoppedChatHermesTurnEndsIdleAndTheNextPromptIsAnswered(t *testing.T) {
	h, phone, term, id := chatACP(t, "hermes", nil)
	phone.Prompt(id, "slow never")
	phone.WaitStatus(id, "working")
	phone.Send(map[string]any{"type": "agent.interrupt", "id": "i1", "agentId": id})
	phone.WaitAll("the stop",
		func(m Message) bool { return m.Type == "ack" && m.ID == "i1" },
		statusOf(id, "idle"),
		itemOf(id, func(it acpItem) bool { return it.Kind == "result" && it.Error == "interrupted" }),
	)
	h.WaitScreen(term, "cancel")

	phone.Prompt(id, "after the stop")
	phone.WaitAll("the next turn", streamOf(id, "assistant", "echo: after the stop"), statusOf(id, "done"))
	time.Sleep(3 * time.Second)
	if strings.Contains(h.Screen(term), "echo: never") {
		t.Fatal("the stopped turn went on")
	}
}

func TestAStopAnswersHermesOpenPermissionCancelled(t *testing.T) {
	h, phone, term, id := chatACP(t, "hermes", nil)
	phone.Prompt(id, "run ls")
	req := phone.WaitFor("permission.request", asked)
	phone.Send(map[string]any{"type": "agent.interrupt", "id": "i1", "agentId": id})
	phone.WaitAll("the stop",
		func(m Message) bool { return m.Type == "permission.resolved" && m.RequestID == req.Request.RequestID },
		statusOf(id, "idle"),
	)
	h.WaitScreen(term, "permission: cancelled")
}

func TestAPromptSentMidTurnWaitsForTheChatHermesTurnToEnd(t *testing.T) {
	h, phone, term, id := chatACP(t, "hermes", nil)
	phone.Prompt(id, "slow first")
	phone.WaitStatus(id, "working")
	phone.Prompt(id, "second")
	ends := 0
	phone.WaitAll("both turns, one after the other",
		streamOf(id, "assistant", "echo: first"),
		streamOf(id, "user", "second"),
		streamOf(id, "assistant", "echo: second"),
		func(m Message) bool {
			if statusOf(id, "done")(m) {
				ends++
			}
			return ends == 2
		},
	)
	if screen := h.Screen(term); strings.Contains(screen, "busy") || strings.Index(screen, "echo: first") > strings.Index(screen, "prompt: second") {
		t.Fatalf("the second prompt reached hermes mid-turn:\n%s", screen)
	}
}

func TestAChatHermesSwitchesModelForItsNextTurn(t *testing.T) {
	h, phone, term, id := chatACP(t, "hermes", nil)
	phone.Prompt(id, "before")
	phone.WaitFor("a turn on the model hermes starts on", func(m Message) bool {
		return statusOf(id, "done")(m) && m.Agent.Model == "fake-model"
	})
	phone.Send(map[string]any{"type": "agent.settings", "id": "s1", "agentId": id, "model": "fake/other"})
	phone.WaitAll("the switch",
		func(m Message) bool { return m.Type == "ack" && m.ID == "s1" },
		func(m Message) bool {
			return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.Model == "fake/other"
		},
	)
	h.WaitScreen(term, "set_model: fake/other")
	phone.Prompt(id, "after the switch")
	phone.WaitAll("the turn on the new model", streamOf(id, "assistant", "echo: after the switch"), statusOf(id, "done"))
}

func TestAModelHermesDoesNotHaveIsRefusedWithItsReason(t *testing.T) {
	_, phone, _, id := chatACP(t, "hermes", nil)
	phone.Send(map[string]any{"type": "agent.settings", "id": "s1", "agentId": id, "model": "bogus"})
	refused := phone.WaitFor("the refusal", func(m Message) bool { return m.Type == "error" && m.ID == "s1" })
	if !strings.Contains(refused.Raw, "model_resolution_failed") {
		t.Fatalf("got %s", refused.Raw)
	}
}

func TestAChatHermesTurnThatFailsEndsFailedWithItsMessage(t *testing.T) {
	_, phone, _, id := chatACP(t, "hermes", nil)
	phone.Prompt(id, "fail No credits left")
	phone.WaitAll("the failed turn",
		itemOf(id, func(it acpItem) bool { return it.Kind == "result" && !it.OK && it.Error == "No credits left" }),
		func(m Message) bool { return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.Failed },
	)

	phone.Prompt(id, "stop refusal")
	phone.WaitFor("the turn the agent cut short", itemOf(id, func(it acpItem) bool {
		return it.Kind == "result" && !it.OK && it.Error == "Hermes Agent ended the turn (refusal)"
	}))
}

func TestAChatHermesShowsItsThinkingItsPlanAndItsCommands(t *testing.T) {
	_, phone, _, id := chatACP(t, "hermes", nil)
	phone.Prompt(id, "think twice")
	phone.WaitAll("the thinking", streamOf(id, "thinking", "thinking about twice"), streamOf(id, "assistant", "echo: twice"), statusOf(id, "done"))

	phone.Prompt(id, "plan read, write")
	phone.WaitAll("the plan",
		itemOf(id, func(it acpItem) bool {
			return it.Kind == "tasks" && len(it.Tasks) == 2 && it.Tasks[0].Text == "read" && it.Tasks[0].Status == "in_progress" && it.Tasks[1].Status == "pending"
		}),
		statusOf(id, "done"),
	)

	phone.Send(map[string]any{"type": "agent.commands", "id": "c1", "agentId": id})
	got := phone.WaitFor("the commands", func(m Message) bool { return m.Type == "agent.commands" && m.ID == "c1" })
	if !strings.Contains(got.Raw, `{"name":"model","description":"Switch models","argumentHint":"model name"}`) {
		t.Fatalf("got %s", got.Raw)
	}
}

func TestARestartLoadsTheChatHermesSessionAndShowsItsHistory(t *testing.T) {
	h, phone, term, id := chatACP(t, "hermes", nil, func(h *Harness) { h.resume = true })
	phone.Prompt(id, "run ls")
	resolve(phone, phone.WaitFor("permission.request", asked), map[string]any{"decision": "allow"})
	phone.WaitStatus(id, "done")

	h.Restart()

	again := h.Owner()
	again.WaitFor("the agent resumed for the chat", func(m Message) bool {
		if m.Type == "agent.list" && len(m.Agents) == 1 {
			return m.Agents[0].ID == id && m.Agents[0].Restore == "resumed" && m.Agents[0].ChatOnly
		}
		return m.Type == "agent.update" && m.Agent.ID == id && m.Agent.Restore == "resumed" && m.Agent.ChatOnly
	})
	h.WaitScreen(term, "load: ")
	again.Send(map[string]any{"type": "agent.timeline", "id": "t1", "agentId": id})
	history := again.WaitFor("the history", func(m Message) bool {
		if statusOf(id, "working")(m) {
			t.Fatal("its history made it work")
		}
		return m.Type == "agent.timeline" && m.ID == "t1"
	})
	var page struct {
		Items []acpItem `json:"items"`
	}
	json.Unmarshal([]byte(history.Raw), &page)
	var got []string
	for _, it := range page.Items {
		got = append(got, strings.TrimSpace(it.Kind+" "+it.Text+it.Call.Detail.Command+" "+it.Call.Status))
	}
	if want := []string{"user run ls", "tool ls ok", "assistant echo: ran ls", "result"}; strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("got %q", got)
	}

	again.Prompt(id, "after the restart")
	again.WaitAll("the next turn", streamOf(id, "assistant", "echo: after the restart"), statusOf(id, "done"))
}

// OpenCode tells a task's sub-agent only of its session loaded, live and on a restart alike.
func TestAChatOpenCodeShowsItsSubAgentsWorkUnderTheTaskAlsoAfterARestart(t *testing.T) {
	h, phone, _, id := chatACP(t, "opencode", nil, func(h *Harness) { h.resume = true })
	phone.Prompt(id, "delegate ls")
	phone.WaitAll("the turn", streamOf(id, "assistant", "echo: delegated ls"), statusOf(id, "done"))
	want := []string{"user delegate ls", "tool task ok did ls", "assistant echo: delegated ls", "result", "> user ls", "> assistant did ls"}
	settled := func(phone *Phone) []string {
		var got []string
		for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(50 * time.Millisecond) {
			// The sub-agent's session loads beside the turn, so its work lands wherever that is by then.
			lines := withSubAgents(phone, id)
			got = slices.DeleteFunc(slices.Clone(lines), func(l string) bool { return strings.HasPrefix(l, "> ") })
			got = append(got, slices.DeleteFunc(lines, func(l string) bool { return !strings.HasPrefix(l, "> ") })...)
			if strings.Join(got, "|") == strings.Join(want, "|") {
				break
			}
		}
		return got
	}
	if got := settled(h.Owner(proto.CapSubagents)); strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("got %q", got)
	}

	h.Restart()
	if got := settled(h.Owner(proto.CapSubagents)); strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("after the restart got %q", got)
	}
}

func TestAnUpgradeKeepsTheChatHermesAndReasksItsOpenPermission(t *testing.T) {
	h, phone, term, id := chatACP(t, "hermes", nil)
	phone.Prompt(id, "run ls")
	first := phone.WaitFor("permission.request", asked)

	h.Upgrade()

	again := h.Owner()
	req := again.WaitFor("the request asked again", func(m Message) bool {
		return asked(m) && m.Request.AgentID == id && m.Request.RequestID != first.Request.RequestID
	})
	resolve(again, req, map[string]any{"decision": "allow"})
	h.WaitScreen(term, "permission: allow_once")
	again.WaitAll("the turn the old pocketd started", streamOf(id, "assistant", "echo: ran ls"), statusOf(id, "done"))

	again.Prompt(id, "still here")
	again.WaitStream(id, "assistant", "echo: still here")
	if n := strings.Count(h.Screen(term), "fake hermes ready"); n != 1 {
		t.Fatalf("hermes started %d times", n)
	}
}

func TestAChatOpenCodeTakesItsModelAndPlanModeAsConfigOptions(t *testing.T) {
	h, phone, term, id := chatACP(t, "opencode", map[string]any{"model": "fake/other", "plan": true})
	h.WaitScreen(term, "config: mode=plan")
	h.WaitScreen(term, "config: model=fake/other")
	phone.Prompt(id, "hello")
	phone.WaitAll("the turn", streamOf(id, "assistant", "echo: hello"), statusOf(id, "done"))

	// Leaving plan mode is an access switch, as in claude.
	phone.Send(map[string]any{"type": "agent.settings", "id": "s1", "agentId": id, "access": "edits"})
	phone.WaitFor("the switch", func(m Message) bool { return m.Type == "ack" && m.ID == "s1" })
	h.WaitScreen(term, "config: mode=build")

	phone.Send(map[string]any{"type": "agent.settings", "id": "s2", "agentId": id, "model": "bogus"})
	refused := phone.WaitFor("the refusal", func(m Message) bool { return m.Type == "error" && m.ID == "s2" })
	if !strings.Contains(refused.Raw, "model not found: bogus") {
		t.Fatalf("got %s", refused.Raw)
	}
}

func TestAChatOpenCodeShowsACallOnceItsInputComesAndAsksAboutIt(t *testing.T) {
	h, phone, term, id := chatACP(t, "opencode", nil)
	phone.Prompt(id, "run ls")
	req := phone.WaitFor("the call, then the question", func(m Message) bool {
		if itemOf(id, func(it acpItem) bool { return it.Kind == "tool" && it.Call.Detail.Command != "ls" })(m) {
			t.Fatalf("the call showed before its command: %s", m.Raw)
		}
		return asked(m)
	})
	if req.Request.Detail.Command != "ls" {
		t.Fatalf("got %s", req.Raw)
	}
	resolve(phone, req, map[string]any{"decision": "allow"})
	phone.WaitAll("the turn", itemOf(id, shellCall("ok", "ls")), streamOf(id, "assistant", "echo: ran ls"), statusOf(id, "done"))
	h.WaitScreen(term, "permission: once")
}

func TestAChatFxOrHermesPlanningRunsNothingUntilTheChatLeavesThePlan(t *testing.T) {
	for provider, modes := range map[string][2]string{"fx": {"ask", "full-access"}, "hermes": {"default", "dont_ask"}} {
		t.Run(provider, func(t *testing.T) {
			h, phone, term, id := chatACP(t, provider, map[string]any{"plan": true, "access": "full"})
			h.WaitScreen(term, "set_mode: "+modes[0])
			phone.Prompt(id, "run ls")
			phone.WaitAll("the refused call", itemOf(id, shellCall("error", "ls")), streamOf(id, "assistant", "echo: skipped ls"), statusOf(id, "done"))
			h.WaitScreen(term, "permission: deny")

			phone.Send(map[string]any{"type": "agent.settings", "id": "s1", "agentId": id, "access": "full"})
			phone.WaitFor("the switch", func(m Message) bool { return m.Type == "ack" && m.ID == "s1" })
			h.WaitScreen(term, "set_mode: "+modes[1])
			phone.Prompt(id, "run pwd")
			phone.WaitAll("the call", itemOf(id, shellCall("ok", "pwd")), streamOf(id, "assistant", "echo: ran pwd"))
		})
	}
}

// Grok takes plan mode and full access only on its command line.
func TestAChatGrokLeavesPlanAndSwitchesFullAccessByRestartingOnItsSession(t *testing.T) {
	h, phone, term, id := chatACP(t, "grok", map[string]any{"plan": true}, func(h *Harness) { h.resume = true })
	h.WaitScreen(term, "--permission-mode plan agent --no-leader stdio")
	phone.Prompt(id, "plan it")
	phone.WaitStatus(id, "done")

	for i, c := range []struct{ access, argv string }{
		{"full", "fake grok ready --no-auto-update agent --no-leader --always-approve stdio"},
		{"ask", "fake grok ready --no-auto-update agent --no-leader stdio"},
	} {
		if got := access(phone, id, fmt.Sprint("s", i), c.access); got.Type != "ack" {
			t.Fatalf("to %s: got %s", c.access, got.Raw)
		}
		h.WaitScreen(term, c.argv)
		h.WaitScreen(term, "load: ")
	}
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
	phone.WaitAll("the turn on its session", streamOf(id, "assistant", "echo: build it"), statusOf(id, "done"))
	if got := timelineOf(phone, id); !slices.Contains(got, "user plan it") {
		t.Fatalf("the session was not the same: %q", got)
	}
}

func TestAChatGrokOrOpenCodeCompactsItsContext(t *testing.T) {
	for _, provider := range []string{"grok", "opencode"} {
		t.Run(provider, func(t *testing.T) {
			h, phone, term, id := chatACP(t, provider, nil)
			phone.Prompt(id, "hello")
			phone.WaitStatus(id, "done")
			phone.Send(map[string]any{"type": "agent.compact", "id": "c1", "agentId": id})
			compacting := false
			phone.WaitAll("the compaction",
				acked("c1"),
				func(m Message) bool {
					compacting = compacting || statusOf(id, "working")(m) && strings.Contains(m.Raw, `"compacting":true`)
					return compacting
				},
				itemOf(id, func(it acpItem) bool { return it.Kind == "compact" }),
				func(m Message) bool {
					return compacting && statusOf(id, "done")(m) && !strings.Contains(m.Raw, `"compacting":true`)
				},
			)
			h.WaitScreen(term, "compact")
			if got, want := timelineOf(phone, id), []string{"user hello", "assistant echo: hello", "result", "compact"}; strings.Join(got, "|") != strings.Join(want, "|") {
				t.Fatalf("got %q", got)
			}
			phone.Prompt(id, "after it")
			phone.WaitAll("the next turn", streamOf(id, "assistant", "echo: after it"), statusOf(id, "done"))
		})
	}
}

func TestEveryACPAgentChatsThroughTheOneDriver(t *testing.T) {
	for _, provider := range []string{"cursor", "grok", "fx", "antigravity"} {
		t.Run(provider, func(t *testing.T) {
			_, phone, _, id := chatACP(t, provider, nil)
			phone.Prompt(id, "hello")
			phone.WaitAll("the turn", streamOf(id, "assistant", "echo: hello"), statusOf(id, "done"))
		})
	}
}

// startsBadly has the fake agent do how instead of opening its session.
func startsBadly(how string) func(*Harness) {
	return func(h *Harness) {
		os.MkdirAll(filepath.Join(h.Home, ".acp-fake"), 0o700)
		os.WriteFile(filepath.Join(h.Home, ".acp-fake", "start"), []byte(how), 0o600)
	}
}

// timelineOf is each of id's timeline items: its kind, its text or command, its status or error.
func timelineOf(p *Phone, id string) []string {
	p.Send(map[string]any{"type": "agent.timeline", "id": "t-" + id, "agentId": id})
	var page struct {
		Items []acpItem `json:"items"`
	}
	json.Unmarshal([]byte(p.WaitFor("the timeline", func(m Message) bool { return m.Type == "agent.timeline" && m.ID == "t-"+id }).Raw), &page)
	var got []string
	for _, it := range page.Items {
		got = append(got, strings.Join(strings.Fields(it.Kind+" "+it.Text+it.Call.Detail.Command+" "+it.Call.Status+" "+it.Error), " "))
	}
	return got
}

func TestAChatWhoseAgentRefusesItsSessionShowsTheAgentsReason(t *testing.T) {
	h := Start(t, launchReady(""), startsBadly("refuse Authentication required"))
	phone := h.Owner()
	create(phone, "r1", inRepo(h, map[string]any{"provider": "grok", "chat": true}))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	const why = "Grok Build didn't start: Authentication required"
	var got []string
	for range 50 {
		if got = timelineOf(phone, done.AgentID); len(got) > 0 {
			break
		}
		time.Sleep(100 * time.Millisecond)
	}
	if len(got) != 1 || got[0] != "result "+why+". Run `grok login` in a terminal to sign in" {
		t.Fatalf("got %q", got)
	}
	phone.Prompt(done.AgentID, "hello")
	if refused := phone.WaitFor("the prompt refused", func(m Message) bool { return m.Type == "error" && m.ID == "prompt-hello" }); !strings.Contains(refused.Raw, why) {
		t.Fatalf("got %s", refused.Raw)
	}
}

func TestAChatWhoseAgentExitsAsItStartsShowsWhatItSaidAndThePromptItNeverGot(t *testing.T) {
	h := Start(t, launchReady(""), startsBadly("exit not logged in"))
	phone := h.Owner()
	create(phone, "r1", inRepo(h, map[string]any{"provider": "fx", "chat": true, "prompt": "do the important thing"}))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	phone.WaitAll("the failure",
		streamOf(done.AgentID, "user", "do the important thing"),
		itemOf(done.AgentID, func(it acpItem) bool {
			return it.Kind == "result" && !it.OK && strings.HasPrefix(it.Error, "fx didn't start, so your prompt was not sent: it exited") && strings.HasSuffix(it.Error, ": not logged in. Run `fx login` in a terminal to sign in")
		}),
	)
}

func TestARestartWhileOpenCodeAsksLeavesNoCallRunning(t *testing.T) {
	h, phone, term, id := chatACP(t, "opencode", nil, func(h *Harness) { h.resume = true })
	phone.Prompt(id, "run ls")
	phone.WaitFor("permission.request", asked)

	h.Restart()

	again := h.Owner()
	h.WaitScreen(term, "load: ")
	want := []string{"user run ls", "tool ls error", "result interrupted"}
	var got []string
	for range 50 {
		if got = timelineOf(again, id); len(got) == len(want) {
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
