//go:build unix

package e2e

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/proto"
)

func TestAnAgentListsLinkedHostsWithWhetherEachIsReachable(t *testing.T) {
	host := Start(t, launchReady(""), asHost)
	mac := Start(t, fakeSSH)
	reach(t, mac, host)
	noKeys := filepath.Join(host.UserHome(), ".fakessh-no-keys")
	if err := os.WriteFile(noKeys, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	owner := mac.Owner(proto.CapHosts)
	addDevbox(owner)
	waitLink(owner, "devbox", proto.LinkFailed, "auth")
	phone, parent := phoneChat(t, mac)

	phone.Prompt(parent, `mcp anywhere_environment_links {}`)
	phone.WaitFor("devbox unreachable", saysAbout(parent, `"environmentId":"devbox"`, `"status":"unreachable"`, "didn't accept your SSH keys", `"notListed"`))

	if err := os.Remove(noKeys); err != nil {
		t.Fatal(err)
	}
	owner.Send(map[string]any{"type": "host.retry", "id": "retry", "hostId": "devbox"})
	waitLink(owner, "devbox", proto.LinkOnline, "")
	phone.Prompt(parent, `mcp anywhere_environment_links {}`)
	phone.WaitFor("devbox reachable", saysAbout(parent, `"environmentId":"devbox"`, `"status":"reachable"`))
}

func TestUnlinkRefusesAHostWithATaskRunningThere(t *testing.T) {
	mac, _ := linkedHost(t)
	// Unlinking needs full access, past what a phone may start a session with by default.
	phone, parent := chatOn(t, mac, mac.Owner(proto.CapLaunch, proto.CapOrchestrator), map[string]any{"chat": true, "access": "full"})
	phone.Prompt(parent, `mcp delegate_task {"task":"hang","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	task := hostTask(mac, phone, parent, 1, "working")

	phone.Prompt(parent, `mcp anywhere_environment_unlink {"environmentId":"devbox"}`)

	phone.WaitFor("the refusal", saysAbout(parent, "conflict", "run on devbox"))
	if saved, err := savedHosts(mac.Home); err != nil || len(saved) != 1 {
		t.Fatalf("hosts after the refusal: %+v %v", saved, err)
	}
	phone.Prompt(parent, `mcp task_cancel {"taskId":"`+task.TaskID+`"}`)
	hostTask(mac, phone, parent, 1, "cancelled")
	phone.Prompt(parent, `mcp anywhere_environment_unlink {"environmentId":"devbox"}`)
	phone.WaitFor("the unlink", saysAbout(parent, `"removed":true`))
	if saved, err := savedHosts(mac.Home); err != nil || len(saved) != 0 {
		t.Fatalf("hosts after the unlink: %+v %v", saved, err)
	}
}

func TestAskingToLinkAHostPostsACardInTheChat(t *testing.T) {
	mac := Start(t, fakeSSH)
	phone, parent := phoneChat(t, mac)

	phone.Prompt(parent, `mcp anywhere_environment_link {"hint":"vps","reason":"The build needs the GPU box."}`)

	phone.WaitAll("the card and the pending reply",
		func(m Message) bool {
			return m.Type == "agent.stream" && m.AgentID == parent && m.Item.Kind == "host_link" &&
				strings.Contains(m.Raw, `"hint":"vps"`) && strings.Contains(m.Raw, `"reason":"The build needs the GPU box."`)
		},
		saysAbout(parent, `"status":"pending"`),
	)
}
