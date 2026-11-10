//go:build unix

package e2e

import (
	"bytes"
	"encoding/json"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"pocketd/internal/proto"
)

// anywhere runs the `anywhere` command from h.Repo, as a process outside every Terminal.
func (h *Harness) anywhere(args ...string) (stdout, stderr string, code int) {
	h.t.Helper()
	cmd := exec.Command(filepath.Join(binDir, "anywhere"), args...)
	cmd.Env, cmd.Dir = h.Env, h.Repo
	var out, errOut bytes.Buffer
	cmd.Stdout, cmd.Stderr = &out, &errOut
	if err := cmd.Run(); err != nil && !errors.As(err, new(*exec.ExitError)) {
		h.t.Fatal(err)
	}
	return strings.TrimSpace(out.String()), strings.TrimSpace(errOut.String()), cmd.ProcessState.ExitCode()
}

func TestAnywhereListsTheProjectsAsJSON(t *testing.T) {
	h := Start(t, launchReady(""))
	out, errOut, code := h.anywhere("project", "list", "--json")
	var list []proto.Project
	if err := json.Unmarshal([]byte(out), &list); code != 0 || err != nil || len(list) != 1 || list[0].Path != h.Repo {
		t.Fatalf("exited %d, %s, %s", code, out, errOut)
	}
}

func TestAnywhereReadsTheLastReply(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner()
	create(o, "r1", inRepo(h, map[string]any{"prompt": "hello"}))
	id := reply(o, "agent.created", "error").AgentID
	h.eventually("the reply", func() bool {
		out, _, code := h.anywhere("session", "read", id)
		return code == 0 && out == "echo: hello"
	})
}

func TestAnywhereWithoutArgumentsPrintsUsage(t *testing.T) {
	h := Start(t, launchReady(""))
	if _, errOut, code := h.anywhere(); code != 2 || !strings.HasPrefix(errOut, "usage: anywhere") {
		t.Fatalf("exited %d: %s", code, errOut)
	}
}

// session creates a session on the fake claude with prompt, which may be empty, and returns its id.
func (h *Harness) session(prompt string) string {
	h.t.Helper()
	args := []string{"session", "create", "--access", "ask"}
	if prompt != "" {
		args = append(args, "--prompt", prompt)
	}
	out, errOut, code := h.anywhere(args...)
	if code != 0 || out == "" {
		h.t.Fatalf("create exited %d: %s %s", code, out, errOut)
	}
	return out
}

func TestAnywhereListsASessionItCreated(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("hello")
	out, _, code := h.anywhere("session", "list")
	if code != 0 || !strings.Contains(out, id) {
		t.Fatalf("exited %d: %s", code, out)
	}
}

func TestAnywhereSendsASessionAPrompt(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("")
	if out, errOut, code := h.anywhere("session", "prompt", id, "again"); code != 0 {
		t.Fatalf("prompt exited %d: %s %s", code, out, errOut)
	}
	h.eventually("the reply", func() bool {
		out, _, _ := h.anywhere("session", "read", id)
		return out == "echo: again"
	})
}

func TestAnywhereClosesASession(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("")
	if out, errOut, code := h.anywhere("session", "close", id); code != 0 {
		t.Fatalf("close exited %d: %s %s", code, out, errOut)
	}
	h.eventually("the session to go", func() bool {
		out, _, _ := h.anywhere("session", "list")
		return !strings.Contains(out, id)
	})
}

func TestAnywhereOpensANewWorktreeWhenAsked(t *testing.T) {
	h := Start(t, launchReady(""))
	out, errOut, code := h.anywhere("session", "create", "--access", "ask", "--new-worktree", "calm-otter", "--prompt", "hello")
	if code != 0 {
		t.Fatalf("exited %d: %s %s", code, out, errOut)
	}
	list, _, _ := h.anywhere("session", "list", "--json")
	var agents []proto.AgentSummary
	if err := json.Unmarshal([]byte(list), &agents); err != nil || len(agents) != 1 || !strings.HasSuffix(agents[0].Cwd, "/calm-otter") {
		t.Fatalf("got %s", list)
	}
}

// listed is what `anywhere session list --json` says of session id, if it lists it.
func (h *Harness) listed(id string) (proto.AgentSummary, bool) {
	h.t.Helper()
	out, _, _ := h.anywhere("session", "list", "--json")
	var agents []proto.AgentSummary
	json.Unmarshal([]byte(out), &agents)
	for _, a := range agents {
		if a.ID == id {
			return a, true
		}
	}
	return proto.AgentSummary{}, false
}

