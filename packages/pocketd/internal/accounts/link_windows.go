package accounts

import (
	"fmt"
	"os"
	"os/exec"
	"strings"
)

// link falls back from a symlink, which needs Developer Mode or admin, to a junction for a folder. A file
// has no such fallback: a hard link would break the first time claude or codex replaces the file by rename.
func link(src, dst string) error {
	err := os.Symlink(src, dst)
	if err == nil {
		return nil
	}
	st, serr := os.Stat(src)
	if serr != nil || !st.IsDir() {
		return fmt.Errorf("sharing %s needs Developer Mode, which Windows requires to link a file: %w", dst, err)
	}
	if out, err := exec.Command("cmd", "/c", "mklink", "/J", dst, src).CombinedOutput(); err != nil {
		return fmt.Errorf("junction %s -> %s: %w: %s", dst, src, err, strings.TrimSpace(string(out)))
	}
	return nil
}
