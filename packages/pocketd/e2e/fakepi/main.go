// Command fakepi stands in for pi and its fork omp, whichever name it is
// built under, in e2e tests. With `--mode rpc` it speaks pi's RPC on stdio,
// one session, with notes on stderr, which is the Terminal's screen.
// Sessions are kept under $HOME/.pi-fake, so `--session <id>`, or omp's
// `--resume <id>`, starts on one again. Prompts:
//
//	<text>           reply "echo: <text>" in two deltas
//	run <cmd>        call bash with cmd, then reply "echo: ran <cmd>"
//	delegate <task>  have the subagent extension run a sub-agent on task, which reports "did <task>"
//	think <text>     think first, then reply
//	slow <text>      reply after 3s, unless aborted; a prompt steered in meanwhile is answered after
//	fail <message>   end the reply with message as the provider's error
//	retry <text>     fail once as overloaded, then try again and reply
//	confirm <title>  have an extension ask yes or no, then reply "echo: yes", "echo: no" or "echo: cancelled"
//	select <a>, <b>  have an extension offer those, then reply "echo: picked <option>"
//	/reload          run as a command of its own: no agent runs
//	/reviewer        omp only: print "Selected reviewer: careful" and say in the answer that no agent ran
//	/workflow        omp only: take the prompt, then answer it again as failed
//
// With `exit <message>` in $HOME/.pi-fake/start it prints message and exits
// instead. Without `--mode rpc` it echoes input, as a TUI.
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

type msg = map[string]any

type agent struct {
	name string // pi or omp

	mu      sync.Mutex
	out     *json.Encoder
	session session
	running bool
	stop    chan struct{} // closed to abort the run under way
	idle    chan struct{} // closed once the run under way has ended
	steered []string
	retried bool // the run under way is pi's second try
	waits   map[string]chan msg
	next    int
}

type session struct {
	ID       string `json:"id"`
	Model    string `json:"model"`
	Thinking string `json:"thinking"`
	Messages []msg  `json:"messages"`
}

var models = []string{"fake/model", "fake/other"}

func main() {
	name := strings.TrimSuffix(filepath.Base(os.Args[0]), ".exe")
	args := os.Args[1:]
	if slices.Contains(args, "--help") {
		fmt.Printf("Usage: %s [--mode text|json|rpc]\n", name)
		return
	}
	fmt.Fprintf(os.Stderr, "fake %s ready %s\r\n", name, strings.Join(args, " "))
	if i := slices.Index(args, "--mode"); i < 0 || i+1 >= len(args) || args[i+1] != "rpc" {
		in := bufio.NewScanner(os.Stdin)
		for in.Scan() {
			fmt.Printf("typed: %s\r\n", in.Text())
		}
		return
	}
	if start, _ := os.ReadFile(filepath.Join(dir(), "start")); strings.HasPrefix(string(start), "exit ") {
		// Long enough for pocketd to see it run.
		time.Sleep(time.Second)
		note("%s", strings.TrimPrefix(string(start), "exit "))
		os.Exit(1)
	}
	a := &agent{name: name, out: json.NewEncoder(os.Stdout), waits: map[string]chan msg{}}
	a.session = session{ID: uuid(), Model: models[0], Thinking: "off"}
	resume := "--session"
	if name == "omp" {
		resume = "--resume"
	}
	for i, arg := range args[:len(args)-1] {
		switch arg {
		case resume:
			b, err := os.ReadFile(filepath.Join(dir(), args[i+1]+".json"))
			if err != nil {
				fmt.Fprintf(os.Stderr, "No session found matching '%s'\r\n", args[i+1])
				os.Exit(1)
			}
			json.Unmarshal(b, &a.session)
			note("load: %s", a.session.ID)
		case "--model":
			a.session.Model = args[i+1]
		case "--thinking":
			a.session.Thinking = args[i+1]
		}
	}
	note("session: %s", a.session.ID)
	sc := bufio.NewScanner(os.Stdin)
	sc.Buffer(nil, 1<<20)
	for sc.Scan() {
		var m msg
		if json.Unmarshal(sc.Bytes(), &m) == nil {
			a.handle(m)
		}
	}
}

func note(format string, a ...any) { fmt.Fprintf(os.Stderr, format+"\r\n", a...) }

