package acp

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"testing"
	"time"

	"pocketd/internal/broker"
	"pocketd/internal/proto"
	"pocketd/internal/state"
)

// What Hermes 0.22.5 wrote for a prompt that had it run `echo ok`.
var hermesEchoOK = []string{
	`{"title":"Run echo ok command","updatedAt":"2026-11-11T05:39:44.491073+00:00","sessionUpdate":"session_info_update"}`,
	`{"content":[{"content":{"text":"$ echo ok","type":"text"},"type":"content"}],"kind":"execute","locations":[],"title":"terminal: echo ok","toolCallId":"call_iuBV","sessionUpdate":"tool_call"}`,
	`{"content":[{"content":{"text":"terminal result\n- **output:** ok\n- **exit_code:** 0","type":"text"},"type":"content"}],"kind":"execute","status":"completed","toolCallId":"call_iuBV","sessionUpdate":"tool_call_update"}`,
	`{"size":272000,"used":18001,"sessionUpdate":"usage_update"}`,
	`{"content":{"text":"\n\nok","type":"text"},"messageId":"62f0ffc3","sessionUpdate":"agent_message_chunk"}`,
	`{"size":272000,"used":18085,"sessionUpdate":"usage_update"}`,
}

func says(updates ...string) func(*agent, string) string {
	return func(a *agent, _ string) string {
		for _, u := range updates {
			a.update(u)
		}
		return endTurn
	}
}

func TestAHermesTurnShowsItsCallItsReplyAndEnds(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "hermes", answer: script(hermesNew, func(a *agent, _ string) string {
		for _, u := range hermesEchoOK {
			a.update(u)
		}
		return `{"stopReason":"end_turn","usage":{"cachedReadTokens":21760,"inputTokens":14154,"outputTokens":24,"totalTokens":35938}}`
	})})
	s.Prompt("run echo ok")
	equal(t, sk.wait(t, "ended:false"), []string{
		"bound:s1",
		"model:openai-codex:gpt-6-sol",
		"user:run echo ok",
		"working",
		"title:Run echo ok command",
		"tool_start:call_iuBV:shell:shell:echo ok",
		"tool_end:call_iuBV:true:terminal result\n- **output:** ok\n- **exit_code:** 0",
		"tokens:18001/272000",
		"assistant_draft:ok",
		"tokens:18085/272000",
		"assistant_text:ok",
		"result:true::14154/24/21760",
		"ended:false",
	})
	equal(t, a.called("initialize", 1), []string{
		`{"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false},"clientInfo":{"name":"anywhere","title":"Anywhere","version":"0"},"protocolVersion":1}`})
	equal(t, a.called("session/new", 1), []string{`{"cwd":"/work","mcpServers":[]}`})
	equal(t, a.called("session/set_mode", 1), []string{`{"modeId":"default","sessionId":"s1"}`})
	equal(t, a.called("session/prompt", 1), []string{`{"prompt":[{"text":"run echo ok","type":"text"}],"sessionId":"s1"}`})
	if got := a.called("authenticate", 0); got != nil {
		t.Fatalf("authenticated: %v", got)
	}
}

func TestALoadedSessionShowsItsHistoryAndStartsNoTurn(t *testing.T) {
	_, a, sk, _ := start(t, opts{provider: "hermes", saved: "s1", answer: func(a *agent, method string, _ json.RawMessage) string {
		if method != "session/load" {
			return `{}`
		}
		// What Hermes replays, with a second prompt it answered.
		a.update(`{"content":{"text":"Run echo ok.","type":"text"},"sessionUpdate":"user_message_chunk"}`)
		a.update(hermesEchoOK[1])
		a.update(hermesEchoOK[2])
		a.update(`{"content":{"text":"ok","type":"text"},"sessionUpdate":"agent_message_chunk"}`)
		a.update(`{"content":{"text":"again","type":"text"},"sessionUpdate":"user_message_chunk"}`)
		a.update(`{"content":{"text":"ok again","type":"text"},"sessionUpdate":"agent_message_chunk"}`)
		return hermesNew
	}})
	equal(t, sk.wait(t, "model:openai-codex:gpt-6-sol"), []string{
		"loaded",
		"user:Run echo ok.",
		"tool_start:call_iuBV:shell:shell:echo ok",
		"tool_end:call_iuBV:true:terminal result\n- **output:** ok\n- **exit_code:** 0",
		"assistant_text:ok",
		"result:true:",
		"user:again",
		"assistant_text:ok again",
		"result:true:",
		"bound:s1",
		"model:openai-codex:gpt-6-sol",
	})
	equal(t, a.called("session/load", 1), []string{`{"cwd":"/work","mcpServers":[],"sessionId":"s1"}`})
	if got := a.called("session/new", 0); got != nil {
		t.Fatalf("a new session: %v", got)
	}
}

func TestASessionTheAgentNoLongerHasStartsANewOneAndSaysSo(t *testing.T) {
	for name, c := range map[string]struct {
		refusal string
		resumes int
	}{
		// Hermes would resume a session it doesn't have as a new one, without telling its id.
		"hermes answers null": {`null`, 0},
		"an error":            {`!{"code":-32002,"message":"Session not found"}`, 1},
	} {
		t.Run(name, func(t *testing.T) {
			_, a, sk, _ := start(t, opts{provider: "hermes", saved: "gone", answer: func(a *agent, method string, _ json.RawMessage) string {
				switch method {
				case "session/load":
					a.update(`{"content":{"text":"half a history","type":"text"},"sessionUpdate":"user_message_chunk"}`)
					return c.refusal
				case "session/resume":
					return `!{"code":-32002,"message":"Session not found"}`
				case "session/new":
					return hermesNew
				}
				return `{}`
			}})
			equal(t, sk.wait(t, "model:openai-codex:gpt-6-sol"), []string{
				"bound:s1", "notice:Hermes Agent could not restore the conversation, so this is a new one", "model:openai-codex:gpt-6-sol"})
			if got := a.called("session/resume", c.resumes); len(got) != c.resumes {
				t.Fatalf("resumes: %v", got)
			}
		})
	}
}

func TestALoadTheAgentRefusesIsResumedAndLeavesTheTimelineAlone(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "grok", saved: "s9", answer: func(a *agent, method string, _ json.RawMessage) string {
		if method == "session/load" {
			return `!{"code":-32601,"message":"Method not found"}`
		}
		return `{}`
	}})
	equal(t, sk.lines(), []string{"bound:s9"})
	equal(t, a.called("session/resume", 1), []string{`{"cwd":"/work","mcpServers":[],"sessionId":"s9"}`})
	if got := a.called("session/new", 0); got != nil {
		t.Fatalf("a new session: %v", got)
	}
	s.Prompt("go on")
	equal(t, a.called("session/prompt", 1), []string{`{"prompt":[{"text":"go on","type":"text"}],"sessionId":"s9"}`})
}

func TestATurnCutByARestartReplaysInterruptedWithItsOpenCallEnded(t *testing.T) {
	// What OpenCode replays of a turn whose command waited on the user when pocketd went.
	history := []string{
		`{"content":{"text":"run pwd","type":"text"},"sessionUpdate":"user_message_chunk"}`,
		`{"sessionUpdate":"tool_call","toolCallId":"call_1","title":"pwd","kind":"execute","status":"in_progress","rawInput":{"command":"pwd"}}`,
	}
	for name, c := range map[string]struct {
		left    Left
		history []string
		want    []string
	}{
		"a call left open says so": {Ended, history, []string{"loaded", "user:run pwd", "tool_start:call_1:shell:shell:pwd", "tool_end:call_1:false:Interrupted", "result:false:interrupted"}},
		"a prompt left unanswered": {Cut, history[:1], []string{"loaded", "user:run pwd", "result:false:interrupted"}},
		// Its agent goes on with it, as after an upgrade.
		"a turn still running": {Running, history, []string{"loaded", "user:run pwd", "tool_start:call_1:shell:shell:pwd"}},
	} {
		t.Run(name, func(t *testing.T) {
			_, _, sk, _ := start(t, opts{provider: "opencode", saved: "s1", left: c.left, answer: func(a *agent, method string, _ json.RawMessage) string {
				if method == "session/load" {
					for _, u := range c.history {
						a.update(u)
					}
				}
				return opencodeNew
			}})
			equal(t, after(sk.wait(t, "bound:s1"), "loaded")[:len(c.want)+1], append(c.want, "bound:s1"))
		})
	}
}

func TestATurnLeftRunningEndsWhenItsAnswerComesToTheNextPocketd(t *testing.T) {
	_, a, sk, _ := start(t, opts{provider: "hermes", saved: "s1", left: Running, answer: func(a *agent, method string, _ json.RawMessage) string {
		if method == "session/load" {
			a.update(`{"content":{"text":"go","type":"text"},"sessionUpdate":"user_message_chunk"}`)
			return hermesNew
		}
		return `{}`
	}})
	sk.wait(t, "model:openai-codex:gpt-6-sol")
	a.update(`{"content":{"text":"done","type":"text"},"messageId":"m1","sessionUpdate":"agent_message_chunk"}`)
	a.say(`{"jsonrpc":"2.0","id":7,"result":{"stopReason":"end_turn"}}`)
	equal(t, after(sk.wait(t, "ended:false"), "bound:s1"), []string{
		"bound:s1", "model:openai-codex:gpt-6-sol", "assistant_draft:done", "assistant_text:done", "result:true:", "ended:false"})
}

func TestAPromptSentMidTurnWaitsForTheTurnToEnd(t *testing.T) {
	release := make(chan struct{})
	s, a, sk, _ := start(t, opts{provider: "hermes", answer: script(hermesNew, func(a *agent, text string) string {
		if text == "one" {
			<-release
		}
		a.update(`{"content":{"text":"re: ` + text + `","type":"text"},"messageId":"` + text + `","sessionUpdate":"agent_message_chunk"}`)
		return endTurn
	})})
	s.Prompt("one")
	s.Prompt("two")
	a.called("session/prompt", 1)
	if got := sk.lines(); slices.Contains(got, "user:two") {
		t.Fatalf("the second prompt went mid-turn: %v", got)
	}
	close(release)
	sk.wait(t, "assistant_text:re: two")
	equal(t, after(sk.wait(t, "assistant_text:re: two"), "user:one")[:9], []string{
		"user:one", "working", "assistant_draft:re: one", "assistant_text:re: one", "result:true:", "ended:false",
		"user:two", "working", "assistant_draft:re: two"})
}

func TestATurnEndsByWhatTheAgentAnswersItsPrompt(t *testing.T) {
	for name, c := range map[string]struct {
		answer string
		want   []string
	}{
		"cancelled is no failure": {`{"stopReason":"cancelled"}`, []string{"result:false:interrupted", "stopped"}},
		"another reason fails it": {`{"stopReason":"max_tokens"}`, []string{"result:false:Hermes Agent ended the turn (max_tokens)", "ended:true"}},
		"an error fails it with the agent's words": {`!{"code":-32603,"message":"Internal error","data":{"details":"no credentials"}}`,
			[]string{"result:false:Internal error: no credentials", "ended:true"}},
	} {
		t.Run(name, func(t *testing.T) {
			s, _, sk, _ := start(t, opts{provider: "hermes", answer: script(hermesNew, func(*agent, string) string { return c.answer })})
			s.Prompt("go")
			equal(t, after(sk.wait(t, c.want[1]), "working")[1:], c.want)
		})
	}
}

func TestAStopCancelsTheTurnAndDropsThePromptsThatWaited(t *testing.T) {
	cancelled := make(chan struct{})
	s, a, sk, _ := start(t, opts{provider: "hermes", answer: func(a *agent, method string, _ json.RawMessage) string {
		switch method {
		case "session/new":
			return hermesNew
		case "session/cancel":
			close(cancelled)
		case "session/prompt":
			<-cancelled
			return `{"stopReason":"cancelled"}`
		}
		return `{}`
	}})
	s.Prompt("slow")
	s.Prompt("never")
	a.called("session/prompt", 1)
	s.Cancel()
	equal(t, after(sk.wait(t, "stopped"), "user:slow"), []string{"user:slow", "working", "result:false:interrupted", "stopped"})
	equal(t, a.called("session/cancel", 1), []string{`{"sessionId":"s1"}`})
	if got := a.called("session/prompt", 1); len(got) != 1 {
		t.Fatalf("the dropped prompt went: %v", got)
	}
}

