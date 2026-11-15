package worktree

import (
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"strings"
)

// Exclude adds paths to info/exclude of the repository dir is a checkout of,
// so git neither lists nor adds them in any of its Worktrees. A line that is
// already there is left alone.
func Exclude(dir string, paths []string) error {
	loc, ok := Locate(dir)
	if !ok {
		return errors.New(dir + " isn't in a git checkout")
	}
	file := filepath.Join(loc.Common, "info", "exclude")
	old, err := os.ReadFile(file)
	if err != nil && !errors.Is(err, fs.ErrNotExist) {
		return err
	}
	text := string(old)
	have := strings.Split(text, "\n")
	if text != "" && !strings.HasSuffix(text, "\n") {
		text += "\n"
	}
	added := false
	for _, p := range paths {
		if slices.Contains(have, p) {
			continue
		}
		text, have, added = text+p+"\n", append(have, p), true
	}
	if !added {
		return nil
	}
	if err := os.MkdirAll(filepath.Dir(file), 0o755); err != nil {
		return err
	}
	return os.WriteFile(file, []byte(text), 0o644)
}
