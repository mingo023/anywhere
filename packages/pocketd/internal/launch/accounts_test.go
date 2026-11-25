package launch

import (
	"path/filepath"
	"testing"

	"pocketd/internal/accounts"
)

// Without this, the skills scan would read ~/.antigravity.
func TestAntigravityHasNoConfigDir(t *testing.T) {
	l := launcher(t, `["/p"]`)
	home := t.TempDir()
	l.d.Accounts = &accounts.Store{Root: filepath.Join(home, "accounts"), Home: home}
	if dir, err := l.ConfigDir("antigravity", "", "/p"); err != nil || dir != "" {
		t.Fatalf("%q, %v", dir, err)
	}
	if dir, _ := l.ConfigDir("claude", "", "/p"); dir != filepath.Join(home, ".claude") {
		t.Fatalf("claude's is %q", dir)
	}
}
