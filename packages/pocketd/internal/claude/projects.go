package claude

import (
	"os"
	"path/filepath"
	"strings"
)

// projectsDir is the projects dir of the claude that runs with env.
func projectsDir(env []string) string {
	dir := ""
	for _, kv := range env {
		if v, ok := strings.CutPrefix(kv, "CLAUDE_CONFIG_DIR="); ok {
			dir = v
		}
	}
	if dir == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return ""
		}
		dir = filepath.Join(home, ".claude")
	}
	return filepath.Join(dir, "projects")
}

// InProjects reports whether path is a transcript in the projects dir of the
// claude that runs with env, so a saved path can't make pocketd read any file.
func InProjects(path string, env []string) bool {
	dir := projectsDir(env)
	if dir == "" || !filepath.IsAbs(path) || !strings.HasSuffix(path, ".jsonl") {
		return false
	}
	root, err := filepath.EvalSymlinks(dir)
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

// Sessions are the transcripts the claude that runs in cwd with env keeps.
// Claude names their dir after its resolved cwd, every character but a
// letter or digit a dash, so a cwd under a symlink like /tmp is looked up
// resolved as well as as given.
func Sessions(cwd string, env []string) []string {
	root := projectsDir(env)
	if root == "" || !filepath.IsAbs(cwd) {
		return nil
	}
	dirs := []string{cwd}
	if real, err := filepath.EvalSymlinks(cwd); err == nil && real != cwd {
		dirs = append(dirs, real)
	}
	var paths []string
	for _, d := range dirs {
		dir := filepath.Join(root, strings.Map(dash, d))
		entries, _ := os.ReadDir(dir)
		for _, e := range entries {
			if e.Type().IsRegular() && strings.HasSuffix(e.Name(), ".jsonl") {
				paths = append(paths, filepath.Join(dir, e.Name()))
			}
		}
	}
	return paths
}

// Transcript is where the claude that runs in cwd with env writes session's
// transcript; "" for a session id that isn't one.
func Transcript(cwd, session string, env []string) string {
	root := projectsDir(env)
	if root == "" || !filepath.IsAbs(cwd) || session == "" || filepath.Base(session) != session || strings.HasPrefix(session, ".") {
		return ""
	}
	return filepath.Join(root, strings.Map(dash, cwd), session+".jsonl")
}

// SameProject says whether claudes in cwds a and b keep their transcripts in
// one dir, as Claude names it.
func SameProject(a, b string) bool {
	return strings.Map(dash, a) == strings.Map(dash, b)
}

func dash(r rune) rune {
	if 'a' <= r && r <= 'z' || 'A' <= r && r <= 'Z' || '0' <= r && r <= '9' {
		return r
	}
	return '-'
}