func TestAStopEndsTheCallsTheAgentLeftOpenSoALaterCallWithTheirIDShows(t *testing.T) {
	cancelled := make(chan struct{})
	call := `{"sessionUpdate":"tool_call","toolCallId":"call_1","title":"sleep 45","kind":"execute","status":"in_progress","rawInput":{"command":"sleep 45"}}`
	s, _, sk, _ := start(t, opts{provider: "grok", answer: func(a *agent, method string, params json.RawMessage) string {
		switch method {
		case "session/new":
			return `{"sessionId":"s1"}`
		case "session/cancel":
			close(cancelled)
		case "session/prompt":
			a.update(call)
			if promptOf(params) == "slow" {
				// Grok ends a cancelled turn with no last word on its calls.
				<-cancelled
				return `{"stopReason":"cancelled"}`
			}
			a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"call_1","status":"completed"}`)
			return endTurn
		}
		return `{}`
	}})
	s.Prompt("slow")
	sk.wait(t, "tool_start:call_1:shell:shell:sleep 45")
	s.Cancel()
	sk.wait(t, "stopped")
	s.Prompt("again")
	equal(t, after(sk.wait(t, "ended:false"), "user:slow"), []string{
		"user:slow", "working", "tool_start:call_1:shell:shell:sleep 45", "tool_end:call_1:false:Interrupted", "result:false:interrupted", "stopped",
		"user:again", "working", "tool_start:call_1:shell:shell:sleep 45", "tool_end:call_1:true", "result:true:", "ended:false"})
}

func TestMessagesThoughtsPlansAndCommandsMapToTheChat(t *testing.T) {
	s, _, sk, _ := start(t, opts{provider: "hermes", answer: script(hermesNew, says(
		`{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"hm"}}`,
		`{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"m"}}`,
		`{"sessionUpdate":"plan","entries":[{"content":"one","priority":"high","status":"in_progress"},{"content":"two","priority":"low","status":"pending"}]}`,
		`{"sessionUpdate":"agent_message_chunk","messageId":"m1","content":{"type":"text","text":"a"}}`,
		`{"sessionUpdate":"agent_message_chunk","messageId":"m2","content":{"type":"text","text":"b"}}`,
		`{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"compact","description":"Compact it","input":{"hint":"[focus]"}}]}`,
		`{"sessionUpdate":"something_new","content":{"type":"text","text":"x"}}`,
	))})
	s.Prompt("go")
	equal(t, after(sk.wait(t, "ended:false"), "working")[1:], []string{
		"thinking_draft:hm",
		"thinking_draft:m",
		"thinking:hmm",
		`tool_start:plan-1:TodoWrite:{"todos":[{"id":"","content":"one","status":"in_progress"},{"id":"","content":"two","status":"pending"}]}`,
		"assistant_draft:a",
		"assistant_text:a",
		"assistant_draft:b",
		"assistant_text:b",
		"result:true:",
		"ended:false",
	})
	got, err := s.Commands()
	if want := []proto.Command{{Name: "compact", Description: "Compact it", ArgumentHint: "[focus]"}}; err != nil || !slices.Equal(got, want) {
		t.Fatalf("commands %v, %v", got, err)
	}
}

func TestACallShowsByItsKind(t *testing.T) {
	old := "a\n"
	for name, c := range map[string]struct {
		call toolCall
		want string
	}{
		"execute by its input": {toolCall{Kind: "execute", Title: "bash", RawInput: json.RawMessage(`{"command":"ls -la","description":"List"}`)}, "shell:shell:ls -laList"},
		"execute by its words": {toolCall{Kind: "execute", RawInput: json.RawMessage(`{"command":["git","status"]}`)}, "shell:shell:git status"},
		"execute by its title": {toolCall{Kind: "execute", Title: "make"}, "shell:shell:make"},
		"read by its location": {toolCall{Kind: "read", Title: "Read", Locations: []struct {
			Path string `json:"path"`
		}{{"/w/a.go"}}}, "read:read:/w/a.go"},
		"read by its input":       {toolCall{Kind: "read", RawInput: json.RawMessage(`{"filePath":"/w/b.go"}`)}, "read:read:/w/b.go"},
		"edit with its diff":      {toolCall{Kind: "edit", Content: []toolContent{{Type: "diff", Path: "/w/a.txt", OldText: &old, NewText: "b\nc\n"}}}, "edit:edit:/w/a.txt:+2-1"},
		"edit of a new file":      {toolCall{Kind: "edit", Content: []toolContent{{Type: "diff", Path: "/w/n.txt", NewText: "b\n"}}}, "write:write:/w/n.txt:+1-0"},
		"search by its pattern":   {toolCall{Kind: "search", Title: "grep", RawInput: json.RawMessage(`{"pattern":"TODO","path":"/w"}`)}, "search:search:/wTODO"},
		"anything else by title":  {toolCall{Kind: "fetch", Title: "Fetch example.com", RawInput: json.RawMessage(`{"url":"https://example.com"}`)}, `Fetch example.com:other:`},
		"grok's by its meta":      {grokMeta("execute", "Run", `{"command":"pwd"}`), "shell:shell:pwd"},
		"a sub-agent by its meta": {grokMeta("task", "Subagent", `{"description":"List the files","prompt":"ls"}`), "task:task:List the files"},
	} {
		t.Run(name, func(t *testing.T) {
			var merged toolCall
			merged.merge(c.call)
			name, d := merged.describe()
			got := name + ":" + d.Kind + ":" + d.Command + d.Path + d.Query + d.Description
			if d.Diff != nil {
				got += ":+" + string(rune('0'+d.Diff.Additions)) + "-" + string(rune('0'+d.Diff.Deletions))
			}
			if got != c.want {
				t.Fatalf("got %q, want %q", got, c.want)
			}
		})
	}
}

func grokMeta(kind, label, input string) (c toolCall) {
	c.Meta.Tool.Kind, c.Meta.Tool.Label, c.Meta.Tool.Input = kind, label, json.RawMessage(input)
	return c
}

func TestOpenCodeTakesItsLaunchAsConfigOptions(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "opencode", launch: state.Launch{Access: "ask", Plan: true, Model: "opencode/glm-5.3-flash-free"},
		answer: func(a *agent, method string, params json.RawMessage) string {
			switch method {
			case "session/new":
				return opencodeNew
			case "session/set_config_option":
				if strings.Contains(string(params), "nope/bogus") {
					return `!{"code":-32602,"message":"Invalid params: model not found: nope/bogus","data":{"providerId":"nope","modelId":"nope/bogus"}}`
				}
				return `{"configOptions":[{"id":"model","category":"model","currentValue":"opencode/glm-5.3-flash-free"},{"id":"mode","category":"mode","currentValue":"plan"}]}`
			}
			return `{}`
		}})
	equal(t, sk.wait(t, "model:opencode/glm-5.3-flash-free"), []string{"bound:s1", "model:opencode/glm-5.3-flash-free"})
	equal(t, a.called("session/set_config_option", 2), []string{
		`{"configId":"mode","sessionId":"s1","value":"plan"}`,
		`{"configId":"model","sessionId":"s1","value":"opencode/glm-5.3-flash-free"}`,
	})
	if err := s.Settings("nope/bogus", ""); err == nil || err.Error() != "Invalid params: model not found: nope/bogus" {
		t.Fatalf("a model it doesn't have: %v", err)
	}
	// Any access leaves plan mode for the mode that builds.
	if err := s.Access("edits"); err != nil {
		t.Fatal(err)
	}
	if got := a.called("session/set_config_option", 4)[3]; got != `{"configId":"mode","sessionId":"s1","value":"build"}` {
		t.Fatalf("leaving plan: %s", got)
	}
	if got := a.called("session/set_mode", 0); got != nil {
		t.Fatalf("set_mode: %v", got)
	}
}

func TestOpenCodesAgentIsTheModeItLeavesPlanningFor(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "opencode", launch: state.Launch{Access: "ask", Plan: true, Options: map[string]string{"mode": "review"}}, answer: script(opencodeNew, says())})
	sk.wait(t, "model:opencode/big-pickle")
	equal(t, a.called("session/set_config_option", 1), []string{`{"configId":"mode","sessionId":"s1","value":"plan"}`})
	if err := s.Access("edits"); err != nil {
		t.Fatal(err)
	}
	equal(t, a.called("session/set_config_option", 2)[1:], []string{`{"configId":"mode","sessionId":"s1","value":"review"}`})

	_, a, sk, _ = start(t, opts{provider: "opencode", launch: state.Launch{Access: "ask", Options: map[string]string{"mode": "review"}}, answer: script(opencodeNew, says())})
	sk.wait(t, "model:opencode/big-pickle")
	equal(t, a.called("session/set_config_option", 1), []string{`{"configId":"mode","sessionId":"s1","value":"review"}`})
}

func TestCursorsModelSettingsAreSetOnTheModelOnceItIsPickedAndOnlyThoseItTookAreTold(t *testing.T) {
	picked := `{"configOptions":[{"id":"model","category":"model","currentValue":"gpt-5.4"},
		{"id":"reasoning","category":"thought_level","type":"select","currentValue":"medium"},
		{"id":"fast","category":"model_config","type":"select","currentValue":"false"}]}`
	_, a, sk, _ := start(t, opts{provider: "cursor", launch: state.Launch{Model: "gpt-5.4", Options: map[string]string{"context": "1m", "fast": "true", "reasoning": "high"}},
		answer: func(a *agent, method string, params json.RawMessage) string {
			switch {
			case method == "session/new":
				return `{"sessionId":"s1","configOptions":[{"id":"model","category":"model","currentValue":"composer-2.5"}]}`
			case strings.Contains(string(params), `"fast"`):
				return `!{"code":-32602,"message":"Invalid value for fast: true"}`
			case method == "session/set_config_option":
				return picked
			}
			return `{}`
		}})
	sk.wait(t, "notice:Cursor couldn't set fast to true: Invalid value for fast: true")
	equal(t, a.called("session/set_config_option", 3), []string{
		`{"configId":"model","sessionId":"s1","value":"gpt-5.4"}`,
		`{"configId":"fast","sessionId":"s1","value":"true"}`,
		`{"configId":"reasoning","sessionId":"s1","value":"high"}`,
	})
	sk.wait(t, "options:map[reasoning:high]")
}

func TestACursorSettingRunsOnWhatItsNewModelSaysOnceTheModelSwitches(t *testing.T) {
	s, _, sk, _ := start(t, opts{provider: "cursor", launch: state.Launch{Model: "gpt-5.4"},
		answer: func(a *agent, method string, params json.RawMessage) string {
			switch {
			case method == "session/new":
				return `{"sessionId":"s1","configOptions":[{"id":"model","category":"model","currentValue":"gpt-5.4"},
					{"id":"fast","category":"model_config","type":"select","currentValue":"false"},{"id":"context","category":"model_config","type":"select","currentValue":"200k"}]}`
			case strings.Contains(string(params), `"configId":"model"`):
				return `{"configOptions":[{"id":"model","category":"model","currentValue":"gpt-5.5"},{"id":"fast","category":"model_config","type":"select","currentValue":"false"}]}`
			}
			return `{}`
		}})
	sk.wait(t, "model:gpt-5.4")
	if err := s.SetOptions(map[string]string{"context": "1m", "fast": "true"}); err != nil {
		t.Fatal(err)
	}
	sk.wait(t, "options:map[context:1m fast:true]")
	if err := s.Settings("gpt-5.5", ""); err != nil {
		t.Fatal(err)
	}
	sk.wait(t, "options:map[fast:false]")
}

func TestASwitchKeepsTheSettingsSwitchedBeforeAndThoseSetBeforeOneThatFails(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "antigravity", launch: state.Launch{Access: "ask"},
		answer: func(a *agent, method string, params json.RawMessage) string {
			if method == "session/new" {
				return `{"sessionId":"s1","configOptions":[{"id":"model","category":"model","type":"select","currentValue":"gemini-3.1-pro"},
					{"id":"fast","name":"Fast","category":"model","type":"boolean","currentValue":false},{"id":"web","name":"Web","currentValue":true}]}`
			}
			return `{}`
		}})
	sk.wait(t, "model:gemini-3.1-pro")
	if err := s.SetOptions(map[string]string{"fast": "true"}); err != nil {
		t.Fatal(err)
	}
	if err := s.SetOptions(map[string]string{"web": "false"}); err != nil {
		t.Fatal(err)
	}
	sk.wait(t, "options:map[fast:true web:false]")
	if err := s.SetOptions(map[string]string{"fast": "false", "web": "maybe"}); err == nil || err.Error() != "web is on or off" {
		t.Fatalf("a boolean set to neither: %v", err)
	}
	sk.wait(t, "options:map[fast:false web:false]")
	if err := s.SetOptions(map[string]string{"fast": "true", "zzz": "1"}); err == nil || err.Error() != "Antigravity has no setting zzz" {
		t.Fatalf("a setting it doesn't list: %v", err)
	}
	if got := a.called("session/set_config_option", 3); len(got) != 3 {
		t.Fatalf("set some of a switch with a setting it doesn't list: %v", got)
	}
}

func TestAModelSettingSwitchesMidChatAndOnlyOneTheSessionLists(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "cursor", launch: state.Launch{Model: "gpt-5.4"},
		answer: func(a *agent, method string, params json.RawMessage) string {
			if method == "session/new" {
				return `{"sessionId":"s1","configOptions":[{"id":"model","category":"model","currentValue":"gpt-5.4"},{"id":"fast","category":"model_config","type":"select","currentValue":"false"}]}`
			}
			return `{}`
		}})
	sk.wait(t, "model:gpt-5.4")
	if err := s.SetOptions(map[string]string{"fast": "true"}); err != nil {
		t.Fatal(err)
	}
	equal(t, a.called("session/set_config_option", 1), []string{`{"configId":"fast","sessionId":"s1","value":"true"}`})
	if err := s.SetOptions(map[string]string{"context": "1m"}); err == nil || err.Error() != "Cursor has no setting context" {
		t.Fatalf("a setting it doesn't list: %v", err)
	}
}

func TestOpenCodesAgentPickedWhilePlanningWaitsForPlanningToEnd(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "opencode", launch: state.Launch{Access: "ask", Plan: true}, answer: script(opencodeNew, says())})
	sk.wait(t, "model:opencode/big-pickle")
	if err := s.SetOptions(map[string]string{"mode": "review"}); err != nil {
		t.Fatal(err)
	}
	equal(t, a.called("session/set_config_option", 1), []string{`{"configId":"mode","sessionId":"s1","value":"plan"}`})
	if err := s.Access("edits"); err != nil {
		t.Fatal(err)
	}
	equal(t, a.called("session/set_config_option", 2)[1:], []string{`{"configId":"mode","sessionId":"s1","value":"review"}`})
	if err := s.SetOptions(map[string]string{"mode": "build"}); err != nil {
		t.Fatal(err)
	}
	equal(t, a.called("session/set_config_option", 3)[2:], []string{`{"configId":"mode","sessionId":"s1","value":"build"}`})
}

func TestABooleanSettingIsSetAsTheJSONBooleanACPTypesIt(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "antigravity", launch: state.Launch{Access: "ask", Options: map[string]string{"fast": "true"}},
		answer: func(a *agent, method string, params json.RawMessage) string {
			if method == "session/new" {
				return `{"sessionId":"s1","configOptions":[{"id":"model","category":"model","type":"select","currentValue":"gemini-3.1-pro"},
					{"id":"fast","name":"Fast","category":"model","type":"boolean","currentValue":false},{"id":"web","name":"Web","currentValue":true}]}`
			}
			return `{}`
		}})
	sk.wait(t, "model:gemini-3.1-pro")
	if err := s.SetOptions(map[string]string{"fast": "false", "web": "false"}); err != nil {
		t.Fatal(err)
	}
	equal(t, a.called("session/set_config_option", 3), []string{
		`{"configId":"fast","sessionId":"s1","type":"boolean","value":true}`,
		`{"configId":"fast","sessionId":"s1","type":"boolean","value":false}`,
		`{"configId":"web","sessionId":"s1","type":"boolean","value":false}`,
	})
	if err := s.SetOptions(map[string]string{"fast": "yes"}); err == nil || err.Error() != "fast is on or off" {
		t.Fatalf("a boolean set to neither: %v", err)
	}
}

func TestHermesSwitchesModelAndModeByTheirOwnCalls(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "hermes", launch: state.Launch{Access: "full", Model: "moa:default", Effort: "high"},
		answer: func(a *agent, method string, params json.RawMessage) string {
			switch method {
			case "session/new":
				return hermesNew
			case "session/set_model":
				if strings.Contains(string(params), "nope") {
					return `!{"code":-32603,"message":"Internal error","data":{"details":"model_resolution_failed"}}`
				}
			}
			return `{}`
		}})
	equal(t, sk.wait(t, "model:moa:default"), []string{"bound:s1", "model:moa:default", "notice:Hermes Agent couldn't set effort to high: its models carry their own effort"})
	equal(t, a.called("session/set_mode", 1), []string{`{"modeId":"dont_ask","sessionId":"s1"}`})
	equal(t, a.called("session/set_model", 1), []string{`{"modelId":"moa:default","sessionId":"s1"}`})
	if err := s.Settings("nope:bogus", ""); err == nil || err.Error() != "Internal error: model_resolution_failed" {
		t.Fatalf("a model it doesn't have: %v", err)
	}
	if err := s.Settings("", "low"); err == nil || err.Error() != "Hermes Agent can't switch effort: its models carry their own effort" {
		t.Fatalf("effort: %v", err)
	}
	if err := s.Access("edits"); err != nil {
		t.Fatal(err)
	}
	if got := a.called("session/set_mode", 2)[1]; got != `{"modeId":"accept_edits","sessionId":"s1"}` {
		t.Fatalf("edits: %s", got)
	}
}

func TestAModelTheAgentRefusesAtLaunchIsSaidInTheChat(t *testing.T) {
	_, _, sk, _ := start(t, opts{provider: "hermes", launch: state.Launch{Model: "nope:bogus"},
		answer: func(a *agent, method string, _ json.RawMessage) string {
			switch method {
			case "session/new":
				return hermesNew
			case "session/set_model":
				return `!{"code":-32603,"message":"Internal error","data":{"details":"model_resolution_failed"}}`
			}
			return `{}`
		}})
	equal(t, sk.wait(t, "model:openai-codex:gpt-6-sol"), []string{
		"bound:s1", "notice:Hermes Agent couldn't switch to nope:bogus: Internal error: model_resolution_failed", "model:openai-codex:gpt-6-sol"})
}

func TestAnEffortTheSessionCantTakeAtLaunchIsSaidInTheChat(t *testing.T) {
	_, _, sk, _ := start(t, opts{provider: "opencode", launch: state.Launch{Effort: "high"}, answer: script(opencodeNew, says())})
	equal(t, after(sk.lines(), "bound:s1"), []string{
		"bound:s1", "model:opencode/big-pickle", "notice:OpenCode couldn't set effort to high: its models carry their own effort"})
}

func TestEachAgentOpensItsSessionItsOwnWay(t *testing.T) {
	const grokInit = `{"authMethods":[{"id":"grok.com"},{"id":"cached_token"},{"id":"xai.api_key"}],"_meta":{"defaultAuthMethodId":"grok.com"}}`
	for _, c := range []struct {
		provider string
		launch   state.Launch
		init     string
		calls    map[string]string
	}{
		{"cursor", state.Launch{Access: "ask"}, `{}`, map[string]string{
			"initialize":  `{"clientCapabilities":{"_meta":{"parameterizedModelPicker":true,"subagents":true},"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false},"clientInfo":{"name":"anywhere","title":"Anywhere","version":"0"},"protocolVersion":1}`,
			"session/new": `{"cwd":"/work","mcpServers":[]}`,
		}},
		{"grok", state.Launch{Access: "full", Effort: "high"}, grokInit, map[string]string{
			"authenticate":     `{"_meta":{"headless":true},"methodId":"xai.api_key"}`,
			"session/new":      `{"_meta":{"yoloMode":true},"cwd":"/work","mcpServers":[]}`,
			"session/set_mode": `{"modeId":"high","sessionId":"s1"}`,
		}},
		{"grok", state.Launch{Access: "auto"}, grokInit, map[string]string{
			"session/new": `{"_meta":{"autoMode":true},"cwd":"/work","mcpServers":[]}`,
		}},
		{"fx", state.Launch{Access: "ask"}, `{}`, map[string]string{
			"session/set_mode": `{"modeId":"auto","sessionId":"s1"}`,
		}},
		{"fx", state.Launch{Access: "full"}, `{}`, map[string]string{
			"session/set_mode": `{"modeId":"full-access","sessionId":"s1"}`,
		}},
		{"antigravity", state.Launch{Access: "edits"}, `{}`, map[string]string{
			"initialize":       `{"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"session":{"configOptions":{"boolean":{}}},"terminal":false},"clientInfo":{"name":"anywhere","title":"Anywhere","version":"0"},"protocolVersion":1}`,
			"session/set_mode": `{"modeId":"accept-edits","sessionId":"s1"}`,
		}},
		{"antigravity", state.Launch{Access: "full"}, `{}`, map[string]string{
			"session/set_mode": `{"modeId":"accept-edits","sessionId":"s1"}`,
		}},
		{"antigravity", state.Launch{Access: "ask", Plan: true}, `{}`, map[string]string{
			"session/set_mode": `{"modeId":"plan","sessionId":"s1"}`,
		}},
		{"cursor", state.Launch{Access: "ask", Plan: true}, `{}`, map[string]string{
			"session/set_mode": `{"modeId":"plan","sessionId":"s1"}`,
		}},
		{"fx", state.Launch{Access: "full", Plan: true}, `{}`, map[string]string{
			"session/set_mode": `{"modeId":"ask","sessionId":"s1"}`,
		}},
		{"hermes", state.Launch{Access: "auto", Plan: true}, `{}`, map[string]string{
			"session/set_mode": `{"modeId":"default","sessionId":"s1"}`,
		}},
	} {
		t.Run(c.provider+" "+c.launch.Access, func(t *testing.T) {
			_, a, _, _ := start(t, opts{provider: c.provider, launch: c.launch, answer: func(a *agent, method string, _ json.RawMessage) string {
				switch method {
				case "initialize":
					return c.init
				case "authenticate":
					return `!{"code":-32000,"message":"already signed in"}`
				case "session/new":
					return `{"sessionId":"s1"}`
				}
				return `{}`
			}})
			for method, want := range c.calls {
				if got := a.called(method, 1)[0]; got != want {
					t.Errorf("%s\n got %s\nwant %s", method, got, want)
				}
			}
		})
	}
}

