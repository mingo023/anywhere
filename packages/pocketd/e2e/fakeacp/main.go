// Command fakeacp stands in for the agents Pocket's chat drives over ACP in
// e2e tests: hermes, opencode, cursor-agent, grok, fx and agy, whichever
// name it is built under. It speaks JSON-RPC on stdio, one session,
// with notes on stderr, which is the Terminal's screen. Sessions are kept
// under $HOME/.acp-fake, so a restarted one loads. Prompts:
//
//	<text>          reply "echo: <text>" in two chunks
//	run <cmd>       ask to run cmd, then reply "echo: ran <cmd>", or "echo: skipped <cmd>" if refused
//	delegate <task> run a sub-agent on task in a session of its own, as OpenCode 2 does, which reports "did <task>";
//	                as hermes, in the background, telling its work only in a transcript
//	[Anywhere internal background handoff] …
//	                as hermes, reply "echo: took back <summary>" of each transcript tail it holds
//	think <text>    think first, then reply
//	plan <a>, <b>   tell a plan of those entries, then reply "echo: planned"
//	slow <text>     reply after 3s, unless cancelled
//	hang <text>     reply only if not cancelled in 30s
//	fail <message>  refuse the prompt with message
//	stop <reason>   end the turn with that stop reason
//	/compact        as opencode, summarise the session in a reply and tell fewer tokens
//
// With $HOME/.acp-fake/start it never opens a session: "exit <message>" has
// it say message and exit 1, "refuse <message>" refuse session/new with it.
//
// As hermes it has models and modes, asks about a call of its own making and
// tells a command only as content. As opencode it has config options, tells a
// call before its input and asks about that call, and its server refuses any
// flag, as OpenCode 2's does. As grok it compacts by
// _x.ai/compact_conversation. Without a server's arguments it echoes input,
// as a TUI.
package main

import (
	"bufio"
	"crypto/rand"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"time"
)

type agent struct {
	name string // hermes, opencode, …

	start string // what $HOME/.acp-fake/start says

	mu      sync.Mutex
	out     *json.Encoder
	session *session
	next    int
	waits   map[string]chan map[string]any
	stop    chan struct{} // closed to cancel the turn under way
	checks  int
}

type session struct {
	ID      string           `json:"id"`
	Model   string           `json:"model"`
	Mode    string           `json:"mode"`
	History []map[string]any `json:"history"`
}

func main() {
	name := strings.TrimSuffix(filepath.Base(os.Args[0]), ".exe")
	args := os.Args[1:]
	if slices.Contains(args, "--help") {
		fmt.Printf("Usage: %s [acp]\n", name)
		return
	}
	fmt.Fprintf(os.Stderr, "fake %s ready %s\r\n", name, strings.Join(args, " "))
	if !slices.Contains(args, "acp") && !slices.Contains(args, "stdio") {
		in := bufio.NewScanner(os.Stdin)
		for in.Scan() {
			fmt.Printf("typed: %s\r\n", in.Text())
		}
		return
	}
	if name == "opencode" {
		for _, f := range args {
			if strings.HasPrefix(f, "-") {
				fmt.Fprintf(os.Stderr, "Unrecognized flag: %s in command opencode acp\r\n", f)
				os.Exit(1)
			}
		}
	}
	start, _ := os.ReadFile(filepath.Join(dir(), "start"))
	if msg, ok := strings.CutPrefix(string(start), "exit "); ok {
		// Long enough for pocketd to see it run.
		time.Sleep(time.Second)
		note("%s", msg)
		os.Exit(1)
	}
	a := &agent{name: name, start: string(start), out: json.NewEncoder(os.Stdout), waits: map[string]chan map[string]any{}}
	sc := bufio.NewScanner(os.Stdin)
	sc.Buffer(nil, 1<<20)
	for sc.Scan() {
		var m struct {
			ID     json.RawMessage `json:"id"`
			Method string          `json:"method"`
			Params map[string]any  `json:"params"`
			Result map[string]any  `json:"result"`
		}
		if json.Unmarshal(sc.Bytes(), &m) != nil {
			continue
		}
		if m.Method == "" {
			a.mu.Lock()
			w := a.waits[string(m.ID)]
			delete(a.waits, string(m.ID))
			a.mu.Unlock()
			if w != nil {
				w <- m.Result
			}
			continue
		}
		a.handle(m.ID, m.Method, m.Params)
	}
}

