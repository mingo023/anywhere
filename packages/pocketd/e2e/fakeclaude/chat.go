package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"slices"
	"strings"
	"sync"
	"time"
)

// chat is claude -p --input-format stream-json: each user line is a turn
// that starts with init and ends with result. It notes what it does on
// stderr, which is the Terminal's screen. Like claude -p, its transcript ends
// a turn with the Stop hooks' summary, or the error. Prompts:
//
//	<text>       reply "echo: <text>"
//	run <cmd>    ask can_use_tool, then run a Bash tool with its answer
//	ask <q>      ask AskUserQuestion through can_use_tool, then reply the answers
//	slow <text>  reply after 3s, unless interrupted
//	bg <cmd>     run a Bash tool in the background, then go on as slow
//	agent <d>    start a sub-agent in the background, then go on as slow
//	/compact     compact
//
// Like claude, a bogus model fails each turn rather than the start.
type chatter struct {
	s     *session
	mu    sync.Mutex
	model string
	out   *json.Encoder
	next  int
	waits map[string]chan map[string]any
	stop  chan struct{}
}

func chat() {
	s := start()
	s.entrypoint = "sdk-cli"
	c := &chatter{s: s, model: "fake-model", out: json.NewEncoder(os.Stdout), waits: map[string]chan map[string]any{}, stop: make(chan struct{}, 1)}
	if i := slices.Index(os.Args, "--model"); i > 0 && i+1 < len(os.Args) {
		c.model = os.Args[i+1]
	}
	c.s.hook("SessionStart", map[string]any{"source": "startup", "model": c.model})()
	note("fake claude ready")
	prompts := make(chan string, 16)
	go c.read(prompts)
	for text := range prompts {
		c.turn(text)
	}
}

func note(format string, a ...any) { fmt.Fprintf(os.Stderr, format+"\n", a...) }

func (c *chatter) read(prompts chan<- string) {
	defer close(prompts)
	sc := bufio.NewScanner(os.Stdin)
	sc.Buffer(nil, 1<<20)
	for sc.Scan() {
		var l struct {
			Type      string `json:"type"`
			RequestID string `json:"request_id"`
			Request   struct {
				Subtype  string         `json:"subtype"`
				TaskID   string         `json:"task_id"`
				Model    string         `json:"model"`
				Settings map[string]any `json:"settings"`
			} `json:"request"`
			Response map[string]any `json:"response"`
			Message  struct {
				Content string `json:"content"`
			} `json:"message"`
		}
		if json.Unmarshal(sc.Bytes(), &l) != nil {
			continue
		}
		switch l.Type {
		case "user":
			prompts <- l.Message.Content
		case "control_response":
			id, _ := l.Response["request_id"].(string)
			c.mu.Lock()
			w := c.waits[id]
			delete(c.waits, id)
			c.mu.Unlock()
			if w != nil {
				w <- l.Response
			}
		case "control_request":
			switch l.Request.Subtype {
			case "set_model":
				c.mu.Lock()
				c.model = l.Request.Model
				c.mu.Unlock()
				note("set_model: %s", l.Request.Model)
			case "apply_flag_settings":
				note("effort: %v", l.Request.Settings["effortLevel"])
			case "stop_task":
				note("stop_task: %s", l.Request.TaskID)
			case "interrupt":
				note("interrupt")
				select {
				case c.stop <- struct{}{}:
				default:
				}
			}
			c.send(map[string]any{"type": "control_response", "response": map[string]any{"subtype": "success", "request_id": l.RequestID, "response": map[string]any{}}})
		}
	}
}

func (c *chatter) send(v any) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.out.Encode(v)
}

func (c *chatter) turn(text string) {
	c.mu.Lock()
	model := c.model
	c.mu.Unlock()
	select {
	case <-c.stop:
	default:
	}
	c.send(map[string]any{"type": "system", "subtype": "init", "session_id": c.s.id, "cwd": c.s.cwd, "model": model, "permissionMode": "default"})
	note("prompt: %s", text)
	c.s.hook("UserPromptSubmit", map[string]any{"prompt": text})()
	c.s.write(user(text))
	c.next++
	id := fmt.Sprintf("toolu_%d", c.next)
	failed := false
	switch {
	case model == "bogus":
		a := assistant(map[string]any{"type": "text", "text": "There's an issue with the selected model (bogus)."})
		a["isApiErrorMessage"] = true
		c.s.write(a)
		c.send(map[string]any{"type": "assistant", "message": a["message"], "session_id": c.s.id})
		failed = true
	case text == "/compact":
		c.send(map[string]any{"type": "system", "subtype": "status", "status": "compacting"})
		time.Sleep(300 * time.Millisecond)
		c.s.hook("SessionStart", map[string]any{"source": "compact", "model": model})()
		note("compacted")
	case strings.HasPrefix(text, "run "):
		failed = c.run(model, id, strings.TrimPrefix(text, "run "))
	case strings.HasPrefix(text, "ask "):
		c.askUser(model, id, strings.TrimPrefix(text, "ask "))
	case strings.HasPrefix(text, "bg "):
		c.background(model, id, strings.TrimPrefix(text, "bg "))
		failed = c.slow(model, "done")
	case strings.HasPrefix(text, "agent "):
		c.backgroundAgent(model, id, strings.TrimPrefix(text, "agent "))
		failed = c.slow(model, "done")
	case strings.HasPrefix(text, "slow "):
		failed = c.slow(model, strings.TrimPrefix(text, "slow "))
	default:
		c.reply(model, text)
	}
	c.s.hook("Stop", nil)()
	if !failed {
		c.s.write(map[string]any{"type": "system", "subtype": "stop_hook_summary", "hookCount": 1})
	}
	c.send(map[string]any{"type": "result", "subtype": "success", "is_error": failed, "session_id": c.s.id})
}

