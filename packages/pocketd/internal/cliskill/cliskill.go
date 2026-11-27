// Package cliskill puts the skill that teaches agents the `zoes` command
// where Claude reads skills, and in ~/.agents/skills, which Codex and the
// other agents that follow that convention read.
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

func roots(home string) []string {
	return []string{filepath.Join(home, ".claude", "skills"), filepath.Join(home, ".agents", "skills")}
}

func skillFile(root, name string) string {
	return filepath.Join(root, name, "SKILL.md")
}

// Install writes the skill to both places under the user's home.
func Install(home string) error {
	for _, root := range roots(home) {
		if err := install(root); err != nil {
			return err
		}
	}
	return nil
}

// install writes skills/zoes, then removes the skills/anywhere/SKILL.md builds before the rename wrote.
func install(root string) error {
	p := skillFile(root, "zoes")
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		return err
	}
	if err := atomicfile.Write(p, skill, 0o644); err != nil {
		return err
	}
	old := skillFile(root, "anywhere")
	if err := os.Remove(old); err != nil && !errors.Is(err, fs.ErrNotExist) {
		return err
	}
	// Fails, and keeps the folder, when the user put something else in it.
	os.Remove(filepath.Dir(old))
	return nil
}

// Refresh brings the copies Install wrote up to this binary's, and adds none.
func Refresh(home string) error {
	for _, root := range roots(home) {
		ok, err := installed(root)
		if err == nil && ok {
			err = install(root)
		}
		if err != nil {
			return err
		}
	}
	return nil
}

func installed(root string) (bool, error) {
	for _, name := range []string{"zoes", "anywhere"} {
		_, err := os.Stat(skillFile(root, name))
		if err == nil {
			return true, nil
		}
		if !errors.Is(err, fs.ErrNotExist) {
			return false, err
		}
	}
	return false, nil
}
