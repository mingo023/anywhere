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
	"testing"

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
	// Only to see the link back up from hosts.json: no owner is connected once the parent delegates.
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
		user := filepath.Dir(h.Home)
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

	other, _ := filepath.EvalSymlinks(filepath.Join(filepath.Dir(host.Home), "other"))
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
	if err := os.WriteFile(filepath.Join(filepath.Dir(host.Home), "released"), nil, 0o600); err != nil {
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
	if err := os.WriteFile(filepath.Join(filepath.Dir(host.Home), ".fakessh-no-keys"), nil, 0o600); err != nil {
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
