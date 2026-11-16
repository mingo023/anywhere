package proto

import (
	"encoding/json"
	"testing"
)

var hostsServerGolden = map[string]any{
	"host_list": NewHostList("h1", []Host{{ID: "devbox", Name: "devbox", Target: HostTarget{Kind: "alias", Name: "devbox"}, Socket: "/Users/me/.coding-pocket/hosts/devbox.sock",
		Link: HostLinkState{State: LinkOnline, Version: "1.4.2", Uptime: 259200}}}),
	"host_list_push": NewHostList("", nil),
	"host_update_added": NewHostUpdate("h2", Host{ID: "10-0-4-12", Name: "10.0.4.12", Socket: "/Users/me/.coding-pocket/hosts/10-0-4-12.sock",
		Target: HostTarget{Kind: "address", Addr: "10.0.4.12", Port: 2222, User: "dev", KeyFile: "~/.ssh/id_ed25519"}, Link: HostLinkState{State: LinkConnecting}}),
	"host_update_failed": NewHostUpdate("", Host{ID: "10-0-4-12", Name: "10.0.4.12", Windows: true, Socket: "/Users/me/.coding-pocket/hosts/10-0-4-12.sock",
		Target: HostTarget{Kind: "address", Addr: "10.0.4.12", Port: 2222, User: "dev"}, Link: HostLinkState{State: LinkFailed, Failure: "auth"}}),
	"host_progress_step":  NewHostProgress("devbox", true, InstallProgress{Kind: "step", Step: "download"}),
	"host_progress_error": NewHostProgress("devbox", false, InstallProgress{Kind: "error", Text: "No release for linux/arm"}),
	"error_host_ssh":      NewCodedError("h6", CodeHostSSH, "Wrong password.", "auth"),
	"delegation_update_remote": NewDelegationUpdate("a1", Delegation{TaskID: "task1", ParentID: "a1", ChildID: "devbox/a2", Title: "Write the tests", Agent: "codex",
		Status: "working", EnvironmentID: "devbox", CreatedAt: 1}),
	"stream_host_link": NewAgentStream("a1", 1, Item{ID: "i3", Seq: 3, Ts: 3, Kind: "host_link", HostLink: &HostLink{Hint: "vps", Reason: "The build needs the GPU box."}}),
}

func TestAHostVerbWithoutAHostIDIsMalformed(t *testing.T) {
	for _, raw := range []string{
		`{"type":"host.remove","id":"1"}`,
		`{"type":"host.remove","id":"1","hostId":""}`,
		`{"type":"host.retry","id":"1"}`,
		`{"type":"host.install","id":"1","update":true}`,
		`{"type":"host.sign_in","id":"1","password":"x"}`,
		`{"type":"host.sign_in","id":"1","hostId":"","password":"x"}`,
	} {
		if _, err := DecodeClient([]byte(raw)); err != ErrMalformed {
			t.Errorf("%s: got %v, want ErrMalformed", raw, err)
		}
	}
}

func TestAHostTargetOfAnUnknownKindIsMalformed(t *testing.T) {
	for _, target := range []string{
		`{"kind":"ssh","name":"devbox"}`,
		`{"kind":"Alias","name":"devbox"}`,
		`{"name":"devbox"}`,
		`{"kind":"alias"}`,
		`{"kind":"alias","name":"devbox","port":22}`,
		`{"kind":"address","addr":"10.0.4.12"}`,
		`{"kind":"address","port":22}`,
		`{"kind":"address","addr":"10.0.4.12","port":22.5}`,
		`{"kind":"address","addr":"10.0.4.12","port":"22"}`,
		`{"kind":"address","addr":"10.0.4.12","port":22,"user":null}`,
		`{"kind":"address","addr":"10.0.4.12","port":22,"key_file":"k"}`,
		`null`,
		`"devbox"`,
	} {
		raw := `{"type":"host.add","id":"1","target":` + target + `}`
		if _, err := DecodeClient([]byte(raw)); err != ErrMalformed {
			t.Errorf("%s: got %v, want ErrMalformed", target, err)
		}
	}
	if _, err := DecodeClient([]byte(`{"type":"host.add","id":"1"}`)); err != ErrMalformed {
		t.Errorf("no target: got %v, want ErrMalformed", err)
	}
}