func note(format string, a ...any) { fmt.Fprintf(os.Stderr, format+"\r\n", a...) }

func hex(n int) string {
	b := make([]byte, n)
	rand.Read(b)
	return fmt.Sprintf("%x", b)
}

func (a *agent) send(v map[string]any) {
	v["jsonrpc"] = "2.0"
	a.mu.Lock()
	defer a.mu.Unlock()
	a.out.Encode(v)
}

func (a *agent) reply(id json.RawMessage, result any) {
	a.send(map[string]any{"id": id, "result": result})
}

func (a *agent) refuse(id json.RawMessage, code int, message string, data any) {
	a.send(map[string]any{"id": id, "error": map[string]any{"code": code, "message": message, "data": data}})
}

// update tells the client of u, and keeps it for a load to replay.
func (a *agent) update(u map[string]any) {
	a.mu.Lock()
	s := a.session
	s.History = append(s.History, u)
	a.mu.Unlock()
	a.tell(u)
	a.save()
}

func (a *agent) tell(u map[string]any) { a.tellIn(a.session.ID, u) }

func (a *agent) tellIn(sid string, u map[string]any) {
	a.send(map[string]any{"method": "session/update", "params": map[string]any{"sessionId": sid, "update": u}})
}

func dir() string { return filepath.Join(os.Getenv("HOME"), ".acp-fake") }

func (a *agent) save() {
	a.mu.Lock()
	b, _ := json.Marshal(a.session)
	a.mu.Unlock()
	os.MkdirAll(dir(), 0o700)
	os.WriteFile(filepath.Join(dir(), a.session.ID+".json"), b, 0o600)
}

var models = []string{"fake-model", "fake/other"}

// opened is what session/new and session/load answer: OpenCode's settings
// are config options, the others' are models and modes.
func (a *agent) opened() map[string]any {
	s := a.session
	if a.name == "opencode" {
		return map[string]any{"configOptions": a.options()}
	}
	available := []map[string]string{}
	for _, m := range models {
		available = append(available, map[string]string{"modelId": m, "name": m})
	}
	return map[string]any{
		"models": map[string]any{"availableModels": available, "currentModelId": s.Model},
		"modes":  map[string]any{"availableModes": []map[string]string{{"id": "default"}, {"id": "accept_edits"}, {"id": "dont_ask"}}, "currentModeId": s.Mode},
	}
}

func (a *agent) options() []map[string]any {
	choices := func(values ...string) []map[string]string {
		out := []map[string]string{}
		for _, v := range values {
			out = append(out, map[string]string{"value": v, "name": v})
		}
		return out
	}
	return []map[string]any{
		{"id": "model", "name": "Model", "category": "model", "type": "select", "currentValue": a.session.Model, "options": choices(models...)},
		{"id": "mode", "name": "Mode", "category": "mode", "type": "select", "currentValue": a.session.Mode, "options": choices("build", "plan")},
	}
}

func (a *agent) commands() {
	a.tell(map[string]any{"sessionUpdate": "available_commands_update", "availableCommands": []map[string]any{
		{"name": "help", "description": "List available commands"},
		{"name": "model", "description": "Switch models", "input": map[string]string{"hint": "model name"}},
	}})
}

