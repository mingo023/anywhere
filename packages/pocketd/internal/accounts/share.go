package accounts

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"slices"

	"pocketd/internal/atomicfile"
)

// shared are the entries of a provider's own folder that a managed account
// reads through, so its agents keep the user's settings, skills, hooks and MCP
// servers. projects and sessions are there so a session can resume on another
// account.
var shared = map[string][]string{
	"claude": {"settings.json", "CLAUDE.md", "skills", "commands", "agents", "hooks", "plugins", "rules", "output-styles", "keybindings.json", "projects"},
	"codex":  {"config.toml", "AGENTS.md", "skills", "prompts", "hooks.json", "sessions"},
}

// history are the shared entries that hold what an account has already run.
// A real folder in their place is merged into the user's first, so a session
// started before sharing can still resume.
var history = []string{"projects", "sessions"}

// claudeJSONShared are the keys of ~/.claude.json that hold the user's config.
// The rest names the login or caches what it entitles, which stay per account.
var claudeJSONShared = []string{"mcpServers", "hasCompletedOnboarding"}

// Share links account id's folder to the built-in one for what shared lists,
// and for Claude copies the user's config out of ~/.claude.json. It leaves
// anything the account already has, and never touches credentials.
func (s *Store) Share(provider, id string) error {
	if id == BuiltIn {
		return nil
	}
	s.share.Lock()
	defer s.share.Unlock()
	src, dst := s.Dir(provider, BuiltIn), s.Dir(provider, id)
	if err := os.MkdirAll(dst, 0o700); err != nil {
		return err
	}
	var errs []error
	for _, name := range shared[provider] {
		if _, err := os.Stat(filepath.Join(src, name)); err != nil {
			continue
		}
		if slices.Contains(history, name) {
			if err := absorb(filepath.Join(dst, name), filepath.Join(src, name)); err != nil {
				errs = append(errs, err)
				continue
			}
		}
		if _, err := os.Lstat(filepath.Join(dst, name)); err == nil {
			continue
		}
		errs = append(errs, os.Symlink(filepath.Join(src, name), filepath.Join(dst, name)))
	}
	if provider == "claude" {
		errs = append(errs, s.shareClaudeJSON(id))
	}
	return errors.Join(errs...)
}

// absorb moves what the real folder from holds into to, leaving anything to
// already has, and removes from once it is empty. A link or a missing from
// is left as it is.
func absorb(from, to string) error {
	st, err := os.Lstat(from)
	if err != nil || !st.IsDir() {
		return nil
	}
	if err := mergeInto(from, to); err != nil {
		return err
	}
	return os.Remove(from)
}

func mergeInto(from, to string) error {
	entries, err := os.ReadDir(from)
	if err != nil {
		return err
	}
	for _, e := range entries {
		f, t := filepath.Join(from, e.Name()), filepath.Join(to, e.Name())
		if _, err := os.Lstat(t); err != nil {
			if err := os.Rename(f, t); err != nil {
				return err
			}
			continue
		}
		if e.IsDir() {
			if err := mergeInto(f, t); err != nil {
				return err
			}
			os.Remove(f)
		}
	}
	return nil
}

func (s *Store) shareClaudeJSON(id string) error {
	home, err := os.ReadFile(filepath.Join(s.Home, ".claude.json"))
	if err != nil {
		return nil
	}
	path := filepath.Join(s.Dir("claude", id), ".claude.json")
	own, _ := os.ReadFile(path)
	merged, err := mergeClaudeJSON(home, own)
	if err != nil || sameJSON(merged, own) {
		return err
	}
	return atomicfile.Write(path, merged, 0o600)
}

// sameJSON compares by value, so a running claude's own key order doesn't
// make every launch rewrite a file that claude is writing to.
func sameJSON(a, b []byte) bool {
	var x, y any
	return json.Unmarshal(a, &x) == nil && json.Unmarshal(b, &y) == nil && reflect.DeepEqual(x, y)
}

// mergeClaudeJSON is an account's .claude.json with the user's config laid
// over it: per project, the user's entry wins, and the account keeps projects
// only it has been in.
func mergeClaudeJSON(home, own []byte) ([]byte, error) {
	var h, o map[string]json.RawMessage
	if err := json.Unmarshal(home, &h); err != nil {
		return nil, err
	}
	if json.Unmarshal(own, &o) != nil || o == nil {
		o = map[string]json.RawMessage{}
	}
	for _, k := range claudeJSONShared {
		if v, ok := h[k]; ok {
			o[k] = v
		}
	}
	if v, ok := h["projects"]; ok {
		var hp, op map[string]json.RawMessage
		if json.Unmarshal(v, &hp) == nil {
			if json.Unmarshal(o["projects"], &op) != nil || op == nil {
				op = map[string]json.RawMessage{}
			}
			for dir, cfg := range hp {
				op[dir] = cfg
			}
			raw, err := json.Marshal(op)
			if err != nil {
				return nil, err
			}
			o["projects"] = raw
		}
	}
	return json.MarshalIndent(o, "", "  ")
}