func TestAnywhereNamesAnUnnamedNewWorktreeFromThePrompt(t *testing.T) {
	h := Start(t, launchReady(""))
	id, errOut, code := h.anywhere("session", "create", "--access", "ask", "--new-worktree", "--prompt", "hello")
	if code != 0 {
		t.Fatalf("exited %d: %s %s", code, id, errOut)
	}
	h.eventually("the session and its branch named from the prompt", func() bool {
		a, ok := h.listed(id)
		return ok && a.Title == "Say hello" && a.Branch == "say-hello" && strings.Contains(a.Cwd, "/cli-")
	})
}

func TestAnywhereStartsAChat(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("hello")
	if a, ok := h.listed(id); !ok || !a.ChatOnly {
		t.Fatalf("got %+v", a)
	}
}

func TestAnywhereAddsATicketToToDo(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapBoard)
	id, errOut, code := h.anywhere("ticket", "create", "Fix the login\n\nIt redirects twice")
	if code != 0 {
		t.Fatalf("exited %d: %s %s", code, id, errOut)
	}
	got := o.WaitTicket("the new ticket", func(x proto.Ticket) bool { return x.Title == "Fix the login" })
	if got.ID != id || got.State != proto.TicketTodo || got.Folder != h.Repo || got.Prompt != "Fix the login\n\nIt redirects twice" {
		t.Fatalf("got %+v", got)
	}
}

func TestAnywhereStartsATicketWhenAsked(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapBoard)
	if out, errOut, code := h.anywhere("ticket", "create", "--start", "--title", "Review", "review the diff"); code != 0 {
		t.Fatalf("exited %d: %s %s", code, out, errOut)
	}
	o.WaitTicket("the ticket started", func(x proto.Ticket) bool { return x.Title == "Review" && x.State != proto.TicketTodo })
}

// addTicket creates a ticket and returns its id.
func (h *Harness) addTicket(args ...string) string {
	h.t.Helper()
	id, errOut, code := h.anywhere(append([]string{"ticket", "create"}, args...)...)
	if code != 0 || id == "" {
		h.t.Fatalf("exited %d: %q %s", code, id, errOut)
	}
	return id
}

func TestAnywhereListsAndReadsTickets(t *testing.T) {
	h := Start(t, launchReady(""))
	if out, errOut, code := h.anywhere("ticket", "list", "--json"); code != 0 || out != "[]" {
		t.Fatalf("exited %d: %q %s", code, out, errOut)
	}
	id := h.addTicket("--title", "Review", "--base", "main", "review the diff")
	h.addTicket("something else")
	list, errOut, code := h.anywhere("ticket", "list")
	if code != 0 || !strings.Contains(list, id) || !strings.Contains(list, "Review") || !strings.Contains(list, "something else") {
		t.Fatalf("exited %d: %q %s", code, list, errOut)
	}
	out, errOut, code := h.anywhere("ticket", "read", "--json", id)
	var got proto.Ticket
	if code != 0 || json.Unmarshal([]byte(out), &got) != nil {
		t.Fatalf("exited %d: %q %s", code, out, errOut)
	}
	if got.ID != id || got.Title != "Review" || got.Prompt != "review the diff" || got.Base != "main" || got.State != proto.TicketTodo {
		t.Fatalf("got %+v", got)
	}
	if out, _, code := h.anywhere("ticket", "read", id); code != 0 || !strings.Contains(out, "review the diff") {
		t.Fatalf("exited %d: %q", code, out)
	}
}

func TestAnywhereSaysWhenThereIsNoSuchTicket(t *testing.T) {
	h := Start(t, launchReady(""))
	for _, args := range [][]string{{"read", "nope"}, {"update", "--title", "x", "nope"}, {"delete", "nope"}} {
		if _, errOut, code := h.anywhere(append([]string{"ticket"}, args...)...); code != 1 || errOut == "" {
			t.Fatalf("%v exited %d: %s", args, code, errOut)
		}
	}
}

func TestAnywhereEditsATicketAndKeepsWhatItDidNotChange(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapBoard)
	id := h.addTicket("--title", "Old", "--base", "main", "first prompt")
	if out, errOut, code := h.anywhere("ticket", "update", "--title", "New", id); code != 0 {
		t.Fatalf("exited %d: %s %s", code, out, errOut)
	}
	got := o.WaitTicket("the new title", func(x proto.Ticket) bool { return x.ID == id && x.Title == "New" })
	if got.Prompt != "first prompt" || got.Base != "main" || got.State != proto.TicketTodo {
		t.Fatalf("got %+v", got)
	}
}

func TestAnywhereDeletesATicket(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.addTicket("hello")
	if out, errOut, code := h.anywhere("ticket", "delete", id); code != 0 {
		t.Fatalf("exited %d: %s %s", code, out, errOut)
	}
	if list, _, _ := h.anywhere("ticket", "list"); strings.Contains(list, id) {
		t.Fatalf("still listed: %q", list)
	}
}

