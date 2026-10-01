package claude

import (
	"os"
	"path/filepath"
	"strings"
)

// InProjects reports whether path is a transcript in the projects dir of the
// claude that runs with env, so a saved path can't make pocketd read any file.
func InProjects(path string, env []string) bool {
	dir := ""
	for _, kv := range env {
		if v, ok := strings.CutPrefix(kv, "CLAUDE_CONFIG_DIR="); ok {
			dir = v
		}
	}
	if dir == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return false
		}
		dir = filepath.Join(home, ".claude")
	}
	if !filepath.IsAbs(path) || !strings.HasSuffix(path, ".jsonl") {
		return false
	}
	root, err := filepath.EvalSymlinks(filepath.Join(dir, "projects"))
	if err != nil {
		return false
	}
	real, err := filepath.EvalSymlinks(path)
	if err != nil {
		return false
	}
	rel, err := filepath.Rel(root, real)
	return err == nil && rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}
