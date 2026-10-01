package claude

import (
	"os"
	"path/filepath"
	"testing"
)

// projects makes a claude config dir whose projects hold -w/s1.jsonl and
// -w/s1.json, next to an outside secrets.jsonl.
func projects(t *testing.T) (cfg, outside string) {
	root := t.TempDir()
	cfg = filepath.Join(root, "c")
	os.MkdirAll(filepath.Join(cfg, "projects", "-w"), 0o700)
	for _, p := range []string{filepath.Join(cfg, "projects", "-w", "s1.jsonl"), filepath.Join(cfg, "projects", "-w", "s1.json"), filepath.Join(root, "secrets.jsonl")} {
		os.WriteFile(p, nil, 0o600)
	}
	return cfg, filepath.Join(root, "secrets.jsonl")
}

func TestInProjectsAcceptsOnlyTranscriptsUnderProjects(t *testing.T) {
	cfg, outside := projects(t)
	env := []string{"CLAUDE_CONFIG_DIR=" + cfg}
	for path, want := range map[string]bool{
		cfg + "/projects/-w/s1.jsonl":         true,
		cfg + "/projects/../../secrets.jsonl": false,
		cfg + "/projects/-w/s1.json":          false,
		outside:                               false,
		"projects/-w/s1.jsonl":                false,
		cfg + "/projects/-w/../../../x.jsonl": false,
	} {
		if got := InProjects(path, env); got != want {
			t.Errorf("InProjects(%q) = %v, want %v", path, got, want)
		}
	}
}

func TestInProjectsDefaultsToTheHomeClaudeDir(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	os.MkdirAll(filepath.Join(home, ".claude", "projects", "-w"), 0o700)
	os.WriteFile(filepath.Join(home, ".claude", "projects", "-w", "s1.jsonl"), nil, 0o600)
	cfg, _ := projects(t)
	if !InProjects(filepath.Join(home, ".claude", "projects", "-w", "s1.jsonl"), nil) || InProjects(cfg+"/projects/-w/s1.jsonl", nil) {
		t.Fatal("wrong default projects dir")
	}
}

func TestInProjectsRefusesASymlinkOutOfProjects(t *testing.T) {
	cfg, outside := projects(t)
	link := filepath.Join(cfg, "projects", "-w", "link.jsonl")
	os.Symlink(outside, link)
	if InProjects(link, []string{"CLAUDE_CONFIG_DIR=" + cfg}) {
		t.Fatal("a symlink led out of projects")
	}
}
