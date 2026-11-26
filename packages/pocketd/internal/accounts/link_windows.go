package accounts

import (
	"fmt"
	"os"
	"os/exec"
	"strings"
)

// link falls back from a symlink, which needs Developer Mode or admin, to a
// junction for a folder and a hard link for a file, which need neither.
func link(src, dst string) error {
	err := os.Symlink(src, dst)
	if err == nil {
		return nil
	}
	st, serr := os.Stat(src)
	if serr != nil {
		return err
	}
	if !st.IsDir() {
		return os.Link(src, dst)
	}
	if out, err := exec.Command("cmd", "/c", "mklink", "/J", dst, src).CombinedOutput(); err != nil {
		return fmt.Errorf("junction %s -> %s: %w: %s", dst, src, err, strings.TrimSpace(string(out)))
	}
	return nil
}
