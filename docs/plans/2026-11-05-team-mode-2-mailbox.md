# Team Mode 2: Mailbox Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** A durable per-session mailbox in pocketd: `Daemon.Mail(agentID, text)` stores a message, delivers it to a Claude or Codex chat as a prompt, and acks it only when the turn that showed it ends normally.

**Architecture:** `internal/mailbox` is a SQLite store (`state/mail.db`) with the API in the index (`docs/plans/2026-11-05-team-mode.md`, "pocketd packages"). `internal/daemon/mail.go` holds `Mail`, the pump (`Pump` → `deliver`) and `settle`; a per-agent lock makes a delivery and the settle that races it take turns. Chat drivers gain `Deliver(text) (ref, error)` through the optional interface `agent.Deliverer`: Claude writes a user line with a fresh `uuid` and settles on `command_lifecycle`; Codex calls `turn/start` or `turn/steer` and settles on `turn/completed`. The pump runs on `Mail`, on every status change (the `OnStatus` hook in `serve.go`), when a restored session settles, and after a stop sends mail back.

**Toolset** (Go 1.27, from `packages/pocketd` unless noted):
- Once per worktree, from the repo root: `scripts/build-ghostty.sh` (cgo needs libghostty; prints `ghostty ok (cached c7f68a7)` once built).
- Once per shell, so tests never reach the pocketd hosting your terminal: `unset POCKETD_PTY; export POCKET_HOME=$(mktemp -d); export POCKETD_SOCK=$POCKET_HOME/pocketd.sock`
- Store: `go test -count=1 ./internal/mailbox/`
- Daemon mail tests: `go test -count=1 -run 'Mail|Deliver' ./internal/daemon/`
- e2e mail tests: `go test -count=1 -run TestMail ./e2e/` (builds pocketd and the fakes first, ~1 min)
- Everything, as `scripts/check.sh` runs it: `go vet ./... && go test -race -count=1 ./...`

**Read first:**
- `docs/plans/2026-11-05-team-mode.md`, sections "Decisions → Storage" and "Contracts → pocketd packages": the mailbox API and delivery rules this plan implements.
- `docs/spike-orchestrator-team.md`, H3–H5 and finding 6: `command_lifecycle` states (`queued → started → completed`, or `cancelled` on a stop), and a mail sent mid-turn joining the running turn.
- `packages/pocketd/internal/skills/store.go`: the SQLite pattern to copy (`modernc.org/sqlite`, `SetMaxOpenConns(1)`).
- `packages/pocketd/internal/daemon/stream.go`: the Claude chat driver (`promptLine`, `streamLine`, `streamDriver`).
- `packages/pocketd/internal/daemon/codex_chat.go`: the Codex chat driver (`Notify`, `prompt`, `codexChatDriver`). Note `prompt` already sends `turn/start` mid-turn and the app-server steers it into the running turn.
- `packages/pocketd/internal/agent/agent.go:20-48, 110-118, 307-327`: `Driver`, the optional driver interfaces, and `Registry.OnStatus`, which runs **under the agent's lock** (never call back into the agent or block there).
- `packages/pocketd/internal/daemon/claude_test.go:654-720`: a chat claude without a real claude (`pocketd pipe claude -p` faked by the test binary; lines sent to claude wait in `pr.stream.inbox`). The daemon tests below reuse this.

---

## PR 1: Mailbox store, the pump, and Claude delivery

**Scope:** `internal/mailbox`, `agent.Deliverer`, Claude `Deliver` and settling, `Daemon.Mail`/`Pump`, settling on session exit, wiring in `serve.go` and in restore. Inert: nothing calls `Daemon.Mail` until plan 4 (team run); the pump runs on every status change but finds no mail. A Codex chat's mail stays queued until PR 2.
**Depends on:** nothing
**Done when:** `go vet ./... && go test -race -count=1 ./...` is green from `packages/pocketd`, including the new `TestMail*` e2e tests for Claude.

### Task 1.1: `internal/mailbox` — the store

**Files:**
- Create: `packages/pocketd/internal/mailbox/mailbox.go`
- Test: `packages/pocketd/internal/mailbox/mailbox_test.go`

**Context:** Copy `internal/skills/store.go`'s shape. States: `queued` (waiting), `shown` (delivered under `Ref`, waiting for its turn to end), `acked` (done, never delivered again). `Open` puts every `shown` mail back to `queued`: the turn that showed it died with the pocketd that sent it. The DSN adds `busy_timeout` because the e2e tests open the same file from their own process while pocketd runs; modernc strips a `?query` from a plain path and runs each `_pragma` (checked in `modernc.org/sqlite@v1.61.0/conn.go`). Order is by `id`, so a mail put back keeps its place ahead of newer ones. `Settle` with ref `""` settles all of the agent's shown mail (a session exit uses it; see "Contract changes needed"). Go tests in this repo are named as sentences (`TestASentenceLikeThis`) and compare whole values with `reflect.DeepEqual`.

**Step 1: Write the failing tests** — `mailbox_test.go`:

