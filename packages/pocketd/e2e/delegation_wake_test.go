//go:build unix

package e2e

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/orch"
)

func storeOf(h *Harness) *orch.Store {
	h.t.Helper()
	store, err := orch.Open(h.Home)
	if err != nil {
		h.t.Fatal(err)
	}
	h.t.Cleanup(func() { store.Close() })
	return store
}

// delegateCall is the script line that has an ACP parent delegate task to a Claude child.
func delegateCall(task string) string {
	args, _ := json.Marshal(map[string]any{"task": task, "title": task, "target": map[string]string{"agent": "claude"}})
	return "mcp delegate_task " + string(args)
}

// gatedParent starts an ACP chat that runs script, then stays in its turn until release. A parent
// that is busy and can't steer holds back what is sent to it, which is how a wake or a notice queues.
func gatedParent(t *testing.T, lines ...string) (h *Harness, phone *Phone, parent string, release func()) {
	t.Helper()
	h, phone, _, parent = chatACP(t, "grok", nil)
	phone.Prompt(parent, strings.Join(append(lines, "await gate"), "\n"))
	return h, phone, parent, func() {
		if err := os.WriteFile(filepath.Join(h.Home, ".acp-fake", "gate"), nil, 0o600); err != nil {
			t.Fatal(err)
		}
	}
}

func allowAsks(phone *Phone, parent string, n int) {
	phone.WaitFor("the children's asks", func(m Message) bool {
		if asked(m) && m.Request.AgentID != parent {
			resolve(phone, m, map[string]any{"decision": "allow"})
			n--
		}
		return n == 0
	})
}

func wakesIn(timeline []string) int {
	n := 0
	for _, item := range timeline {
		if strings.HasPrefix(item, "user Delegated task") {
			n++
		}
	}
	return n
}

func TestAWaitThatTimesOutSaysTheTaskIsStillWorkingAndTheParentIsWokenWhenItSettles(t *testing.T) {
	h, phone, _, parent := chatClaude(t)
	phone.Prompt(parent, `mcp delegate_task {"task":"run ls","title":"Ls","mode":"wait","timeoutMs":300}`)

	var ask Message
	phone.WaitAll("the wait that timed out, and the child's ask",
		saysAbout(parent, `"waitTimedOut":true`, `"workState":"working"`),
		func(m Message) bool {
			if asked(m) && m.Request.AgentID != parent {
				ask = m
			}
			return ask.Raw != ""
		},
	)
	resolve(phone, ask, map[string]any{"decision": "allow"})

	task := settledAndWoken(h, phone, parent, "completed")
	if task.Summary != "echo: ran ls" {
		t.Fatalf("task = %+v", task)
	}
}

func TestAThreadWaitThatTimesOutSaysTheChildIsStillWorkingAndTheParentIsWokenWhenItSettles(t *testing.T) {
	h, phone, _, parent := chatClaude(t)
	phone.Prompt(parent, `mcp delegate_task {"task":"await release","title":"Held"}`)
	child := phone.WaitFor("the child working", func(m Message) bool {
		return m.Type == "agent.update" && m.Agent.Parent == parent && m.Agent.Status == "working"
	}).Agent

	phone.Prompt(parent, `mcp anywhere_thread_wait {"threadId":"`+child.ID+`","timeoutMs":300}`)
	phone.WaitFor("the wait that timed out", saysAbout(parent, `"waitTimedOut":true`, `"status":"working"`))
	if err := os.WriteFile(filepath.Join(h.Home, "release"), nil, 0o600); err != nil {
		t.Fatal(err)
	}

	task := settledAndWoken(h, phone, parent, "completed")
	if task.Summary != "echo: released" {
		t.Fatalf("task = %+v", task)
	}
}

func TestSiblingsThatSettleWhileTheParentIsBusyShareOneWake(t *testing.T) {
	h, phone, parent, release := gatedParent(t, delegateCall("run ls"), delegateCall("run pwd"))
	allowAsks(phone, parent, 2)
	store := storeOf(h)

	var wake orch.QueueEntry
	h.eventually("one queued wake that holds both tasks", func() bool {
		entries, _ := store.Queue(parent, true)
		for _, e := range entries {
			if e.Origin == orch.OriginWake {
				wake = e
			}
		}
		return len(wake.Refs) == 2
	})
	release()

	h.eventually("the wake sent", func() bool { return wakesIn(timelineOf(phone, parent)) > 0 })
	got := timelineOf(phone, parent)
	want := "user " + orch.WakeText(wake.Refs)
	if wakesIn(got) != 1 || !containsItem(got, want) {
		t.Fatalf("timeline %q, want one wake: %q", got, want)
	}
}