func TestCursorNeverSignsInThroughABrowser(t *testing.T) {
	_, a, _, _ := start(t, opts{provider: "cursor", answer: func(a *agent, method string, _ json.RawMessage) string {
		if method == "session/new" {
			return `{"sessionId":"s1"}`
		}
		return `{}`
	}})
	a.called("session/new", 1)
	a.mu.Lock()
	defer a.mu.Unlock()
	for _, c := range a.calls {
		if strings.HasPrefix(c, "authenticate ") {
			t.Errorf("called %s", c)
		}
	}
}

func TestGrokNeverSignsInThroughABrowser(t *testing.T) {
	for init, want := range map[string]string{
		`{"authMethods":[{"id":"grok.com"},{"id":"cached_token"},{"id":"xai.api_key"}]}`:                               "xai.api_key",
		`{"authMethods":[{"id":"grok.com"},{"id":"cached_token"},{"id":"sso"}],"_meta":{"defaultAuthMethodId":"sso"}}`: "sso",
		`{"authMethods":[{"id":"grok.com"},{"id":"cached_token"}],"_meta":{"defaultAuthMethodId":"grok.com"}}`:         "cached_token",
		`{"authMethods":[{"id":"grok.com"},{"id":"other"}]}`:                                                           "other",
		`{"authMethods":[{"id":"grok.com"}]}`:                                                                          "",
		`{}`:                                                                                                           "",
	} {
		if got := grokMethod(json.RawMessage(init)); got != want {
			t.Errorf("%s: %q, want %q", init, got, want)
		}
	}
}

var hermesOptions = []option{
	{"allow_once", "Allow once", "allow_once"}, {"allow_session", "Allow for session", "allow_always"},
	{"allow_always", "Allow always", "allow_always"}, {"deny", "Deny", "reject_once"},
}

var opencodeOptions = []option{{"once", "Allow once", "allow_once"}, {"always", "Always allow", "allow_always"}, {"reject", "Reject", "reject_once"}}

func TestAccessDecidesWhatPocketdAnswersByItself(t *testing.T) {
	for _, c := range []struct{ access, kind, want string }{
		{"ask", "read", ""},
		{"ask", "execute", ""},
		{"settings", "read", ""},
		{"edits", "read", "allow_once"},
		{"edits", "edit", "allow_once"},
		{"edits", "search", "allow_once"},
		{"edits", "execute", ""},
		{"edits", "fetch", ""},
		{"edits", "other", ""},
		{"auto", "execute", "allow_once"},
		{"auto", "other", "allow_once"},
		{"full", "execute", "allow_once"},
		{"full", "edit", "allow_once"},
	} {
		if got := auto(c.access, c.kind, hermesOptions); got != c.want {
			t.Errorf("%s %s: %q, want %q", c.access, c.kind, got, c.want)
		}
	}
	if got := auto("full", "execute", opencodeOptions); got != "once" {
		t.Errorf("by kind: %q", got)
	}
	// An always outlasts the access that picked it, so the user picks it or no one does.
	if got := auto("full", "execute", opencodeOptions[1:]); got != "" {
		t.Errorf("with only an always: %q", got)
	}
	if got := auto("auto", "execute", opencodeOptions[2:]); got != "" {
		t.Errorf("with nothing to allow: %q", got)
	}
}

// permission is Hermes's request to run a command, as it asked 0.22.5's client.
const hermesPermission = `{"sessionId":"s1","toolCall":{"toolCallId":"perm-check-1","title":"rm -rf build","kind":"execute","status":"pending","rawInput":{"command":"rm -rf build","description":"recursive delete"}},
	"options":[{"optionId":"allow_once","name":"Allow once","kind":"allow_once"},{"optionId":"allow_session","name":"Allow for session","kind":"allow_always"},{"optionId":"allow_always","name":"Allow always","kind":"allow_always"},{"optionId":"deny","name":"Deny","kind":"reject_once"}]}`

// asking opens a hermes session whose turn asks hermesPermission and ends once it is answered.
func asking(t *testing.T, access string) (*Session, *agent, *sink, *broker.Broker) {
	answered := make(chan struct{})
	s, a, sk, b := start(t, opts{provider: "hermes", launch: state.Launch{Access: access}, answer: script(hermesNew, func(a *agent, _ string) string {
		a.ask("41", "session/request_permission", hermesPermission)
		go func() {
			a.replied("41")
			close(answered)
		}()
		<-answered
		// Hermes ends each check as a call it never started.
		a.update(`{"toolCallId":"perm-check-1","status":"completed","sessionUpdate":"tool_call_update"}`)
		return endTurn
	})})
	s.Prompt("clean")
	return s, a, sk, b
}

func TestAPermissionIsAskedOfTheUserAndAnsweredByWhatTheyPick(t *testing.T) {
	for name, c := range map[string]struct {
		answer broker.Answer
		want   string
	}{
		"allow":            {broker.Answer{Decision: "allow"}, `{"outcome":{"optionId":"allow_once","outcome":"selected"}}`},
		"allow for good":   {broker.Answer{Decision: "allow", Option: "allow_session"}, `{"outcome":{"optionId":"allow_session","outcome":"selected"}}`},
		"deny":             {broker.Answer{Decision: "deny"}, `{"outcome":{"optionId":"deny","outcome":"selected"}}`},
		"an option unseen": {broker.Answer{Decision: "allow", Option: "nope"}, `{"outcome":{"optionId":"allow_once","outcome":"selected"}}`},
	} {
		t.Run(name, func(t *testing.T) {
			_, a, sk, b := asking(t, "ask")
			req := asked(t, b)
			if req.AgentID != "a1" || req.ToolName != "shell" || req.Detail.Command != "rm -rf build" || req.Detail.Description != "recursive delete" {
				t.Fatalf("asked %+v", req)
			}
			if want := []proto.PermissionOption{{ID: "allow_session", Label: "Allow for session"}, {ID: "allow_always", Label: "Allow always"}}; !slices.Equal(req.Options, want) {
				t.Fatalf("options %+v", req.Options)
			}
			sk.wait(t, "needsYou")
			b.Resolve(req.RequestID, c.answer)
			if got := a.replied("41"); got != c.want {
				t.Fatalf("answered %s, want %s", got, c.want)
			}
			equal(t, after(sk.wait(t, "ended:false"), "user:clean"), []string{"user:clean", "working", "needsYou", "working", "result:true:", "ended:false"})
		})
	}
}

func TestAccessAnswersAPermissionWithoutAsking(t *testing.T) {
	for access, want := range map[string]string{"auto": "allow_once", "full": "allow_once"} {
		t.Run(access, func(t *testing.T) {
			_, a, sk, b := asking(t, access)
			if got := a.replied("41"); got != `{"outcome":{"optionId":"`+want+`","outcome":"selected"}}` {
				t.Fatalf("answered %s", got)
			}
			if got := sk.wait(t, "ended:false"); slices.Contains(got, "needsYou") || len(b.Open()) != 0 {
				t.Fatalf("asked: %v", got)
			}
		})
	}
}

func TestAnAccessSwitchChangesWhatIsAsked(t *testing.T) {
	s, a, sk, b := start(t, opts{provider: "hermes", answer: script(hermesNew, func(a *agent, text string) string {
		a.ask(text, "session/request_permission", hermesPermission)
		a.replied(text)
		return endTurn
	})})
	if err := s.Access("auto"); err != nil {
		t.Fatal(err)
	}
	s.Prompt("1")
	sk.wait(t, "ended:false")
	if len(b.Open()) != 0 || a.replied("1") != `{"outcome":{"optionId":"allow_once","outcome":"selected"}}` {
		t.Fatalf("auto asked, or answered %s", a.replied("1"))
	}
	if err := s.Access("ask"); err != nil {
		t.Fatal(err)
	}
	s.Prompt("2")
	b.Resolve(asked(t, b).RequestID, broker.Answer{Decision: "deny"})
	if got := a.replied("2"); got != `{"outcome":{"optionId":"deny","outcome":"selected"}}` {
		t.Fatalf("answered %s", got)
	}
}

func TestAStopAnswersAnOpenPermissionCancelled(t *testing.T) {
	s, a, sk, b := asking(t, "ask")
	asked(t, b)
	sk.wait(t, "needsYou")
	s.Cancel()
	if got := a.replied("41"); got != `{"outcome":{"outcome":"cancelled"}}` {
		t.Fatalf("answered %s", got)
	}
	equal(t, after(sk.wait(t, "stopped"), "user:clean"), []string{"user:clean", "working", "needsYou", "result:false:interrupted", "stopped"})
	if open := b.Open(); len(open) != 0 {
		t.Fatalf("still asked: %v", open)
	}
}

func TestHermesAsksAboutAnEditAsACallOfItsOwnWithTheDiff(t *testing.T) {
	// What Hermes 0.22.5 wrote for a file write the user denied.
	s, a, sk, b := start(t, opts{provider: "hermes", answer: script(hermesNew, func(a *agent, _ string) string {
		a.update(`{"content":[{"content":{"text":"Preparing write to nope.txt. Approval prompt shows the diff.","type":"text"},"type":"content"}],"kind":"edit","locations":[{"path":"nope.txt"}],"title":"write_file: nope.txt","toolCallId":"call_IrWN","sessionUpdate":"tool_call"}`)
		a.ask("0", "session/request_permission", `{"options":[{"kind":"allow_once","name":"Allow once","optionId":"allow_once"},{"kind":"reject_once","name":"Deny","optionId":"deny"}],"sessionId":"s1",
			"toolCall":{"content":[{"newText":"no","path":"/w/nope.txt","type":"diff"}],"kind":"edit","rawInput":{"tool":"write_file","arguments":{"path":"nope.txt","content":"no"}},"status":"pending","title":"Approve edit: /w/nope.txt","toolCallId":"edit-approval-1"}}`)
		a.replied("0")
		a.update(`{"status":"failed","toolCallId":"edit-approval-1","sessionUpdate":"tool_call_update"}`)
		a.update(`{"content":[{"content":{"text":"write_file failed for nope.txt: Edit approval denied by ACP client; file was not modified.","type":"text"},"type":"content"}],"kind":"edit","status":"failed","toolCallId":"call_IrWN","sessionUpdate":"tool_call_update"}`)
		return endTurn
	})})
	s.Prompt("write")
	req := asked(t, b)
	if req.ToolName != "write" || req.Detail.Path != "/w/nope.txt" || req.Detail.Diff == nil || req.Detail.Diff.Additions != 1 || len(req.Options) != 0 {
		t.Fatalf("asked %+v", req)
	}
	b.Resolve(req.RequestID, broker.Answer{Decision: "deny"})
	if got := a.replied("0"); got != `{"outcome":{"optionId":"deny","outcome":"selected"}}` {
		t.Fatalf("answered %s", got)
	}
	equal(t, after(sk.wait(t, "ended:false"), "working")[1:], []string{
		"tool_start:call_IrWN:write:write:/w/nope.txt:+1-0", "needsYou", "working",
		"tool_end:call_IrWN:false:write_file failed for nope.txt: Edit approval denied by ACP client; file was not modified.",
		"result:true:", "ended:false"})
}

func TestWhatTheBrokerAllowsIsNotAsked(t *testing.T) {
	answered := make(chan struct{})
	s, ag, sk, b := start(t, opts{provider: "hermes", answer: script(hermesNew, func(a *agent, _ string) string {
		<-answered
		return endTurn
	})})
	b.Allow = func(proto.PermissionRequest) bool { return true }
	s.Prompt("go")
	ag.called("session/prompt", 1)
	ag.ask("5", "session/request_permission", hermesPermission)
	if got := ag.replied("5"); got != `{"outcome":{"optionId":"allow_once","outcome":"selected"}}` {
		t.Fatalf("answered %s", got)
	}
	close(answered)
	if got := sk.wait(t, "ended:false"); slices.Contains(got, "needsYou") {
		t.Fatalf("needed you: %v", got)
	}
}