func (a *agent) handle(id json.RawMessage, method string, p map[string]any) {
	str := func(key string) string {
		s, _ := p[key].(string)
		return s
	}
	switch method {
	case "initialize":
		note("initialize")
		a.reply(id, map[string]any{"protocolVersion": 1, "agentCapabilities": map[string]any{"loadSession": true},
			"agentInfo": map[string]string{"name": a.name, "version": "0"}, "authMethods": []map[string]string{{"id": "cached_token"}}})
	case "authenticate":
		note("authenticate: %s", str("methodId"))
		a.reply(id, map[string]any{})
	case "session/new":
		if msg, ok := strings.CutPrefix(a.start, "refuse "); ok {
			a.refuse(id, -32000, msg, nil)
			return
		}
		mode, prefix := "default", "local-"
		if a.name == "opencode" {
			mode, prefix = "build", "ses_"
		}
		a.session = &session{ID: prefix + hex(8), Model: models[0], Mode: mode}
		a.save()
		note("session: %s", a.session.ID)
		res := a.opened()
		res["sessionId"] = a.session.ID
		a.reply(id, res)
		a.commands()
	case "session/load":
		s := a.session
		if s == nil || s.ID != str("sessionId") {
			b, err := os.ReadFile(filepath.Join(dir(), str("sessionId")+".json"))
			if err != nil && a.name == "hermes" {
				a.reply(id, nil)
				return
			}
			if err != nil {
				a.refuse(id, -32002, "Session not found", nil)
				return
			}
			s = &session{}
			json.Unmarshal(b, s)
		}
		// A sub-agent's session loads beside the one open.
		if a.session == nil {
			a.session = s
		}
		note("load: %s", s.ID)
		a.mu.Lock()
		history := slices.Clone(s.History)
		a.mu.Unlock()
		for _, u := range history {
			a.tellIn(s.ID, u)
		}
		if s != a.session {
			a.reply(id, map[string]any{})
			return
		}
		a.reply(id, a.opened())
		a.commands()
	case "session/set_model":
		a.setModel(id, str("modelId"), func() { a.reply(id, map[string]any{}) })
	case "session/set_mode":
		a.session.Mode = str("modeId")
		note("set_mode: %s", a.session.Mode)
		a.reply(id, map[string]any{})
	case "session/set_config_option":
		if a.name != "opencode" {
			a.refuse(id, -32601, "Method not found", map[string]string{"method": method})
			return
		}
		note("config: %s=%s", str("configId"), str("value"))
		done := func() { a.reply(id, map[string]any{"configOptions": a.options()}) }
		if str("configId") == "model" {
			a.setModel(id, str("value"), done)
			return
		}
		a.session.Mode = str("value")
		done()
	case "session/prompt":
		a.mu.Lock()
		busy := a.stop != nil
		if !busy {
			a.stop = make(chan struct{})
		}
		stop := a.stop
		a.mu.Unlock()
		if busy {
			note("busy")
			a.refuse(id, -32603, "a turn is running", nil)
			return
		}
		go a.turn(id, promptText(p), stop)
	case "_x.ai/compact_conversation":
		if a.name != "grok" {
			a.refuse(id, -32601, "Method not found", map[string]string{"method": method})
			return
		}
		note("compact")
		a.send(map[string]any{"method": "_x.ai/session_notification", "params": map[string]any{"sessionId": a.session.ID,
			"update": map[string]any{"sessionUpdate": "auto_compact_completed", "tokens_before": 1200, "tokens_after": 300}}})
		a.reply(id, map[string]any{})
	case "session/cancel":
		note("cancel")
		a.mu.Lock()
		if a.stop != nil {
			close(a.stop)
			a.stop = nil
		}
		a.mu.Unlock()
	default:
		if id != nil {
			a.refuse(id, -32601, "Method not found", map[string]string{"method": method})
		}
	}
}

// setModel refuses a model it doesn't have the way the real agent does.
func (a *agent) setModel(id json.RawMessage, model string, done func()) {
	switch {
	case slices.Contains(models, model):
		a.session.Model = model
		a.save()
		note("set_model: %s", model)
		done()
	case a.name == "opencode":
		a.refuse(id, -32602, "Invalid params: model not found: "+model, nil)
	default:
		a.refuse(id, -32603, "Internal error", map[string]string{"details": "model_resolution_failed"})
	}
}

func promptText(p map[string]any) string {
	blocks, _ := p["prompt"].([]any)
	if len(blocks) == 0 {
		return ""
	}
	text, _ := blocks[0].(map[string]any)["text"].(string)
	return text
}

func textBlock(text string) map[string]string { return map[string]string{"type": "text", "text": text} }

