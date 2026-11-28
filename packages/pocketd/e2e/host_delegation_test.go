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
	"syscall"
	"testing"
	"time"

	"pocketd/internal/holder"
	"pocketd/internal/ops"
	"pocketd/internal/orch"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
)

func linkedHost(t *testing.T, hostOpts ...func(*Harness)) (mac, host *Harness) {
	t.Helper()
	host = Start(t, append([]func(*Harness){launchReady(""), asHost}, hostOpts...)...)
	mac = Start(t, fakeSSH)
	reach(t, mac, host)
	owner := mac.Owner(proto.CapHosts)
	addDevbox(owner)
	waitLink(owner, "devbox", proto.LinkOnline, "")
	owner.ws.CloseNow()
	return mac, host
}

func phoneChat(t *testing.T, mac *Harness) (phone *Phone, chat string) {
	t.Helper()
	return chatOn(t, mac, mac.Paired(proto.CapLaunch, proto.CapOrchestrator), map[string]any{"chat": true})
}

// chatOn is a Claude chat in mac's repo that phone started with fields and drives.
func chatOn(t *testing.T, mac *Harness, phone *Phone, fields map[string]any) (*Phone, string) {
	t.Helper()
	create(phone, "r1", inRepo(mac, fields))
	done := reply(phone, "agent.created", "error")
	if done.Type != "agent.created" {
		t.Fatalf("got %s", done.Raw)
	}
	mac.WaitScreen(done.TerminalID, "fake claude ready")
	return phone, done.AgentID
}

func hostAgent(t *testing.T, host *Harness, tagged string) (proto.AgentSummary, bool) {
	t.Helper()
	_, theirs, _ := strings.Cut(tagged, "/")
	var list proto.AgentList
	json.Unmarshal([]byte(host.Owner().WaitFor("the host's agents", func(m Message) bool { return m.Type == "agent.list" }).Raw), &list)
	i := slices.IndexFunc(list.Agents, func(a proto.AgentSummary) bool { return a.ID == theirs })
	if i < 0 {
		return proto.AgentSummary{}, false
	}
	return list.Agents[i], true
}

func TestADelegationToALinkedHostCompletesWithTheDesktopDisconnected(t *testing.T) {
	mac, host := linkedHost(t)
	mac.Restart()
	// Only to see the link back up from the saved hosts: no owner is connected once the parent delegates.
	watch := mac.Owner(proto.CapHosts)
	waitLink(watch, "devbox", proto.LinkOnline, "")
	watch.ws.CloseNow()
	phone, parent := phoneChat(t, mac)

	phone.Prompt(parent, `mcp delegate_task {"task":"write the tests","target":{"environmentId":"devbox"},"isolation":"shared"}`)

	var child string
	phone.WaitAll("the task completed on devbox and the parent's wake",
		func(m Message) bool {
			d := m.Delegation
			if m.Type == "delegation.update" && m.AgentID == parent && d.Status == "completed" && strings.HasPrefix(d.ChildID, "devbox/") &&
				d.EnvironmentID == "devbox" && d.Summary == "echo: write the tests" {
				child = d.ChildID
			}
			return child != ""
		},
		func(m Message) bool {
			return m.Type == "agent.stream" && m.AgentID == parent && m.Item.Kind == "user" && strings.Contains(m.Item.Text, "reached a terminal state")
		},
	)
	if _, ok := hostAgent(t, host, child); !ok {
		t.Fatalf("the host doesn't list %s", child)
	}
}

func withProject(name string) func(*Harness) {
	return func(h *Harness) {
		user := h.UserHome()
		dir := filepath.Join(user, name)
		os.MkdirAll(dir, 0o755)
		dir, _ = filepath.EvalSymlinks(dir)
		for _, args := range [][]string{{"init", "-q", "-b", "main"}, {"commit", "-q", "--allow-empty", "-m", "init"}} {
			cmd := exec.Command("git", append([]string{"-C", dir, "-c", "user.name=e2e", "-c", "user.email=e2e@x", "-c", "commit.gpgsign=false"}, args...)...)
			if out, err := cmd.CombinedOutput(); err != nil {
				h.t.Fatalf("git %v: %s", args, out)
			}
		}
		reg, _ := json.Marshal(registry.File{Projects: []string{h.Repo, dir}, Repos: map[string]registry.Repo{h.Repo: {Name: "wt"}, dir: {Name: name}}, Worktree: registry.Worktree{Root: user}})
		if err := os.WriteFile(filepath.Join(h.Home, "desktop.json"), reg, 0o600); err != nil {
			h.t.Fatal(err)
		}
	}
}