func TestOpenCodeShowsACallOnceItsInputComesAndAsksAboutIt(t *testing.T) {
	// What OpenCode 1.19.5 wrote for a command its config has it ask about.
	s, a, sk, b := start(t, opts{provider: "opencode", answer: script(opencodeNew, func(a *agent, _ string) string {
		a.update(`{"sessionUpdate":"tool_call","toolCallId":"call_1","title":"bash","kind":"execute","status":"pending","locations":[{"path":"/w"}],"rawInput":{"cwd":"/w"}}`)
		a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"call_1","status":"in_progress","kind":"execute","title":"date +%s","locations":[{"path":"/w"}],"rawInput":{"command":"date +%s","cwd":"/w"}}`)
		a.ask("9", "session/request_permission", `{"sessionId":"s1","toolCall":{"toolCallId":"call_1","title":"date +%s","kind":"execute","status":"pending","locations":[],"rawInput":{"command":"date +%s"}},
			"options":[{"optionId":"once","kind":"allow_once","name":"Allow once"},{"optionId":"always","kind":"allow_always","name":"Always allow"},{"optionId":"reject","kind":"reject_once","name":"Reject"}]}`)
		a.replied("9")
		a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"call_1","status":"in_progress","kind":"execute","title":"date +%s","locations":[{"path":"/w"}],"rawInput":{"command":"date +%s","cwd":"/w"},"content":[{"type":"content","content":{"type":"text","text":"1794378490\n"}}]}`)
		a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"call_1","status":"completed","title":"date +%s","content":[{"type":"content","content":{"type":"text","text":"1794378490\n"}}],"rawOutput":{"output":"1794378490\n","metadata":{"output":"1794378490\n","exit":0,"truncated":false}}}`)
		a.update(`{"sessionUpdate":"agent_message_chunk","messageId":"msg_1","content":{"type":"text","text":"1794378490"}}`)
		a.update(`{"sessionUpdate":"usage_update","used":23584,"size":200000,"cost":{"amount":0,"currency":"USD"}}`)
		return `{"stopReason":"end_turn","usage":{"inputTokens":736,"outputTokens":8,"totalTokens":23592,"cachedReadTokens":22848},"_meta":{}}`
	})})
	s.Prompt("run")
	req := asked(t, b)
	if req.ToolName != "shell" || req.Detail.Command != "date +%s" || !slices.Equal(req.Options, []proto.PermissionOption{{ID: "always", Label: "Always allow"}}) {
		t.Fatalf("asked %+v", req)
	}
	b.Resolve(req.RequestID, broker.Answer{Decision: "allow", Option: "always"})
	if got := a.replied("9"); got != `{"outcome":{"optionId":"always","outcome":"selected"}}` {
		t.Fatalf("answered %s", got)
	}
	equal(t, after(sk.wait(t, "ended:false"), "working")[1:], []string{
		"tool_start:call_1:shell:shell:date +%s", "needsYou", "working", "tool_end:call_1:true:1794378490\n",
		"assistant_draft:1794378490", "tokens:23584/200000", "assistant_text:1794378490", "result:true::736/8/22848", "ended:false"})
}

func TestARequestTheSessionDoesNotKnowIsRefusedButCursorsIsAnsweredEmpty(t *testing.T) {
	for provider, want := range map[string]string{"hermes": "!-32601 Method not found: fs/read_text_file", "cursor": `{}`} {
		_, a, _, _ := start(t, opts{provider: provider, answer: script(`{"sessionId":"s1"}`, says())})
		a.ask("3", "fs/read_text_file", `{"path":"/etc/passwd"}`)
		if got := a.replied("3"); got != want {
			t.Errorf("%s answered %s, want %s", provider, got, want)
		}
	}
}

func TestCursorsQuestionIsAskedAsAChatQuestionAndAnsweredByOptionID(t *testing.T) {
	const params = `{"toolCallId":"t1","title":"Pick","questions":[
		{"id":"q1","prompt":"Which database?","options":[{"id":"pg","label":"Postgres"},{"id":"lite","label":"SQLite"}]},
		{"id":"q2","prompt":"Which caches?","allowMultiple":true,"options":[{"id":"r","label":"Redis"},{"id":"m","label":"Memcached"},{"id":"n","label":"None"}]}]}`
	for name, c := range map[string]struct {
		answer broker.Answer
		want   string
	}{
		"answered": {broker.Answer{Decision: "allow", Answers: map[string]string{"Which database?": "SQLite", "Which caches?": "Redis, Memcached"}},
			`{"outcome":{"answers":[{"questionId":"q1","selectedOptionIds":["lite"]},{"questionId":"q2","selectedOptionIds":["r","m"]}],"outcome":"answered"}}`},
		"skipped": {broker.Answer{Decision: "deny"}, `{"outcome":{"outcome":"skipped","reason":"User skipped"}}`},
	} {
		t.Run(name, func(t *testing.T) {
			_, a, sk, b := start(t, opts{provider: "cursor", answer: script(`{"sessionId":"s1"}`, says())})
			a.ask("8", "cursor/ask_question", params)
			req := asked(t, b)
			want := `{"kind":"other","name":"AskUserQuestion","input":{"questions":[` +
				`{"question":"Which database?","options":[{"description":"","label":"Postgres"},{"description":"","label":"SQLite"}],"multiSelect":false},` +
				`{"question":"Which caches?","options":[{"description":"","label":"Redis"},{"description":"","label":"Memcached"},{"description":"","label":"None"}],"multiSelect":true}]}}`
			if got, _ := json.Marshal(req.Detail); req.ToolName != "AskUserQuestion" || string(got) != want {
				t.Fatalf("asked %s\n%s", req.ToolName, got)
			}
			sk.wait(t, "needsYou")
			b.Resolve(req.RequestID, c.answer)
			if got := a.replied("8"); got != c.want {
				t.Fatalf("answered %s\nwant %s", got, c.want)
			}
		})
	}
}

func TestGroksQuestionIsAnsweredByItsText(t *testing.T) {
	const params = `{"questions":[{"question":"Which one?","header":"Pick","options":[{"label":"A","description":"first"},{"label":"B"}],"multiSelect":false},
		{"question":"Which extras?","options":["x","y"],"multiSelect":true}]}`
	for method, c := range map[string]struct {
		answer broker.Answer
		want   string
	}{
		"_x.ai/ask_user_question": {broker.Answer{Decision: "allow", Answers: map[string]string{"Which one?": "B", "Which extras?": "x, y"}},
			`{"answers":{"Which extras?":["x","y"],"Which one?":"B"},"outcome":"accepted"}`},
		"x.ai/ask_user_question": {broker.Answer{}, `{"outcome":"skip_interview"}`},
	} {
		t.Run(method, func(t *testing.T) {
			_, a, _, b := start(t, opts{provider: "grok", answer: script(`{"sessionId":"s1"}`, says())})
			a.ask("8", method, params)
			req := asked(t, b)
			if got, _ := json.Marshal(req.Detail); !strings.Contains(string(got), `{"question":"Which one?","header":"Pick","options":[{"description":"first","label":"A"},{"description":"","label":"B"}],"multiSelect":false}`) {
				t.Fatalf("asked %s", got)
			}
			if c.answer.Decision == "" {
				b.Dismiss("a1\x008", "")
			} else {
				b.Resolve(req.RequestID, c.answer)
			}
			if got := a.replied("8"); got != c.want {
				t.Fatalf("answered %s\nwant %s", got, c.want)
			}
		})
	}
}

func TestPlansTodosAndSubAgentsOfCursorAndGrokShowInTheChat(t *testing.T) {
	_, a, sk, _ := start(t, opts{provider: "cursor", answer: script(`{"sessionId":"s1"}`, says())})
	a.ask("1", "cursor/create_plan", `{"toolCallId":"p","name":"Plan","plan":"# Do it"}`)
	a.ask("2", "cursor/update_todos", `{"merge":false,"todos":[{"id":"a","content":"one","status":"pending"},{"id":"b","content":"two","status":"pending"}]}`)
	a.say(`{"jsonrpc":"2.0","method":"cursor/update_todos","params":{"merge":true,"todos":[{"id":"a","status":"completed"}]}}`)
	a.ask("3", "cursor/task", `{"toolCallId":"sub1","description":"Look around","prompt":"look","subagentType":"explore","model":"fast"}`)
	a.ask("4", "cursor/task", `{"toolCallId":"sub1","description":"Look around","durationMs":1200}`)
	for id, want := range map[string]string{"1": `{"outcome":{"outcome":"accepted"}}`, "2": `{}`, "3": `{}`, "4": `{}`} {
		if got := a.replied(id); got != want {
			t.Errorf("%s answered %s, want %s", id, got, want)
		}
	}
	equal(t, after(sk.lines(), "bound:s1")[1:], []string{
		`tool_start:plan-1:ExitPlanMode:{"plan":"# Do it"}`,
		`tool_start:plan-2:TodoWrite:{"todos":[{"id":"a","content":"one","status":"pending"},{"id":"b","content":"two","status":"pending"}]}`,
		`tool_start:plan-3:TodoWrite:{"todos":[{"id":"a","content":"one","status":"completed"},{"id":"b","content":"two","status":"pending"}]}`,
		"tool_start:sub1:task:task:Look around",
		"tool_end:sub1:true",
	})

	_, g, gk, _ := start(t, opts{provider: "grok", answer: script(`{"sessionId":"s1"}`, says())})
	g.ask("1", "_x.ai/exit_plan_mode", `{"planContent":"# Grok's plan\n"}`)
	g.say(`{"jsonrpc":"2.0","method":"_x.ai/session_notification","params":{"sessionId":"s1","update":{"sessionUpdate":"turn_end","usage":{"totalTokens":4200}}}}`)
	if got := g.replied("1"); got != `{"outcome":"abandoned"}` {
		t.Fatalf("answered %s", got)
	}
	equal(t, after(gk.wait(t, "tokens:4200/0"), "bound:s1")[1:], []string{`tool_start:plan-1:ExitPlanMode:{"plan":"# Grok's plan"}`, "tokens:4200/0"})
}

// As cursor-agent 2026.11.11 tells a sub-agent to a client that takes them:
// its SubagentPublisher, and the task call as a call whose end tells no report.
// The tells of a call, Cursor sends whichever session made it.
func TestACursorSubAgentsWorkNestsUnderItsTaskWhichEndsWithItsLastMessage(t *testing.T) {
	s, _, sk, _ := start(t, opts{provider: "cursor", answer: script(`{"sessionId":"s1"}`, func(a *agent, _ string) string {
		a.update(`{"sessionUpdate":"tool_call","toolCallId":"sub1","title":"Task: Look around","kind":"other","rawInput":{"_toolName":"task","prompt":"look","description":"Look around","subagentType":"explore"}}`)
		a.update(`{"sessionUpdate":"subagent_spawned","subagentSessionId":"agent-7","name":"explore","task":"look","capabilities":{},
			"_meta":{"cursor":{"toolCallId":"sub1","agentId":"agent-7","model":"composer-2"}}}`)
		sayIn(a, "agent-7", `{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Look"}}`)
		sayIn(a, "agent-7", `{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"ing."}}`)
		sayIn(a, "agent-7", `{"sessionUpdate":"tool_call","toolCallId":"t1","title":"ls","kind":"execute","status":"pending","rawInput":{"command":"ls"}}`)
		sayIn(a, "agent-7", `{"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"completed","content":[{"type":"content","content":{"type":"text","text":"a.go"}}]}`)
		sayIn(a, "agent-7", `{"sessionUpdate":"tool_call","toolCallId":"t2","title":"Update TODOs: read a.go","kind":"other","rawInput":{"_toolName":"updateTodos","todos":[{"id":"x","content":"read a.go"}]}}`)
		sayIn(a, "agent-7", `{"sessionUpdate":"tool_call_update","toolCallId":"t2","status":"completed"}`)
		a.ask("1", "cursor/update_todos", `{"toolCallId":"t2","todos":[{"id":"x","content":"read a.go","status":"pending"}],"merge":false}`)
		a.replied("1")
		sayIn(a, "agent-7", `{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Found a.go"}}`)
		a.update(`{"sessionUpdate":"subagent_state_update","subagentSessionId":"agent-7","state":"completed","_meta":{"cursor":{"toolCallId":"sub1","agentId":"agent-7"}}}`)
		a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"sub1","status":"completed","rawOutput":{"durationMs":1200,"isBackground":false}}`)
		a.ask("2", "cursor/task", `{"toolCallId":"sub1","description":"Look around","prompt":"look","subagentType":"explore","agentId":"agent-7","durationMs":1200}`)
		a.replied("2")
		return endTurn
	})})
	s.Prompt("look around")
	equal(t, after(sk.wait(t, "ended:false"), "working"), []string{
		"working",
		"tool_start:sub1:task:task:Look around",
		"tool_model:sub1:composer-2",
		"sub1>assistant_text:Looking.",
		"sub1>tool_start:t1:shell:shell:ls",
		"tool_end:t1:true:a.go",
		"sub1>tool_start:t2:Update TODOs: read a.go:other:",
		"tool_end:t2:true",
		"sub1>assistant_text:Found a.go",
		"tool_end:sub1:true:Found a.go",
		"result:true:",
		"ended:false",
	})
}

// cursor-agent 2026.11.11 replays a task as its call and its sub-agent's
// start and end, without the sub-agent's work, which only its own store keeps.
func TestACursorHistoryShowsATaskWithItsSubAgentsModel(t *testing.T) {
	_, _, sk, _ := start(t, opts{provider: "cursor", saved: "s1", answer: func(a *agent, method string, _ json.RawMessage) string {
		if method == "session/load" {
			a.update(`{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"look around"}}`)
			a.update(`{"sessionUpdate":"tool_call","toolCallId":"replay-0-1","title":"Task: Look around","kind":"other","status":"completed","rawInput":{"_toolName":"task","prompt":"look","description":"Look around","subagentType":"explore"},"rawOutput":{"durationMs":1200,"isBackground":false}}`)
			a.update(`{"sessionUpdate":"subagent_spawned","subagentSessionId":"agent-7","name":"explore","task":"look","capabilities":{},"_meta":{"cursor":{"toolCallId":"replay-0-1","agentId":"agent-7","model":"composer-2"}}}`)
			a.update(`{"sessionUpdate":"subagent_state_update","subagentSessionId":"agent-7","state":"completed","_meta":{"cursor":{"toolCallId":"replay-0-1","agentId":"agent-7"}}}`)
		}
		return `{}`
	}})
	equal(t, sk.wait(t, "bound:s1"), []string{
		"loaded",
		"user:look around",
		"tool_start:replay-0-1:task:task:Look around",
		"tool_end:replay-0-1:true",
		"tool_model:replay-0-1:composer-2",
		"result:true:",
		"bound:s1",
	})
}

// sayIn says update in session sid.
func sayIn(a *agent, sid, update string) {
	a.say(`{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"` + sid + `","update":` + update + `}}`)
}

func grokTells(a *agent, update string) {
	a.say(`{"jsonrpc":"2.0","method":"_x.ai/session_notification","params":{"sessionId":"s1","update":` + update + `}}`)
}

// Trimmed from what grok 0.1 tells, spawning a sub-agent against a stub model.
func TestAGrokSubAgentsWorkNestsUnderItsCallWhichEndsWithItsReport(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "grok", answer: script(`{"sessionId":"s1"}`, func(a *agent, _ string) string {
		a.update(`{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Delegating."}}`)
		a.update(`{"sessionUpdate":"tool_call","toolCallId":"call_s","title":"spawn_subagent","rawInput":{"description":"List the files","prompt":"CHILD: run echo sub"},
			"_meta":{"x.ai/tool":{"name":"spawn_subagent","kind":"task","label":"Subagent"},"subagentBackground":true}}`)
		a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"call_s","kind":"other","title":"List the files","rawInput":{"variant":"Task","prompt":"CHILD: run echo sub","description":"List the files","run_in_background":true}}`)
		a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"call_s","status":"completed","content":[{"type":"content","content":{"type":"text","text":"Subagent started in background.\nsubagent_id: kid\ndescription: List the files"}}]}`)
		grokTells(a, `{"sessionUpdate":"subagent_spawned","subagent_id":"kid","parent_session_id":"s1","child_session_id":"kid","subagent_type":"general-purpose","description":"List the files","model":"stub"}`)
		sayIn(a, "kid", `{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"CHILD: run echo sub"}}`)
		sayIn(a, "kid", `{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Running."}}`)
		sayIn(a, "elsewhere", `{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Not ours."}}`)
		return endTurn
	})})
	s.Prompt("Delegate")
	sk.wait(t, "ended:false")
	sayIn(a, "kid", `{"sessionUpdate":"tool_call","toolCallId":"call_c","title":"run_terminal_command","rawInput":{"command":"echo sub"},"_meta":{"x.ai/tool":{"kind":"execute","label":"Run Command"}}}`)
	sayIn(a, "kid", `{"sessionUpdate":"tool_call_update","toolCallId":"call_c","status":"completed","content":[{"type":"content","content":{"type":"text","text":"sub\n"}}]}`)
	sayIn(a, "kid", `{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Done: "}}`)
	sayIn(a, "kid", `{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"sub"}}`)
	grokTells(a, `{"sessionUpdate":"subagent_finished","subagent_id":"kid","child_session_id":"kid","status":"completed","output":"Done: sub\n","will_wake":true}`)
	a.say(`{"jsonrpc":"2.0","method":"_x.ai/queue/changed","params":{"sessionId":"s1","entries":[],"runningPromptId":"subagent-completed-kid"}}`)
	a.update(`{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Wrapped up."}}`)
	grokTells(a, `{"sessionUpdate":"turn_completed","prompt_id":"subagent-completed-kid","stop_reason":"end_turn"}`)
	a.update(`{"sessionUpdate":"usage_update","used":1,"size":2}`)
	equal(t, after(sk.wait(t, "tokens:1/2"), "user:Delegate"), []string{
		"user:Delegate",
		"working",
		"assistant_draft:Delegating.",
		"assistant_text:Delegating.",
		"tool_start:call_s:task:task:List the files",
		"tool_background:call_s",
		"tool_model:call_s:stub",
		"call_s>user:CHILD: run echo sub",
		"result:true:",
		"ended:false",
		"call_s>assistant_text:Running.",
		"call_s>tool_start:call_c:shell:shell:echo sub",
		"tool_end:call_c:true:sub\n",
		"call_s>assistant_text:Done: sub",
		"tool_end:call_s:true:Done: sub\n",
		"working",
		"assistant_draft:Wrapped up.",
		"assistant_text:Wrapped up.",
		"result:true:",
		"ended:false",
		"tokens:1/2",
	})
}

// Grok starts the turn that takes a background sub-agent's report as soon as
// the turn under way ends, which it may tell before that turn's answer comes.
func TestAGrokTurnStartedAsTheOneBeforeEndsIsATurnOfItsOwnWhichPromptsWaitFor(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "grok", answer: script(`{"sessionId":"s1"}`, func(a *agent, text string) string {
		if text != "Delegate" {
			return endTurn
		}
		a.say(`{"jsonrpc":"2.0","method":"_x.ai/queue/changed","params":{"sessionId":"s1","entries":[],"runningPromptId":"p1"}}`)
		a.update(`{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Hi."}}`)
		grokTells(a, `{"sessionUpdate":"turn_completed","prompt_id":"p1","stop_reason":"end_turn"}`)
		a.say(`{"jsonrpc":"2.0","method":"_x.ai/queue/changed","params":{"sessionId":"s1","entries":[],"runningPromptId":"subagent-completed-kid"}}`)
		return endTurn
	})})
	s.Prompt("Delegate")
	sk.waitN(t, "working", 2)
	s.Prompt("Next")
	a.update(`{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Wrapped up."}}`)
	grokTells(a, `{"sessionUpdate":"turn_completed","prompt_id":"subagent-completed-kid","stop_reason":"end_turn"}`)
	equal(t, after(sk.waitN(t, "ended:false", 3), "user:Delegate"), []string{
		"user:Delegate",
		"working",
		"assistant_draft:Hi.",
		"assistant_text:Hi.",
		"result:true:",
		"ended:false",
		"working",
		"assistant_draft:Wrapped up.",
		"assistant_text:Wrapped up.",
		"result:true:",
		"ended:false",
		"user:Next",
		"working",
		"result:true:",
		"ended:false",
	})
}

