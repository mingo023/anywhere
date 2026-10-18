package main

import (
	"bufio"
	"crypto/rand"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

// appServer is `codex app-server`: JSON-RPC on stdio, one thread, with notes
// on stderr, which is the Terminal's screen. Threads are kept under
// $HOME/.codex-fake, so a restarted one resumes. Prompts:
//
//	<text>       reply "echo: <text>"
//	run <cmd>    ask to run cmd, then reply "echo: ran <cmd>" if allowed
//	slow <text>  reply after 3s, unless interrupted; a prompt meanwhile joins the reply
//
// Like codex, a bogus model fails each turn rather than the start.
type appServer struct {
	mu           sync.Mutex
	out          *json.Encoder
	initialized  bool
	experimental bool
	model        string
	effort       string
	thread       *thread
	turn         *liveTurn
	next         int
	waits        map[string]chan map[string]any
}

type thread struct {
	ID    string           `json:"id"`
	Turns []map[string]any `json:"turns"`
}

type liveTurn struct {
	id     string
	items  []map[string]any
	steers chan string
	stop   chan struct{}
}

func appServerMain() {
	a := &appServer{out: json.NewEncoder(os.Stdout), model: "fake-model", waits: map[string]chan map[string]any{}}
	for i := 2; i+1 < len(os.Args); i++ {
		if os.Args[i] != "-c" {
			continue
		}
		k, v, _ := strings.Cut(os.Args[i+1], "=")
		switch v = strings.Trim(v, `"`); k {
		case "model":
			a.model = v
		case "model_reasoning_effort":
			a.effort = v
		}
	}
	note("fake codex ready %s", strings.Join(os.Args[1:], " "))
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

func uuid() string {
	var b [8]byte
	rand.Read(b[:])
	return fmt.Sprintf("%x", b)
}

func (a *appServer) send(v map[string]any) {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.out.Encode(v)
}

func (a *appServer) notify(method string, params map[string]any) {
	a.send(map[string]any{"method": method, "params": params})
}

func (a *appServer) reply(id json.RawMessage, result any) {
	a.send(map[string]any{"id": id, "result": result})
}

func (a *appServer) refuse(id json.RawMessage, message string) {
	a.send(map[string]any{"id": id, "error": map[string]any{"code": -32600, "message": message}})
}

func threadsDir() string {
	return filepath.Join(os.Getenv("HOME"), ".codex-fake")
}

func (a *appServer) save() {
	b, _ := json.Marshal(a.thread)
	os.MkdirAll(threadsDir(), 0o700)
	os.WriteFile(filepath.Join(threadsDir(), a.thread.ID+".json"), b, 0o600)
}

func (a *appServer) opened(id json.RawMessage) {
	a.mu.Lock()
	status := "idle"
	turns := a.thread.Turns
	if a.turn != nil {
		status = "active"
		turns = append(turns, map[string]any{"id": a.turn.id, "status": "inProgress", "items": a.turn.items})
	}
	res := map[string]any{"thread": map[string]any{"id": a.thread.ID, "status": map[string]any{"type": status, "activeFlags": []string{}}, "turns": turns},
		"model": a.model, "reasoningEffort": a.effort}
	a.mu.Unlock()
	a.reply(id, res)
}

func (a *appServer) handle(id json.RawMessage, method string, p map[string]any) {
	threadID, _ := p["threadId"].(string)
	switch method {
	case "initialize":
		if a.initialized {
			a.refuse(id, "Already initialized")
			return
		}
		a.initialized = true
		caps, _ := p["capabilities"].(map[string]any)
		a.experimental = caps["experimentalApi"] == true
		a.reply(id, map[string]any{})
	case "initialized":
	case "thread/start":
		a.thread = &thread{ID: "th-" + uuid()}
		a.save()
		note("thread: %s", a.thread.ID)
		a.opened(id)
	case "thread/resume":
		if a.thread == nil || a.thread.ID != threadID {
			b, err := os.ReadFile(filepath.Join(threadsDir(), threadID+".json"))
			if err != nil {
				a.refuse(id, "no rollout found for thread id "+threadID)
				return
			}
			a.thread = &thread{}
			json.Unmarshal(b, a.thread)
		}
		note("resume: %s", threadID)
		a.opened(id)
	case "turn/start":
		text := ""
		if input, _ := p["input"].([]any); len(input) > 0 {
			text, _ = input[0].(map[string]any)["text"].(string)
		}
		a.mu.Lock()
		t := a.turn
		a.mu.Unlock()
		if t != nil {
			note("steer: %s", text)
			t.steers <- text
			a.reply(id, map[string]any{"turn": map[string]any{"id": t.id, "status": "inProgress"}})
			return
		}
		t = &liveTurn{id: "turn-" + uuid(), steers: make(chan string, 16), stop: make(chan struct{})}
		a.mu.Lock()
		a.turn = t
		a.mu.Unlock()
		a.reply(id, map[string]any{"turn": map[string]any{"id": t.id, "status": "inProgress"}})
		go a.run(t, text)
	case "turn/interrupt":
		a.mu.Lock()
		t := a.turn
		a.mu.Unlock()
		if t == nil {
			a.refuse(id, "no active turn to interrupt")
			return
		}
		note("interrupt")
		a.reply(id, map[string]any{})
		close(t.stop)
	case "thread/settings/update":
		if !a.experimental {
			a.refuse(id, "thread/settings/update requires experimentalApi capability")
			return
		}
		a.mu.Lock()
		if m, _ := p["model"].(string); m != "" {
			a.model = m
			note("set_model: %s", m)
		}
		if e, _ := p["effort"].(string); e != "" {
			a.effort = e
			note("effort: %s", e)
		}
		a.mu.Unlock()
		a.reply(id, map[string]any{})
	case "thread/compact/start":
		t := &liveTurn{id: "turn-" + uuid(), steers: make(chan string, 16), stop: make(chan struct{})}
		a.mu.Lock()
		a.turn = t
		a.mu.Unlock()
		a.reply(id, map[string]any{})
		go a.compact(t)
	default:
		a.refuse(id, "unknown method "+method)
	}
}

func (a *appServer) status(typ string, flags ...string) {
	a.notify("thread/status/changed", map[string]any{"threadId": a.thread.ID, "status": map[string]any{"type": typ, "activeFlags": append([]string{}, flags...)}})
}

func (a *appServer) item(t *liveTurn, it map[string]any, done bool) {
	method := "item/started"
	if done {
		method = "item/completed"
		a.mu.Lock()
		t.items = append(t.items, it)
		a.mu.Unlock()
	}
	a.notify(method, map[string]any{"threadId": a.thread.ID, "turnId": t.id, "item": it})
}

func (a *appServer) user(t *liveTurn, text string) {
	it := map[string]any{"type": "userMessage", "id": "u-" + uuid(), "content": []map[string]any{{"type": "text", "text": text}}}
	a.item(t, it, false)
	a.item(t, it, true)
}

func (a *appServer) say(t *liveTurn, text string) {
	note("echo: %s", text)
	id := "a-" + uuid()
	a.item(t, map[string]any{"type": "agentMessage", "id": id, "text": ""}, false)
	a.notify("item/agentMessage/delta", map[string]any{"threadId": a.thread.ID, "turnId": t.id, "itemId": id, "delta": "echo: " + text})
	a.item(t, map[string]any{"type": "agentMessage", "id": id, "text": "echo: " + text}, true)
}

func (a *appServer) start(t *liveTurn) {
	a.status("active")
	a.notify("turn/started", map[string]any{"threadId": a.thread.ID, "turn": map[string]any{"id": t.id, "status": "inProgress", "items": []any{}}})
}

// end ends turn t with status, and its error if it failed.
func (a *appServer) end(t *liveTurn, status, e string) {
	turn := map[string]any{"id": t.id, "status": status, "items": t.items, "durationMs": 5}
	if e != "" {
		turn["error"] = map[string]any{"message": e}
	}
	a.mu.Lock()
	a.turn = nil
	a.thread.Turns = append(a.thread.Turns, turn)
	a.save()
	a.mu.Unlock()
	if status == "failed" {
		a.status("systemError")
	} else {
		a.status("idle")
	}
	a.notify("turn/completed", map[string]any{"threadId": a.thread.ID, "turn": turn})
}

func (a *appServer) run(t *liveTurn, text string) {
	a.mu.Lock()
	model := a.model
	a.mu.Unlock()
	a.start(t)
	note("prompt: %s", text)
	a.user(t, text)
	switch {
	case model == "bogus":
		msg := "The 'bogus' model is not supported when using Codex with a ChatGPT account."
		a.notify("error", map[string]any{"threadId": a.thread.ID, "turnId": t.id, "error": map[string]any{"message": msg}})
		a.end(t, "failed", msg)
		return
	case strings.HasPrefix(text, "run "):
		a.runCommand(t, strings.TrimPrefix(text, "run "))
	case strings.HasPrefix(text, "slow "):
		replies := []string{strings.TrimPrefix(text, "slow ")}
		done := time.After(3 * time.Second)
		for {
			select {
			case <-t.stop:
				a.end(t, "interrupted", "")
				return
			case s := <-t.steers:
				a.user(t, s)
				replies = append(replies, s)
				continue
			case <-done:
			}
			break
		}
		a.say(t, strings.Join(replies, " + "))
	default:
		a.say(t, text)
	}
	a.end(t, "completed", "")
}

// runCommand asks to run cmd as codex does, and runs it if allowed.
func (a *appServer) runCommand(t *liveTurn, cmd string) {
	id := "c-" + uuid()
	it := map[string]any{"type": "commandExecution", "id": id, "command": cmd, "status": "inProgress"}
	a.item(t, it, false)
	a.status("active", "waitingOnApproval")
	a.mu.Lock()
	a.next++
	n := a.next
	w := make(chan map[string]any, 1)
	a.waits[fmt.Sprint(n)] = w
	a.mu.Unlock()
	a.send(map[string]any{"id": n, "method": "item/commandExecution/requestApproval",
		"params": map[string]any{"threadId": a.thread.ID, "turnId": t.id, "itemId": id, "command": cmd}})
	r := <-w
	note("approval: %v", r["decision"])
	a.notify("serverRequest/resolved", map[string]any{"threadId": a.thread.ID, "requestId": n})
	a.status("active")
	if r["decision"] != "accept" {
		it["status"] = "declined"
		a.item(t, it, true)
		return
	}
	out, code := "ran: "+cmd+"\n", 0
	it["status"], it["aggregatedOutput"], it["exitCode"] = "completed", out, code
	a.item(t, it, true)
	a.say(t, "ran "+cmd)
}

func (a *appServer) compact(t *liveTurn) {
	a.start(t)
	time.Sleep(300 * time.Millisecond)
	a.item(t, map[string]any{"type": "contextCompaction", "id": "k-" + uuid()}, true)
	note("compacted")
	a.end(t, "completed", "")
}
