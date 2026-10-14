package accounts

import (
	"os/exec"
	"path/filepath"
	"testing"
)

func TestTheKeychainItemUnderTheUsersNameWinsOverAStaleOneWithTheSameService(t *testing.T) {
	kc := filepath.Join(t.TempDir(), "test.keychain")
	security := func(args ...string) {
		t.Helper()
		if out, err := exec.Command("/usr/bin/security", args...).CombinedOutput(); err != nil {
			t.Fatalf("security %v: %v %s", args, err, out)
		}
	}
	security("create-keychain", "-p", "pw", kc)
	t.Cleanup(func() { exec.Command("/usr/bin/security", "delete-keychain", kc).Run() })
	security("unlock-keychain", "-p", "pw", kc)
	security("add-generic-password", "-s", "Claude Code-credentials", "-a", "unknown", "-w", `{"mcpOAuth":{}}`, kc)
	security("add-generic-password", "-s", "Claude Code-credentials", "-a", "me", "-w", `{"claudeAiOauth":{"accessToken":"t"}}`, kc)

	got, ok := findPassword("Claude Code-credentials", "me", kc)
	if !ok || got != `{"claudeAiOauth":{"accessToken":"t"}}` {
		t.Fatalf("got %q, %v", got, ok)
	}
}