// Trimmed from what grok 0.1 tells of a sub-agent the call waits for, which it names only as the call ends.
func TestAGrokSubAgentTheCallWaitsForNestsUnderItAndEndsItWithItsReport(t *testing.T) {
	s, _, sk, _ := start(t, opts{provider: "grok", answer: script(`{"sessionId":"s1"}`, func(a *agent, _ string) string {
		a.update(`{"sessionUpdate":"tool_call","toolCallId":"call_s","title":"spawn_subagent","rawInput":{"description":"List the files","prompt":"CHILD: run echo sub","run_in_background":false},
			"_meta":{"x.ai/tool":{"name":"spawn_subagent","kind":"task","label":"Subagent"},"subagentBackground":false}}`)
		a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"call_s","kind":"other","title":"List the files","rawInput":{"variant":"Task","prompt":"CHILD: run echo sub","description":"List the files","run_in_background":false}}`)
		grokTells(a, `{"sessionUpdate":"subagent_spawned","subagent_id":"kid","parent_session_id":"s1","child_session_id":"kid","description":"List the files","model":"stub"}`)
		sayIn(a, "kid", `{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"CHILD: run echo sub"}}`)
		sayIn(a, "kid", `{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Done: sub"}}`)
		a.say(`{"jsonrpc":"2.0","method":"_x.ai/session_notification","params":{"sessionId":"kid","update":{"sessionUpdate":"turn_completed","prompt_id":"p2","stop_reason":"end_turn","usage":{"totalTokens":240}}}}`)
		grokTells(a, `{"sessionUpdate":"subagent_finished","subagent_id":"kid","child_session_id":"kid","status":"completed","output":"Done: sub\n","will_wake":false}`)
		a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"call_s","status":"completed","content":[{"type":"content","content":{"type":"text","text":"Done: sub\n\n\n<subagent_meta>id=kid, tool_calls=1</subagent_meta>\n\n<subagent_result>\nsubagent_id: kid\nTo continue this subagent's conversation, use resume_from=\"kid\".\n</subagent_result>"}}],
			"rawOutput":{"type":"SubagentCompleted","output":"Done: sub\n","subagent_id":"kid"}}`)
		grokTells(a, `{"sessionUpdate":"turn_completed","prompt_id":"p1","stop_reason":"end_turn","usage":{"totalTokens":480}}`)
		return endTurn
	})})
	s.Prompt("Delegate")
	equal(t, after(sk.wait(t, "ended:false"), "working"), []string{
		"working",
		"tool_start:call_s:task:task:List the files",
		"tool_model:call_s:stub",
		"call_s>user:CHILD: run echo sub",
		"call_s>assistant_text:Done: sub",
		"tool_end:call_s:true:Done: sub",
		"tokens:480/0",
		"result:true:",
		"ended:false",
	})
}

// As grok 0.1 replays a session whose sub-agent ran on past its call, and that sub-agent's own.
func TestAGrokHistoryLoadsEachSubAgentsSessionUnderItsCall(t *testing.T) {
	_, a, sk, _ := start(t, opts{provider: "grok", saved: "s1", answer: func(a *agent, method string, params json.RawMessage) string {
		replays := func(update string) {
			a.say(`{"jsonrpc":"2.0","method":"_x.ai/session/update","params":{"sessionId":"s1","update":` + update + `,"_meta":{"isReplay":true}}}`)
		}
		switch {
		case method != "session/load":
		case strings.Contains(string(params), `"kid"`):
			sayIn(a, "kid", `{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"CHILD: run echo sub"}}`)
			sayIn(a, "kid", `{"sessionUpdate":"tool_call","toolCallId":"call_c","title":"Execute `+"`echo sub`"+`","kind":"execute","status":"completed","content":[{"type":"content","content":{"type":"text","text":"sub\n"}}],"rawInput":{"variant":"Bash","command":"echo sub"}}`)
			sayIn(a, "kid", `{"sessionUpdate":"session_info_update","title":"Its own title"}`)
			sayIn(a, "kid", `{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Done: sub"}}`)
		default:
			a.update(`{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"Delegate"}}`)
			a.update(`{"sessionUpdate":"tool_call","toolCallId":"call_s","title":"List the files","status":"completed","content":[{"type":"content","content":{"type":"text","text":"Subagent started in background.\nsubagent_id: kid\ndescription: List the files"}}],
				"rawInput":{"variant":"Task","prompt":"CHILD: run echo sub","description":"List the files","run_in_background":true},"rawOutput":{"type":"Text","text":"Subagent started in background."}}`)
			replays(`{"sessionUpdate":"subagent_spawned","subagent_id":"kid","parent_session_id":"s1","child_session_id":"kid","description":"List the files","model":"stub"}`)
			replays(`{"sessionUpdate":"turn_completed","prompt_id":"p1","stop_reason":"end_turn"}`)
			replays(`{"sessionUpdate":"subagent_finished","subagent_id":"kid","child_session_id":"kid","status":"completed","output":"Done: sub\n","will_wake":true}`)
			a.update(`{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"<system-reminder>\nWhile you were idle, 1 background subagent completed"}}`)
			a.update(`{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Wrapped up."}}`)
		}
		return `{}`
	}})
	equal(t, sk.wait(t, "call_s>assistant_text:Done: sub"), []string{
		"loaded",
		"user:Delegate",
		"tool_start:call_s:task:task:List the files",
		"tool_end:call_s:true:Subagent started in background.\nsubagent_id: kid\ndescription: List the files",
		"tool_model:call_s:stub",
		"tool_end:call_s:true:Done: sub\n",
		"result:true:",
		"assistant_text:Wrapped up.",
		"result:true:",
		"bound:s1",
		"call_s>user:CHILD: run echo sub",
		"call_s>tool_start:call_c:shell:shell:echo sub",
		"tool_end:call_c:true:sub\n",
		"call_s>assistant_text:Done: sub",
	})
	equal(t, a.called("session/load", 2), []string{`{"cwd":"/work","mcpServers":[],"sessionId":"s1"}`, `{"cwd":"/work","mcpServers":[],"sessionId":"kid"}`})
}

// grok 0.1 replays a sub-agent's start as often before the call that spawned it as after.
func TestAGrokHistoryShowsTheModelOfASubAgentToldOfBeforeItsCall(t *testing.T) {
	_, _, sk, _ := start(t, opts{provider: "grok", saved: "s1", answer: func(a *agent, method string, params json.RawMessage) string {
		if method == "session/load" && strings.Contains(string(params), `"s1"`) {
			a.say(`{"jsonrpc":"2.0","method":"_x.ai/session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"subagent_spawned","child_session_id":"kid","description":"List the files","model":"stub"}}}`)
			a.update(`{"sessionUpdate":"tool_call","toolCallId":"call_s","title":"List the files","status":"completed","content":[{"type":"content","content":{"type":"text","text":"Subagent started in background.\nsubagent_id: kid"}}],
				"rawInput":{"variant":"Task","prompt":"CHILD: run echo sub","description":"List the files","run_in_background":true}}`)
		}
		return `{}`
	}})
	equal(t, after(sk.wait(t, "bound:s1"), "tool_start:call_s:task:task:List the files")[:3], []string{
		"tool_start:call_s:task:task:List the files",
		"tool_model:call_s:stub",
		"tool_end:call_s:true:Subagent started in background.\nsubagent_id: kid",
	})
}

func TestAGrokHistoryEndsASubAgentsCallWithTheReportToldBeforeIt(t *testing.T) {
	_, a, sk, _ := start(t, opts{provider: "grok", saved: "s1", answer: func(a *agent, method string, params json.RawMessage) string {
		if method == "session/load" && strings.Contains(string(params), `"s1"`) {
			a.say(`{"jsonrpc":"2.0","method":"_x.ai/session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"subagent_finished","child_session_id":"kid","status":"completed","output":"Done: sub\n"}}}`)
			a.update(`{"sessionUpdate":"tool_call","toolCallId":"call_s","title":"List the files","status":"completed","content":[{"type":"content","content":{"type":"text","text":"Subagent started in background.\nsubagent_id: kid"}}],
				"rawInput":{"variant":"Task","prompt":"CHILD: run echo sub","description":"List the files","run_in_background":true}}`)
		}
		return `{}`
	}})
	equal(t, after(sk.wait(t, "bound:s1"), "tool_start:call_s:task:task:List the files")[:3], []string{
		"tool_start:call_s:task:task:List the files",
		"tool_end:call_s:true:Subagent started in background.\nsubagent_id: kid",
		"tool_end:call_s:true:Done: sub\n",
	})
	a.called("session/load", 2)
}

// A sub-agent still running in the agent a pocketd before this one left goes on telling of its work in its own session.
func TestWhatAnotherSessionTellsWhileASavedOneLoadsIsNotItsHistory(t *testing.T) {
	_, _, sk, _ := start(t, opts{provider: "grok", saved: "s1", answer: func(a *agent, method string, _ json.RawMessage) string {
		if method == "session/load" {
			a.update(`{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"Hi"}}`)
			sayIn(a, "kid", `{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Not ours."}}`)
			a.update(`{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Hello."}}`)
		}
		return `{}`
	}})
	equal(t, sk.wait(t, "bound:s1"), []string{"loaded", "user:Hi", "assistant_text:Hello.", "result:true:", "bound:s1"})
}

// As opencode 1.19.5 replays a session whose task ran, and then the task's own session.
func TestAnOpenCodeHistoryLoadsEachTasksSessionUnderItOnceItHasLoaded(t *testing.T) {
	_, a, sk, _ := start(t, opts{provider: "opencode", saved: "s1", answer: func(a *agent, method string, params json.RawMessage) string {
		switch {
		case method != "session/load":
			return `{}`
		case strings.Contains(string(params), `"kid"`):
			sayIn(a, "kid", `{"sessionUpdate":"user_message_chunk","messageId":"m1","content":{"type":"text","text":"read NOTE.md"}}`)
			sayIn(a, "kid", `{"sessionUpdate":"agent_message_chunk","messageId":"m2","content":{"type":"text","text":"hel"}}`)
			sayIn(a, "kid", `{"sessionUpdate":"agent_message_chunk","messageId":"m2","content":{"type":"text","text":"lo"}}`)
			return `{}`
		}
		a.update(`{"sessionUpdate":"user_message_chunk","messageId":"u1","content":{"type":"text","text":"Delegate"}}`)
		a.update(`{"sessionUpdate":"tool_call","toolCallId":"t1","title":"Read the note","kind":"think","status":"completed","rawInput":{"description":"Read the note","prompt":"read NOTE.md","subagent_type":"general"},
			"content":[{"type":"content","content":{"type":"text","text":"<task_result>\nhello\n</task_result>"}}],"rawOutput":{"metadata":{"sessionId":"kid","model":{"modelID":"glm","providerID":"opencode"}}}}`)
		a.update(`{"sessionUpdate":"agent_message_chunk","messageId":"a1","content":{"type":"text","text":"It said hello."}}`)
		return opencodeNew
	}})
	equal(t, sk.wait(t, "t1>assistant_text:hello"), []string{
		"loaded",
		"user:Delegate",
		"tool_start:t1:task:task:Read the note",
		"tool_model:t1:opencode/glm",
		"tool_end:t1:true:hello",
		"assistant_text:It said hello.",
		"result:true:",
		"bound:s1",
		"model:opencode/big-pickle",
		"t1>user:read NOTE.md",
		"t1>assistant_text:hello",
	})
	equal(t, a.called("session/load", 2), []string{`{"cwd":"/work","mcpServers":[],"sessionId":"s1"}`, `{"cwd":"/work","mcpServers":[],"sessionId":"kid"}`})
}

func TestTheMessagesOfSubAgentsWritingAtOnceEachGoInWhole(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "grok", answer: script(`{"sessionId":"s1"}`, func(a *agent, _ string) string {
		for _, kid := range []string{"k1", "k2"} {
			a.update(`{"sessionUpdate":"tool_call","toolCallId":"c` + kid + `","title":"spawn_subagent","rawInput":{"variant":"Task","description":"` + kid + `","run_in_background":true}}`)
			a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"c` + kid + `","status":"completed","content":[{"type":"content","content":{"type":"text","text":"subagent_id: ` + kid + `"}}]}`)
		}
		return endTurn
	})})
	s.Prompt("Delegate")
	sk.wait(t, "ended:false")
	sayIn(a, "k1", `{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Hel"}}`)
	sayIn(a, "k2", `{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Wor"}}`)
	sayIn(a, "k1", `{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"lo"}}`)
	sayIn(a, "k2", `{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"ld"}}`)
	grokTells(a, `{"sessionUpdate":"subagent_finished","child_session_id":"k1","status":"completed","output":"Hello"}`)
	grokTells(a, `{"sessionUpdate":"subagent_finished","child_session_id":"k2","status":"completed","output":"World"}`)
	equal(t, after(sk.wait(t, "tool_end:ck2:true:World"), "ended:false"), []string{
		"ended:false",
		"ck1>assistant_text:Hello",
		"tool_end:ck1:true:Hello",
		"ck2>assistant_text:World",
		"tool_end:ck2:true:World",
	})
}

// As opencode 1.19.5 tells a task: its sub-agent's session only as the call ends, and that session's work only once it is loaded.
func TestAnOpenCodeSubAgentsWorkLoadsUnderItsTaskOnceItEnds(t *testing.T) {
	loaded := make(chan struct{})
	turn := script(opencodeNew, func(a *agent, _ string) string {
		a.update(`{"sessionUpdate":"tool_call","toolCallId":"t1","title":"task","kind":"think","status":"pending","locations":[],"rawInput":{}}`)
		a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"in_progress","kind":"think","title":"Read the note","locations":[],"rawInput":{"description":"Read the note","prompt":"read NOTE.md","subagent_type":"general"}}`)
		a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"completed","title":"Read the note","content":[{"type":"content","content":{"type":"text","text":"<task id=\"kid\" state=\"completed\">\n<task_result>\nhello\n</task_result>\n</task>"}}],
			"rawOutput":{"output":"<task id=\"kid\" state=\"completed\">\n<task_result>\nhello\n</task_result>\n</task>","metadata":{"parentSessionId":"s1","sessionId":"kid","model":{"modelID":"glm-5.3-flash-free","providerID":"opencode"},"truncated":false}}}`)
		<-loaded
		return endTurn
	})
	s, a, sk, _ := start(t, opts{provider: "opencode", answer: func(a *agent, method string, params json.RawMessage) string {
		if method != "session/load" {
			return turn(a, method, params)
		}
		sayIn(a, "kid", `{"sessionUpdate":"user_message_chunk","messageId":"m1","content":{"type":"text","text":"read NOTE.md"}}`)
		sayIn(a, "kid", `{"sessionUpdate":"tool_call","toolCallId":"c1","title":"NOTE.md","kind":"read","status":"pending","locations":[{"path":"/work/NOTE.md"}],"rawInput":{"filePath":"/work/NOTE.md"}}`)
		sayIn(a, "kid", `{"sessionUpdate":"tool_call_update","toolCallId":"c1","status":"completed","title":"NOTE.md","content":[{"type":"content","content":{"type":"text","text":"hello"}}]}`)
		sayIn(a, "kid", `{"sessionUpdate":"agent_message_chunk","messageId":"m2","content":{"type":"text","text":"hello"}}`)
		sayIn(a, "kid", `{"sessionUpdate":"available_commands_update","availableCommands":[]}`)
		close(loaded)
		return opencodeNew
	}})
	s.Prompt("Delegate")
	equal(t, after(sk.wait(t, "ended:false"), "working"), []string{
		"working",
		"tool_start:t1:task:task:Read the note",
		"tool_model:t1:opencode/glm-5.3-flash-free",
		"tool_end:t1:true:hello",
		"t1>user:read NOTE.md",
		"t1>tool_start:c1:read:read:/work/NOTE.md",
		"tool_end:c1:true:hello",
		"t1>assistant_text:hello",
		"result:true:",
		"ended:false",
	})
	equal(t, a.called("session/load", 1), []string{`{"cwd":"/work","mcpServers":[],"sessionId":"kid"}`})
}

// As opencode 1.19.5 runs a task whose sub-agent asks to run a command: it
// passes on what that sub-agent asks only once its session is loaded, which
// its server names while the call runs.
func TestAnOpenCodeSubAgentsSessionLoadsAsItStartsSoWhatItAsksReachesTheUser(t *testing.T) {
	children := make(chan string, 1)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/session/s1/children" || r.URL.Query().Get("directory") != "/work" {
			http.NotFound(w, r)
			return
		}
		select {
		case kids := <-children:
			w.Write([]byte(kids))
		default:
			w.Write([]byte(`[]`))
		}
	}))
	defer server.Close()
	loaded := make(chan struct{})
	turn := script(opencodeNew, func(a *agent, _ string) string {
		a.update(`{"sessionUpdate":"tool_call","toolCallId":"t1","title":"task","kind":"think","status":"pending","locations":[],"rawInput":{}}`)
		a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"in_progress","kind":"think","title":"Run echo","locations":[],"rawInput":{"description":"Run echo","prompt":"run echo sub","subagent_type":"general"}}`)
		children <- `[{"id":"old","title":"Run echo (@general subagent)"},{"id":"other","title":"Read the note (@general subagent)"},{"id":"kid","title":"Run echo (@general subagent)"}]`
		<-loaded
		sayIn(a, "kid", `{"sessionUpdate":"tool_call","toolCallId":"c1","title":"bash","kind":"execute","status":"pending","locations":[{"path":"/work"}],"rawInput":{"cwd":"/work"}}`)
		sayIn(a, "kid", `{"sessionUpdate":"tool_call_update","toolCallId":"c1","status":"in_progress","kind":"execute","title":"echo sub","locations":[{"path":"/work"}],"rawInput":{"command":"echo sub","cwd":"/work"}}`)
		a.ask("7", "session/request_permission", `{"sessionId":"kid","toolCall":{"toolCallId":"c1","title":"echo sub","kind":"execute","status":"pending","locations":[],"rawInput":{"command":"echo sub"}},`+
			`"options":[{"optionId":"once","kind":"allow_once","name":"Allow once"},{"optionId":"always","kind":"allow_always","name":"Always allow"},{"optionId":"reject","kind":"reject_once","name":"Reject"}]}`)
		a.replied("7")
		sayIn(a, "kid", `{"sessionUpdate":"tool_call_update","toolCallId":"c1","status":"completed","title":"echo sub","content":[{"type":"content","content":{"type":"text","text":"sub\n"}}]}`)
		sayIn(a, "kid", `{"sessionUpdate":"agent_message_chunk","messageId":"m2","content":{"type":"text","text":"sub"}}`)
		a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"completed","title":"Run echo","content":[{"type":"content","content":{"type":"text","text":"<task id=\"kid\" state=\"completed\">\n<task_result>\nsub\n</task_result>\n</task>"}}],
			"rawOutput":{"output":"<task id=\"kid\" state=\"completed\">\n<task_result>\nsub\n</task_result>\n</task>","metadata":{"parentSessionId":"s1","sessionId":"kid","model":{"modelID":"glm-5.3-flash-free","providerID":"opencode"}}}}`)
		return endTurn
	})
	s, a, sk, b := start(t, opts{provider: "opencode", answer: func(a *agent, method string, params json.RawMessage) string {
		if method != "session/load" {
			return turn(a, method, params)
		}
		sayIn(a, "kid", `{"sessionUpdate":"user_message_chunk","messageId":"m1","content":{"type":"text","text":"run echo sub"}}`)
		close(loaded)
		return opencodeNew
	}})
	// "old" is a sub-agent an earlier task ran, loaded once that task ended.
	s.mu.Lock()
	s.children["old"] = "t0"
	s.mu.Unlock()
	s.Server([]string{"pipe", "opencode", "acp", "--port", strings.TrimPrefix(server.URL, "http://127.0.0.1:"), "--pocketd"}, nil)
	s.Prompt("Delegate")
	req := asked(t, b)
	if req.ToolName != "shell" || req.Detail.Command != "echo sub" {
		t.Fatalf("asked %+v", req)
	}
	b.Resolve(req.RequestID, broker.Answer{Decision: "allow"})
	if got := a.replied("7"); got != `{"outcome":{"optionId":"once","outcome":"selected"}}` {
		t.Fatalf("answered %s", got)
	}
	equal(t, after(sk.wait(t, "ended:false"), "working"), []string{
		"working",
		"tool_start:t1:task:task:Run echo",
		"t1>user:run echo sub",
		"t1>tool_start:c1:shell:shell:echo sub",
		"needsYou",
		"working",
		"tool_end:c1:true:sub\n",
		"t1>assistant_text:sub",
		"tool_model:t1:opencode/glm-5.3-flash-free",
		"tool_end:t1:true:sub",
		"result:true:",
		"ended:false",
	})
	equal(t, a.called("session/load", 1), []string{`{"cwd":"/work","mcpServers":[],"sessionId":"kid"}`})
}

