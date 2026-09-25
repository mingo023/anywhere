// Package envdir finds a tool's config directory from an environment.
package envdir

import (
	"os"
	"path/filepath"
	"strings"
)

// Lookup returns key's last value in env, or ~/fallback when it is unset or empty.
func Lookup(env []string, key, fallback string) string {
	dir := ""
	for _, kv := range env {
		if v, ok := strings.CutPrefix(kv, key+"="); ok {
			dir = v
		}
	}
	if dir == "" {
		home, _ := os.UserHomeDir()
		dir = filepath.Join(home, fallback)
	}
	return dir
}