```go
package mailbox

import (
	"reflect"
	"testing"
	"time"
)

func open(t *testing.T, home string) *Store {
	t.Helper()
	s, err := Open(home)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { s.Close() })
	return s
}

func send(t *testing.T, s *Store, agent, text string) int64 {
	t.Helper()
	id, err := s.Send(agent, text)
	if err != nil {
		t.Fatal(err)
	}
	return id
}

func shown(t *testing.T, s *Store, id int64, ref string) {
	t.Helper()
	if err := s.Shown(id, ref); err != nil {
		t.Fatal(err)
	}
}

func settle(t *testing.T, s *Store, agent, ref string, ok bool) {
	t.Helper()
	if err := s.Settle(agent, ref, ok); err != nil {
		t.Fatal(err)
	}
}

// queued is agent's queued mail without Created, which TestMailIsStampedWhenSent checks.
func queued(t *testing.T, s *Store, agent string) []Mail {
	t.Helper()
	q, err := s.Queued(agent)
	if err != nil {
		t.Fatal(err)
	}
	for i := range q {
		q[i].Created = time.Time{}
	}
	return q
}

// Proves: Queued is per agent and oldest first.
func TestMailWaitsInItsAgentsQueueOldestFirst(t *testing.T) {
	s := open(t, t.TempDir())
	a := send(t, s, "lead", "first")
	send(t, s, "worker", "elsewhere")
	b := send(t, s, "lead", "second")

	want := []Mail{{ID: a, Agent: "lead", Text: "first", State: "queued"}, {ID: b, Agent: "lead", Text: "second", State: "queued"}}
	if got := queued(t, s, "lead"); !reflect.DeepEqual(got, want) {
		t.Fatalf("got  %+v\nwant %+v", got, want)
	}
}

// Proves: Created is the send time.
func TestMailIsStampedWhenSent(t *testing.T) {
	s := open(t, t.TempDir())
	before := time.Now().Truncate(time.Millisecond)
	send(t, s, "lead", "hi")
	after := time.Now()

	q, err := s.Queued("lead")
	if err != nil {
		t.Fatal(err)
	}
	if len(q) != 1 || q[0].Created.Before(before) || q[0].Created.After(after) {
		t.Fatalf("got %+v, want one stamped between %v and %v", q, before, after)
	}
}

// Proves: shown mail is no longer queued.
func TestShownMailLeavesTheQueue(t *testing.T) {
	s := open(t, t.TempDir())
	a := send(t, s, "lead", "first")
	b := send(t, s, "lead", "second")
	shown(t, s, a, "r1")

	want := []Mail{{ID: b, Agent: "lead", Text: "second", State: "queued"}}
	if got := queued(t, s, "lead"); !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}

// Proves: settled ok is acked, and a restart doesn't bring it back.
func TestMailSettledOkIsAckedForGood(t *testing.T) {
	home := t.TempDir()
	s, err := Open(home)
	if err != nil {
		t.Fatal(err)
	}
	id := send(t, s, "lead", "hi")
	shown(t, s, id, "r1")
	settle(t, s, "lead", "r1", true)
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}

	if got := queued(t, open(t, home), "lead"); got != nil {
		t.Fatalf("got %+v", got)
	}
}

// Proves: settled not ok is queued again, in its old place, with no ref.
func TestMailSettledNotOkGoesBackToTheQueue(t *testing.T) {
	s := open(t, t.TempDir())
	a := send(t, s, "lead", "first")
	b := send(t, s, "lead", "second")
	shown(t, s, a, "r1")
	settle(t, s, "lead", "r1", false)

	want := []Mail{{ID: a, Agent: "lead", Text: "first", State: "queued"}, {ID: b, Agent: "lead", Text: "second", State: "queued"}}
	if got := queued(t, s, "lead"); !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}

// Proves: a settle touches only its agent's mail shown under its ref.
func TestSettlingTouchesOnlyTheMailItsRefShowed(t *testing.T) {
	s := open(t, t.TempDir())
	a := send(t, s, "lead", "a")
	b := send(t, s, "lead", "b")
	c := send(t, s, "worker", "c")
	shown(t, s, a, "r1")
	shown(t, s, b, "r2")
	shown(t, s, c, "r1")

	settle(t, s, "lead", "r1", false)

	if got, want := queued(t, s, "lead"), []Mail{{ID: a, Agent: "lead", Text: "a", State: "queued"}}; !reflect.DeepEqual(got, want) {
		t.Fatalf("lead: got %+v", got)
	}
	if got := queued(t, s, "worker"); got != nil {
		t.Fatalf("worker: got %+v", got)
	}
}

// Proves: ref "" settles every shown mail of the agent, as a session exit does.
func TestSettlingWithNoRefSettlesAllTheAgentsShownMail(t *testing.T) {
	s := open(t, t.TempDir())
	a := send(t, s, "lead", "a")
	b := send(t, s, "lead", "b")
	shown(t, s, a, "r1")
	shown(t, s, b, "r2")

	settle(t, s, "lead", "", false)

	want := []Mail{{ID: a, Agent: "lead", Text: "a", State: "queued"}, {ID: b, Agent: "lead", Text: "b", State: "queued"}}
	if got := queued(t, s, "lead"); !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}

// Proves: Open puts mail shown before back in the queue.
func TestReopeningPutsShownMailBackInTheQueue(t *testing.T) {
	home := t.TempDir()
	s, err := Open(home)
	if err != nil {
		t.Fatal(err)
	}
	id := send(t, s, "lead", "hi")
	shown(t, s, id, "r1")
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}

	want := []Mail{{ID: id, Agent: "lead", Text: "hi", State: "queued"}}
	if got := queued(t, open(t, home), "lead"); !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}
```

**Step 2: Run the tests to verify they fail**

Run: `go test -count=1 ./internal/mailbox/`
Expected: FAIL, build errors such as `undefined: Store`, `undefined: Open`, `undefined: Mail`.

**Step 3: Write the implementation** — `mailbox.go`:

```go
// Package mailbox keeps what pocketd sends a chat session until a turn that
// showed it ends normally. It survives a pocketd restart.
package mailbox

import (
	"database/sql"
	"os"
	"path/filepath"
	"time"

	_ "modernc.org/sqlite"
)

const schema = `CREATE TABLE IF NOT EXISTS mail (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	agent TEXT NOT NULL,
	text TEXT NOT NULL,
	state TEXT NOT NULL,
	ref TEXT NOT NULL,
	created INTEGER NOT NULL
)`

const index = `CREATE INDEX IF NOT EXISTS mail_agent_state ON mail (agent, state)`

type Mail struct {
	ID      int64
	Agent   string
	Text    string
	State   string // queued | shown | acked
	Ref     string // Claude: the prompt's uuid. Codex: the turn id
	Created time.Time
}

type Store struct{ db *sql.DB }

// Open opens home/state/mail.db, creating it. Mail shown before goes back in
// the queue: the turn that showed it ended with the pocketd that sent it.
func Open(home string) (*Store, error) {
	path := filepath.Join(home, "state", "mail.db")
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, err
	}
	// Another process may hold the file a moment, as the e2e tests do: wait, don't fail with SQLITE_BUSY.
	db, err := sql.Open("sqlite", path+"?_pragma=busy_timeout(5000)")
	if err != nil {
		return nil, err
	}
	// One writer at a time; SQLite answers a second with SQLITE_BUSY.
	db.SetMaxOpenConns(1)
	for _, q := range []string{schema, index, `UPDATE mail SET state = 'queued', ref = '' WHERE state = 'shown'`} {
		if _, err := db.Exec(q); err != nil {
			db.Close()
			return nil, err
		}
	}
	return &Store{db}, nil
}

func (s *Store) Close() error { return s.db.Close() }

func (s *Store) Send(agent, text string) (int64, error) {
	r, err := s.db.Exec(`INSERT INTO mail (agent, text, state, ref, created) VALUES (?, ?, 'queued', '', ?)`, agent, text, time.Now().UnixMilli())
	if err != nil {
		return 0, err
	}
	return r.LastInsertId()
}

// Queued is agent's mail waiting to be shown, oldest first.
func (s *Store) Queued(agent string) ([]Mail, error) {
	rows, err := s.db.Query(`SELECT id, agent, text, state, ref, created FROM mail WHERE agent = ? AND state = 'queued' ORDER BY id`, agent)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []Mail
	for rows.Next() {
		var m Mail
		var created int64
		if err := rows.Scan(&m.ID, &m.Agent, &m.Text, &m.State, &m.Ref, &created); err != nil {
			return nil, err
		}
		m.Created = time.UnixMilli(created)
		out = append(out, m)
	}
	return out, rows.Err()
}

// Shown marks mail id delivered under ref.
func (s *Store) Shown(id int64, ref string) error {
	_, err := s.db.Exec(`UPDATE mail SET state = 'shown', ref = ? WHERE id = ? AND state = 'queued'`, ref, id)
	return err
}

// Settle acks agent's mail shown under ref, or puts it back in the queue.
// Ref "" settles all of agent's shown mail, as when its session exits.
func (s *Store) Settle(agent, ref string, ok bool) error {
	q := `UPDATE mail SET state = 'queued', ref = '' WHERE agent = ? AND state = 'shown' AND (? = '' OR ref = ?)`
	if ok {
		q = `UPDATE mail SET state = 'acked' WHERE agent = ? AND state = 'shown' AND (? = '' OR ref = ?)`
	}
	_, err := s.db.Exec(q, agent, ref, ref)
	return err
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test -count=1 ./internal/mailbox/ && go vet ./internal/mailbox/`
Expected: `ok  	pocketd/internal/mailbox`, no vet output.

