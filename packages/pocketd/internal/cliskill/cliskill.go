// Package cliskill puts the skill that teaches agents the `anywhere` command
// where Claude and Codex read skills.
package cliskill

import (
	_ "embed"
	"errors"
	"io/fs"
	"os"
	"path/filepath"

	"pocketd/internal/atomicfile"
)

//go:embed SKILL.md
var skill []byte

func paths(home string) []string {
	return []string{
		filepath.Join(home, ".claude", "skills", "anywhere", "SKILL.md"),
		filepath.Join(home, ".agents", "skills", "anywhere", "SKILL.md"),
	}
}

// Install writes the skill for Claude and Codex under the user's home.
func Install(home string) error {
	for _, p := range paths(home) {
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			return err
		}
		if err := atomicfile.Write(p, skill, 0o644); err != nil {
			return err
		}
	}
	return nil
}

// Refresh brings the copies Install wrote up to this binary's, and adds none.
func Refresh(home string) error {
	for _, p := range paths(home) {
		_, err := os.Stat(p)
		if errors.Is(err, fs.ErrNotExist) {
			continue
		}
		if err == nil {
			err = atomicfile.Write(p, skill, 0o644)
		}
		if err != nil {
			return err
		}
	}
	return nil
}
