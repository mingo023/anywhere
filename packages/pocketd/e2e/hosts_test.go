//go:build unix

package e2e

import (
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/config"
	"pocketd/internal/hosts"
	"pocketd/internal/ops"
	"pocketd/internal/proto"
)

// fakeSSH makes pocketd's ssh, in its login env too, e2e/fakessh reaching as its host whatever h.Home/remote leads to.
func fakeSSH(h *Harness) {
	launchReady("")(h)
	remote := filepath.Join(h.Home, "remote")
	sh := fmt.Sprintf("export FAKESSH_HOME='%s'\n", remote)
	for name, line := range map[string]string{".zshenv": sh, ".bash_profile": sh, ".profile": sh, ".config/fish/config.fish": fmt.Sprintf("set -gx FAKESSH_HOME '%s'\n", remote)} {
		f, err := os.OpenFile(filepath.Join(h.Home, name), os.O_APPEND|os.O_WRONLY, 0)
		if err == nil {
			_, err = f.WriteString(line)
			f.Close()
		}
		if err != nil {
			h.t.Fatal(err)
		}
	}
	h.Env = append(h.Env, "FAKESSH_HOME="+remote)
}

// asHost serves pocketd from $HOME/<HomeName>, as an installed host has it, with desktop.json beside its socket.
func asHost(h *Harness) {
	user := h.Home
	h.Home, h.Sock = filepath.Join(user, config.HomeName()), filepath.Join(user, config.HomeName(), "pocketd.sock")
	if err := os.MkdirAll(h.Home, 0o700); err != nil {
		h.t.Fatal(err)
	}
	os.Rename(filepath.Join(user, "desktop.json"), filepath.Join(h.Home, "desktop.json"))
	h.Env = append(h.Env, "POCKET_HOME="+h.Home, "POCKETD_SOCK="+h.Sock)
}

// reach lets mac's ssh reach host from now on.
func reach(t *testing.T, mac, host *Harness) {
	t.Helper()
	if err := os.Symlink(filepath.Dir(host.Home), filepath.Join(mac.Home, "remote")); err != nil {
		t.Fatal(err)
	}
}

type hostFrame struct {
	Type    string       `json:"type"`
	ID      string       `json:"id"`
	Code    string       `json:"code"`
	Detail  string       `json:"detail"`
	Message string       `json:"message"`
	Hosts   []proto.Host `json:"hosts"`
	Host    proto.Host   `json:"host"`
}

func hostFrameOf(m Message) hostFrame {
	var f hostFrame
	json.Unmarshal([]byte(m.Raw), &f)
	return f
}

// shown is the host id as m shows it, if m is a host.list or host.update holding it.
func shown(m Message, id string) (proto.Host, bool) {
	f := hostFrameOf(m)
	if f.Type == "host.update" && f.Host.ID == id {
		return f.Host, true
	}
	if f.Type == "host.list" {
		for _, h := range f.Hosts {
			if h.ID == id {
				return h, true
			}
		}
	}
	return proto.Host{}, false
}

func waitLink(p *Phone, id, state, failure string) proto.Host {
	p.t.Helper()
	var found proto.Host
	p.WaitFor(fmt.Sprintf("%s %s %s", id, state, failure), func(m Message) bool {
		h, ok := shown(m, id)
		found = h
		return ok && h.Link.State == state && h.Link.Failure == failure
	})
	return found
}

func addDevbox(p *Phone) hostFrame {
	p.t.Helper()
	p.Send(map[string]any{"type": "host.add", "id": "add", "target": map[string]any{"kind": "alias", "name": "devbox"}})
	return hostFrameOf(p.WaitFor("the added host", func(m Message) bool { return m.ID == "add" }))
}

func answer(p *Phone, id string, seen *[]string) hostFrame {
	p.t.Helper()
	return hostFrameOf(p.WaitFor("the answer to "+id, func(m Message) bool {
		if seen != nil {
			*seen = append(*seen, m.Raw)
		}
		return m.ID == id
	}))
}