### Task 1.2: fakeclaude reports `command_lifecycle`, and the Claude e2e mail tests

**Files:**
- Modify: `packages/pocketd/e2e/fakeclaude/chat.go` (doc comment `:14-28`, `chat` `:39-53`, `read` `:57-112`, `turn` `:120-171`; add `message`, `lifecycle`, `dropQueued`)
- Modify: `packages/pocketd/e2e/phone_test.go:46-50` (`Message.Agents` gains `Status`)
- Create: `packages/pocketd/e2e/mail_test.go`

**Context:** The e2e tests are written first: they stay red until Task 1.6. Real Claude (spike H3–H5) answers a stream-json user line that has a `uuid` with `{"type":"command_lifecycle","command_uuid":<uuid>,"state":…}` lines: `queued` when it reads the line, then `completed` when the turn that took it ends, or `cancelled` when a stop ends that turn or an interrupt with `cancel_queued: true` drops it while it waits. pocketd's stop sends `{"subtype":"interrupt","cancel_queued":true}` (`stream.go:465`). The fake mirrors this and notes each state on its screen (`queued: <text>`, …) so tests can wait on it. Lines without a `uuid` (every prompt typed in the chat) get no lifecycle lines, as in real Claude.

Nothing on the wire calls `Daemon.Mail` until plan 4, so the e2e tests queue mail by opening `state/mail.db` from the test process with `mailbox.Open(h.Home)` (e2e already imports `internal/ops`, `internal/state`). Pocketd picks it up at the agent's next status change, or when a restored session settles. `mailbox.Open` puts shown mail back in the queue, so tests open it only while no mail is shown: before the prompt that triggers delivery, or with pocketd stopped. `unacked` reads with pocketd stopped, where Open turns any shown-but-unacked mail back into queued, so an empty result means acked.

`h.Stop(sig)` and `h.up()` (`harness_test.go:117-198`) split `h.Restart()`; with `h.resume = true` the chat resumes under its old agent id. A phone gets `agent.list` right after `hello.ok`, so `statusIs` also reads the list in case the mail's turn ended before the phone connected. `agent.seen` clears Done, so after the restart the only Done is the mail's turn ending.

**Step 1: Write the failing tests**

`phone_test.go:46-50` becomes:

```go
	Agents []struct {
		ID       string `json:"id"`
		Status   string `json:"status"`
		Restore  string `json:"restore"`
		ChatOnly bool   `json:"chatOnly"`
	} `json:"agents"`
```

`e2e/mail_test.go`:

```go
//go:build unix

package e2e

import (
	"syscall"
	"testing"

	"pocketd/internal/mailbox"
)

// mail queues text for agent from the test's own process: nothing on the wire
// sends mail until team mode, so it goes at the agent's next status change or
// once a restored session settles. Opening the mailbox puts shown mail back in
// the queue, so call it only while no mail is shown.
func mail(t *testing.T, h *Harness, agent, text string) {
	t.Helper()
	box, err := mailbox.Open(h.Home)
	if err != nil {
		t.Fatal(err)
	}
	defer box.Close()
	if _, err := box.Send(agent, text); err != nil {
		t.Fatal(err)
	}
}

// unacked is the text of agent's mail no turn acked. Call it with pocketd
// stopped: opening the mailbox puts shown mail back in the queue.
func unacked(t *testing.T, h *Harness, agent string) []string {
	t.Helper()
	box, err := mailbox.Open(h.Home)
	if err != nil {
		t.Fatal(err)
	}
	defer box.Close()
	q, err := box.Queued(agent)
	if err != nil {
		t.Fatal(err)
	}
	var out []string
	for _, m := range q {
		out = append(out, m.Text)
	}
	return out
}

// statusIs matches agent's status in an update, or in the list a phone gets on hello.
func statusIs(agent, status string) func(Message) bool {
	return func(m Message) bool {
		if m.Type == "agent.list" {
			for _, a := range m.Agents {
				if a.ID == agent && a.Status == status {
					return true
				}
			}
			return false
		}
		return statusOf(agent, status)(m)
	}
}

func stop(t *testing.T, h *Harness) {
	t.Helper()
	if code := h.Stop(syscall.SIGTERM); code != 0 {
		t.Fatalf("pocketd exited %d", code)
	}
}

// Proves: mail survives a restart, goes to the chat once it resumes, and its
// completed turn acks it.
func TestMailWaitingWhilePocketdIsDownReachesTheResumedChatAndIsAcked(t *testing.T) {
	h, phone, term, id := chatClaude(t, func(h *Harness) { h.resume = true })
	phone.Prompt(id, "before")
	phone.WaitStatus(id, "done")
	phone.Send(map[string]any{"type": "agent.seen", "id": "seen", "agentIds": []string{id}})
	phone.WaitStatus(id, "idle")
	stop(t, h)

	mail(t, h, id, "from the mailbox")
	h.up()

	again := h.Owner()
	again.WaitFor("the mail's turn to end", statusIs(id, "done"))
	h.WaitScreen(term, "completed: from the mailbox")
	stop(t, h)
	if got := unacked(t, h, id); got != nil {
		t.Fatalf("unacked: %q", got)
	}
}

// Proves: mail a stop cancels goes to the chat again.
func TestMailAStopCancelsGoesToTheChatAgain(t *testing.T) {
	h, phone, term, id := chatClaude(t)
	mail(t, h, id, "from the mailbox")
	phone.Prompt(id, "hang")
	h.WaitScreen(term, "queued: from the mailbox")

	phone.Send(map[string]any{"type": "agent.interrupt", "id": "i1", "agentId": id})
	phone.WaitFor("the stop", func(m Message) bool { return m.Type == "ack" && m.ID == "i1" })

	h.WaitScreen(term, "cancelled: from the mailbox")
	phone.WaitStream(id, "assistant", "echo: from the mailbox")
	h.WaitScreen(term, "completed: from the mailbox")
}
```

`fakeclaude/chat.go` changes. Add to the doc comment above `type chatter` (after the prompt list):

```go
// A user line with a uuid gets command_lifecycle lines about it, each noted
// as "<state>: <text>": queued when read, then completed when its turn ends,
// or cancelled when a stop ends its turn or an interrupt with cancel_queued
// drops it while it waits. Such an interrupt drops every waiting prompt.
```

Replace `chat`'s prompt loop (`:48-52`) with:

```go
	prompts := make(chan message, 16)
	go c.read(prompts)
	for m := range prompts {
		c.turn(m)
	}
}

// message is a user line pocketd sent; claude reports on one with a uuid.
type message struct{ text, uuid string }
```

Replace `read` (`:57-112`) with:

```go
func (c *chatter) read(prompts chan message) {
	defer close(prompts)
	sc := bufio.NewScanner(os.Stdin)
	sc.Buffer(nil, 1<<20)
	for sc.Scan() {
		var l struct {
			Type      string `json:"type"`
			UUID      string `json:"uuid"`
			RequestID string `json:"request_id"`
			Request   struct {
				Subtype      string         `json:"subtype"`
				TaskID       string         `json:"task_id"`
				Model        string         `json:"model"`
				Settings     map[string]any `json:"settings"`
				CancelQueued bool           `json:"cancel_queued"`
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
			m := message{l.Message.Content, l.UUID}
			c.lifecycle(m, "queued")
			prompts <- m
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
				// Before the stop: the turn it ends must not take a dropped prompt first.
				if l.Request.CancelQueued {
					c.dropQueued(prompts)
				}
				select {
				case c.stop <- struct{}{}:
				default:
				}
			}
			c.send(map[string]any{"type": "control_response", "response": map[string]any{"subtype": "success", "request_id": l.RequestID, "response": map[string]any{}}})
		}
	}
}

// lifecycle reports state of m, as claude does for a line with a uuid.
func (c *chatter) lifecycle(m message, state string) {
	if m.uuid == "" {
		return
	}
	note("%s: %s", state, m.text)
	c.send(map[string]any{"type": "command_lifecycle", "command_uuid": m.uuid, "state": state})
}

// dropQueued cancels the prompts waiting for a turn.
func (c *chatter) dropQueued(prompts chan message) {
	for {
		select {
		case m := <-prompts:
			c.lifecycle(m, "cancelled")
		default:
			return
		}
	}
}
```

Replace `turn` (`:120-171`) with:

```go
func (c *chatter) turn(m message) {
	text := m.text
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
	failed, cancelled := false, false
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
		cancelled = failed
	case strings.HasPrefix(text, "agent "):
		c.backgroundAgent(model, id, strings.TrimPrefix(text, "agent "))
		failed = c.slow(model, "done")
		cancelled = failed
	case strings.HasPrefix(text, "slow "):
		failed = c.slow(model, strings.TrimPrefix(text, "slow "))
		cancelled = failed
	case text == "hang":
		<-c.stop
		c.s.write(user("[Request interrupted by user]"))
		failed, cancelled = true, true
	default:
		c.reply(model, text)
	}
	c.s.hook("Stop", nil)()
	if !failed {
		c.s.write(map[string]any{"type": "system", "subtype": "stop_hook_summary", "hookCount": 1})
	}
	if cancelled {
		c.lifecycle(m, "cancelled")
	} else {
		c.lifecycle(m, "completed")
	}
	c.send(map[string]any{"type": "result", "subtype": "success", "is_error": failed, "session_id": c.s.id})
}
```

**Step 2: Run the tests to verify they fail, and that the fake still serves the old ones**

Run: `go test -count=1 -run TestMail ./e2e/`
Expected: FAIL. `TestMailWaitingWhilePocketdIsDownReachesTheResumedChatAndIsAcked` fails with `waiting for the mail's turn to end: context deadline exceeded`; `TestMailAStopCancelsGoesToTheChatAgain` fails with `timed out waiting for screen to show "queued: from the mailbox"` (pocketd has no mailbox yet).

Run: `go test -count=1 -run 'TestAChat|TestAStop|TestAPrompt|TestACompact|TestATool|TestAQuestion|TestACreatedChat|TestAnUpgradeKeepsTheChatAgent|TestARestartResumesTheChatAgent' ./e2e/`
Expected: `ok  	pocketd/e2e`.

Step 3 and Step 4 of this task are Tasks 1.3–1.6; the e2e tests go green at Task 1.6.

### Task 1.3: `agent.Deliverer` and Claude's `Deliver`

**Files:**
- Modify: `packages/pocketd/internal/agent/agent.go:43-48` (add `Deliverer` after `McpLister`)
- Modify: `packages/pocketd/internal/daemon/stream.go` (`promptLine` `:231-233`, `Prompt` `:412-425`, `Compact` `:509-516`, import `pocketd/internal/terminal`; add `Deliver`)
- Create (test): `packages/pocketd/internal/daemon/mail_test.go`

**Context:** `promptLine` gains a `uuid`; only `Deliver` passes one, so prompts typed in the chat stay as they are and make no lifecycle lines. The mail line carries `uuid`, `session_id: ""` and `parent_tool_use_id: null`, the shape the spike proved (`/tmp/spike-team/spike.mjs:41`). `terminal.NewID()` (`internal/terminal/terminal.go:120`) is already a v4 UUID. `Deliver` calls `a.Prompted()` as `Prompt` does, so a stop before claude's turn starts still counts. The test harness: `fakeAgent(t, "pocketd")` with args `pipe claude -p` makes pocketd think a chat claude runs in the terminal (`presence_test.go:30-63`); nothing attaches the pipe, so every line sent to claude waits in `pr.stream.inbox`. The helpers below are reused by Tasks 1.4 and 1.5.

**Step 1: Write the failing test** — `mail_test.go`:

```go
//go:build unix

package daemon

import (
	"encoding/json"
	"fmt"
	"reflect"
	"testing"

	"pocketd/internal/agent"
	"pocketd/internal/terminal"
)

// chatIn is a chat claude behind a pipe nothing reads: what pocketd sends it
// waits in its stream's inbox.
func chatIn(t *testing.T, d *Daemon) *presence {
	t.Helper()
	term, err := d.Terminals.Spawn(terminal.Spec{Cmd: fakeAgent(t, "pocketd"), Args: []string{"pipe", "claude", "-p"}, Env: []string{"PATH=/bin:/usr/bin"}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(term.Close)
	waitAgent(t, d, term)
	return d.presentIn(term.Info().ID)
}

// sentLine is a user line pocketd sent claude.
type sentLine struct{ UUID, Text string }

func sentToClaude(pr *presence) []sentLine {
	pr.stream.mu.Lock()
	defer pr.stream.mu.Unlock()
	var out []sentLine
	for _, raw := range pr.stream.inbox {
		var l struct {
			Type    string `json:"type"`
			UUID    string `json:"uuid"`
			Message struct {
				Content string `json:"content"`
			} `json:"message"`
		}
		if json.Unmarshal([]byte(raw), &l) == nil && l.Type == "user" {
			out = append(out, sentLine{l.UUID, l.Message.Content})
		}
	}
	return out
}

// sentN waits until pocketd has sent pr's claude n user lines.
func sentN(t *testing.T, pr *presence, n int) []sentLine {
	t.Helper()
	var got []sentLine
	eventually(t, fmt.Sprintf("%d user lines", n), func() bool {
		got = sentToClaude(pr)
		return len(got) == n
	})
	return got
}

// Proves: Deliver sends a prompt under a fresh uuid it returns; a chat prompt has none.
func TestDeliverSendsClaudeAPromptUnderTheUUIDItReturns(t *testing.T) {
	d := newDaemon(t)
	pr := chatIn(t, d)
	if err := pr.a.Driver().Prompt("typed"); err != nil {
		t.Fatal(err)
	}

	ref, err := pr.a.Driver().(agent.Deliverer).Deliver("mailed")
	if err != nil {
		t.Fatal(err)
	}

	want := []sentLine{{"", "typed"}, {ref, "mailed"}}
	if got := sentToClaude(pr); !reflect.DeepEqual(got, want) || len(ref) != 36 {
		t.Fatalf("got %+v (ref %q), want %+v with a uuid", got, ref, want)
	}
}
```