// As after a pocketd restart while OpenCode runs a task: the history replays the call still running.
func TestAnOpenCodeSubAgentStillRunningInTheHistoryLoadsSoWhatItAsksReachesTheUser(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(`[{"id":"kid","title":"Run echo (@general subagent)"}]`))
	}))
	defer server.Close()
	_, a, _, _ := start(t, opts{provider: "opencode", saved: "s1", left: Running, args: []string{"opencode", "acp", "--port", strings.TrimPrefix(server.URL, "http://127.0.0.1:")},
		answer: func(a *agent, method string, params json.RawMessage) string {
			if method == "session/load" && strings.Contains(string(params), `"s1"`) {
				a.update(`{"sessionUpdate":"user_message_chunk","messageId":"m0","content":{"type":"text","text":"Delegate"}}`)
				a.update(`{"sessionUpdate":"tool_call","toolCallId":"t1","title":"task","kind":"think","status":"pending","locations":[],"rawInput":{}}`)
				a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"in_progress","kind":"think","title":"Run echo","locations":[],"rawInput":{"description":"Run echo","prompt":"run echo sub","subagent_type":"general"}}`)
			}
			return opencodeNew
		}})
	equal(t, a.called("session/load", 2)[1:], []string{`{"cwd":"/work","mcpServers":[],"sessionId":"kid"}`})
}

// OpenCode's server asks for basic auth once OPENCODE_SERVER_PASSWORD is set,
// as user OPENCODE_SERVER_USERNAME or "opencode", and for none without it.
func TestOpenCodesServerIsAskedAsTheLoginTheAgentsEnvSetsForIt(t *testing.T) {
	for user, env := range map[string][]string{
		"opencode": {"OPENCODE_SERVER_PASSWORD=secret"},
		"me":       {"OPENCODE_SERVER_USERNAME=me", "OPENCODE_SERVER_PASSWORD=secret"},
		"":         {"OPENCODE_SERVER_USERNAME=me"},
	} {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if u, p, ok := r.BasicAuth(); ok != (user != "") || u != user || ok && p != "secret" {
				http.Error(w, "Authentication required", http.StatusUnauthorized)
				return
			}
			w.Write([]byte(`[{"id":"kid","title":"Run echo (@general subagent)"}]`))
		}))
		_, a, _, _ := start(t, opts{provider: "opencode", saved: "s1", left: Running, args: []string{"opencode", "acp", "--port", strings.TrimPrefix(server.URL, "http://127.0.0.1:")}, env: env,
			answer: func(a *agent, method string, params json.RawMessage) string {
				if method == "session/load" && strings.Contains(string(params), `"s1"`) {
					a.update(`{"sessionUpdate":"tool_call","toolCallId":"t1","title":"task","kind":"think","status":"pending","locations":[],"rawInput":{}}`)
					a.update(`{"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"in_progress","kind":"think","title":"Run echo","locations":[],"rawInput":{"description":"Run echo","prompt":"run echo sub","subagent_type":"general"}}`)
				}
				return opencodeNew
			}})
		equal(t, a.called("session/load", 2)[1:], []string{`{"cwd":"/work","mcpServers":[],"sessionId":"kid"}`})
		server.Close()
	}
}

// hermesDelegates is what Hermes's ACP server tells of a delegate_task call, as its acp_adapter/tools.py builds it.
func hermesDelegates(a *agent, goal, result string) {
	a.update(`{"content":[{"content":{"text":` + strconv.Quote("Delegating task:\n"+goal) + `,"type":"text"},"type":"content"}],"kind":"execute","locations":[],"title":"delegate_task: ` + goal + `","toolCallId":"tc-d","sessionUpdate":"tool_call"}`)
	a.update(`{"content":[{"content":{"text":` + strconv.Quote(result) + `,"type":"text"},"type":"content"}],"kind":"execute","status":"completed","toolCallId":"tc-d","sessionUpdate":"tool_call_update"}`)
}

// A top-level delegation runs in the background: its call ends with a handle
// at once, and the sub-agent tells its work only in a transcript beside a manifest.
func TestAHermesSubAgentShowsUnderItsCallWhatItsTranscriptTellsAndEndsItWithItsSummary(t *testing.T) {
	defer func(p time.Duration) { DelegationPoll = p }(DelegationPoll)
	DelegationPoll = 10 * time.Millisecond
	dir := t.TempDir()
	log, manifest := filepath.Join(dir, "task-0.log"), filepath.Join(dir, "manifest.json")
	os.WriteFile(manifest, []byte(`{"delegation_id":"deleg_1","tasks":[{"index":0,"status":"running"}]}`), 0o600)
	os.WriteFile(log, []byte("=== Hermes subagent live transcript ===\ngoal: List the files\n========================================\n"+
		"12:00:01 user     | kickoff: List the files\n12:00:01 start    | List the files\n"), 0o600)
	dispatched, _ := json.Marshal(map[string]any{"status": "dispatched", "mode": "background", "count": 1, "delegation_id": "deleg_1",
		"goals": []string{"List the files"}, "live_transcripts": []string{log}})
	s, _, sk, _ := start(t, opts{provider: "hermes", answer: script(hermesNew, func(a *agent, text string) string {
		if text == "Delegate" {
			hermesDelegates(a, "List the files", string(dispatched))
		}
		return endTurn
	})})
	s.Prompt("Delegate")
	sk.wait(t, "result:true:")
	f, _ := os.OpenFile(log, os.O_APPEND|os.O_WRONLY, 0)
	f.WriteString("12:00:02 think    | Listing them.\n12:00:02 tool     | -> terminal(ls)\n12:00:03 result   | terminal ok 0.1s: {\"output\": \"a.txt\", \"exit_code\": 0}\n" +
		"12:00:04 assistant| Found a.txt\n12:00:04 final    | status=completed duration=3.1s summary: Found a.txt\n12:00:04 final    | end status=")
	sk.wait(t, "tc-d>assistant_text:Found a.txt")
	f.WriteString("completed\n")
	f.Close()
	os.WriteFile(manifest, []byte(`{"delegation_id":"deleg_1","completed":"2026-11-11 12:00:04","tasks":[{"index":0,"status":"completed"}]}`), 0o600)
	equal(t, after(sk.wait(t, "ended:false"), "working"), []string{
		"working",
		"tool_start:tc-d:task:task:List the files",
		"tc-d>user:List the files",
		"tool_background:tc-d:detached",
		"result:true:",
		"tc-d>thinking:Listing them.",
		"tc-d>tool_start:tc-d.1:shell:shell:ls",
		`tool_end:tc-d.1:true:{"output": "a.txt", "exit_code": 0}`,
		"tc-d>assistant_text:Found a.txt",
		"tool_end:tc-d:true:Found a.txt",
		"working",
		"result:true:",
		"ended:false",
	})
}

// Hermes writes the manifest just before it dispatches, and rewrites it as its sub-agents end.
func TestAHermesDelegationRunsOnUntilItsManifestSaysEveryTaskItListsHasEnded(t *testing.T) {
	for _, manifest := range []string{"", `{"completed":"2026-11-11 12:00:04","tasks":[]}`,
		`{"completed":"2026-11-11 12:00:04","tasks":[{"index":0,"status":"pending"}]}`,
		`{"completed":"2026-11-11 12:00:04","tasks":[{"index":0,"status":"finalizing"}]}`} {
		s, _, sk, dir := handsBack(t, "", nil)
		os.Remove(filepath.Join(dir, "manifest.json"))
		if manifest != "" {
			writeManifest(dir, manifest)
		}
		s.Prompt("Delegate")
		sk.wait(t, "result:true:")
		time.Sleep(5 * DelegationPoll)
		if got := sk.lines(); slices.ContainsFunc(got, func(l string) bool { return strings.HasPrefix(l, "tool_end:tc-d") }) {
			t.Fatalf("ended on %q: %v", manifest, got)
		}
		writeManifest(dir, completed)
		sk.wait(t, "tool_end:tc-d:true")
	}
}

// delegated is a delegation's transcript and manifest in a folder of their
// own, and the handle its call ends with.
func delegated(t *testing.T, manifest, transcript string) (dir, handle string) {
	dir = t.TempDir()
	os.WriteFile(filepath.Join(dir, "manifest.json"), []byte(manifest), 0o600)
	os.WriteFile(filepath.Join(dir, "task-0.log"), []byte("=== Hermes subagent live transcript ===\n"+transcript), 0o600)
	b, _ := json.Marshal(map[string]any{"status": "dispatched", "mode": "background", "delegation_id": "deleg_1", "live_transcripts": []string{filepath.Join(dir, "task-0.log")}})
	return dir, string(b)
}

// restoresDelegation answers a session/load with a history whose last turn delegated in the background.
func restoresDelegation(handle string) func(*agent, string, json.RawMessage) string {
	return func(a *agent, method string, _ json.RawMessage) string {
		if method != "session/load" {
			return `{}`
		}
		a.update(`{"content":{"text":"Delegate","type":"text"},"sessionUpdate":"user_message_chunk"}`)
		hermesDelegates(a, "List the files", handle)
		return hermesNew
	}
}

func delegationLines(lines []string) []string {
	return slices.DeleteFunc(lines, func(l string) bool { return !strings.Contains(l, "tc-d") })
}

// Its sub-agents ran in the Hermes that ran the call, which is gone, so nothing writes to its transcript again.
func TestAHermesDelegationTheHistoryNamesEndsWithWhatItsTranscriptToldOnceItsHermesIsGone(t *testing.T) {
	defer func(p time.Duration) { DelegationPoll = p }(DelegationPoll)
	DelegationPoll = 10 * time.Millisecond
	_, handle := delegated(t, `{"tasks":[{"index":0,"status":"running"}]}`, "12:00:02 assistant| Listing them\n")
	_, _, sk, _ := start(t, opts{provider: "hermes", saved: "s1", answer: restoresDelegation(handle)})
	equal(t, delegationLines(sk.wait(t, "tool_end:tc-d:false")), []string{
		"tool_start:tc-d:task:task:List the files",
		"tc-d>user:List the files",
		"tool_background:tc-d:detached",
		"tc-d>assistant_text:Listing them",
		"tool_end:tc-d:false",
	})
}

func TestAHermesDelegationTheHistoryNamesRunsOnInAHermesCarriedOverFromTheLastPocketd(t *testing.T) {
	defer func(p time.Duration) { DelegationPoll = p }(DelegationPoll)
	DelegationPoll = 10 * time.Millisecond
	dir, handle := delegated(t, `{"tasks":[{"index":0,"status":"running"}]}`, "12:00:02 assistant| Listing them\n")
	_, _, sk, _ := start(t, opts{provider: "hermes", saved: "s1", carried: true, answer: restoresDelegation(handle)})
	sk.wait(t, "tc-d>assistant_text:Listing them")
	time.Sleep(5 * DelegationPoll)
	if got := sk.lines(); slices.ContainsFunc(got, func(l string) bool { return strings.HasPrefix(l, "tool_end:tc-d") }) {
		t.Fatalf("ended while its sub-agent runs: %v", got)
	}
	os.WriteFile(filepath.Join(dir, "manifest.json"), []byte(`{"completed":"2026-11-11 12:00:04","tasks":[{"index":0,"status":"completed"}]}`), 0o600)
	sk.wait(t, "tool_end:tc-d:true")
}

// Hermes prunes a delegation's transcripts after a week.
func TestAHermesDelegationTheHistoryNamesWhoseTranscriptIsGoneEndsAsItsCallDid(t *testing.T) {
	defer func(p time.Duration) { DelegationPoll = p }(DelegationPoll)
	DelegationPoll = 10 * time.Millisecond
	dir, handle := delegated(t, `{}`, "")
	os.RemoveAll(dir)
	_, _, sk, _ := start(t, opts{provider: "hermes", saved: "s1", answer: restoresDelegation(handle)})
	sk.wait(t, "tool_end:tc-d:true")
}

// With Hermes's delegation.independent_completions on, each unit of tasks marks the manifest completed as it ends.
func TestAHermesDelegationEndsOnceEveryOneOfItsSubAgentsHas(t *testing.T) {
	defer func(p time.Duration) { DelegationPoll = p }(DelegationPoll)
	DelegationPoll = 10 * time.Millisecond
	dir, handle := delegated(t, `{"tasks":[{"index":0,"status":"running"},{"index":1,"status":"running"}]}`, "")
	s, _, sk, _ := start(t, opts{provider: "hermes", answer: script(hermesNew, func(a *agent, text string) string {
		if text == "Delegate" {
			hermesDelegates(a, "List the files", handle)
		}
		return endTurn
	})})
	s.Prompt("Delegate")
	sk.wait(t, "result:true:")
	os.WriteFile(filepath.Join(dir, "manifest.json"), []byte(`{"completed":"2026-11-11 12:00:04","tasks":[{"index":0,"status":"completed"},{"index":1,"status":"running"}]}`), 0o600)
	time.Sleep(5 * DelegationPoll)
	if got := sk.lines(); slices.ContainsFunc(got, func(l string) bool { return strings.HasPrefix(l, "tool_end:tc-d") }) {
		t.Fatalf("ended while a sub-agent runs: %v", got)
	}
	os.WriteFile(filepath.Join(dir, "manifest.json"), []byte(`{"completed":"2026-11-11 12:00:09","tasks":[{"index":0,"status":"completed"},{"index":1,"status":"completed"}]}`), 0o600)
	sk.wait(t, "tool_end:tc-d:true")
}

const completed = `{"completed":"2026-11-11 12:00:04","tasks":[{"index":0,"status":"completed"}]}`

func writeManifest(dir, manifest string) {
	os.WriteFile(filepath.Join(dir, "manifest.json"), []byte(manifest), 0o600)
}

// handsBack runs Hermes delegating in the background on "Delegate", as turn
// does if given, and replying "Took it." to anything else, with what the
// sub-agent's transcript says. Its manifest is in dir.
func handsBack(t *testing.T, transcript string, turn func(delegate func()) string) (s *Session, a *agent, sk *sink, dir string) {
	t.Helper()
	p := DelegationPoll
	t.Cleanup(func() { DelegationPoll = p })
	DelegationPoll = 10 * time.Millisecond
	dir, handle := delegated(t, `{"tasks":[{"index":0,"status":"running"}]}`, transcript)
	if turn == nil {
		turn = func(delegate func()) string {
			delegate()
			return endTurn
		}
	}
	s, a, sk, _ = start(t, opts{provider: "hermes", answer: script(hermesNew, func(a *agent, text string) string {
		if text == "Delegate" {
			return turn(func() { hermesDelegates(a, "List the files", handle) })
		}
		a.update(`{"content":{"text":"Took it.","type":"text"},"messageId":"m2","sessionUpdate":"agent_message_chunk"}`)
		return endTurn
	})})
	return s, a, sk, dir
}

// handedBack is what pocketd handed Hermes in the nth prompt.
func handedBack(t *testing.T, a *agent, n int) []transcriptTail {
	t.Helper()
	head, data, _ := strings.Cut(promptOf(json.RawMessage(a.called("session/prompt", n)[n-1])), "\n\n")
	var tails []transcriptTail
	if !strings.HasPrefix(head, handoffHeader) || json.Unmarshal([]byte(data), &tails) != nil {
		t.Fatalf("prompted %q", head)
	}
	return tails
}

// Hermes's ACP server never takes what its background sub-agents hand back,
// as its CLI does, so the turn goes on until pocketd has handed it over.
func TestATurnHandsHermesWhatItsBackgroundSubAgentsToldOnceTheyEnd(t *testing.T) {
	transcript := "12:00:04 final    | status=completed duration=3.1s summary: Found a.txt\n"
	s, a, sk, dir := handsBack(t, transcript, nil)
	s.Prompt("Delegate")
	sk.wait(t, "result:true:")
	writeManifest(dir, completed)
	equal(t, after(sk.wait(t, "ended:false"), "tool_background:tc-d:detached"), []string{
		"tool_background:tc-d:detached",
		"result:true:",
		"tool_end:tc-d:true:Found a.txt",
		"working",
		"assistant_draft:Took it.",
		"assistant_text:Took it.",
		"result:true:",
		"ended:false",
	})
	tails := handedBack(t, a, 2)
	if len(tails) != 1 || tails[0].DelegationID != "deleg_1" || filepath.Base(tails[0].Path) != "task-0.log" ||
		tails[0].Tail != "=== Hermes subagent live transcript ===\n"+transcript {
		t.Fatalf("handed back %+v", tails)
	}
}

func TestAHandoffHandsTheEndOfEachTranscriptFromItsFirstWholeCharacter(t *testing.T) {
	s, a, sk, dir := handsBack(t, strings.Repeat("é", tailSize)+"x", nil)
	s.Prompt("Delegate")
	sk.wait(t, "result:true:")
	writeManifest(dir, completed)
	if tails := handedBack(t, a, 2); len(tails) != 1 || tails[0].Tail != strings.Repeat("é", tailSize/2-1)+"x" {
		t.Fatalf("handed back %+v", tails)
	}
}

