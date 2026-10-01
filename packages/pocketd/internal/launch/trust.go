package launch

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// Trusted mirrors claude 2.1.285's folder trust: the canonical git root
// first, then every folder from cwd up to the git toplevel, or up to /
// outside git.
func Trusted(claudeJSON []byte, cwd, canonical, toplevel string) bool {
	var cfg struct {
		Projects map[string]struct {
			Accepted bool `json:"hasTrustDialogAccepted"`
		} `json:"projects"`
	}
	if json.Unmarshal(claudeJSON, &cfg) != nil {
		return false
	}
	if canonical != "" && cfg.Projects[canonical].Accepted {
		return true
	}
	for dir := filepath.Clean(cwd); ; dir = filepath.Dir(dir) {
		if cfg.Projects[dir].Accepted {
			return true
		}
		if dir == toplevel || dir == filepath.Dir(dir) {
			return false
		}
	}
}

// trustedAt reads claude's global config and cwd's git layout. It never
// writes: every running claude rewrites that file without a lock.
func trustedAt(env []string, cwd string) bool {
	if lookup(env, "CLAUDE_CODE_SANDBOXED") != "" {
		return true
	}
	dir := lookup(env, "CLAUDE_CONFIG_DIR")
	if dir == "" {
		dir = lookup(env, "HOME")
	}
	raw, _ := os.ReadFile(filepath.Join(dir, ".claude.json"))
	if real, err := filepath.EvalSymlinks(cwd); err == nil {
		cwd = real
	}
	canonical := ""
	if common := revParse(cwd, "--git-common-dir"); common != "" {
		canonical = filepath.Dir(common)
	}
	return Trusted(raw, cwd, canonical, revParse(cwd, "--show-toplevel"))
}

func revParse(cwd, flag string) string {
	out, err := exec.Command("git", "-C", cwd, "rev-parse", "--path-format=absolute", flag).Output()
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(out))
}

func lookup(env []string, key string) string {
	v := ""
	for _, kv := range env {
		if k, val, ok := strings.Cut(kv, "="); ok && k == key {
			v = val
		}
	}
	return v
}