**Step 2: Run the test to verify it fails**

Run: `go test -count=1 -run Deliver ./internal/daemon/`
Expected: FAIL, build error `undefined: agent.Deliverer`.

**Step 3: Write the implementation**

`agent.go`, after `McpLister` (`:48`):

```go
// Deliverer is a chat driver that takes mail: it sends text as a prompt and
// returns the ref the turn that shows it settles under.
type Deliverer interface {
	Deliver(text string) (ref string, err error)
}
```

`stream.go`, replace `promptLine` (`:231-233`):

```go
// promptLine is a user line. Claude reports on one with a uuid in command_lifecycle lines.
func promptLine(text, uuid string) map[string]any {
	line := map[string]any{"type": "user", "message": map[string]any{"role": "user", "content": text}}
	if uuid != "" {
		line["uuid"], line["session_id"], line["parent_tool_use_id"] = uuid, "", nil
	}
	return line
}
```

In `Prompt` (`:423`) and `Compact` (`:514`), `promptLine(text)` becomes `promptLine(text, "")` and `promptLine("/compact")` becomes `promptLine("/compact", "")`.

After `Prompt` (`:425`), add:

```go
// Deliver sends text as a prompt under a fresh uuid, the ref claude's
// command_lifecycle lines name.
func (c streamDriver) Deliver(text string) (string, error) {
	if !c.inForeground() {
		return "", errNotForeground
	}
	ref := terminal.NewID()
	c.a.Prompted()
	c.pr.stream.send(promptLine(text, ref))
	return ref, nil
}
```

Add `"pocketd/internal/terminal"` to `stream.go`'s imports.

**Step 4: Run the test to verify it passes**

Run: `go test -count=1 -run 'Deliver|Chat|Stop|SendNow' ./internal/daemon/`
Expected: `ok  	pocketd/internal/daemon`.

### Task 1.4: `Daemon.Mail` and the pump

**Files:**
- Create: `packages/pocketd/internal/daemon/mail.go`
- Modify: `packages/pocketd/internal/daemon/daemon.go:27-59` (`Daemon` gains `Mailbox` and `mailing`; import `pocketd/internal/mailbox`)
- Test: `packages/pocketd/internal/daemon/mail_test.go`

**Context:** Index rules: deliver every queued mail of an agent whose status is `idle`, `working` or `done`; hold it while `needsYou`, closed (the agent is gone from `d.Agents`), or not a chat (its driver isn't an `agent.Deliverer`: a TUI's `termDriver`, or the `pending` driver of a session still restoring, `restore.go:46-70`). `OnStatus` runs under the agent's lock, so `Pump` only starts a goroutine. `deliver` holds a per-agent lock from `Deliver` until `Shown` is written, so a settle that races it (Task 1.5) waits and finds the mail shown. A failed `Deliver` (claude left the foreground, codex still starting) leaves the rest queued for the next pump. `withMailbox` wires `OnStatus` as `serve.go` will in Task 1.6; set it before any agent exists (`agent.go:115-117`). Tests that check holding call `d.deliver` directly, so the check isn't racing a goroutine.

**Step 1: Write the failing tests** — append to `mail_test.go` (add `"pocketd/internal/mailbox"` to its imports):

```go
// withMailbox gives d a mailbox pumped on each status change, as serve wires it.
func withMailbox(t *testing.T, d *Daemon) {
	t.Helper()
	box, err := mailbox.Open(d.Home)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { box.Close() })
	d.Mailbox = box
	d.Agents.OnStatus = func(id, _, _, _ string) { d.Pump(id) }
}

func queuedTexts(t *testing.T, d *Daemon, agentID string) []string {
	t.Helper()
	q, err := d.Mailbox.Queued(agentID)
	if err != nil {
		t.Fatal(err)
	}
	var out []string
	for _, m := range q {
		out = append(out, m.Text)
	}
	return out
}

// Proves: mail to an idle chat goes to claude under a uuid and leaves the queue.
func TestMailToAnIdleChatGoesToClaudeAndLeavesTheQueue(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	pr := chatIn(t, d)
	id := pr.a.ID()

	if err := d.Mail(id, "hello"); err != nil {
		t.Fatal(err)
	}

	if got := sentN(t, pr, 1); got[0].Text != "hello" || got[0].UUID == "" {
		t.Fatalf("got %+v", got)
	}
	eventually(t, "the mail shown", func() bool { return queuedTexts(t, d, id) == nil })
}

// Proves: mail goes mid-turn too, for claude to take at a tool boundary or next.
func TestMailSentMidTurnGoesToClaudeAtOnce(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	pr := chatIn(t, d)
	pr.working("")

	if err := d.Mail(pr.a.ID(), "hello"); err != nil {
		t.Fatal(err)
	}

	if got := sentN(t, pr, 1); got[0].Text != "hello" {
		t.Fatalf("got %+v", got)
	}
}

// Proves: needsYou holds mail, and the next status change sends it.
func TestMailWaitsWhileTheChatNeedsYouAndGoesOnceItWorksAgain(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	pr := chatIn(t, d)
	id := pr.a.ID()
	pr.a.NeedsYou()
	if _, err := d.Mailbox.Send(id, "hello"); err != nil {
		t.Fatal(err)
	}

	d.deliver(id)
	if got := sentToClaude(pr); got != nil {
		t.Fatalf("sent while it needs you: %+v", got)
	}

	pr.a.Working()
	if got := sentN(t, pr, 1); got[0].Text != "hello" {
		t.Fatalf("got %+v", got)
	}
}

// Proves: an agent in its TUI is not a chat, so its mail stays queued.
func TestMailToATerminalAgentStaysQueued(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	_, pr := claudeIn(t, d)
	id := pr.a.ID()
	if _, err := d.Mailbox.Send(id, "hello"); err != nil {
		t.Fatal(err)
	}

	d.deliver(id)

	if got := queuedTexts(t, d, id); !reflect.DeepEqual(got, []string{"hello"}) {
		t.Fatalf("queued %q", got)
	}
}
```

**Step 2: Run the tests to verify they fail**

Run: `go test -count=1 -run Mail ./internal/daemon/`
Expected: FAIL, build errors `d.Mailbox undefined`, `d.Mail undefined`, `d.Pump undefined`, `d.deliver undefined`.

**Step 3: Write the implementation**

`daemon.go`, in `Daemon` after `Accounts` (`:49`):

```go
	// Mailbox keeps Mail's messages until a turn that showed them ends normally; nil takes none.
	Mailbox *mailbox.Store
```

and after `restoring` (`:58`):

```go
	mailing sync.Map // a *sync.Mutex per agent id: one delivery or settle at a time
```

Add `"pocketd/internal/mailbox"` to `daemon.go`'s imports.

`mail.go`:

```go
package daemon

import (
	"errors"
	"log"
	"sync"

	"pocketd/internal/agent"
)

var errNoMailbox = errors.New("pocketd has no mailbox")

// Mail keeps text for agentID's chat until a turn that showed it ends normally.
func (d *Daemon) Mail(agentID, text string) error {
	if d.Mailbox == nil {
		return errNoMailbox
	}
	if _, err := d.Mailbox.Send(agentID, text); err != nil {
		return err
	}
	d.Pump(agentID)
	return nil
}

// Pump delivers agentID's queued mail off the caller's goroutine: a status
// change calls it under the agent's lock.
func (d *Daemon) Pump(agentID string) {
	if d.Mailbox != nil {
		go d.deliver(agentID)
	}
}

func (d *Daemon) deliver(agentID string) {
	lock := d.mailLock(agentID)
	lock.Lock()
	defer lock.Unlock()
	a, err := d.Agents.Get(agentID)
	if err != nil {
		return
	}
	to, ok := a.Driver().(agent.Deliverer)
	if !ok || !takesMail(a.Summary().Status) {
		return
	}
	queued, err := d.Mailbox.Queued(agentID)
	if err != nil {
		log.Printf("mail %s: %v", agentID, err)
		return
	}
	for _, m := range queued {
		ref, err := to.Deliver(m.Text)
		if err != nil {
			log.Printf("mail %s: deliver: %v", agentID, err)
			return
		}
		if err := d.Mailbox.Shown(m.ID, ref); err != nil {
			log.Printf("mail %s: %v", agentID, err)
			return
		}
	}
}

// takesMail holds mail while the chat waits on the user.
func takesMail(status string) bool {
	return status == "idle" || status == "working" || status == "done"
}

func (d *Daemon) mailLock(agentID string) *sync.Mutex {
	l, _ := d.mailing.LoadOrStore(agentID, &sync.Mutex{})
	return l.(*sync.Mutex)
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test -count=1 -race -run 'Mail|Deliver' ./internal/daemon/`
Expected: `ok  	pocketd/internal/daemon`.

### Task 1.5: Settling — Claude's `command_lifecycle` and a session's exit

**Files:**
- Modify: `packages/pocketd/internal/daemon/mail.go` (add `settle`)
- Modify: `packages/pocketd/internal/daemon/stream.go` (`streamLine` struct `:275-302`, `streamLine` func `:313-346`)
- Modify: `packages/pocketd/internal/daemon/presence.go:405-415` (`endAgent`)
- Test: `packages/pocketd/internal/daemon/mail_test.go`

**Context:** `completed` acks the mail shown under `command_uuid`; `cancelled` puts it back and pumps, so mail a stop cancelled goes again (with a new uuid) once the chat takes mail. A lifecycle line with no `command_uuid` must settle nothing: ref `""` means "all". `endAgent` (`presence.go:383-422`) is where every session ends; it settles all the agent's shown mail not ok and doesn't pump: the agent is gone, or a restore or account switch brings it back and `finish` pumps then (Task 1.6). `settle` takes the per-agent lock, so a settle that comes while `deliver` is between `Deliver` and `Shown` waits for it. `streamLine` runs on the pipe's read loop; `settle` only waits on a short local write, so it doesn't stall claude's output.

**Step 1: Write the failing tests** — append to `mail_test.go`:

```go
func lifecycle(uuid, state string) string {
	return `{"type":"command_lifecycle","command_uuid":"` + uuid + `","state":"` + state + `"}`
}

// Proves: completed acks the mail, so the session's exit doesn't put it back.
func TestMailClaudeCompletesIsAckedAndOutlivesTheSession(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	pr := chatIn(t, d)
	id := pr.a.ID()
	if err := d.Mail(id, "hello"); err != nil {
		t.Fatal(err)
	}
	sent := sentN(t, pr, 1)[0]

	d.streamLine(pr, lifecycle(sent.UUID, "completed"))
	d.endAgent(pr)

	if got := queuedTexts(t, d, id); got != nil {
		t.Fatalf("queued after the exit: %q", got)
	}
}

// Proves: a session that exits puts the mail it was shown back in the queue.
func TestMailAChatWasShownGoesBackToTheQueueWhenItExits(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	pr := chatIn(t, d)
	id := pr.a.ID()
	if err := d.Mail(id, "hello"); err != nil {
		t.Fatal(err)
	}
	sentN(t, pr, 1)

	d.endAgent(pr)

	if got := queuedTexts(t, d, id); !reflect.DeepEqual(got, []string{"hello"}) {
		t.Fatalf("queued %q", got)
	}
}

// Proves: cancelled mail goes to claude again, under a new uuid.
func TestMailClaudeCancelsGoesToItAgainUnderANewUUID(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	pr := chatIn(t, d)
	if err := d.Mail(pr.a.ID(), "hello"); err != nil {
		t.Fatal(err)
	}
	first := sentN(t, pr, 1)[0]

	d.streamLine(pr, lifecycle(first.UUID, "cancelled"))

	again := sentN(t, pr, 2)[1]
	if again.Text != "hello" || again.UUID == "" || again.UUID == first.UUID {
		t.Fatalf("first %+v, again %+v", first, again)
	}
}

// Proves: a lifecycle line naming no uuid settles nothing.
func TestMailIsNotAckedByALifecycleLineWithNoUUID(t *testing.T) {
	d := newDaemon(t)
	withMailbox(t, d)
	pr := chatIn(t, d)
	id := pr.a.ID()
	if err := d.Mail(id, "hello"); err != nil {
		t.Fatal(err)
	}
	sentN(t, pr, 1)

	d.streamLine(pr, lifecycle("", "completed"))
	d.endAgent(pr)

	if got := queuedTexts(t, d, id); !reflect.DeepEqual(got, []string{"hello"}) {
		t.Fatalf("queued %q", got)
	}
}
```

**Step 2: Run the tests to verify they fail**

Run: `go test -count=1 -run Mail ./internal/daemon/`
Expected: FAIL. `TestMailClaudeCompletesIsAckedAndOutlivesTheSession` passes (nothing requeues yet, so this one only proves itself once exit settling exists); `TestMailAChatWasShownGoesBackToTheQueueWhenItExits` fails with `queued []`; `TestMailClaudeCancelsGoesToItAgainUnderANewUUID` fails with `timed out waiting for 2 user lines`; `TestMailIsNotAckedByALifecycleLineWithNoUUID` fails with `queued []`.

**Step 3: Write the implementation**

`mail.go`, append:

```go
// settle acks the mail shown under ref, or puts it back in the queue; ref ""
// is all of agentID's shown mail.
func (d *Daemon) settle(agentID, ref string, ok bool) {
	if d.Mailbox == nil {
		return
	}
	lock := d.mailLock(agentID)
	lock.Lock()
	err := d.Mailbox.Settle(agentID, ref, ok)
	lock.Unlock()
	if err != nil {
		log.Printf("mail %s: settle: %v", agentID, err)
	}
}
```

`stream.go`, in `type streamLine` after `IsError` (`:290`):

```go
	// A command_lifecycle's: what came of the user line with this uuid.
	CommandUUID string `json:"command_uuid"`
	State       string `json:"state"`
```

`stream.go`, in `streamLine`'s switch, after the `control_cancel_request` case (`:325`):

```go
	case l.Type == "command_lifecycle" && l.CommandUUID != "":
		switch l.State {
		case "completed":
			d.settle(pr.a.ID(), l.CommandUUID, true)
		case "cancelled":
			d.settle(pr.a.ID(), l.CommandUUID, false)
			d.Pump(pr.a.ID())
		}
```