func TestSubAgentsThatEndMidTurnAreHandedBackOnceTheTurnEnds(t *testing.T) {
	hold := make(chan struct{})
	s, a, sk, dir := handsBack(t, "", func(delegate func()) string {
		delegate()
		<-hold
		return endTurn
	})
	s.Prompt("Delegate")
	sk.wait(t, "tool_background:tc-d:detached")
	writeManifest(dir, completed)
	sk.wait(t, "tool_end:tc-d:true")
	close(hold)
	equal(t, after(sk.wait(t, "ended:false"), "tool_end:tc-d:true"), []string{
		"tool_end:tc-d:true",
		"result:true:",
		"working",
		"assistant_draft:Took it.",
		"assistant_text:Took it.",
		"result:true:",
		"ended:false",
	})
	handedBack(t, a, 2)
}

func TestAPromptSentWhileHermesSubAgentsRunWaitsBehindTheirHandoff(t *testing.T) {
	s, a, sk, dir := handsBack(t, "", nil)
	s.Prompt("Delegate")
	sk.wait(t, "result:true:")
	s.Prompt("Next")
	writeManifest(dir, completed)
	sk.waitN(t, "ended:false", 2)
	handedBack(t, a, 2)
	if got := a.called("session/prompt", 3); promptOf(json.RawMessage(got[2])) != "Next" {
		t.Fatalf("prompted %v", got)
	}
}

func TestAStopEndsTheTurnThatWaitsOnHermesSubAgentsAndDropsTheirHandoff(t *testing.T) {
	s, a, sk, dir := handsBack(t, "", nil)
	s.Prompt("Delegate")
	sk.wait(t, "result:true:")
	s.Cancel()
	sk.wait(t, "stopped")
	writeManifest(dir, completed)
	sk.wait(t, "tool_end:tc-d:true")
	time.Sleep(5 * DelegationPoll)
	if got := a.called("session/prompt", 1); len(got) != 1 {
		t.Fatalf("handed back after a stop: %v", got)
	}
}

// Hermes may tell of a delegation it dispatched after the user stopped the turn, before its answer to the stop.
func TestADelegationToldAfterAStopIsNotHandedBack(t *testing.T) {
	hold := make(chan struct{})
	s, a, sk, dir := handsBack(t, "", func(delegate func()) string {
		<-hold
		delegate()
		return `{"stopReason":"cancelled"}`
	})
	s.Prompt("Delegate")
	sk.wait(t, "working")
	s.Cancel()
	close(hold)
	sk.wait(t, "stopped")
	writeManifest(dir, completed)
	sk.wait(t, "tool_end:tc-d:true")
	time.Sleep(5 * DelegationPoll)
	if got := a.called("session/prompt", 1); len(got) != 1 {
		t.Fatalf("handed back after a stop: %v", got)
	}
}

func TestAHistoryShowsTheTurnThatHandedHermesBackItsSubAgentsWorkWithoutItsPrompt(t *testing.T) {
	_, _, sk, _ := start(t, opts{provider: "hermes", saved: "s1", answer: func(a *agent, method string, _ json.RawMessage) string {
		if method != "session/load" {
			return `{}`
		}
		a.update(`{"content":{"text":"Delegate","type":"text"},"sessionUpdate":"user_message_chunk"}`)
		a.update(`{"content":{"text":"On it.","type":"text"},"sessionUpdate":"agent_message_chunk"}`)
		a.update(`{"content":{"text":` + strconv.Quote(handoffPrompt([]transcriptTail{{Path: "/t/task-0.log", Tail: "done"}})) + `,"type":"text"},"sessionUpdate":"user_message_chunk"}`)
		a.update(`{"content":{"text":"Took it.","type":"text"},"sessionUpdate":"agent_message_chunk"}`)
		return hermesNew
	}})
	equal(t, sk.wait(t, "bound:s1")[:6], []string{
		"loaded",
		"user:Delegate",
		"assistant_text:On it.",
		"result:true:",
		"assistant_text:Took it.",
		"result:true:",
	})
}

// A delegation Hermes runs in the call, as when its pool is full, ends the call with its results.
func TestAHermesDelegationRunInItsCallEndsItWithItsResults(t *testing.T) {
	results := "Delegation results: 1 task in 3.2s\n\n✅ Task 1: completed (gpt-6, 3.1s)\nFound a.txt\nTools: terminal"
	s, _, sk, _ := start(t, opts{provider: "hermes", answer: script(hermesNew, func(a *agent, _ string) string {
		hermesDelegates(a, "List the files", results)
		a.update(`{"content":[{"content":{"text":"Delegating task","type":"text"},"type":"content"}],"kind":"execute","locations":[],"title":"delegate_task: list","toolCallId":"tc-l","sessionUpdate":"tool_call"}`)
		return endTurn
	})})
	s.Prompt("Delegate")
	equal(t, after(sk.wait(t, "ended:false"), "working"), []string{
		"working",
		"tool_start:tc-d:task:task:List the files",
		"tc-d>user:List the files",
		"tool_end:tc-d:true:" + results,
		"tool_start:tc-l:shell:shell:delegate_task: list",
		"result:true:",
		"ended:false",
	})
}

func TestAnAgentThatNeverOpensGivesBackThePromptsThatWaitedAndRefusesTheNext(t *testing.T) {
	a := &agent{t: t, in: make(chan []byte, 64), out: make(chan []byte, 64), closed: make(chan struct{}), replies: map[string]string{},
		answer: func(*agent, string, json.RawMessage) string {
			return `!{"code":-32000,"message":"Authentication required"}`
		}}
	go a.serve()
	sk := &sink{}
	s := NewSession("a1", "hermes", sk, nil)
	t.Cleanup(s.Close)
	s.Prompt("queued")
	if unsent, err := s.Open(a, "/work", "", state.Launch{Access: "ask"}, Ended); err == nil || err.Error() != "Authentication required" || !slices.Equal(unsent, []string{"queued"}) {
		t.Fatalf("open: %v, unsent %v", err, unsent)
	}
	if err := s.Prompt("go"); err == nil || err.Error() != "Hermes Agent didn't start: Authentication required" {
		t.Fatalf("prompt: %v", err)
	}
	if got := sk.lines(); len(got) != 0 {
		t.Fatalf("said %v", got)
	}
}

func TestAPromptSentBeforeTheSessionOpensStartsItsFirstTurn(t *testing.T) {
	a := &agent{t: t, in: make(chan []byte, 64), out: make(chan []byte, 64), closed: make(chan struct{}), replies: map[string]string{},
		answer: script(hermesNew, says())}
	go a.serve()
	sk := &sink{}
	s := NewSession("a1", "hermes", sk, nil)
	t.Cleanup(s.Close)
	s.Prompt("first")
	if _, err := s.Open(a, "/work", "", state.Launch{Access: "ask"}, Ended); err != nil {
		t.Fatal(err)
	}
	equal(t, sk.wait(t, "ended:false"), []string{"bound:s1", "model:openai-codex:gpt-6-sol", "user:first", "working", "result:true:", "ended:false"})
}

func TestAReplyHermesSendsAgainWholeReplacesWhatItStreamed(t *testing.T) {
	// What Hermes 0.22.5 wrote once the user denied its edit: the reply it streamed, then that reply as a plugin rewrote it.
	s, _, sk, _ := start(t, opts{provider: "hermes", answer: script(hermesNew, says(
		`{"content":{"text":"REFUSED","type":"text"},"messageId":"m1","sessionUpdate":"agent_message_chunk"}`,
		`{"content":{"text":"-H","type":"text"},"messageId":"m1","sessionUpdate":"agent_message_chunk"}`,
		`{"content":{"text":"\n\nREFUSED-H\n\nFile-mutation verifier: 1 file edit(s) FAILED","type":"text"},"messageId":"m1","sessionUpdate":"agent_message_chunk"}`,
	))})
	s.Prompt("write")
	equal(t, after(sk.wait(t, "ended:false"), "working")[1:], []string{
		"assistant_draft:REFUSED", "assistant_draft:-H", "assistant_draft:\n\nFile-mutation verifier: 1 file edit(s) FAILED",
		"assistant_text:REFUSED-H\n\nFile-mutation verifier: 1 file edit(s) FAILED", "result:true:", "ended:false"})
}

func TestOpenCodesEditShowsTheDiffItsInputHoldsAndIsForgottenOnceItEnds(t *testing.T) {
	// What OpenCode 1.19.5 wrote for a write it didn't ask about.
	s, _, sk, _ := start(t, opts{provider: "opencode", launch: state.Launch{Access: "edits"}, answer: script(opencodeNew, says(
		`{"sessionUpdate":"tool_call","toolCallId":"call_1","title":"write","kind":"edit","status":"pending","locations":[],"rawInput":{}}`,
		`{"sessionUpdate":"tool_call_update","toolCallId":"call_1","status":"in_progress","kind":"edit","title":"write","locations":[{"path":"/w/a.txt"}],"rawInput":{"filePath":"/w/a.txt","content":"gamma\n"}}`,
		`{"sessionUpdate":"tool_call_update","toolCallId":"call_1","status":"completed","kind":"edit","title":"a.txt","content":[{"type":"content","content":{"type":"text","text":"Wrote file successfully."}}]}`,
		`{"sessionUpdate":"tool_call","toolCallId":"call_2","title":"edit","kind":"edit","status":"in_progress","rawInput":{"filePath":"/w/a.txt","oldString":"gamma","newString":"delta\nepsilon"}}`,
		`{"sessionUpdate":"tool_call","toolCallId":"call_3","title":"edit","kind":"edit","status":"in_progress","rawInput":{"filePath":"/w/b.txt"}}`,
		`{"sessionUpdate":"tool_call_update","toolCallId":"call_3","status":"completed","content":[{"type":"diff","path":"/w/b.txt","oldText":"a","newText":"b"}]}`,
	))})
	s.Prompt("write")
	equal(t, after(sk.wait(t, "ended:false"), "working")[1:], []string{
		"tool_start:call_1:write:write:/w/a.txt:+1-0", "tool_end:call_1:true:Wrote file successfully.",
		"tool_start:call_2:edit:edit:/w/a.txt:+2-1",
		// An edit that tells nothing of its change shows once its diff comes.
		"tool_start:call_3:edit:edit:/w/b.txt:+1-1", "tool_end:call_3:true",
		"result:true:", "ended:false"})
	s.mu.Lock()
	defer s.mu.Unlock()
	if len(s.tools) != 1 || s.tools["call_2"] == nil {
		t.Fatalf("kept %d calls", len(s.tools))
	}
}

func TestAPermissionNamedForItsQuestionLeavesTheCallItsNameAndKind(t *testing.T) {
	// What OpenCode 1.19.5 asked before a write outside the project.
	s, a, sk, b := start(t, opts{provider: "opencode", launch: state.Launch{Access: "edits"}, answer: script(opencodeNew, func(a *agent, _ string) string {
		a.update(`{"sessionUpdate":"tool_call","toolCallId":"call_1","title":"write","kind":"edit","status":"pending","locations":[],"rawInput":{}}`)
		a.ask("9", "session/request_permission", `{"sessionId":"s1","toolCall":{"toolCallId":"call_1","title":"/out","kind":"other","status":"pending","rawInput":{"filepath":"/out/a.txt","parentDir":"/out"}},
			"options":[{"optionId":"once","kind":"allow_once","name":"Allow once"},{"optionId":"reject","kind":"reject_once","name":"Reject"}]}`)
		a.replied("9")
		return endTurn
	})})
	s.Prompt("write outside")
	// Edits are allowed here, but what is asked is no edit.
	req := asked(t, b)
	if req.ToolName != "edit" || req.Detail.Kind != "edit" || req.Detail.Path != "/out/a.txt" {
		t.Fatalf("asked %+v", req)
	}
	b.Resolve(req.RequestID, broker.Answer{Decision: "deny"})
	if got := a.replied("9"); got != `{"outcome":{"optionId":"reject","outcome":"selected"}}` {
		t.Fatalf("answered %s", got)
	}
	if got := after(sk.wait(t, "ended:false"), "working")[1]; got != "tool_start:call_1:edit:edit:/out/a.txt" {
		t.Fatalf("the call shows as %q", got)
	}
}

func TestGrokSwitchesPlanAndFullAccessOnlyByStartingAgainAndKeepsTheAutoModeItStartedOn(t *testing.T) {
	const respawn = "Grok Build takes plan mode and full access only as it starts"
	for _, c := range []struct {
		launch   state.Launch
		to       string
		respawns bool
		want     string
	}{
		{state.Launch{Access: "ask", Plan: true}, "edits", true, respawn},
		{state.Launch{Access: "full"}, "ask", true, respawn},
		{state.Launch{Access: "ask"}, "full", true, respawn},
		{state.Launch{Access: "auto"}, "full", true, respawn},
		{state.Launch{Access: "ask"}, "auto", false, ""},
		{state.Launch{Access: "auto"}, "ask", false, "Grok Build started its session on auto access and keeps it, so start a new session to switch"},
	} {
		s, _, _, _ := start(t, opts{provider: "grok", launch: c.launch, answer: script(`{"sessionId":"s1"}`, says())})
		got := ""
		if err := s.Access(c.to); err != nil {
			got = err.Error()
		}
		if s.Respawns(c.to) != c.respawns || got != c.want {
			t.Errorf("%+v to %s: respawns %v, %q", c.launch, c.to, s.Respawns(c.to), got)
		}
	}
}

func TestGrokFxAndHermesPlanningAreRefusedWhatIsNoReadOrSearch(t *testing.T) {
	const options = `"options":[{"optionId":"o1","kind":"allow_once","name":"Allow"},{"optionId":"o2","kind":"allow_always","name":"Always"},{"optionId":"o3","kind":"reject_once","name":"Reject"}]`
	for _, c := range []struct{ provider, access string }{{"grok", "full"}, {"fx", "full"}, {"hermes", "full"}, {"grok", "ask"}, {"fx", "ask"}, {"hermes", "ask"}} {
		t.Run(c.provider+" "+c.access, func(t *testing.T) {
			_, a, _, b := start(t, opts{provider: c.provider, launch: state.Launch{Access: c.access, Plan: true}, answer: script(`{"sessionId":"s1"}`, says())})
			a.ask("1", "session/request_permission", `{"toolCall":{"toolCallId":"c1","title":"Edit a.go","kind":"edit"},`+options+`}`)
			a.ask("2", "session/request_permission", `{"toolCall":{"toolCallId":"c2","title":"Read a.go","kind":"read"},`+options+`}`)
			if got := a.replied("1"); got != `{"outcome":{"optionId":"o3","outcome":"selected"}}` {
				t.Fatalf("an edit while planning: %s", got)
			}
			if got := a.replied("2"); got != `{"outcome":{"optionId":"o1","outcome":"selected"}}` || len(b.Open()) != 0 {
				t.Fatalf("a read while planning: %s", got)
			}
		})
	}
}

func TestFxAndHermesLeaveTheirPlanByAnAccessSwitch(t *testing.T) {
	const edit = `{"toolCall":{"toolCallId":"c1","title":"Edit a.go","kind":"edit"},"options":[{"optionId":"o1","kind":"allow_once","name":"Allow"},{"optionId":"o3","kind":"reject_once","name":"Reject"}]}`
	for provider, mode := range map[string]string{"fx": "auto", "hermes": "accept_edits"} {
		t.Run(provider, func(t *testing.T) {
			s, a, _, _ := start(t, opts{provider: provider, launch: state.Launch{Access: "ask", Plan: true}, answer: script(`{"sessionId":"s1"}`, says())})
			if err := s.Access("edits"); err != nil {
				t.Fatal(err)
			}
			if got := a.called("session/set_mode", 2)[1]; got != `{"modeId":"`+mode+`","sessionId":"s1"}` {
				t.Fatalf("left its plan for %s", got)
			}
			a.ask("1", "session/request_permission", edit)
			if got := a.replied("1"); got != `{"outcome":{"optionId":"o1","outcome":"selected"}}` {
				t.Fatalf("an edit once building: %s", got)
			}
		})
	}
}

func TestTheModelIsTheModelOptionNotTheProviderListedBeforeIt(t *testing.T) {
	// fx's options, as MonoCode's fixture has them, and Antigravity's shape of the same.
	for name, c := range map[string]struct{ options, id string }{
		"by id":       {`[{"id":"provider","category":"model","currentValue":"gateway"},{"id":"model","category":"model","currentValue":"zai/glm-5.2"},{"id":"mode","category":"mode","currentValue":"code"}]`, "model"},
		"by category": {`[{"id":"provider","category":"model","currentValue":"google"},{"id":"fast","category":"model","type":"boolean","currentValue":true},{"id":"llm","category":"model","currentValue":"zai/glm-5.2"}]`, "llm"},
	} {
		t.Run(name, func(t *testing.T) {
			s, a, sk, _ := start(t, opts{provider: "fx", answer: script(`{"sessionId":"s1","configOptions":`+c.options+`}`, says())})
			equal(t, sk.wait(t, "model:zai/glm-5.2"), []string{"bound:s1", "model:zai/glm-5.2"})
			if err := s.Settings("zai/glm-6", ""); err != nil {
				t.Fatal(err)
			}
			equal(t, a.called("session/set_config_option", 1), []string{`{"configId":"` + c.id + `","sessionId":"s1","value":"zai/glm-6"}`})
		})
	}
}

