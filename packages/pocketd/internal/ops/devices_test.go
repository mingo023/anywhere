package ops

import (
	"path/filepath"
	"strings"
	"testing"
	"time"

	"pocketd/internal/devices"
	"pocketd/internal/terminal"
)

func withDevices(t *testing.T) (*Server, *[]string) {
	t.Helper()
	store, err := devices.Open(filepath.Join(t.TempDir(), "devices.json"), "tok")
	if err != nil {
		t.Fatal(err)
	}
	var kicked []string
	return &Server{Terminals: terminal.NewManager(), Devices: store, Kick: func(id, reason string) { kicked = append(kicked, id+" "+reason) }}, &kicked
}

func TestTheOwnerListsRenamesAndRevokesDevices(t *testing.T) {
	srv, kicked := withDevices(t)
	d, _, _ := srv.Devices.Add("iPhone", "ios", devices.PhoneScopes)
	c := start(t, srv)
	c.Send(Msg{Op: "devices"})
	if m := recv(t, c, "devices"); len(m.Devices) != 2 || m.Devices[1].ID != d.ID || m.Devices[1].TokenHash != "" {
		t.Fatalf("%+v", m.Devices)
	}
	c.Send(Msg{Op: "devices.rename", ID: d.ID[:6], Text: "Work"})
	if m := recv(t, c, "ok"); m.ID != d.ID || m.Text != "Work" {
		t.Fatalf("%+v", m)
	}
	c.Send(Msg{Op: "devices.revoke", ID: d.ID[:6]})
	if m := recv(t, c, "ok"); m.Text != "Work" || len(*kicked) != 1 || (*kicked)[0] != d.ID+" revoked" {
		t.Fatalf("%+v %v", m, *kicked)
	}
	c.Send(Msg{Op: "devices.revoke", ID: "legacy"})
	if m := recv(t, c, "error"); m.Error != devices.ErrLegacy.Error() {
		t.Fatalf("%+v", m)
	}
}

func TestAProcessInsideATerminalCantManageDevices(t *testing.T) {
	srv, _ := withDevices(t)
	sock := serve(t, srv)
	c := dial(t, sock)
	c.Send(Msg{Op: "spawn", Cmd: "sh", Args: []string{"-c", `printf '{"op":"devices"}\n' | nc -U "$0"; sleep 5`, sock}, Cols: 200, Rows: 5})
	id := recv(t, c, "spawned").ID
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(50 * time.Millisecond) {
		c.Send(Msg{Op: "screen", ID: id})
		screen := recv(t, c, "screen").Text
		if strings.Contains(screen, `"errorCode":"scope_denied"`) {
			return
		}
		if strings.Contains(screen, `"ev":"devices"`) {
			t.Fatalf("a terminal listed devices:\n%s", screen)
		}
	}
	t.Fatal("no reply")
}
