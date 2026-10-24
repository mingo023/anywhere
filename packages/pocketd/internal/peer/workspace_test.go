package peer

import (
	"testing"

	"pocketd/internal/devices"
)

func TestAWorkspacePhoneManagesLocalsWithSpawnAndEveryoneElseNeedsOwner(t *testing.T) {
	owner := OwnerOf(1)
	phone := FromDevice(devices.Device{ID: "d1", Scopes: devices.PhoneScopes})
	legacy := FromDevice(devices.Device{ID: devices.LegacyID, Scopes: devices.LegacyScopes})
	pty := Principal{Kind: PTY, Pid: 2, Terminal: "t1", Scopes: PTYScopes}
	for _, v := range []string{"local.create", "local.rename", "local.delete"} {
		for verb, want := range map[string][4]string{
			"ws:" + v:                {"", "scope_denied", "scope_denied", "scope_denied"},
			"ws:" + v + ".workspace": {"", "", "scope_denied", "scope_denied"},
		} {
			for i, p := range []Principal{owner, phone, legacy, pty} {
				if got := code(p.Check(verb, "", "")); got != want[i] {
					t.Errorf("%s by principal %d: %q, want %q", verb, i, got, want[i])
				}
			}
		}
	}
}