func TestAHostTargetWithAnUnsafeValueDecodesForTheHostsPackageToRefuse(t *testing.T) {
	m, err := DecodeClient([]byte(`{"type":"host.add","id":"1","target":{"kind":"alias","name":"-oProxyCommand=x"}}`))
	if err != nil || m.HostTarget == nil || m.HostTarget.Name != "-oProxyCommand=x" {
		t.Fatalf("%+v %v", m, err)
	}
}

func TestTheHostGoldensDecodeToTheirFields(t *testing.T) {
	m, err := DecodeClient(golden(t, "client", "host_add_alias.json"))
	if err != nil || m.Type != "host.add" || m.HostTarget == nil || *m.HostTarget != (HostTarget{Kind: "alias", Name: "devbox"}) || m.Name != "" {
		t.Fatalf("%+v %v", m, err)
	}
	m, err = DecodeClient(golden(t, "client", "host_add_address.json"))
	want := HostTarget{Kind: "address", Addr: "10.0.4.12", Port: 2222, User: "dev", KeyFile: "~/.ssh/id_ed25519"}
	if err != nil || m.HostTarget == nil || *m.HostTarget != want || m.Name != "build box" {
		t.Fatalf("%+v %v", m, err)
	}
	m, err = DecodeClient(golden(t, "client", "host_remove_uninstall.json"))
	if err != nil || m.HostID != "devbox" || !m.Uninstall {
		t.Fatalf("%+v %v", m, err)
	}
	m, err = DecodeClient(golden(t, "client", "host_retry.json"))
	if err != nil || m.HostID != "devbox" {
		t.Fatalf("%+v %v", m, err)
	}
	m, err = DecodeClient(golden(t, "client", "host_install_update.json"))
	if err != nil || m.HostID != "devbox" || !m.Update || m.ArchiveDir != "/Users/me/src/anywhere/packages/pocketd/dist" {
		t.Fatalf("%+v %v", m, err)
	}
	m, err = DecodeClient(golden(t, "client", "host_sign_in.json"))
	if err != nil || m.HostID != "10-0-4-12" || m.Password != "hunter2" {
		t.Fatalf("%+v %v", m, err)
	}
	if m, err = DecodeClient(golden(t, "client", "host_list.json")); err != nil || m.Type != "host.list" {
		t.Fatalf("%+v %v", m, err)
	}
}

func TestEveryHostVerbHasAGolden(t *testing.T) {
	seen := map[string]bool{}
	for _, name := range []string{"host_list", "host_add_alias", "host_add_address", "host_remove_uninstall", "host_retry", "host_install_update", "host_sign_in"} {
		m, err := DecodeClient(golden(t, "client", name+".json"))
		if err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		seen[m.Type] = true
	}
	for _, verb := range HostVerbs() {
		if !seen[verb] {
			t.Errorf("%s has no golden", verb)
		}
	}
}

func TestALinkOfAnUnknownStateOrFailureIsNotWritten(t *testing.T) {
	for _, l := range []HostLinkState{{}, {State: "gone"}, {State: LinkFailed}, {State: LinkRetrying, Failure: "wifi"}} {
		if raw, err := json.Marshal(l); err == nil {
			t.Errorf("%+v wrote %s", l, raw)
		}
	}
}

func TestAnOnlineLinkWritesItsVersionAndUptimeEvenWhenEmpty(t *testing.T) {
	raw, err := json.Marshal(HostLinkState{State: LinkOnline})
	if err != nil || string(raw) != `{"state":"online","version":"","uptime":0}` {
		t.Fatalf("%s %v", raw, err)
	}
}