// run asks to run cmd as claude does: the PermissionRequest hook fires too,
// but the answer comes through the stream.
func (c *chatter) run(model, id, cmd string) (failed bool) {
	input := map[string]any{"command": cmd}
	c.s.write(assistant(map[string]any{"type": "tool_use", "id": id, "name": "Bash", "input": input}))
	hook := c.s.hook("PermissionRequest", map[string]any{"tool_name": "Bash", "tool_input": input})
	d := c.ask(map[string]any{"subtype": "can_use_tool", "tool_name": "Bash", "input": input, "tool_use_id": id})
	if out := hook(); len(out) > 0 {
		note("hook replied: %s", out)
	}
	note("can_use_tool: %v", d["behavior"])
	if d["behavior"] != "allow" {
		msg, _ := d["message"].(string)
		c.s.write(toolResult(id, msg, true))
		if d["interrupt"] == true {
			c.s.write(user("[Request interrupted by user for tool use]"))
			return true
		}
		return false
	}
	c.s.write(toolResult(id, "ran: "+cmd, false))
	c.reply(model, "ran "+cmd)
	return false
}

// slow replies text after 3s, unless interrupted first.
func (c *chatter) slow(model, text string) (failed bool) {
	select {
	case <-c.stop:
		c.s.write(user("[Request interrupted by user]"))
		return true
	case <-time.After(3 * time.Second):
		c.reply(model, text)
		return false
	}
}

func (c *chatter) background(model, id, cmd string) {
	c.s.write(assistant(map[string]any{"type": "tool_use", "id": id, "name": "Bash", "input": map[string]any{"command": cmd, "run_in_background": true}}))
	r := toolResult(id, "Command running in background with ID: bg_"+id+".", false)
	r["toolUseResult"] = map[string]any{"stdout": "", "backgroundTaskId": "bg_" + id}
	c.s.write(r)
	c.reply(model, "started "+cmd)
}

func (c *chatter) backgroundAgent(model, id, description string) {
	c.s.write(assistant(map[string]any{"type": "tool_use", "id": id, "name": "Agent", "input": map[string]any{"description": description, "prompt": description, "run_in_background": true}}))
	r := toolResult(id, "Async agent launched successfully.", false)
	r["toolUseResult"] = map[string]any{"isAsync": true, "status": "async_launched", "agentId": "agent_" + id}
	c.s.write(r)
	c.reply(model, "started "+description)
}

func (c *chatter) askUser(model, id, q string) {
	input := map[string]any{"questions": []map[string]any{{"question": q, "header": "Q", "multiSelect": false, "options": []map[string]any{{"label": "Yes", "description": "yes"}, {"label": "No", "description": "no"}}}}}
	c.s.write(assistant(map[string]any{"type": "tool_use", "id": id, "name": "AskUserQuestion", "input": input}))
	d := c.ask(map[string]any{"subtype": "can_use_tool", "tool_name": "AskUserQuestion", "input": input, "tool_use_id": id})
	updated, _ := d["updatedInput"].(map[string]any)
	answers, _ := json.Marshal(updated["answers"])
	c.s.write(toolResult(id, "answers: "+string(answers), false))
	c.reply(model, "answers "+string(answers))
}

func (c *chatter) reply(model, text string) {
	note("echo: %s", text)
	a := assistant(map[string]any{"type": "text", "text": "echo: " + text})
	a["message"].(map[string]any)["model"] = model
	c.s.write(a)
	c.send(map[string]any{"type": "assistant", "message": a["message"], "session_id": c.s.id})
}

// ask sends a control request and waits for pocketd's answer to it.
func (c *chatter) ask(request map[string]any) map[string]any {
	c.mu.Lock()
	c.next++
	id := fmt.Sprintf("req_%d", c.next)
	w := make(chan map[string]any, 1)
	c.waits[id] = w
	c.mu.Unlock()
	c.send(map[string]any{"type": "control_request", "request_id": id, "request": request})
	r := <-w
	d, _ := r["response"].(map[string]any)
	return d
}