func TestAnAddedHostComesOnlineThroughItsTunnel(t *testing.T) {
	host := Start(t, asHost)
	mac := Start(t, fakeSSH)
	reach(t, mac, host)
	owner := mac.Owner(proto.CapHosts)
	owner.WaitFor("the empty host list", func(m Message) bool {
		f := hostFrameOf(m)
		return f.Type == "host.list" && f.Hosts != nil && len(f.Hosts) == 0
	})
	added := addDevbox(owner)
	socket := filepath.Join(mac.Home, "hosts", "devbox.sock")
	if added.Type != "host.update" || added.Host.ID != "devbox" || added.Host.Socket != socket || added.Host.Link.State != proto.LinkConnecting {
		t.Fatalf("%+v", added)
	}
	online := waitLink(owner, "devbox", proto.LinkOnline, "")
	if want := host.Status(); online.Link.Version != want.Version || want.Version == "" {
		t.Fatalf("online as %+v, the host runs %q", online.Link, want.Version)
	}
	c, err := ops.Dial(socket)
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	c.Send(ops.Msg{Op: "status"})
	if m, err := c.Recv(); err != nil || m.Status == nil || m.Status.PID != host.Status().PID {
		t.Fatalf("the tunnel reached %+v %v, not the host's pocketd", m.Status, err)
	}
}

func TestAHostThatRefusesTheConnectionRetriesAndReconnectsOnRetry(t *testing.T) {
	host := Start(t, asHost)
	mac := Start(t, fakeSSH)
	owner := mac.Owner(proto.CapHosts)
	addDevbox(owner)
	waitLink(owner, "devbox", proto.LinkRetrying, "refused")
	waitLink(owner, "devbox", proto.LinkConnecting, "")
	waitLink(owner, "devbox", proto.LinkRetrying, "refused")
	reach(t, mac, host)
	owner.Send(map[string]any{"type": "host.retry", "id": "retry", "hostId": "devbox"})
	if f := answer(owner, "retry", nil); f.Type != "ack" {
		t.Fatalf("%+v", f)
	}
	waitLink(owner, "devbox", proto.LinkOnline, "")
	owner.Send(map[string]any{"type": "host.retry", "id": "nobody", "hostId": "nobody"})
	if f := answer(owner, "nobody", nil); f.Code != proto.CodeUnknownHost {
		t.Fatalf("%+v", f)
	}
}