func hostTask(h *Harness, p *Phone, parent string, n int, status string) proto.Delegation {
	h.t.Helper()
	var task proto.Delegation
	h.eventually(fmt.Sprintf("task %d %s", n, status), func() bool {
		ts := delegationsOf(p, parent)
		if len(ts) < n {
			return false
		}
		task = ts[n-1]
		return task.Status == status && task.ChildID != ""
	})
	return task
}

func TestATaskOnAHostUsesTheProjectNamedLikeTheParentsOrTheOneGiven(t *testing.T) {
	mac, host := linkedHost(t, withProject("other"))
	phone, parent := phoneChat(t, mac)

	phone.Prompt(parent, `mcp delegate_task {"task":"in wt","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	named := hostTask(mac, phone, parent, 1, "completed")
	phone.Prompt(parent, `mcp delegate_task {"task":"in other","target":{"environmentId":"devbox","projectId":"other"},"isolation":"shared"}`)
	given := hostTask(mac, phone, parent, 2, "completed")

	other, _ := filepath.EvalSymlinks(filepath.Join(host.UserHome(), "other"))
	for child, want := range map[string]string{named.ChildID: host.Repo, given.ChildID: other} {
		if a, ok := hostAgent(t, host, child); !ok || a.Cwd != want {
			t.Errorf("%s runs in %q (listed %v), want %s", child, a.Cwd, ok, want)
		}
	}
}

func TestCancellingAHostTaskInterruptsTheChildThere(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := phoneChat(t, mac)
	phone.Prompt(parent, `mcp delegate_task {"task":"hang","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	task := hostTask(mac, phone, parent, 1, "working")
	child, ok := hostAgent(t, host, task.ChildID)
	if !ok {
		t.Fatalf("the host doesn't list %s", task.ChildID)
	}

	phone.Prompt(parent, `mcp task_cancel {"taskId":"`+task.TaskID+`"}`)

	hostTask(mac, phone, parent, 1, "cancelled")
	host.WaitScreen(child.TerminalID, "interrupt")
}

func TestARestartMidHostDelegationResumesAndSettles(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := phoneChat(t, mac)
	phone.Prompt(parent, `mcp delegate_task {"task":"await released","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	hostTask(mac, phone, parent, 1, "working")

	mac.Restart()
	if err := os.WriteFile(filepath.Join(host.UserHome(), "released"), nil, 0o600); err != nil {
		t.Fatal(err)
	}

	task := hostTask(mac, mac.Owner(), parent, 1, "completed")
	if task.Summary != "echo: released" {
		t.Fatalf("task = %+v", task)
	}
}

func TestADelegationToAHostThatIsNotOnlineIsRefusedWithItsLink(t *testing.T) {
	host := Start(t, launchReady(""), asHost)
	mac := Start(t, fakeSSH)
	reach(t, mac, host)
	if err := os.WriteFile(filepath.Join(host.UserHome(), ".fakessh-no-keys"), nil, 0o600); err != nil {
		t.Fatal(err)
	}
	owner := mac.Owner(proto.CapHosts)
	addDevbox(owner)
	waitLink(owner, "devbox", proto.LinkFailed, "auth")
	owner.ws.CloseNow()
	phone, parent := phoneChat(t, mac)

	phone.Prompt(parent, `mcp delegate_task {"task":"write the tests","target":{"environmentId":"devbox"}}`)

	phone.WaitFor("the refusal", saysAbout(parent, "unavailable", "devbox isn't online (failed)", "didn't accept your SSH keys"))
	if ts := delegationsOf(phone, parent); len(ts) != 0 {
		t.Fatalf("tasks %+v", ts)
	}
}

func TestAHostChildsQuestionReachesTheParentAndItsAnswerGoesBack(t *testing.T) {
	mac, _ := linkedHost(t)
	phone, parent := phoneChat(t, mac)
	phone.Prompt(parent, `mcp delegate_task {"task":"ask Ship it?","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	task := hostTask(mac, phone, parent, 1, "working")
	phone.WaitFor("the parent told of the question", func(m Message) bool {
		return m.Type == "agent.stream" && m.AgentID == parent && m.Item.Kind == "user" && strings.Contains(m.Item.Text, "is blocked on a question")
	})
	open, err := storeOf(mac).Requests(task.ChildID, "open")
	if err != nil || len(open) != 1 || open[0].Kind != "question" || !strings.HasPrefix(open[0].ID, "devbox/") {
		t.Fatalf("open requests %+v %v", open, err)
	}

	phone.Prompt(parent, `mcp anywhere_pending_request {"taskId":"`+task.TaskID+`","requestId":"`+open[0].ID+`","answers":{"Ship it?":"Yes"}}`)

	done := settledAndWoken(mac, phone, parent, "completed")
	if done.Summary != `echo: answers {"Ship it?":"Yes"}` {
		t.Fatalf("task = %+v", done)
	}
}

func TestAHostPocketdKilledMidTaskCancelsItSayingTheHostWentAway(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := phoneChat(t, mac)
	phone.Prompt(parent, `mcp delegate_task {"task":"hang","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	hostTask(mac, phone, parent, 1, "working")

	// The host machine goes down: its pocketd and the child with it.
	host.Stop(syscall.SIGKILL)
	holder.EndAll(holder.Dir(host.Home), 0)
	host.up()

	if task := settledAndWoken(mac, phone, parent, "cancelled"); !strings.Contains(task.Error, "devbox") {
		t.Fatalf("task = %+v", task)
	}
}

func TestAHostChildsPermissionAskIsMirroredUntilTheUserAnswersItThere(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := phoneChat(t, mac)
	phone.Prompt(parent, `mcp delegate_task {"task":"run ls","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	task := hostTask(mac, phone, parent, 1, "working")
	store := storeOf(mac)
	var open []orch.Request
	mac.eventually("the child's ask mirrored", func() bool {
		open, _ = store.Requests(task.ChildID, "open")
		s, _ := store.Session(task.ChildID)
		return len(open) == 1 && s != nil && s.Status == "needsYou"
	})
	if open[0].Kind != "permission" || !strings.HasPrefix(open[0].ID, "devbox/") {
		t.Fatalf("open requests %+v", open)
	}

	owner := host.Owner()
	resolve(owner, owner.WaitFor("the ask on the host", asked), map[string]any{"decision": "allow"})

	if done := hostTask(mac, phone, parent, 1, "completed"); done.Summary != "echo: ran ls" {
		t.Fatalf("task = %+v", done)
	}
	if open, err := store.Requests(task.ChildID, "open"); err != nil || len(open) != 0 {
		t.Fatalf("still open %+v %v", open, err)
	}
}

func TestAHostChildIdleWithoutAResultSettlesCompletedWithItsLastReply(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := phoneChat(t, mac)

	phone.Prompt(parent, `mcp delegate_task {"task":"quiet left no result","target":{"environmentId":"devbox"},"isolation":"shared"}`)

	task := settledAndWoken(mac, phone, parent, "completed")
	if task.Summary != "echo: left no result" {
		t.Fatalf("task = %+v", task)
	}
	_, theirs, _ := strings.Cut(task.ChildID, "/")
	items := timelineOf(host.Owner(), theirs)
	if !slices.Contains(items, "assistant echo: left no result") || slices.ContainsFunc(items, func(s string) bool { return strings.HasPrefix(s, "result") }) {
		t.Fatalf("the host's timeline %q", items)
	}
}

// dropLink takes devbox's link down, as a host that stops taking this Mac's key, until back brings it up again.
func dropLink(t *testing.T, mac, host *Harness) (back func()) {
	t.Helper()
	owner := mac.Owner(proto.CapHosts)
	keys := filepath.Join(host.UserHome(), ".fakessh-no-keys")
	if err := os.WriteFile(keys, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	owner.Send(map[string]any{"type": "host.retry", "id": "drop", "hostId": "devbox"})
	waitLink(owner, "devbox", proto.LinkFailed, "auth")
	return func() {
		t.Helper()
		os.Remove(keys)
		owner.Send(map[string]any{"type": "host.retry", "id": "back", "hostId": "devbox"})
		waitLink(owner, "devbox", proto.LinkOnline, "")
	}
}

func TestAHostChildsQuestionStillOpenAfterAReconnectReachesTheParentOnce(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := phoneChat(t, mac)
	phone.Prompt(parent, `mcp delegate_task {"task":"ask Ship it?","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	task := hostTask(mac, phone, parent, 1, "working")
	var open []orch.Request
	mac.eventually("the question mirrored", func() bool {
		open, _ = storeOf(mac).Requests(task.ChildID, "open")
		return len(open) == 1
	})

	dropLink(t, mac, host)()
	// A task started past the reconnect is mirrored after all the host sent on reconnecting.
	phone.Prompt(parent, `mcp delegate_task {"task":"quiet after","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	hostTask(mac, phone, parent, 2, "completed")
	phone.Prompt(parent, `mcp anywhere_pending_request {"taskId":"`+task.TaskID+`","requestId":"`+open[0].ID+`","answers":{"Ship it?":"Yes"}}`)

	hostTask(mac, phone, parent, 1, "completed")
	mac.eventually("the parent's wake", func() bool {
		return slices.ContainsFunc(timelineOf(phone, parent), func(s string) bool {
			return strings.HasPrefix(s, "user ") && strings.Contains(s, task.TaskID) && strings.Contains(s, "reached a terminal state")
		})
	})
	notices := slices.DeleteFunc(timelineOf(phone, parent), func(s string) bool {
		return !strings.HasPrefix(s, "user Delegated task "+task.TaskID) || !strings.Contains(s, "is blocked on a question")
	})
	if len(notices) != 1 {
		t.Fatalf("the parent was told %d times: %q", len(notices), notices)
	}
}

func TestAHostChildsQuestionAnsweredThereWhileTheLinkWasDownClosesHereOnReconnect(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := phoneChat(t, mac)
	phone.Prompt(parent, `mcp delegate_task {"task":"ask Ship it?","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	task := hostTask(mac, phone, parent, 1, "working")
	store := storeOf(mac)
	mac.eventually("the question mirrored", func() bool {
		open, _ := store.Requests(task.ChildID, "open")
		return len(open) == 1
	})
	owner := host.Owner()
	question := owner.WaitFor("the question on the host", asked)

	back := dropLink(t, mac, host)
	resolve(owner, question, map[string]any{"decision": "allow", "answers": map[string]string{"Ship it?": "No"}})
	owner.WaitFor("the answer there", func(m Message) bool { return m.Type == "permission.resolved" })
	back()

	hostTask(mac, phone, parent, 1, "completed")
	mac.eventually("the question closed here", func() bool {
		open, _ := store.Requests(task.ChildID, "open")
		return len(open) == 0
	})
}

func TestATaskOnAHostLeftDownKeepsWaitingAndCancellingItSettlesItHereAtOnce(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := phoneChat(t, mac)
	phone.Prompt(parent, `mcp delegate_task {"task":"hang","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	task := hostTask(mac, phone, parent, 1, "working")

	dropLink(t, mac, host)
	phone.Prompt(parent, `mcp task_status {"taskId":"`+task.TaskID+`"}`)
	phone.WaitFor("the task still running", func(m Message) bool {
		return m.Type == "agent.stream" && m.AgentID == parent && m.Item.Kind == "assistant" && strings.Contains(m.Item.Text, `"status":"running"`)
	})
	phone.Prompt(parent, `mcp task_cancel {"taskId":"`+task.TaskID+`"}`)

	hostTask(mac, phone, parent, 1, "cancelled")
}

func TestAHostChildsQuestionAnsweredThereWhileThisMacWasDownClosesHereOnceItIsBack(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := phoneChat(t, mac)
	phone.Prompt(parent, `mcp delegate_task {"task":"ask Ship it?","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	task := hostTask(mac, phone, parent, 1, "working")
	store := storeOf(mac)
	mac.eventually("the question mirrored", func() bool {
		open, _ := store.Requests(task.ChildID, "open")
		return len(open) == 1
	})
	owner := host.Owner()
	question := owner.WaitFor("the question on the host", asked)

	mac.Stop(syscall.SIGTERM)
	resolve(owner, question, map[string]any{"decision": "allow", "answers": map[string]string{"Ship it?": "No"}})
	owner.WaitFor("the answer there", func(m Message) bool { return m.Type == "permission.resolved" })
	mac.up()

	hostTask(mac, mac.Owner(), parent, 1, "completed")
	mac.eventually("the question closed here", func() bool {
		open, _ := store.Requests(task.ChildID, "open")
		return len(open) == 0
	})
}

func TestAHostChildsQuestionStillOpenAcrossARestartReachesTheParentOnce(t *testing.T) {
	mac, _ := linkedHost(t)
	mac.resume = true
	phone, parent := phoneChat(t, mac)
	phone.Prompt(parent, `mcp delegate_task {"task":"ask Ship it?","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	task := hostTask(mac, phone, parent, 1, "working")
	phone.WaitFor("the parent told of the question", func(m Message) bool {
		return m.Type == "agent.stream" && m.AgentID == parent && m.Item.Kind == "user" && strings.Contains(m.Item.Text, "is blocked on a question")
	})
	mac.eventually("the notice's turn ended", func() bool {
		tl := timelineOf(phone, parent)
		return len(tl) > 1 && tl[len(tl)-1] == "result" && strings.Contains(tl[len(tl)-2], "is blocked on a question")
	})

	mac.Restart()

	again := resumedChat(mac, parent)
	// A task started past the restart is mirrored after all the host sent on reconnecting.
	again.Prompt(parent, `mcp delegate_task {"task":"quiet after","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	hostTask(mac, again, parent, 2, "completed")
	open, err := storeOf(mac).Requests(task.ChildID, "open")
	if err != nil || len(open) != 1 {
		t.Fatalf("open requests %+v %v", open, err)
	}
	again.Prompt(parent, `mcp anywhere_pending_request {"taskId":"`+task.TaskID+`","requestId":"`+open[0].ID+`","answers":{"Ship it?":"Yes"}}`)
	hostTask(mac, again, parent, 1, "completed")
	mac.eventually("the parent's wake", func() bool {
		return slices.ContainsFunc(timelineOf(again, parent), func(s string) bool {
			return strings.HasPrefix(s, "user ") && strings.Contains(s, task.TaskID) && strings.Contains(s, "reached a terminal state")
		})
	})
	notices := slices.DeleteFunc(timelineOf(again, parent), func(s string) bool {
		return !strings.HasPrefix(s, "user Delegated task "+task.TaskID) || !strings.Contains(s, "is blocked on a question")
	})
	if len(notices) != 1 {
		t.Fatalf("the parent was told %d times: %q", len(notices), notices)
	}
}

func TestAHostTaskCancelledWhileTheHostWasDownInterruptsTheChildThereOnceItIsBackAfterARestart(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := phoneChat(t, mac)
	phone.Prompt(parent, `mcp delegate_task {"task":"hang","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	task := hostTask(mac, phone, parent, 1, "working")
	child, ok := hostAgent(t, host, task.ChildID)
	if !ok {
		t.Fatalf("the host doesn't list %s", task.ChildID)
	}
	dropLink(t, mac, host)
	phone.Prompt(parent, `mcp task_cancel {"taskId":"`+task.TaskID+`"}`)
	hostTask(mac, phone, parent, 1, "cancelled")

	mac.Restart()
	os.Remove(filepath.Join(host.UserHome(), ".fakessh-no-keys"))
	owner := mac.Owner(proto.CapHosts)
	owner.Send(map[string]any{"type": "host.retry", "id": "back", "hostId": "devbox"})
	waitLink(owner, "devbox", proto.LinkOnline, "")

	host.WaitScreen(child.TerminalID, "interrupt")
}

func TestClosingAParentClosesItsChildsTerminalOnALinkedHost(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := phoneChat(t, mac)
	phone.Prompt(parent, `mcp delegate_task {"task":"await never","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	task := hostTask(mac, phone, parent, 1, "working")
	child, ok := hostAgent(t, host, task.ChildID)
	if !ok {
		t.Fatalf("the host doesn't list %s", task.ChildID)
	}
	p, _ := mac.listed(parent)

	mac.Ops().Send(ops.Msg{Op: "close", ID: p.TerminalID})

	host.eventually("the child's terminal on the host to end", func() bool { return !host.lists(child.TerminalID) })
}

func TestDeletingTheParentsWorktreeFromThePhoneTakesItsSubagentsWorktreeOnALinkedHost(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := chatOn(t, mac, mac.Paired(proto.CapLaunch, proto.CapOrchestrator), map[string]any{"chat": true, "checkout": map[string]any{"new": map[string]any{"name": "calm-otter"}}})
	phone.Prompt(parent, `mcp delegate_task {"task":"hello","title":"Notes","target":{"environmentId":"devbox"}}`)
	task := hostTask(mac, phone, parent, 1, "completed")
	child, ok := hostAgent(t, host, task.ChildID)
	if !ok {
		t.Fatalf("the host doesn't list %s", task.ChildID)
	}
	if !strings.Contains(gitOut(t, host.Repo, "worktree", "list"), child.Cwd) {
		t.Fatalf("the child's folder %s isn't a worktree on the host", child.Cwd)
	}
	p, _ := mac.listed(parent)

	workspace := mac.Owner(proto.CapWorkspace)
	workspace.Send(map[string]any{"type": "worktree.delete", "id": "wd", "path": p.Cwd, "deleteBranch": true, "force": true})
	if m := workspace.WaitFor("the delete's answer", func(m Message) bool { return m.ID == "wd" }); m.Type != "ack" {
		t.Fatalf("got %s", m.Raw)
	}
	for _, tree := range []string{p.Cwd, child.Cwd} {
		if _, err := os.Stat(tree); !os.IsNotExist(err) {
			t.Errorf("%s stayed: %v", tree, err)
		}
	}
	if branches := gitOut(t, host.Repo, "branch", "--format=%(refname:short)"); branches != "main" {
		t.Errorf("the host's branches %q, want main alone", branches)
	}
}

func TestDeletingTheWorktreeOfAParentThatClosedTakesItsSubagentsWorktreeOnALinkedHost(t *testing.T) {
	deleteAfterTheChildClosed(t, false)
}

func TestAWorktreeMadeAtAClosedSubagentsFolderOnALinkedHostSinceStays(t *testing.T) {
	deleteAfterTheChildClosed(t, true)
}

func deleteAfterTheChildClosed(t *testing.T, replaced bool) {
	mac, host := linkedHost(t)
	phone, parent := chatOn(t, mac, mac.Paired(proto.CapLaunch, proto.CapOrchestrator), map[string]any{"chat": true, "checkout": map[string]any{"new": map[string]any{"name": "calm-otter"}}})
	phone.Prompt(parent, `mcp delegate_task {"task":"hello","title":"Notes","target":{"environmentId":"devbox"}}`)
	task := hostTask(mac, phone, parent, 1, "completed")
	child, ok := hostAgent(t, host, task.ChildID)
	if !ok {
		t.Fatalf("the host doesn't list %s", task.ChildID)
	}
	p, _ := mac.listed(parent)
	mac.Ops().Send(ops.Msg{Op: "close", ID: p.TerminalID})
	host.eventually("the child's terminal on the host to end", func() bool { return !host.lists(child.TerminalID) })
	if replaced {
		// The host cleans up the closed child's clean worktree itself; someone makes one at its folder after.
		host.eventually("the child's worktree on the host to go", func() bool {
			_, err := os.Stat(child.Cwd)
			return os.IsNotExist(err)
		})
		time.Sleep(50 * time.Millisecond)
		gitOut(t, host.Repo, "worktree", "add", "-q", "-b", "someone-elses", child.Cwd)
		os.WriteFile(filepath.Join(host.Repo, ".git", "worktrees", filepath.Base(child.Cwd), "anywhere"), nil, 0o644)
	}

	workspace := mac.Owner(proto.CapWorkspace)
	workspace.Send(map[string]any{"type": "worktree.delete", "id": "wd", "path": p.Cwd, "deleteBranch": true, "force": true})
	if m := workspace.WaitFor("the delete's answer", func(m Message) bool { return m.ID == "wd" }); m.Type != "ack" {
		t.Fatalf("got %s", m.Raw)
	}
	if _, err := os.Stat(child.Cwd); os.IsNotExist(err) != !replaced {
		t.Errorf("the worktree at the closed child's folder on the host, replaced %v: %v", replaced, err)
	}
}

func TestDeletingAParentsWorktreeAfterRetentionDroppedItsSessionsStillTakesItsSubagentsWorktreeOnALinkedHost(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := chatOn(t, mac, mac.Paired(proto.CapLaunch, proto.CapOrchestrator), map[string]any{"chat": true, "checkout": map[string]any{"new": map[string]any{"name": "calm-otter"}}})
	phone.Prompt(parent, `mcp delegate_task {"task":"hello","title":"Notes","target":{"environmentId":"devbox"}}`)
	task := hostTask(mac, phone, parent, 1, "completed")
	child, ok := hostAgent(t, host, task.ChildID)
	if !ok {
		t.Fatalf("the host doesn't list %s", task.ChildID)
	}
	p, _ := mac.listed(parent)
	mac.Ops().Send(ops.Msg{Op: "close", ID: p.TerminalID})
	host.eventually("the child's terminal on the host to end", func() bool { return !host.lists(child.TerminalID) })

	// Both stores, as long past retention: each pocketd prunes as it starts.
	old := time.Now().Add(-orch.ClosedKept - time.Hour).UnixMilli()
	for _, h := range []*Harness{mac, host} {
		h.Shutdown()
		store, err := orch.Open(h.Home)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := store.DB().Exec(`UPDATE sessions SET status = 'closed', closed_at = ?`, old); err != nil {
			t.Fatal(err)
		}
		store.Close()
	}
	host.up()
	mac.up()
	waitLink(mac.Owner(proto.CapHosts), "devbox", proto.LinkOnline, "")
	for _, h := range []*Harness{mac, host} {
		h.eventually("retention to drop the sessions", func() bool {
			store, err := orch.Open(h.Home)
			if err != nil {
				return false
			}
			defer store.Close()
			var n int
			store.DB().QueryRow(`SELECT COUNT(*) FROM sessions WHERE closed_at = ?`, old).Scan(&n)
			return n == 0
		})
	}

	workspace := mac.Owner(proto.CapWorkspace)
	workspace.Send(map[string]any{"type": "worktree.delete", "id": "wd", "path": p.Cwd, "deleteBranch": true, "force": true})
	if m := workspace.WaitFor("the delete's answer", func(m Message) bool { return m.ID == "wd" }); m.Type != "ack" {
		t.Fatalf("got %s", m.Raw)
	}
	for _, dir := range []string{p.Cwd, child.Cwd} {
		if _, err := os.Stat(dir); !os.IsNotExist(err) {
			t.Errorf("%s stayed: %v", dir, err)
		}
	}
}

func TestAWorktreesSubagentWorktreesOnALinkedHostAreListedWithTheirHost(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := chatOn(t, mac, mac.Paired(proto.CapLaunch, proto.CapOrchestrator), map[string]any{"chat": true, "checkout": map[string]any{"new": map[string]any{"name": "calm-otter"}}})
	phone.Prompt(parent, `mcp delegate_task {"task":"hello","title":"Notes","target":{"environmentId":"devbox"}}`)
	task := hostTask(mac, phone, parent, 1, "completed")
	child, ok := hostAgent(t, host, task.ChildID)
	if !ok {
		t.Fatalf("the host doesn't list %s", task.ChildID)
	}
	p, _ := mac.listed(parent)
	mac.Ops().Send(ops.Msg{Op: "close", ID: p.TerminalID})
	host.eventually("the child's terminal on the host to end", func() bool { return !host.lists(child.TerminalID) })

	c := mac.Ops()
	c.Send(ops.Msg{Op: "worktree.subagents", ID: "s", Path: p.Cwd})
	r, err := c.Recv()
	if err != nil || r.Ev != "worktree.subagents" || len(r.Trees) != 1 || r.Trees[0].Host != "devbox" || r.Trees[0].Path != child.Cwd || r.Trees[0].Since == 0 {
		t.Fatalf("got %+v %v, want devbox's %s", r, err, child.Cwd)
	}
}

func TestAParentClosedWhileItsHostIsDownClosesTheChildThereAndTakesItsWorktreeOnceTheLinkIsBack(t *testing.T) {
	mac, host := linkedHost(t)
	phone, parent := phoneChat(t, mac)
	phone.Prompt(parent, `mcp delegate_task {"task":"await never","title":"Notes","target":{"environmentId":"devbox"}}`)
	task := hostTask(mac, phone, parent, 1, "working")
	child, ok := hostAgent(t, host, task.ChildID)
	if !ok {
		t.Fatalf("the host doesn't list %s", task.ChildID)
	}
	if child.Cwd == host.Repo {
		t.Fatal("the child runs in the project's own checkout, not a worktree of its own")
	}
	back := dropLink(t, mac, host)
	p, _ := mac.listed(parent)
	mac.Ops().Send(ops.Msg{Op: "close", ID: p.TerminalID})
	mac.eventually("the parent's terminal to end", func() bool { return !mac.lists(p.TerminalID) })
	if !host.lists(child.TerminalID) {
		t.Fatal("the child closed though its host was down")
	}

	back()
	host.eventually("the child's terminal on the host to end", func() bool { return !host.lists(child.TerminalID) })
	host.eventually("the child's worktree on the host to go", func() bool {
		_, err := os.Stat(child.Cwd)
		return os.IsNotExist(err)
	})
}

func TestATaskOnAHostInAFolderNotListedThereListsTheFolderAndRunsInIt(t *testing.T) {
	mac, host := linkedHost(t)
	dir := filepath.Join(host.UserHome(), "unlisted")
	os.MkdirAll(dir, 0o755)
	dir, _ = filepath.EvalSymlinks(dir)
	phone, parent := phoneChat(t, mac)

	phone.Prompt(parent, `mcp delegate_task {"task":"in unlisted","target":{"environmentId":"devbox","projectId":"`+dir+`/"},"isolation":"shared"}`)
	task := hostTask(mac, phone, parent, 1, "completed")

	if a, ok := hostAgent(t, host, task.ChildID); !ok || a.Cwd != dir {
		t.Errorf("the child runs in %q (listed %v), want %s", a.Cwd, ok, dir)
	}
	host.Owner(proto.CapRegistryV2).WaitFor("the folder in the host's projects", func(m Message) bool {
		return m.Type == "registry.list" && strings.Contains(m.Raw, `"path":"`+dir+`"`)
	})
}

func TestATaskOnAHostIsRefusedForAFolderMissingThereOrANameItDoesNotList(t *testing.T) {
	mac, host := linkedHost(t)
	missing := filepath.Join(host.UserHome(), "missing")
	phone, parent := phoneChat(t, mac)

	phone.Prompt(parent, `mcp delegate_task {"task":"in missing","target":{"environmentId":"devbox","projectId":"`+missing+`"}}`)
	phone.WaitFor("the missing folder's refusal", saysAbout(parent, "Couldn't add "+missing+" on devbox: "))
	phone.Prompt(parent, `mcp delegate_task {"task":"in nope","target":{"environmentId":"devbox","projectId":"nope"}}`)
	phone.WaitFor("the unknown name's refusal", saysAbout(parent, "devbox has no project", "nope", "Pass the folder's absolute path on devbox to add it."))

	if ts := delegationsOf(phone, parent); len(ts) != 0 {
		t.Fatalf("tasks %+v", ts)
	}
}
