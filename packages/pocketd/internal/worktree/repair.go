package worktree

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// RepairMoved points each repository with a worktree in root's project folders back at it, where the repository
// still names it under from, a home that moved. Each runs `git worktree repair` from inside the worktree, so one
// whose repository is gone is skipped rather than failing the rest.
func RepairMoved(root, from string) error {
	trees, _ := filepath.Glob(filepath.Join(root, "*", "*", ".git"))
	var errs []error
	for _, dotgit := range trees {
		tree := filepath.Dir(dotgit)
		if !namesUnder(dotgit, from) {
			continue
		}
		if out, err := exec.Command("git", "-C", tree, "worktree", "repair").CombinedOutput(); err != nil {
			errs = append(errs, fmt.Errorf("%s: %s", tree, strings.TrimSpace(string(out))))
		}
	}
	return errors.Join(errs...)
}

// namesUnder is whether the repository the worktree file dotgit points at records the worktree under from.
func namesUnder(dotgit, from string) bool {
	raw, err := os.ReadFile(dotgit)
	admin, ok := strings.CutPrefix(strings.TrimSpace(string(raw)), "gitdir: ")
	if err != nil || !ok {
		return false
	}
	back, err := os.ReadFile(filepath.Join(admin, "gitdir"))
	return err == nil && strings.HasPrefix(strings.TrimSpace(string(back)), from+string(filepath.Separator))
}
