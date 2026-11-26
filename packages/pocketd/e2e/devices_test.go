package e2e

import (
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestTheOwnerSeesTheSharedTokenInDevices(t *testing.T) {
	h := Start(t)
	cmd := exec.Command(filepath.Join(binDir, "pocketd"), "devices")
	cmd.Env = h.Env
	out, err := cmd.CombinedOutput()
	if err != nil || !strings.Contains(string(out), "Shared token (legacy)") {
		t.Fatalf("devices: %v\n%s", err, out)
	}
}

func TestDevicesRefusesAPocketTerminal(t *testing.T) {
	h := Start(t)
	id := h.Spawn("sh", "-c", `"$0" devices; sleep 5`, filepath.Join(binDir, "pocketd"))
	h.WaitScreen(id, "devices needs owner; run it outside Pocket Terminals")
}