func TestRemovingAHostClosesItsTunnelAndForgetsItsFiles(t *testing.T) {
	host := Start(t, asHost)
	mac := Start(t, fakeSSH)
	reach(t, mac, host)
	owner := mac.Owner(proto.CapHosts)
	addDevbox(owner)
	waitLink(owner, "devbox", proto.LinkOnline, "")
	dir := filepath.Join(mac.Home, "hosts")
	files := []string{"devbox.known_hosts", "devbox.token", "devbox.key", "devbox.key.pub"}
	for _, f := range files {
		if err := os.WriteFile(filepath.Join(dir, f), []byte("x"), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	owner.Send(map[string]any{"type": "host.remove", "id": "remove", "hostId": "devbox"})
	owner.WaitAll("the removal and the list without it",
		func(m Message) bool { return m.ID == "remove" && m.Type == "ack" },
		func(m Message) bool {
			f := hostFrameOf(m)
			return f.Type == "host.list" && f.ID == "" && f.Hosts != nil && len(f.Hosts) == 0
		})
	if _, err := ops.Dial(filepath.Join(dir, "devbox.sock")); err == nil {
		t.Fatal("the tunnel still answers")
	}
	for _, f := range append(files, "devbox.sock") {
		if _, err := os.Lstat(filepath.Join(dir, f)); err == nil {
			t.Errorf("%s is still there", f)
		}
	}
	if saved, err := hosts.Load(mac.Home); err != nil || len(saved) != 0 {
		t.Fatalf("hosts.json holds %+v %v", saved, err)
	}
}

func TestHostEventsReachTheOwnerAndNeverAPhone(t *testing.T) {
	host := Start(t, asHost)
	mac := Start(t, fakeSSH)
	reach(t, mac, host)
	phone := mac.Paired(proto.CapHosts)
	owner := mac.Owner(proto.CapHosts)
	addDevbox(owner)
	waitLink(owner, "devbox", proto.LinkOnline, "")
	var seen []string
	phone.Send(map[string]any{"type": "host.list", "id": "peek"})
	if f := answer(phone, "peek", &seen); f.Type != "error" || len(f.Hosts) != 0 {
		t.Fatalf("a phone listed hosts: %+v", f)
	}
	for _, raw := range seen {
		var m struct{ Type string }
		json.Unmarshal([]byte(raw), &m)
		if m.Type == "host.list" || m.Type == "host.update" || m.Type == "host.progress" {
			t.Errorf("the phone got %s", raw)
		}
	}
}

func TestAHostAddedBeforeARestartComesBackOnlineAfterIt(t *testing.T) {
	host := Start(t, asHost)
	mac := Start(t, fakeSSH)
	reach(t, mac, host)
	addDevbox(mac.Owner(proto.CapHosts))
	mac.Restart()
	owner := mac.Owner(proto.CapHosts)
	waitLink(owner, "devbox", proto.LinkOnline, "")
}

func TestASignInPasswordIsNeverLoggedStoredOrSentToAClient(t *testing.T) {
	host := Start(t, asHost)
	mac := Start(t, fakeSSH)
	reach(t, mac, host)
	right, wrong := "right-horse-battery-7f3a", "wrong-staple-91c2"
	if err := os.WriteFile(filepath.Join(filepath.Dir(host.Home), ".fakessh-password"), []byte(right), 0o600); err != nil {
		t.Fatal(err)
	}
	phone := mac.Paired(proto.CapHosts)
	owner := mac.Owner(proto.CapHosts)
	addDevbox(owner)
	waitLink(owner, "devbox", proto.LinkOnline, "")
	var seen []string
	owner.Send(map[string]any{"type": "host.sign_in", "id": "wrong", "hostId": "devbox", "password": wrong})
	if f := answer(owner, "wrong", &seen); f.Code != proto.CodeHostSSH || f.Detail != "auth" || f.Message != "Wrong password." {
		t.Fatalf("%+v", f)
	}
	owner.Send(map[string]any{"type": "host.sign_in", "id": "right", "hostId": "devbox", "password": right})
	if f := answer(owner, "right", &seen); f.Type != "ack" {
		t.Fatalf("%+v", f)
	}
	key, err := os.ReadFile(filepath.Join(mac.Home, "hosts", "devbox.key.pub"))
	authorized, _ := os.ReadFile(filepath.Join(filepath.Dir(host.Home), ".ssh", "authorized_keys"))
	if err != nil || !strings.Contains(string(authorized), strings.TrimSpace(string(key))) {
		t.Fatalf("the host's authorized_keys %q lacks the app's key %q %v", authorized, key, err)
	}
	owner.WaitFor("devbox online with the app's key", func(m Message) bool {
		seen = append(seen, m.Raw)
		h, ok := shown(m, "devbox")
		return ok && h.Link.State == proto.LinkOnline
	})
	phone.Send(map[string]any{"type": "agent.list", "id": "sentinel"})
	answer(phone, "sentinel", &seen)
	leaks := func(where, text string) {
		for _, pw := range []string{right, wrong} {
			if strings.Contains(text, pw) {
				t.Errorf("%s holds a password: %s", where, text)
			}
		}
	}
	for _, raw := range seen {
		leaks("a frame", raw)
	}
	leaks("pocketd's output", mac.log.String())
	filepath.WalkDir(mac.Home, func(path string, d fs.DirEntry, err error) error {
		if err == nil && d.Type().IsRegular() {
			b, _ := os.ReadFile(path)
			leaks(path, string(b))
		}
		return nil
	})
}

func TestRemovingAHostFailsTheTasksRunningThereAndWakesTheirParent(t *testing.T) {
	mac, _ := linkedHost(t)
	phone, parent := phoneChat(t, mac)
	phone.Prompt(parent, `mcp delegate_task {"task":"hang","target":{"environmentId":"devbox"},"isolation":"shared"}`)
	hostTask(mac, phone, parent, 1, "working")

	owner := mac.Owner(proto.CapHosts)
	owner.Send(map[string]any{"type": "host.remove", "id": "remove", "hostId": "devbox"})
	if f := answer(owner, "remove", nil); f.Type != "ack" {
		t.Fatalf("%+v", f)
	}

	if task := settledAndWoken(mac, phone, parent, "failed"); !strings.Contains(task.Error, "devbox") {
		t.Fatalf("task = %+v", task)
	}
}