`presence.go`, in `endAgent`, between `pr.mu.Unlock()` (`:412`) and `if moving {` (`:413`):

```go
	// The session took the turns that showed its mail with it.
	d.settle(pr.a.ID(), "", false)
```

**Step 4: Run the tests to verify they pass**

Run: `go test -count=1 -race -run 'Mail|Deliver|Request' ./internal/daemon/`
Expected: `ok  	pocketd/internal/daemon`.

### Task 1.6: Wire the mailbox into pocketd, and pump when a restore settles

**Files:**
- Modify: `packages/pocketd/cmd/pocketd/serve.go` (imports `:22-57`; `OnStatus` `:143-150`; after the skills store `:213-216`)
- Modify: `packages/pocketd/internal/daemon/restore.go:258-261` (`finish`)
- Test: `packages/pocketd/e2e/mail_test.go` (from Task 1.2)

**Context:** The mailbox must be open before `d.Adopt` / `d.Restore` (`serve.go:233-252`), since a settling restore pumps. A restored agent drives through `pending`, which takes no mail, until `finish(h, "")` swaps in its live driver (`restore.go:253-256`); its status may not change then, so `finish` pumps. That is how mail waiting across a restart is delivered ("on start" in the index). An upgrade's handoff also settles through `finish`. If the mailbox can't open, pocketd logs it and runs without one, as it does for skills; `Mail` then returns `errNoMailbox`.

**Step 1: The failing tests** are Task 1.2's e2e tests.

**Step 2: Run them to verify they fail**

Run: `go test -count=1 -run TestMail ./e2e/`
Expected: FAIL as in Task 1.2 (`serve` sets no `Mailbox` yet).

**Step 3: Write the implementation**

`serve.go` imports: add `"pocketd/internal/mailbox"` (alphabetical, after `"pocketd/internal/logfile"`).

`serve.go`, `OnStatus` (`:143-150`) becomes:

```go
	d.Agents.OnStatus = func(id, provider, from, to string) {
		evs.Emit(events.Event{Kind: "status", Agent: id, Provider: provider, From: from, To: to})
		notifier.Kick(id, from, to)
		d.Pump(id)
		select {
		case kick <- struct{}{}:
		default:
		}
	}
```

`serve.go`, after the skills store block (`:213-216`):

```go
	mail, err := mailbox.Open(home)
	if err != nil {
		log.Printf("mailbox: %v", err)
	} else {
		defer mail.Close()
		d.Mailbox = mail
	}
```

`restore.go`, in `finish`, the block at `:258-261` becomes:

```go
	if reason == "" && h.pr != nil {
		// A switch picked while the agent was settling waits for it.
		go d.switchPending(h.pr)
		// So does its mail: a restoring agent's driver takes none.
		d.Pump(h.a.ID())
	}
```

**Step 4: Run the tests to verify they pass**

Run: `go test -count=1 -run TestMail ./e2e/`
Expected: `ok  	pocketd/e2e`.

### PR 1 final checks

From `packages/pocketd`: `go vet ./... && go test -race -count=1 ./...` → every package `ok`. `gofmt -l .` prints nothing.

---

## PR 2: Codex delivery

**Scope:** Codex chats take mail: `turn/start` when no turn is under way, `turn/steer` with `expectedTurnId` mid-turn; settled on `turn/completed`. fakecodex gains `turn/steer`. Still inert until plan 4.
**Depends on:** PR 1
**Done when:** `go vet ./... && go test -race -count=1 ./...` is green, including the Codex `TestMail*` e2e tests.

### Task 2.1: fakecodex takes `turn/steer`, and the Codex e2e mail tests

**Files:**
- Modify: `packages/pocketd/e2e/fakecodex/appserver.go` (doc comment `:15-23`, `turn/start` case `:170-189`; add `turn/steer` case and `inputText`)
- Modify: `packages/pocketd/e2e/mail_test.go` (append)

**Context:** The chat codex speaks JSON-RPC over `pocketd pipe`'s stdio (`codexChat` is the `codex.Wire`), not the app-server's websocket, so `internal/codex/codextest` (a websocket server) can't stand in for it; the e2e fake `e2e/fakecodex` is the app-server these tests drive. Its `turn/start` already steers a running turn (`steer: <text>` on screen; only a `slow` turn takes steers) and replies with that turn's id. Real `turn/steer` (`/tmp/spike-team/schema/v2/TurnSteerParams.json`) takes `threadId`, `expectedTurnId`, `input` and returns `{"turnId"}`; it fails when `expectedTurnId` isn't the active turn. `turn/completed` carries `turn.status`: `completed`, `interrupted` or `failed`. Ordering the tests rely on: the app-server's notifications are handled in order (`codex/rpc.go:109-142`), so once the phone sees the next turn's answer, the earlier `turn/completed` (and its settle) has been handled.

**Step 1: Write the failing tests**

`appserver.go`: add to the doc comment's prompt list:

```go
//	turn/steer   joins the slow turn under way, as a prompt sent mid-turn does
```

Replace the start of the `turn/start` case (`:171-174`, the `text := ""` … `}` lines) with `text := inputText(p)`, and add after the `turn/start` case:

```go
	case "turn/steer":
		text := inputText(p)
		a.mu.Lock()
		t := a.turn
		a.mu.Unlock()
		if t == nil || p["expectedTurnId"] != t.id {
			a.refuse(id, "expected active turn id mismatch")
			return
		}
		note("steer: %s", text)
		t.steers <- text
		a.reply(id, map[string]any{"turnId": t.id})
```

and at the end of the file:

```go
// inputText is the text of a turn call's first input.
func inputText(p map[string]any) string {
	input, _ := p["input"].([]any)
	if len(input) == 0 {
		return ""
	}
	text, _ := input[0].(map[string]any)["text"].(string)
	return text
}
```

Append to `e2e/mail_test.go`:

```go
// Proves: mail sent mid-turn steers into the running codex turn, and that
// turn completing acks it.
func TestMailSteeredIntoAChatCodexTurnIsAckedWhenTheTurnCompletes(t *testing.T) {
	h, phone, term, id := chatCodex(t, nil)
	mail(t, h, id, "from the mailbox")
	phone.Prompt(id, "slow first")
	h.WaitScreen(term, "steer: from the mailbox")
	phone.WaitStream(id, "assistant", "echo: first + from the mailbox")
	phone.WaitStatus(id, "done")

	phone.Prompt(id, "after")
	phone.WaitStream(id, "assistant", "echo: after")
	stop(t, h)
	if got := unacked(t, h, id); got != nil {
		t.Fatalf("unacked: %q", got)
	}
}

// Proves: mail in an interrupted codex turn goes in again, as its own turn.
func TestMailInAnInterruptedChatCodexTurnGoesInAgain(t *testing.T) {
	h, phone, term, id := chatCodex(t, nil)
	mail(t, h, id, "from the mailbox")
	phone.Prompt(id, "slow never")
	h.WaitScreen(term, "steer: from the mailbox")

	phone.Send(map[string]any{"type": "agent.interrupt", "id": "i1", "agentId": id})
	phone.WaitFor("the stop", func(m Message) bool { return m.Type == "ack" && m.ID == "i1" })

	phone.WaitStream(id, "assistant", "echo: from the mailbox")
	h.WaitScreen(term, "prompt: from the mailbox")
}
```

