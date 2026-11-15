package factory

import (
	"os"
	"path/filepath"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/providers"
	"pocketd/internal/worktree"
)

// skillRoots is where each agent looks for a project's skills.
var skillRoots = map[string]string{providers.Claude.ID: ".claude/skills", providers.Codex.ID: ".agents/skills"}

// CopySkills puts p's skills into checkout for provider's agent and hides them
// from git. A skill whose folder the checkout already has is skipped, never
// overwritten.
func CopySkills(p proto.Profile, provider, checkout string) (copied, skipped []string, err error) {
	root, ok := skillRoots[provider]
	if !ok || len(p.Files) == 0 {
		return nil, nil, nil
	}
	var hide []string
	for _, name := range skillsOf(p.Files) {
		dst := filepath.Join(checkout, filepath.FromSlash(root), name)
		if _, err := os.Lstat(dst); err == nil {
			skipped = append(skipped, name)
			continue
		}
		for _, f := range p.Files {
			rel, ok := strings.CutPrefix(f.Path, "skills/"+name+"/")
			if !ok {
				continue
			}
			data, err := body(f)
			if err != nil {
				return copied, skipped, err
			}
			file := filepath.Join(dst, filepath.FromSlash(rel))
			if err := os.MkdirAll(filepath.Dir(file), 0o755); err != nil {
				return copied, skipped, err
			}
			if err := os.WriteFile(file, data, 0o644); err != nil {
				return copied, skipped, err
			}
		}
		copied, hide = append(copied, name), append(hide, "/"+root+"/"+name+"/")
	}
	if len(hide) == 0 {
		return copied, skipped, nil
	}
	return copied, skipped, worktree.Exclude(checkout, hide)
}