func TestAnywhereLeavesAStartedTicketAlone(t *testing.T) {
	h := Start(t, launchReady(""))
	o := h.Owner(proto.CapBoard)
	id := h.addTicket("--start", "run ls")
	o.WaitTicket("the ticket live", func(x proto.Ticket) bool { return x.ID == id && x.State == proto.TicketLive })
	if _, errOut, code := h.anywhere("ticket", "update", "--title", "New", id); code != 1 || !strings.Contains(errOut, "already started") {
		t.Fatalf("update exited %d: %s", code, errOut)
	}
	if _, errOut, code := h.anywhere("ticket", "delete", id); code != 1 || errOut == "" {
		t.Fatalf("delete exited %d: %s", code, errOut)
	}
}

func TestAProcessInsideATerminalIsToldItCanOnlyRead(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.Spawn("/bin/sh", "-c", filepath.Join(binDir, "anywhere")+" session close nope; sleep 30")
	h.WaitScreen(id, "can only read")
}

func TestAProcessInsideATerminalCannotAddATicket(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.Spawn("/bin/sh", "-c", "cd "+h.Repo+" && "+filepath.Join(binDir, "anywhere")+" ticket create hello; sleep 30")
	h.WaitScreen(id, "can only read")
}

func TestAProcessInsideATerminalIsNotShownTheBoard(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.Spawn("/bin/sh", "-c", filepath.Join(binDir, "anywhere")+" ticket list; sleep 30")
	h.WaitScreen(id, "can only read")
}

func TestAnywhereCreatesASessionThenWaitsForItAndReadsTheReply(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("hello")
	if out, errOut, code := h.anywhere("session", "wait", id); code != 0 || out != "done" {
		t.Fatalf("wait exited %d: %s %s", code, out, errOut)
	}
	if out, errOut, code := h.anywhere("session", "read", id); code != 0 || out != "echo: hello" {
		t.Fatalf("read exited %d: %q %s", code, out, errOut)
	}
}

func TestAWaitRightAfterAPromptWaitsForThatTurn(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("hello")
	h.anywhere("session", "wait", id)
	if out, errOut, code := h.anywhere("session", "prompt", id, "again"); code != 0 {
		t.Fatalf("prompt exited %d: %s %s", code, out, errOut)
	}
	if out, _, code := h.anywhere("session", "wait", id); code != 0 || out != "done" {
		t.Fatalf("wait exited %d: %s", code, out)
	}
	if out, _, _ := h.anywhere("session", "read", id); out != "echo: again" {
		t.Fatalf("read = %q", out)
	}
}

func TestWaitStopsWithItsOwnCodeWhenTheAgentAsks(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("run ls")
	if out, errOut, code := h.anywhere("session", "wait", id); code != 4 || out != "needsYou" {
		t.Fatalf("wait exited %d: %s %s", code, out, errOut)
	}
}

func TestWaitTimesOutOnATurnThatDoesntEnd(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("hang")
	if out, errOut, code := h.anywhere("session", "wait", "--timeout", "200ms", id); code != 5 || out != "timeout" {
		t.Fatalf("wait exited %d: %s %s", code, out, errOut)
	}
}

func TestWaitSaysSoWhenTheSessionCloses(t *testing.T) {
	h := Start(t, launchReady(""))
	id := h.session("hang")
	done := make(chan string)
	go func() {
		out, _, code := h.anywhere("session", "wait", id)
		done <- out + string(rune('0'+code))
	}()
	time.Sleep(300 * time.Millisecond)
	if out, errOut, code := h.anywhere("session", "close", id); code != 0 {
		t.Fatalf("close exited %d: %s %s", code, out, errOut)
	}
	select {
	case got := <-done:
		if got != "closed6" {
			t.Fatalf("wait = %s", got)
		}
	case <-time.After(10 * time.Second):
		t.Fatal("wait did not end")
	}
}

func TestPocketdBringsAnInstalledSkillUpToDateWhenItStarts(t *testing.T) {
	h := Start(t)
	if out, code := h.Pocketd("skill", "install"); code != 0 {
		t.Fatalf("install exited %d: %s", code, out)
	}
	skill := filepath.Join(h.Home, ".claude", "skills", "anywhere", "SKILL.md")
	if err := os.WriteFile(skill, []byte("old"), 0o644); err != nil {
		t.Fatal(err)
	}
	h.Restart()
	if got, _ := os.ReadFile(skill); !strings.Contains(string(got), "name: anywhere") {
		t.Fatalf("got %s", got)
	}
}

func TestAnEmptyWorktreeNameIsAUsageError(t *testing.T) {
	h := Start(t, launchReady(""))
	if out, errOut, code := h.anywhere("session", "create", "--new-worktree="); code != 2 {
		t.Fatalf("exited %d: %s %s", code, out, errOut)
	}
}