func uuid() string {
	b := make([]byte, 16)
	rand.Read(b)
	return fmt.Sprintf("%x-%x-%x-%x-%x", b[:4], b[4:6], b[6:8], b[8:10], b[10:])
}

func dir() string { return filepath.Join(os.Getenv("HOME"), ".pi-fake") }

func (a *agent) send(v msg) {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.out.Encode(v)
}

func (a *agent) respond(m msg, data any) {
	r := msg{"id": m["id"], "type": "response", "command": m["type"], "success": true}
	if data != nil {
		r["data"] = data
	}
	a.send(r)
}

func (a *agent) refuse(m msg, reason string) {
	a.send(msg{"id": m["id"], "type": "response", "command": m["type"], "success": false, "error": reason})
}

// add puts message in the session, and tells of it as pi does.
func (a *agent) add(message msg) {
	message["timestamp"] = time.Now().UnixMilli()
	a.send(msg{"type": "message_start", "message": message})
	a.keep(message)
	a.send(msg{"type": "message_end", "message": message})
}

func (a *agent) keep(message msg) {
	a.mu.Lock()
	a.session.Messages = append(a.session.Messages, message)
	b, _ := json.Marshal(a.session)
	a.mu.Unlock()
	os.MkdirAll(dir(), 0o700)
	os.WriteFile(filepath.Join(dir(), a.session.ID+".json"), b, 0o600)
}

func (a *agent) model() msg {
	provider, id, _ := strings.Cut(a.session.Model, "/")
	return msg{"id": id, "name": id, "provider": provider, "reasoning": true, "contextWindow": 200000}
}

func (a *agent) handle(m msg) {
	str := func(key string) string {
		s, _ := m[key].(string)
		return s
	}
	switch m["type"] {
	case "get_state":
		a.mu.Lock()
		state := msg{"model": a.model(), "thinkingLevel": a.session.Thinking, "isStreaming": a.running, "isCompacting": false,
			"sessionId": a.session.ID, "sessionFile": filepath.Join(dir(), a.session.ID+".json"), "messageCount": len(a.session.Messages)}
		a.mu.Unlock()
		a.respond(m, state)
	case "get_messages":
		a.mu.Lock()
		messages := slices.Clone(a.session.Messages)
		a.mu.Unlock()
		note("get_messages")
		a.respond(m, msg{"messages": messages})
	case "prompt":
		a.prompt(m, str("message"), str("streamingBehavior"))
	case "clear_queue":
		a.mu.Lock()
		steered := a.steered
		a.steered = nil
		a.mu.Unlock()
		a.respond(m, msg{"steering": steered, "followUp": []string{}})
	case "abort":
		note("abort")
		a.mu.Lock()
		idle := a.idle
		if a.running && a.stop != nil {
			close(a.stop)
			a.stop = nil
		}
		a.mu.Unlock()
		go func() {
			if idle != nil {
				<-idle
			}
			a.respond(m, nil)
		}()
	case "compact":
		note("compact")
		a.send(msg{"type": "compaction_start", "reason": "manual"})
		if len(a.session.Messages) == 0 {
			a.send(msg{"type": "compaction_end", "reason": "manual", "aborted": false, "willRetry": false, "errorMessage": "Compaction failed: Nothing to compact (session too small)"})
			a.refuse(m, "Nothing to compact (session too small)")
			return
		}
		result := msg{"summary": "what was said", "tokensBefore": 1200, "estimatedTokensAfter": 300}
		a.keep(msg{"role": "compactionSummary", "summary": "what was said", "tokensBefore": 1200})
		a.send(msg{"type": "compaction_end", "reason": "manual", "result": result, "aborted": false, "willRetry": false})
		a.respond(m, result)
	case "set_model":
		name := str("provider") + "/" + str("modelId")
		if !slices.Contains(models, name) {
			a.refuse(m, "Model not found: "+name)
			return
		}
		note("set_model: %s", name)
		a.session.Model = name
		a.respond(m, a.model())
	case "set_thinking_level":
		note("set_thinking_level: %s", str("level"))
		a.session.Thinking = str("level")
		a.send(msg{"type": "thinking_level_changed", "level": str("level")})
		a.respond(m, nil)
	case "get_session_stats":
		a.respond(m, msg{"sessionId": a.session.ID, "contextUsage": msg{"tokens": 1200, "contextWindow": 200000, "percent": 0.6}})
	case "get_commands":
		a.respond(m, msg{"commands": []msg{
			{"name": "mcp", "description": "Manage MCP servers", "source": "extension"},
			{"name": "fix-tests", "description": "Fix failing tests", "source": "prompt"},
			{"name": "skill:review", "description": "Review the changes", "source": "skill"},
		}})
	case "get_available_commands":
		a.respond(m, msg{"commands": []msg{{"name": "plan", "description": "Plan before building", "source": "builtin", "input": msg{"hint": "goal"}}}})
	case "extension_ui_response":
		a.mu.Lock()
		w := a.waits[str("id")]
		delete(a.waits, str("id"))
		a.mu.Unlock()
		if w != nil {
			w <- m
		}
	default:
		a.refuse(m, fmt.Sprintf("Unknown command: %v", m["type"]))
	}
}

