package acp

import (
	"encoding/json"
	"slices"
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
		"read by its input":      {toolCall{Kind: "read", RawInput: json.RawMessage(`{"filePath":"/w/b.go"}`)}, "read:read:/w/b.go"},
		"edit with its diff":     {toolCall{Kind: "edit", Content: []toolContent{{Type: "diff", Path: "/w/a.txt", OldText: &old, NewText: "b\nc\n"}}}, "edit:edit:/w/a.txt:+2-1"},
		"edit of a new file":     {toolCall{Kind: "edit", Content: []toolContent{{Type: "diff", Path: "/w/n.txt", NewText: "b\n"}}}, "write:write:/w/n.txt:+1-0"},
		"search by its pattern":  {toolCall{Kind: "search", Title: "grep", RawInput: json.RawMessage(`{"pattern":"TODO","path":"/w"}`)}, "search:search:/wTODO"},
		"anything else by title": {toolCall{Kind: "fetch", Title: "Fetch example.com", RawInput: json.RawMessage(`{"url":"https://example.com"}`)}, `Fetch example.com:other:`},
		"grok's by its meta":     {toolCall{Meta: grokMeta("execute", "Run", `{"command":"pwd"}`)}, "shell:shell:pwd"},
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

func grokMeta(kind, label, input string) (m struct {
	Tool struct {
		Kind  string          `json:"kind"`
		Label string          `json:"label"`
		Input json.RawMessage `json:"input"`
	} `json:"x.ai/tool"`
}) {
	m.Tool.Kind, m.Tool.Label, m.Tool.Input = kind, label, json.RawMessage(input)
	return m
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
	equal(t, sk.wait(t, "model:moa:default"), []string{"bound:s1", "model:moa:default"})
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

func TestEachAgentOpensItsSessionItsOwnWay(t *testing.T) {
	const grokInit = `{"authMethods":[{"id":"grok.com"},{"id":"cached_token"},{"id":"xai.api_key"}],"_meta":{"defaultAuthMethodId":"grok.com"}}`
	for _, c := range []struct {
		provider string
		launch   state.Launch
		init     string
		calls    map[string]string
	}{
		{"cursor", state.Launch{Access: "ask"}, `{}`, map[string]string{
			"initialize":  `{"clientCapabilities":{"_meta":{"parameterizedModelPicker":true},"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false},"clientInfo":{"name":"anywhere","title":"Anywhere","version":"0"},"protocolVersion":1}`,
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
		if got := grokAuth(json.RawMessage(init)); got != want {
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

func TestGrokSwitchesNeitherPlanNorFullAccessOnceItRuns(t *testing.T) {
	const refusal = "Grok Build takes plan mode and full access only as it starts, so start a new session to switch"
	for _, c := range []struct {
		launch state.Launch
		to     string
		want   string
	}{
		{state.Launch{Access: "ask", Plan: true}, "edits", refusal},
		{state.Launch{Access: "full"}, "ask", refusal},
		{state.Launch{Access: "ask"}, "full", refusal},
		{state.Launch{Access: "ask"}, "auto", ""},
	} {
		s, _, _, _ := start(t, opts{provider: "grok", launch: c.launch, answer: script(`{"sessionId":"s1"}`, says())})
		got := ""
		if err := s.Access(c.to); err != nil {
			got = err.Error()
		}
		if got != c.want {
			t.Errorf("%+v to %s: %q", c.launch, c.to, got)
		}
	}
}

func TestGrokPlanningIsRefusedWhatIsNoReadOrSearch(t *testing.T) {
	const options = `"options":[{"optionId":"o1","kind":"allow_once","name":"Allow"},{"optionId":"o2","kind":"allow_always","name":"Always"},{"optionId":"o3","kind":"reject_once","name":"Reject"}]`
	_, a, _, b := start(t, opts{provider: "grok", launch: state.Launch{Access: "full", Plan: true}, answer: script(`{"sessionId":"s1"}`, says())})
	a.ask("1", "session/request_permission", `{"toolCall":{"toolCallId":"c1","title":"Edit a.go","kind":"edit"},`+options+`}`)
	a.ask("2", "session/request_permission", `{"toolCall":{"toolCallId":"c2","title":"Read a.go","kind":"read"},`+options+`}`)
	if got := a.replied("1"); got != `{"outcome":{"optionId":"o3","outcome":"selected"}}` {
		t.Fatalf("an edit while planning: %s", got)
	}
	if got := a.replied("2"); got != `{"outcome":{"optionId":"o1","outcome":"selected"}}` || len(b.Open()) != 0 {
		t.Fatalf("a read while planning: %s", got)
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