**Step 2: Run the tests to verify they fail, and that fakecodex still serves the old ones**

Run: `go test -count=1 -run 'TestMail.*Codex' ./e2e/`
Expected: FAIL, both with `timed out waiting for screen to show "steer: from the mailbox"` (a codex chat driver is no `agent.Deliverer` yet, so its mail stays queued).

Run: `go test -count=1 -run 'Codex' ./e2e/`
Expected: every test except the two new ones passes (`TestAPromptSentMidTurnSteersTheChatCodexTurn` among them).

### Task 2.2: Codex `Deliver`, and settling on `turn/completed`

**Files:**
- Modify: `packages/pocketd/internal/daemon/codex_chat.go` (`Notify` `:112-148`; add `Deliver` after `Prompt` `:442-457`)
- Test: `packages/pocketd/e2e/mail_test.go` (from Task 2.1)

**Context:** `x.turn` is the turn under way as pocketd last heard (`turn/started` sets it, `turn/completed` clears it, `Notify` `:138-145`). With a turn, `Deliver` steers it with `expectedTurnId`; if that turn just ended, codex refuses, `deliver` logs it and leaves the mail queued, and the turn's end (a status change) pumps again. With none, `turn/start` starts one (or, in the moment before pocketd hears `turn/started`, the app-server steers it into the running turn, as `prompt` relies on, `codex_chat.go:291-311`); its reply's `turn.id` is the ref. `usable()` refuses while the thread is still opening; the mail stays queued. In `Notify`, settle **after** clearing `x.turn`, so the pump the settle starts never steers a turn that ended. An `interrupted` turn's mail is pumped again at once (a stop sent it back); a `failed` turn's waits for the next status change, so a model that keeps failing doesn't spin. Guard the empty turn id: ref `""` means all mail.

**Step 1: The failing tests** are Task 2.1's.

**Step 2: Run them to verify they fail**

Run: `go test -count=1 -run 'TestMail.*Codex' ./e2e/`
Expected: FAIL as in Task 2.1.

**Step 3: Write the implementation**

`codex_chat.go`, in `Notify`'s params struct (`:116-118`), `Turn` becomes:

```go
		Turn struct {
			ID     string `json:"id"`
			Status string `json:"status"`
		} `json:"turn"`
```

and the `turn/started`, `turn/completed` case (`:138-145`) becomes:

```go
	case "turn/started", "turn/completed":
		if x.wait() && p.ThreadID == x.threadID() {
			x.mu.Lock()
			if x.turn = p.Turn.ID; method == "turn/completed" {
				x.turn = ""
			}
			x.mu.Unlock()
			if method == "turn/completed" && p.Turn.ID != "" {
				x.d.settle(x.pr.a.ID(), p.Turn.ID, p.Turn.Status == "completed")
				// A stop sent it back; a failed turn's mail waits for the next turn, so a failing model doesn't spin.
				if p.Turn.Status == "interrupted" {
					x.d.Pump(x.pr.a.ID())
				}
			}
		}
```

After `codexChatDriver.Prompt` (`:457`):

```go
// Deliver steers the turn under way, or starts one; the turn's id is the ref.
func (c codexChatDriver) Deliver(text string) (string, error) {
	if !c.inForeground() {
		return "", errNotForeground
	}
	if err := c.x.usable(); err != nil {
		return "", err
	}
	input := []map[string]any{{"type": "text", "text": text}}
	c.x.mu.Lock()
	turn := c.x.turn
	c.x.mu.Unlock()
	if turn != "" {
		res, err := c.x.call("turn/steer", map[string]any{"expectedTurnId": turn, "input": input})
		if err != nil {
			return "", err
		}
		var r struct {
			TurnID string `json:"turnId"`
		}
		json.Unmarshal(res, &r)
		return r.TurnID, nil
	}
	res, err := c.x.call("turn/start", map[string]any{"input": input})
	if err != nil {
		return "", err
	}
	var r struct {
		Turn struct {
			ID string `json:"id"`
		} `json:"turn"`
	}
	json.Unmarshal(res, &r)
	return r.Turn.ID, nil
}
```

**Step 4: Run the tests to verify they pass**

Run: `go test -count=1 -run 'TestMail|Codex' ./e2e/`
Expected: `ok  	pocketd/e2e`.

### PR 2 final checks

From `packages/pocketd`: `go vet ./... && go test -race -count=1 ./...` → every package `ok`. `gofmt -l .` prints nothing.

---

## Contract changes needed

Folded into the index (`docs/plans/2026-11-05-team-mode.md`, "pocketd packages"):

1. **`Settle(agent, "", ok)`** settles all of `agent`'s shown mail. A session exit uses it (`Settle(agent, "", false)`). Callers must never pass an empty ref by accident; both drivers guard it.
2. **Pump triggers.** Besides `Mail`, every status change, and "on start", the pump also runs **after a stop sends mail back** (Claude `cancelled`, Codex `interrupted`). "On start" means **when a restored session settles** (`finish`): until then its driver takes no mail, and its status may not change when it settles. A Codex `failed` turn's mail goes back to the queue but waits for the next status change, so a model that keeps failing doesn't loop.
3. **Which lines carry a uuid.** Only mail lines carry `uuid` (with `session_id: ""` and `parent_tool_use_id: null`, as the spike sent). Prompts typed in the chat don't, so they produce no `command_lifecycle` lines.
4. **Codex mid-turn.** `turn/steer` is used when pocketd has heard the turn start. In the short time before it hears `turn/started`, `turn/start` is used, and the app-server steers it into the running turn (the existing `Prompt` path). The ref is the turn id in the reply either way.

## Open questions

1. **Upgrade duplicates in-flight mail.** An upgrade execs a new pocketd, whose `Open` puts shown mail back in the queue. The handoff keeps the claude running, so it may already hold that uuid's line, and `finish` then delivers it again. Accept the duplicate, or skip the reset when pocketd starts from a handoff (`serve(sock, handed)` with `handed != ""`)?
2. **Claude's lifecycle for a failed turn.** The spike checked a stop (`cancelled`) but not an API error. If a failed turn reports `completed`, its mail is acked though the turn failed. If it reports `cancelled`, the settle-and-pump redelivers right away and a persistent error loops. This needs a probe against the real CLI before plan 4. A fix could cap tries per mail (a `tries` column).
3. **Codex `turn/start` reply when it steers.** We assume its `turn.id` is the running turn's id, as fakecodex does. This is unverified against the real app-server; if it's wrong, mail steered in that short window never acks until an exit or restart.
4. **Housekeeping.** Acked rows are never deleted. Mail for an agent that never comes back stays queued for good. Plan 4 may delete a team's mail when the team is merged or stopped.
5. **A stop redelivers at once.** When the user stops a Claude lead, claude cancels queued mail, and pocketd sends it again straight away, so the lead starts a new turn right after the stop. Is that the behavior plan 4 wants, or should mail hold after a user stop until the next prompt?
6. **e2e senders.** The e2e tests queue mail by opening `mail.db` from the test process, because nothing on the wire sends mail yet. Plan 4's e2e covers the real senders (handbacks, notes) but leaves these tests as they are; moving them is still open.