func (a *agent) prompt(m msg, text, streaming string) {
	a.mu.Lock()
	running := a.running
	switch {
	case running && streaming != "":
		a.steered = append(a.steered, text)
	case !running && !strings.HasPrefix(text, "/"):
		a.running, a.stop, a.idle = true, make(chan struct{}), make(chan struct{})
	}
	stop, idle := a.stop, a.idle
	a.mu.Unlock()
	switch {
	case running && streaming == "":
		note("busy")
		a.refuse(m, "Agent is already processing. Specify streamingBehavior ('steer' or 'followUp') to queue the message.")
	case running:
		note("%s: %s", streaming, text)
		a.respond(m, msg{"disposition": "queued"})
	// A command of its own. omp tells that it ran no agent for it after it answers.
	case strings.HasPrefix(text, "/"):
		note("command: %s", text)
		switch {
		case a.name == "omp" && text == "/reviewer":
			a.send(msg{"type": "command_output", "text": "Selected reviewer: careful"})
			a.respond(m, msg{"agentInvoked": false})
			return
		case a.name == "omp" && text == "/workflow":
			a.respond(m, nil)
			a.refuse(m, "Workflow failed")
			return
		case a.name == "omp":
			a.respond(m, nil)
			a.send(msg{"type": "prompt_result", "id": m["id"], "agentInvoked": false})
			return
		}
		a.respond(m, msg{"disposition": "handled"})
	default:
		a.respond(m, msg{"disposition": "started"})
		go a.run(text, stop, idle)
	}
}

// run answers text, and then whatever was steered in meanwhile, as one of pi's runs.
func (a *agent) run(text string, stop, idle chan struct{}) {
	a.send(msg{"type": "agent_start"})
	aborted := false
	for !aborted {
		note("prompt: %s", text)
		a.send(msg{"type": "turn_start"})
		a.add(msg{"role": "user", "content": []msg{{"type": "text", "text": text}}})
		aborted = !a.turn(text, stop)
		a.mu.Lock()
		more := len(a.steered) > 0
		if more {
			text, a.steered = a.steered[0], a.steered[1:]
		}
		a.mu.Unlock()
		if !more {
			break
		}
	}
	a.mu.Lock()
	a.running, a.stop, a.idle = false, nil, nil
	a.mu.Unlock()
	// pi ends its run, then the retry that run was, then says it goes on with nothing.
	a.send(msg{"type": "agent_end", "messages": []msg{}, "willRetry": false})
	if a.retried {
		a.retried = false
		a.send(msg{"type": "auto_retry_end", "success": true, "attempt": 1})
	}
	a.send(msg{"type": "agent_settled", "aborted": aborted})
	close(idle)
}

func usage(n int) msg {
	return msg{"input": n, "output": n / 5, "cacheRead": 0, "cacheWrite": 0, "totalTokens": n + n/5}
}

// assistant is a reply pi ended with reason, as its events and its history tell it.
func (a *agent) assistant(blocks []msg, reason, failure string) msg {
	provider, id, _ := strings.Cut(a.session.Model, "/")
	m := msg{"role": "assistant", "content": blocks, "provider": provider, "model": id, "usage": usage(1000), "stopReason": reason}
	if failure != "" {
		m["usage"], m["errorMessage"] = usage(0), failure
	}
	return m
}