func (a *agent) say(text string) {
	note("echo: %s", text)
	msg := "msg-" + hex(4)
	for _, chunk := range []string{"echo: ", text} {
		a.update(map[string]any{"sessionUpdate": "agent_message_chunk", "messageId": msg, "content": textBlock(chunk)})
		// Long enough for the chat to show the first chunk on its own.
		time.Sleep(60 * time.Millisecond)
	}
}

func (a *agent) turn(id json.RawMessage, text string, stop chan struct{}) {
	note("prompt: %s", text)
	if a.name == "opencode" && text == "/compact" {
		// The real one keeps its summary as a reply no prompt asked for.
		a.update(map[string]any{"sessionUpdate": "agent_thought_chunk", "content": textBlock("summarising")})
		a.say("summary")
		a.tell(map[string]any{"sessionUpdate": "usage_update", "used": 300, "size": 200000})
		a.end()
		a.reply(id, map[string]any{"stopReason": "end_turn"})
		return
	}
	a.mu.Lock()
	first := len(a.session.History) == 0
	a.session.History = append(a.session.History, map[string]any{"sessionUpdate": "user_message_chunk", "content": textBlock(text)})
	a.mu.Unlock()
	if first {
		a.tell(map[string]any{"sessionUpdate": "session_info_update", "title": text})
	}
	reason := "end_turn"
	arg := func(prefix string) (string, bool) { return strings.CutPrefix(text, prefix) }
	if msg, ok := arg("fail "); ok {
		a.end()
		a.refuse(id, -32603, msg, nil)
		return
	} else if cmd, ok := arg("run "); ok {
		reason = a.run(cmd, stop)
	} else if task, ok := arg("delegate "); ok {
		a.delegate(task)
	} else if strings.HasPrefix(text, "[Anywhere internal background handoff]") && a.name == "hermes" {
		a.takeBack(text)
	} else if rest, ok := arg("think "); ok {
		a.update(map[string]any{"sessionUpdate": "agent_thought_chunk", "content": textBlock("thinking about " + rest)})
		a.say(rest)
	} else if rest, ok := arg("plan "); ok {
		entries := []map[string]string{}
		for i, e := range strings.Split(rest, ", ") {
			status := "pending"
			if i == 0 {
				status = "in_progress"
			}
			entries = append(entries, map[string]string{"content": e, "priority": "medium", "status": status})
		}
		a.update(map[string]any{"sessionUpdate": "plan", "entries": entries})
		a.say("planned")
	} else if rest, ok := arg("slow "); ok {
		reason = a.wait(stop, 3*time.Second, rest, reason)
	} else if rest, ok := arg("hang "); ok {
		reason = a.wait(stop, 30*time.Second, rest, reason)
	} else if rest, ok := arg("stop "); ok {
		reason = rest
	} else {
		a.say(text)
	}
	a.tell(map[string]any{"sessionUpdate": "usage_update", "used": 1200, "size": 200000})
	a.end()
	a.reply(id, map[string]any{"stopReason": reason, "usage": map[string]int{"inputTokens": 1000, "outputTokens": 200, "totalTokens": 1200}})
}

// wait replies text after d unless stop closes first, which ends the turn cancelled.
func (a *agent) wait(stop <-chan struct{}, d time.Duration, text, reason string) string {
	select {
	case <-stop:
		return "cancelled"
	case <-time.After(d):
		a.say(text)
		return reason
	}
}

func (a *agent) end() {
	a.mu.Lock()
	a.stop = nil
	a.mu.Unlock()
}

