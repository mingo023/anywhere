// Package envdir reads an environment, as for a tool's config directory.
package envdir

import (
	"os"
	"path/filepath"
	"strings"
)

// Value returns key's last value in env, or "" when it is unset.
func Value(env []string, key string) string {
	v := ""
	for _, kv := range env {
		if value, ok := strings.CutPrefix(kv, key+"="); ok {
			v = value
		}
	}
	return v
}

// Lookup returns key's last value in env, or ~/fallback when it is unset or empty.
func Lookup(env []string, key, fallback string) string {
	dir := Value(env, key)
	if dir == "" {
		home, _ := os.UserHomeDir()
		dir = filepath.Join(home, fallback)
	}
	return dir
}