// say streams text as a reply, in two deltas. It reports false if aborted first.
func (a *agent) say(text string, stop chan struct{}) bool {
	note("echo: %s", text)
	a.send(msg{"type": "message_start", "message": a.assistant([]msg{}, "pending", "")})
	for _, delta := range []string{"echo: ", text} {
		a.send(msg{"type": "message_update", "usage": usage(0), "assistantMessageEvent": msg{"type": "text_delta", "contentIndex": 0, "delta": delta}})
		// Long enough for the chat to show the first delta on its own.
		time.Sleep(60 * time.Millisecond)
	}
	whole := "echo: " + text
	a.send(msg{"type": "message_update", "usage": usage(1000), "assistantMessageEvent": msg{"type": "text_end", "contentIndex": 0, "content": whole}})
	reply := a.assistant([]msg{{"type": "text", "text": whole}}, "stop", "")
	a.keep(reply)
	a.send(msg{"type": "message_end", "message": reply})
	a.send(msg{"type": "turn_end", "message": reply, "toolResults": []msg{}})
	return true
}

// fails ends the reply with the provider's error, which pi tells in the
// message, at its start already.
func (a *agent) fails(failure string) {
	reply := a.assistant([]msg{}, "error", failure)
	a.add(reply)
	a.send(msg{"type": "turn_end", "message": reply, "toolResults": []msg{}})
}

// ask has an extension open a dialog, and waits for its answer; nil if aborted first.
func (a *agent) ask(dialog msg, stop chan struct{}) msg {
	a.mu.Lock()
	a.next++
	id := fmt.Sprintf("ui-%d", a.next)
	w := make(chan msg, 1)
	a.waits[id] = w
	a.mu.Unlock()
	dialog["type"], dialog["id"] = "extension_ui_request", id
	a.send(dialog)
	select {
	case answer := <-w:
		return answer
	case <-stop:
		return nil
	}
}