// delegate runs a sub-agent on task as OpenCode 2 does: it tells the
// sub-agent's work in the session as it runs, marked with the sub-agent's
// session, names that session only as the call ends, and its history tells
// that work only to a load of that session.
func (a *agent) delegate(task string) {
	if a.name == "hermes" {
		a.background(task)
		return
	}
	call := "call_" + hex(4)
	kid := &session{ID: "ses_" + hex(8), Model: a.session.Model, Mode: a.session.Mode, History: []map[string]any{
		{"sessionUpdate": "user_message_chunk", "messageId": "msg-" + hex(4), "content": textBlock(task)},
		{"sessionUpdate": "agent_message_chunk", "messageId": "msg-" + hex(4), "content": textBlock("did " + task)},
	}}
	b, _ := json.Marshal(kid)
	os.WriteFile(filepath.Join(dir(), kid.ID+".json"), b, 0o600)
	a.update(map[string]any{"sessionUpdate": "tool_call", "toolCallId": call, "name": "subagent", "title": "subagent", "kind": "think", "status": "pending", "rawInput": map[string]any{}})
	a.update(map[string]any{"sessionUpdate": "tool_call_update", "toolCallId": call, "title": "subagent", "kind": "think", "status": "in_progress",
		"rawInput": map[string]string{"agent": "general", "description": task, "prompt": task}})
	a.tell(map[string]any{"sessionUpdate": "agent_message_chunk", "messageId": "msg-" + hex(4), "content": textBlock("did " + task),
		"_meta": map[string]any{"opencode/child-session": map[string]any{"id": kid.ID, "parentID": a.session.ID, "depth": 1, "title": task}}})
	out := "<subagent sessionID=\"" + kid.ID + "\" state=\"completed\">\ndid " + task + "\n</subagent>"
	a.update(map[string]any{"sessionUpdate": "tool_call_update", "toolCallId": call, "status": "completed", "content": []map[string]any{{"type": "content", "content": textBlock(out)}},
		"rawOutput": map[string]any{"metadata": map[string]any{"sessionID": kid.ID, "status": "completed", "truncated": false}}})
	a.say("delegated " + task)
}

// background runs a sub-agent on task as Hermes does: its call ends with a
// handle to a transcript, which the sub-agent writes its work to, and a
// manifest beside it that says when it ends.
func (a *agent) background(task string) {
	call, d := "tc-"+hex(6), filepath.Join(dir(), "deleg_"+hex(4))
	log, manifest := filepath.Join(d, "task-0.log"), filepath.Join(d, "manifest.json")
	os.MkdirAll(d, 0o700)
	os.WriteFile(log, []byte("=== Hermes subagent live transcript ===\n12:00:00 user     | kickoff: "+task+"\n"), 0o600)
	os.WriteFile(manifest, []byte(`{"tasks":[{"index":0,"status":"running"}]}`), 0o600)
	a.update(map[string]any{"sessionUpdate": "tool_call", "toolCallId": call, "title": "delegate_task: " + task, "kind": "execute", "locations": []any{},
		"content": []map[string]any{{"type": "content", "content": textBlock("Delegating task:\n" + task)}}})
	handle, _ := json.Marshal(map[string]any{"status": "dispatched", "mode": "background", "count": 1, "goals": []string{task}, "live_transcripts": []string{log}})
	a.update(map[string]any{"sessionUpdate": "tool_call_update", "toolCallId": call, "kind": "execute", "status": "completed",
		"content": []map[string]any{{"type": "content", "content": textBlock(string(handle))}}})
	a.say("delegated " + task)
	go func() {
		time.Sleep(300 * time.Millisecond)
		f, _ := os.OpenFile(log, os.O_APPEND|os.O_WRONLY, 0)
		fmt.Fprintf(f, "12:00:01 tool     | -> terminal(echo %[1]s)\n12:00:01 result   | terminal ok 0.1s: %[1]s\n12:00:02 assistant| did %[1]s\n"+
			"12:00:02 final    | status=completed duration=2s summary: did %[1]s\n12:00:02 final    | end status=completed\n", task)
		f.Close()
		os.WriteFile(manifest, []byte(`{"completed":"2026-11-11 12:00:02","tasks":[{"index":0,"status":"completed"}]}`), 0o600)
	}()
}

// takeBack replies with the summaries the transcript tails a handoff holds end with.
func (a *agent) takeBack(text string) {
	_, data, _ := strings.Cut(text, "\n\n")
	var told []struct{ Tail string }
	json.Unmarshal([]byte(data), &told)
	var summaries []string
	for _, t := range told {
		if _, summary, ok := strings.Cut(t.Tail, "summary: "); ok {
			summary, _, _ = strings.Cut(summary, "\n")
			summaries = append(summaries, summary)
		}
	}
	a.say("took back " + strings.Join(summaries, ", "))
}