func containsItem(items []string, want string) bool {
	for _, it := range items {
		if it == want {
			return true
		}
	}
	return false
}

func TestAUserStopOnTheParentDisposesItsQueuedWakeSoTheChildSettlesAndTheParentIsNotStarted(t *testing.T) {
	h, phone, parent, _ := gatedParent(t, delegateCall("run ls"))
	allowAsks(phone, parent, 1)
	store := storeOf(h)
	h.eventually("the wake queued", func() bool {
		entries, _ := store.Queue(parent, true)
		return len(entries) == 1 && entries[0].Origin == orch.OriginWake
	})

	phone.Send(map[string]any{"type": "agent.interrupt", "id": "stop-parent", "agentId": parent})

	h.eventually("the wake cancelled and the task's delivery disposed", func() bool {
		entries, _ := store.Queue(parent, false)
		tasks, _ := store.Delegations(`parent_id = ?`, parent)
		return len(entries) == 1 && entries[0].State == orch.EntryCancelled && len(tasks) == 1 && tasks[0].Status == orch.TaskCompleted && tasks[0].Delivery == orch.DeliverDisposed
	})
	phone.Prompt(parent, "after the stop")
	phone.WaitFor("the next turn", streamOf(parent, "assistant", "echo: after the stop"))
	if got := timelineOf(phone, parent); wakesIn(got) != 0 {
		t.Fatalf("the parent was woken after the stop: %q", got)
	}
}

func TestAnAnswerThatComesBeforeAChildsQuestionNoticeIsSentCancelsTheNotice(t *testing.T) {
	h, phone, parent, release := gatedParent(t, delegateCall("ask Ship it?"))
	var ask Message
	phone.WaitFor("the child's question", func(m Message) bool {
		if asked(m) && m.Request.AgentID != parent && m.Request.ToolName == "AskUserQuestion" {
			ask = m
		}
		return ask.Raw != ""
	})
	store := storeOf(h)
	h.eventually("the notice queued", func() bool {
		entries, _ := store.Queue(parent, true)
		return len(entries) == 1 && entries[0].Origin == orch.OriginQuestion
	})

	resolve(phone, ask, map[string]any{"decision": "allow", "answers": map[string]string{"Ship it?": "Yes"}})

	h.eventually("the notice cancelled", func() bool {
		entries, _ := store.Queue(parent, false)
		return len(entries) >= 1 && entries[0].Origin == orch.OriginQuestion && entries[0].State == orch.EntryCancelled
	})
	release()
	settledAndWoken(h, phone, parent, "completed")
	for _, item := range timelineOf(phone, parent) {
		if strings.Contains(item, "blocked on a question") {
			t.Fatalf("the parent was told of a question that was already answered: %q", item)
		}
	}
}

func TestAChildPromptedAfterItSettledWorksItsTaskAgainAndWakesTheParentWithTheNewResult(t *testing.T) {
	h, phone, _, parent := chatClaude(t)
	phone.Prompt(parent, `mcp delegate_task {"task":"first pass","title":"Pass"}`)
	first := settledAndWoken(h, phone, parent, "completed")
	if first.Summary != "echo: first pass" {
		t.Fatalf("task = %+v", first)
	}

	phone.Prompt(first.ChildID, "second pass")
	phone.WaitFor("the task working again", func(m Message) bool {
		return m.Type == "delegation.update" && m.Delegation.TaskID == first.TaskID && m.Delegation.Status == "working"
	})
	h.eventually("the task settled with the new reply", func() bool {
		ts := delegationsOf(phone, parent)
		return ts[0].Status == "completed" && ts[0].Summary == "echo: second pass"
	})
	h.eventually("the parent woken a second time", func() bool { return wakesIn(timelineOf(phone, parent)) == 2 })
}