// turn answers one prompt. It reports false if aborted first.
func (a *agent) turn(text string, stop chan struct{}) bool {
	arg := func(prefix string) (string, bool) { return strings.CutPrefix(text, prefix) }
	if failure, ok := arg("fail "); ok {
		a.fails(failure)
		return true
	} else if cmd, ok := arg("run "); ok {
		call := "call_" + uuid()[:8]
		args := msg{"command": cmd}
		a.send(msg{"type": "message_start", "message": a.assistant([]msg{}, "pending", "")})
		a.send(msg{"type": "message_update", "usage": usage(0), "assistantMessageEvent": msg{"type": "toolcall_start", "contentIndex": 0, "id": call, "toolName": "bash"}})
		a.send(msg{"type": "message_update", "usage": usage(0), "assistantMessageEvent": msg{"type": "toolcall_delta", "contentIndex": 0, "delta": `{"command":`}})
		block := msg{"type": "toolCall", "id": call, "name": "bash", "arguments": args}
		a.send(msg{"type": "message_update", "usage": usage(1000), "assistantMessageEvent": msg{"type": "toolcall_end", "contentIndex": 0, "toolCall": block}})
		reply := a.assistant([]msg{block}, "toolUse", "")
		a.keep(reply)
		a.send(msg{"type": "message_end", "message": reply})
		a.send(msg{"type": "tool_execution_start", "toolCallId": call, "toolName": "bash", "args": args})
		// Long enough for the chat to show the call running.
		time.Sleep(60 * time.Millisecond)
		result := msg{"content": []msg{{"type": "text", "text": "ran: " + cmd}}, "details": msg{}}
		a.send(msg{"type": "tool_execution_end", "toolCallId": call, "toolName": "bash", "result": result, "isError": false, "durationMs": 60})
		a.add(msg{"role": "toolResult", "toolCallId": call, "toolName": "bash", "content": result["content"], "isError": false})
		a.send(msg{"type": "turn_end", "message": reply, "toolResults": []msg{}})
		a.send(msg{"type": "turn_start"})
		return a.say("ran "+cmd, stop)
	} else if task, ok := arg("delegate "); ok {
		call := "call_" + uuid()[:8]
		args := msg{"agent": "worker", "task": task}
		block := msg{"type": "toolCall", "id": call, "name": "subagent", "arguments": args}
		a.send(msg{"type": "message_start", "message": a.assistant([]msg{}, "pending", "")})
		reply := a.assistant([]msg{block}, "toolUse", "")
		a.keep(reply)
		a.send(msg{"type": "message_end", "message": reply})
		a.send(msg{"type": "tool_execution_start", "toolCallId": call, "toolName": "subagent", "args": args})
		// As the extension pi ships tells a sub-agent: all its messages so far, on each update.
		messages := []msg{{"role": "user", "content": []msg{{"type": "text", "text": "Task: " + task}}}}
		result := func(said string) msg {
			return msg{"content": []msg{{"type": "text", "text": said}}, "details": msg{"mode": "single", "results": []msg{
				{"agent": "worker", "task": task, "exitCode": 0, "messages": messages, "model": "fake/model", "stopReason": "stop", "usage": msg{"contextTokens": 120}}}}}
		}
		a.send(msg{"type": "tool_execution_update", "toolCallId": call, "toolName": "subagent", "args": args, "partialResult": result("(running...)")})
		time.Sleep(60 * time.Millisecond)
		messages = append(messages, msg{"role": "assistant", "content": []msg{{"type": "text", "text": "did " + task}}, "stopReason": "stop"})
		done := result("did " + task)
		a.send(msg{"type": "tool_execution_end", "toolCallId": call, "toolName": "subagent", "result": done, "isError": false})
		a.add(msg{"role": "toolResult", "toolCallId": call, "toolName": "subagent", "content": done["content"], "details": done["details"], "isError": false})
		a.send(msg{"type": "turn_end", "message": reply, "toolResults": []msg{}})
		a.send(msg{"type": "turn_start"})
		return a.say("delegated "+task, stop)
	} else if rest, ok := arg("think "); ok {
		a.send(msg{"type": "message_start", "message": a.assistant([]msg{}, "pending", "")})
		a.send(msg{"type": "message_update", "usage": usage(0), "assistantMessageEvent": msg{"type": "thinking_delta", "contentIndex": 0, "delta": "thinking about " + rest}})
		a.send(msg{"type": "message_update", "usage": usage(0), "assistantMessageEvent": msg{"type": "thinking_end", "contentIndex": 0, "content": "thinking about " + rest}})
		a.send(msg{"type": "message_update", "usage": usage(0), "assistantMessageEvent": msg{"type": "text_delta", "contentIndex": 1, "delta": "echo: " + rest}})
		reply := a.assistant([]msg{{"type": "thinking", "thinking": "thinking about " + rest}, {"type": "text", "text": "echo: " + rest}}, "stop", "")
		a.keep(reply)
		a.send(msg{"type": "message_end", "message": reply})
		return true
	} else if rest, ok := arg("slow "); ok {
		select {
		case <-stop:
			reply := a.assistant([]msg{}, "aborted", "")
			reply["errorMessage"] = "Request was aborted"
			a.add(reply)
			return false
		case <-time.After(3 * time.Second):
			return a.say(rest, stop)
		}
	} else if rest, ok := arg("retry "); ok {
		a.fails("529 overloaded")
		a.send(msg{"type": "agent_end", "messages": []msg{}, "willRetry": true})
		a.send(msg{"type": "auto_retry_start", "attempt": 1, "maxAttempts": 3, "delayMs": 50, "errorMessage": "529 overloaded"})
		time.Sleep(50 * time.Millisecond)
		a.send(msg{"type": "agent_start"})
		a.retried = true
		return a.say(rest, stop)
	} else if title, ok := arg("confirm "); ok {
		answer := a.ask(msg{"method": "confirm", "title": title, "message": "It can't be undone."}, stop)
		switch {
		case answer == nil:
			return false
		case answer["cancelled"] == true:
			return a.say("cancelled", stop)
		case answer["confirmed"] == true:
			return a.say("yes", stop)
		}
		return a.say("no", stop)
	} else if rest, ok := arg("select "); ok {
		answer := a.ask(msg{"method": "select", "title": "Pick one", "options": strings.Split(rest, ", ")}, stop)
		if answer == nil {
			return false
		}
		if answer["cancelled"] == true {
			return a.say("picked nothing", stop)
		}
		return a.say(fmt.Sprintf("picked %v", answer["value"]), stop)
	}
	return a.say(text, stop)
}