// run asks to run cmd as the real agent does, and runs it if allowed. It
// returns the turn's stop reason.
func (a *agent) run(cmd string, stop chan struct{}) string {
	call := "call_" + hex(4)
	asked := map[string]any{"toolCallId": call, "title": cmd, "kind": "execute", "status": "pending", "rawInput": map[string]string{"command": cmd, "description": "Run it"}}
	options := []map[string]string{{"optionId": "once", "kind": "allow_once", "name": "Allow once"}, {"optionId": "always", "kind": "allow_always", "name": "Always allow"},
		{"optionId": "reject", "kind": "reject_once", "name": "Reject"}}
	if a.name == "opencode" {
		cwd, _ := os.Getwd()
		a.update(map[string]any{"sessionUpdate": "tool_call", "toolCallId": call, "title": "bash", "kind": "execute", "status": "pending",
			"locations": []map[string]string{{"path": cwd}}, "rawInput": map[string]string{"cwd": cwd}})
		a.update(map[string]any{"sessionUpdate": "tool_call_update", "toolCallId": call, "title": cmd, "kind": "execute", "status": "in_progress",
			"locations": []map[string]string{{"path": cwd}}, "rawInput": map[string]string{"command": cmd, "cwd": cwd}})
		asked["rawInput"] = map[string]string{"command": cmd}
	} else {
		a.update(map[string]any{"sessionUpdate": "tool_call", "toolCallId": call, "title": "terminal: " + cmd, "kind": "execute", "locations": []any{},
			"content": []map[string]any{{"type": "content", "content": textBlock("$ " + cmd)}}})
		a.mu.Lock()
		a.checks++
		check := fmt.Sprintf("perm-check-%d", a.checks)
		a.mu.Unlock()
		asked["toolCallId"] = check
		options = []map[string]string{{"optionId": "allow_once", "kind": "allow_once", "name": "Allow once"}, {"optionId": "allow_session", "kind": "allow_always", "name": "Allow for session"},
			{"optionId": "allow_always", "kind": "allow_always", "name": "Allow always"}, {"optionId": "deny", "kind": "reject_once", "name": "Deny"}}
	}
	a.mu.Lock()
	a.next++
	n := a.next
	w := make(chan map[string]any, 1)
	a.waits[fmt.Sprint(n)] = w
	a.mu.Unlock()
	a.send(map[string]any{"id": n, "method": "session/request_permission", "params": map[string]any{"sessionId": a.session.ID, "toolCall": asked, "options": options}})
	var outcome map[string]any
	select {
	case r := <-w:
		outcome, _ = r["outcome"].(map[string]any)
	case <-stop:
	}
	picked, _ := outcome["optionId"].(string)
	if outcome["outcome"] != "selected" {
		picked = "cancelled"
	}
	note("permission: %s", picked)
	allowed := strings.Contains(picked, "allow") || picked == "once" || picked == "always"
	if a.name != "opencode" {
		status := "failed"
		if allowed {
			status = "completed"
		}
		a.tell(map[string]any{"sessionUpdate": "tool_call_update", "toolCallId": asked["toolCallId"], "status": status})
	}
	switch {
	case picked == "cancelled":
		a.update(map[string]any{"sessionUpdate": "tool_call_update", "toolCallId": call, "kind": "execute", "status": "failed"})
		return "cancelled"
	case !allowed:
		a.update(map[string]any{"sessionUpdate": "tool_call_update", "toolCallId": call, "kind": "execute", "status": "failed",
			"content": []map[string]any{{"type": "content", "content": textBlock("The user refused to run this")}}})
		a.say("skipped " + cmd)
		return "end_turn"
	}
	a.update(map[string]any{"sessionUpdate": "tool_call_update", "toolCallId": call, "kind": "execute", "status": "completed",
		"content": []map[string]any{{"type": "content", "content": textBlock("ran: " + cmd)}}})
	a.say("ran " + cmd)
	return "end_turn"
}