func TestCursorsEffortGoesToItsEffortOptionNotTheThinkingSwitch(t *testing.T) {
	for name, c := range map[string]struct{ options, id string }{
		"by id":       {`[{"id":"model","category":"model"},{"id":"thinking","category":"thought_level","type":"boolean","currentValue":true},{"id":"effort","category":"thought_level","currentValue":"medium"}]`, "effort"},
		"by category": {`[{"id":"thinking","category":"thought_level","currentValue":"on"},{"id":"depth","category":"thought_level","currentValue":"medium"}]`, "depth"},
	} {
		t.Run(name, func(t *testing.T) {
			s, a, _, _ := start(t, opts{provider: "cursor", launch: state.Launch{Effort: "high"}, answer: script(`{"sessionId":"s1","configOptions":`+c.options+`}`, says())})
			if err := s.Settings("", "low"); err != nil {
				t.Fatal(err)
			}
			equal(t, a.called("session/set_config_option", 2), []string{
				`{"configId":"` + c.id + `","sessionId":"s1","value":"high"}`, `{"configId":"` + c.id + `","sessionId":"s1","value":"low"}`})
		})
	}
}

func TestAnAgentWhoseOnlyEffortOptionIsThinkingHasItsEffortSetThere(t *testing.T) {
	options := `[{"id":"model","category":"model"},{"id":"thinking","category":"thought_level","currentValue":"high","options":[{"value":"low"},{"value":"high"}]}]`
	s, a, _, _ := start(t, opts{provider: "antigravity", launch: state.Launch{Effort: "high"}, answer: script(`{"sessionId":"s1","configOptions":`+options+`}`, says())})
	if err := s.Settings("", "low"); err != nil {
		t.Fatal(err)
	}
	equal(t, a.called("session/set_config_option", 2), []string{
		`{"configId":"thinking","sessionId":"s1","value":"high"}`, `{"configId":"thinking","sessionId":"s1","value":"low"}`})
}

func TestAStopTheAgentNeverAnswersEndsTheTurnSoTheNextPromptStarts(t *testing.T) {
	was := StopWait
	StopWait = 50 * time.Millisecond
	t.Cleanup(func() { StopWait = was })
	late, done := make(chan struct{}), make(chan struct{})
	s, a, sk, _ := start(t, opts{provider: "antigravity", answer: script(`{"sessionId":"s1"}`, func(a *agent, text string) string {
		if text == "slow" {
			<-late
			return `{"stopReason":"cancelled"}`
		}
		<-done
		return endTurn
	})})
	s.Prompt("slow")
	a.called("session/prompt", 1)
	s.Cancel()
	sk.wait(t, "stopped")
	s.Prompt("next")
	a.called("session/prompt", 2)
	// The stopped prompt's answer, come late, ends no turn but its own.
	close(late)
	time.Sleep(50 * time.Millisecond)
	close(done)
	equal(t, after(sk.wait(t, "ended:false"), "user:slow"), []string{
		"user:slow", "working", "result:false:interrupted", "stopped", "user:next", "working", "result:true:", "ended:false"})
}

func TestWhatGrokAndCursorNameTheirOwnWayIsRead(t *testing.T) {
	// The shapes MonoCode reads from Grok and Cursor beside ACP's own.
	s, a, sk, b := start(t, opts{provider: "grok", answer: script(`{"sessionId":"s1"}`, func(a *agent, _ string) string {
		a.update(`{"session_update":"agent_message_chunk","text":"snake"}`)
		a.update(`{"type":"tool_call","tool_call_id":"c1","title":"Run","kind":"execute","rawInput":{"command":"ls"}}`)
		a.update(`{"sessionUpdate":"tool_call_update","toolCall":{"toolCallId":"c1","status":"completed"}}`)
		a.update(`{"sessionUpdate":"tool_call","tool_call":{"tool_call_id":"c2","title":"Read a.go","kind":"read","status":"completed"}}`)
		a.ask("7", "session/request_permission", `{"subject":{"toolCallId":"c3","kind":"execute","command":"rm -rf build","title":"Shell"},"options":[{"optionId":"no","kind":"reject_once","name":"Reject"}]}`)
		a.replied("7")
		return endTurn
	})})
	s.Prompt("go")
	if req := asked(t, b); req.ToolName != "shell" || req.Detail.Command != "rm -rf build" {
		t.Fatalf("asked %+v", req)
	} else {
		b.Resolve(req.RequestID, broker.Answer{Decision: "deny"})
	}
	if got := a.replied("7"); got != `{"outcome":{"optionId":"no","outcome":"selected"}}` {
		t.Fatalf("answered %s", got)
	}
	equal(t, after(sk.wait(t, "ended:false"), "working")[1:], []string{
		"assistant_draft:snake", "assistant_text:snake", "tool_start:c1:shell:shell:ls", "tool_end:c1:true",
		"tool_start:c2:read:read:Read a.go", "tool_end:c2:true", "needsYou", "working", "result:true:", "ended:false"})
}

func TestARequestTheSessionDoesNotKnowIsRefusedWhileItStillOpens(t *testing.T) {
	refused := make(chan string, 1)
	start(t, opts{provider: "hermes", answer: func(a *agent, method string, _ json.RawMessage) string {
		if method == "session/new" {
			a.ask("3", "fs/read_text_file", `{"path":"/etc/passwd"}`)
			for deadline := time.Now().Add(2 * time.Second); time.Now().Before(deadline); time.Sleep(5 * time.Millisecond) {
				a.mu.Lock()
				r, ok := a.replies["3"]
				a.mu.Unlock()
				if ok {
					refused <- r
					break
				}
			}
			return hermesNew
		}
		return `{}`
	}})
	select {
	case got := <-refused:
		if got != "!-32601 Method not found: fs/read_text_file" {
			t.Fatalf("answered %s", got)
		}
	default:
		t.Fatal("it waited for the session to open")
	}
}

func TestAnAgentThatNeverAnswersFailsToOpenSayingWhatItLeftUnanswered(t *testing.T) {
	was := OpenWait
	OpenWait = 20 * time.Millisecond
	t.Cleanup(func() { OpenWait = was })
	a := &agent{t: t, in: make(chan []byte, 64), out: make(chan []byte, 64), closed: make(chan struct{}), replies: map[string]string{},
		answer: func(a *agent, _ string, _ json.RawMessage) string {
			<-a.closed
			return `{}`
		}}
	go a.serve()
	t.Cleanup(a.Close)
	s := NewSession("a1", "grok", &sink{}, nil)
	t.Cleanup(s.Close)
	if _, err := s.Open(a, "/work", "", state.Launch{Access: "ask"}, Ended); err == nil || err.Error() != "it didn't answer initialize in 20ms" {
		t.Fatalf("open: %v", err)
	}
}

func TestAnAgentsRefusalSaysItsReasonOnce(t *testing.T) {
	for _, c := range []struct {
		e    Error
		want string
	}{
		{Error{Message: "Internal error", Data: json.RawMessage(`{"details":"model_resolution_failed"}`)}, "Internal error: model_resolution_failed"},
		{Error{Message: "Authentication required", Data: json.RawMessage(`{"message":"Authentication required. Please run 'agent login' first."}`)}, "Authentication required. Please run 'agent login' first."},
		{Error{Message: "Authentication required"}, "Authentication required"},
	} {
		if got := c.e.Error(); got != c.want {
			t.Errorf("%q, want %q", got, c.want)
		}
	}
}

// compacts opens provider's session, whose agent answers the call that
// compacts it with what compact gives.
func compacts(t *testing.T, provider string, compact func(a *agent) string) (*Session, *agent, *sink) {
	s, a, sk, _ := start(t, opts{provider: provider, answer: func(a *agent, method string, params json.RawMessage) string {
		switch {
		case method == "session/new":
			return `{"sessionId":"s1"}`
		case method == "_x.ai/compact_conversation", method == "session/prompt" && promptOf(params) == "/compact":
			return compact(a)
		case method == "session/prompt":
			return endTurn
		}
		return `{}`
	}})
	return s, a, sk
}

func TestGrokCompactsByItsOwnCallAndHoldsThePromptSentMeanwhile(t *testing.T) {
	release := make(chan struct{})
	s, a, sk := compacts(t, "grok", func(a *agent) string {
		<-release
		// As Grok 1.0.77 tells of the compaction it was asked for.
		a.say(`{"jsonrpc":"2.0","method":"_x.ai/session_notification","params":{"sessionId":"s1","update":{"sessionUpdate":"auto_compact_completed","tokens_before":1200,"tokens_after":120,"summary_preview":null}}}`)
		return `{}`
	})
	if err := s.Compact(); err != nil {
		t.Fatal(err)
	}
	sk.wait(t, "compacting")
	if err := s.Compact(); err == nil || err.Error() != "Grok Build is compacting already" {
		t.Fatalf("compacting twice: %v", err)
	}
	s.Prompt("next")
	close(release)
	equal(t, after(sk.wait(t, "ended:false"), "compacting"), []string{
		"compacting", "tokens:120/0", "compacted:manual", "compactingEnded", "user:next", "working", "result:true:", "ended:false"})
	equal(t, a.called("_x.ai/compact_conversation", 1), []string{`{"sessionId":"s1"}`})
}

func TestOpenCodeCompactsByItsCommandAndShowsNoneOfItsSummary(t *testing.T) {
	s, a, sk := compacts(t, "opencode", func(a *agent) string {
		a.update(`{"sessionUpdate":"agent_thought_chunk","messageId":"p1","content":{"type":"text","text":"Fill template."}}`)
		a.update(`{"sessionUpdate":"agent_message_chunk","messageId":"m1","content":{"type":"text","text":"## Objective"}}`)
		a.update(`{"sessionUpdate":"usage_update","used":432,"size":200000}`)
		return `{"stopReason":"end_turn","_meta":{}}`
	})
	if err := s.Compact(); err != nil {
		t.Fatal(err)
	}
	equal(t, after(sk.wait(t, "compactingEnded"), "compacting"), []string{"compacting", "tokens:432/200000", "compacted:manual", "compactingEnded"})
	equal(t, a.called("session/prompt", 1), []string{`{"prompt":[{"text":"/compact","type":"text"}],"sessionId":"s1"}`})
}

func TestACompactionThatFailsIsANoticeWithTheAgentsReason(t *testing.T) {
	s, _, sk := compacts(t, "grok", func(*agent) string {
		return `!{"code":-32603,"message":"Internal error","data":{"kind":"compact_failed","message":"degenerate summary (8 chars for ~2012 input tokens)"}}`
	})
	s.Compact()
	equal(t, after(sk.wait(t, "compactingEnded"), "compacting"), []string{
		"compacting", "notice:Grok Build couldn't compact: Internal error: degenerate summary (8 chars for ~2012 input tokens)", "compactingEnded"})
}

func TestAStopEndsACompactionOnceTheAgentAnswersAndShowsNoneOfIt(t *testing.T) {
	cancelled := make(chan struct{})
	s, a, sk := compacts(t, "opencode", func(a *agent) string {
		<-cancelled
		// OpenCode goes on with its summary until it takes the cancel.
		a.update(`{"sessionUpdate":"agent_message_chunk","messageId":"m1","content":{"type":"text","text":"## Objective"}}`)
		return `{"stopReason":"cancelled"}`
	})
	s.Compact()
	sk.wait(t, "compacting")
	s.Prompt("dropped")
	s.Cancel()
	s.Prompt("after")
	if got := a.called("session/prompt", 1); len(got) != 1 {
		t.Fatalf("a prompt went while the compaction was still open: %v", got)
	}
	close(cancelled)
	equal(t, after(sk.wait(t, "ended:false"), "compacting"), []string{"compacting", "user:after", "working", "result:true:", "ended:false"})
	equal(t, a.called("session/prompt", 2), []string{
		`{"prompt":[{"text":"/compact","type":"text"}],"sessionId":"s1"}`, `{"prompt":[{"text":"after","type":"text"}],"sessionId":"s1"}`})
}

func TestACompactionTheAgentNeverEndsAfterAStopEndsOnceStopWaitIsUp(t *testing.T) {
	defer func(w time.Duration) { StopWait = w }(StopWait)
	StopWait = 50 * time.Millisecond
	release := make(chan struct{})
	t.Cleanup(func() { close(release) })
	s, _, sk := compacts(t, "grok", func(*agent) string {
		<-release
		return `{}`
	})
	s.Compact()
	sk.wait(t, "compacting")
	s.Cancel()
	s.Prompt("after")
	equal(t, after(sk.wait(t, "ended:false"), "compacting"), []string{"compacting", "user:after", "working", "result:true:", "ended:false"})
}

func TestACompactionAskedForMidTurnRunsOnceTheTurnEnds(t *testing.T) {
	release := make(chan struct{})
	s, a, sk, _ := start(t, opts{provider: "grok", answer: func(a *agent, method string, params json.RawMessage) string {
		switch method {
		case "session/new":
			return `{"sessionId":"s1"}`
		case "session/prompt":
			<-release
			return endTurn
		}
		return `{}`
	}})
	s.Prompt("slow")
	a.called("session/prompt", 1)
	if err := s.Compact(); err != nil {
		t.Fatal(err)
	}
	if got := sk.lines(); slices.Contains(got, "compacting") {
		t.Fatalf("compacted mid-turn: %v", got)
	}
	close(release)
	equal(t, after(sk.wait(t, "compactingEnded"), "user:slow"), []string{
		"user:slow", "working", "result:true:", "ended:false", "compacting", "compacted:manual", "compactingEnded"})
	equal(t, a.called("_x.ai/compact_conversation", 1), []string{`{"sessionId":"s1"}`})
}

func TestAStopDropsTheCompactionAskedForMidTurn(t *testing.T) {
	s, a, sk, _ := start(t, opts{provider: "grok", answer: func(a *agent, method string, _ json.RawMessage) string {
		switch method {
		case "session/new":
			return `{"sessionId":"s1"}`
		case "session/prompt":
			a.called("session/cancel", 1)
			return `{"stopReason":"cancelled"}`
		}
		return `{}`
	}})
	s.Prompt("slow")
	a.called("session/prompt", 1)
	s.Compact()
	s.Cancel()
	sk.wait(t, "stopped")
	if got := a.called("_x.ai/compact_conversation", 0); got != nil {
		t.Fatalf("compacted after the stop: %v", got)
	}
}

func TestARestartMidCompactionEndsTheCompactionWhenGrokAnswersIt(t *testing.T) {
	_, a, sk, _ := start(t, opts{provider: "grok", saved: "s1", left: Running, answer: func(a *agent, method string, _ json.RawMessage) string {
		return `{"sessionId":"s1"}`
	}})
	sk.wait(t, "bound:s1")
	a.say(`{"jsonrpc":"2.0","id":7,"result":{}}`)
	equal(t, after(sk.wait(t, "ended:false"), "result:true:"), []string{"result:true:", "ended:false"})
}

func TestALateAnswerToACallThisPocketdGaveUpOnEndsNoTurnLeftRunning(t *testing.T) {
	defer func(w time.Duration) { CallWait = w }(CallWait)
	CallWait = 50 * time.Millisecond
	release := make(chan struct{})
	s, a, sk, _ := start(t, opts{provider: "grok", saved: "s1", left: Running, answer: func(a *agent, method string, _ json.RawMessage) string {
		if method == "session/set_model" {
			<-release
		}
		return `{"sessionId":"s1"}`
	}})
	if err := s.Settings("grok-5", ""); err == nil {
		t.Fatal("the switch was answered in time")
	}
	close(release)
	a.say(`{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"session_info_update","title":"after"}}}`)
	if got := sk.wait(t, "title:after"); slices.Contains(got, "result:true:") {
		t.Fatalf("the late answer ended the turn: %v", got)
	}
}

func TestOpenCodesSummaryInALoadedHistoryShowsAsTheCompaction(t *testing.T) {
	_, _, sk, _ := start(t, opts{provider: "opencode", saved: "s1", answer: func(a *agent, method string, _ json.RawMessage) string {
		if method == "session/load" {
			// What OpenCode 1.19.5 replays after a /compact: none of the compaction's prompt, and its summary as a reply.
			a.update(`{"content":{"text":"fix it","type":"text"},"messageId":"u1","sessionUpdate":"user_message_chunk"}`)
			a.update(`{"content":{"text":"done","type":"text"},"messageId":"m1","sessionUpdate":"agent_message_chunk"}`)
			a.update(`{"content":{"text":"## Objective\n- Fix it\n\n## Next Move\n1. (none)","type":"text"},"messageId":"m2","sessionUpdate":"agent_message_chunk"}`)
			a.update(`{"content":{"text":"go on","type":"text"},"messageId":"u3","sessionUpdate":"user_message_chunk"}`)
			a.update(`{"content":{"text":"## Objective\nmine","type":"text"},"messageId":"u4","sessionUpdate":"user_message_chunk"}`)
		}
		return opencodeNew
	}})
	equal(t, after(sk.wait(t, "bound:s1"), "loaded")[:7], []string{
		"loaded", "user:fix it", "assistant_text:done", "result:true:", "compacted:", "user:go on", "user:## Objective\nmine"})
}

func TestAnAgentThatCompactsOnlyByItselfIsNotAskedTo(t *testing.T) {
	s, _, sk, _ := start(t, opts{provider: "hermes", answer: script(`{"sessionId":"s1"}`, says())})
	if err := s.Compact(); err == nil || err.Error() != "Hermes Agent compacts its context by itself" {
		t.Fatal(err)
	}
	if got := sk.lines(); slices.Contains(got, "compacting") {
		t.Fatal(got)
	}
}
