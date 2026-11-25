package registry

import (
	"encoding/json"
	"errors"
	"io/fs"
	"log"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"pocketd/internal/proto"
)

// ReadLegacy is the registry a pocketd from before pocketd.db kept in home/state/registry.json. While that file
// is missing or broken it is home/desktop.json as that pocketd first imported it: the projects there that aren't
// another host's ("//host/..." paths), their settings, the tracked worktrees and the shared settings, each
// project given an id. It writes nothing.
func ReadLegacy(home string) (State, error) {
	path := filepath.Join(home, "state", "registry.json")
	raw, err := os.ReadFile(path)
	if errors.Is(err, fs.ErrNotExist) {
		return importDesktop(filepath.Join(home, "desktop.json")), nil
	}
	if err != nil {
		return State{}, err
	}
	var s State
	if err := json.Unmarshal(raw, &s); err != nil {
		log.Printf("registry: %s unreadable, importing desktop.json: %v", path, err)
		return importDesktop(filepath.Join(home, "desktop.json")), nil
	}
	return s, nil
}

func importDesktop(path string) State {
	var keys map[string]json.RawMessage
	raw, err := os.ReadFile(path)
	if err == nil {
		err = json.Unmarshal(raw, &keys)
	}
	if err != nil {
		if !errors.Is(err, fs.ErrNotExist) {
			log.Printf("registry: %s: %v", path, err)
		}
		return State{}
	}
	// Key by key, so one the desktop wrote in a shape this can't read loses only itself.
	var f File
	for key, into := range map[string]any{"projects": &f.Projects, "repos": &f.Repos, "tracked": &f.Tracked, "git": &f.Git, "agents": &f.Agents, "worktree": &f.Worktree} {
		if v, ok := keys[key]; ok {
			if err := json.Unmarshal(v, into); err != nil {
				log.Printf("registry: %s: %s: %v", path, key, err)
			}
		}
	}
	local := func(p string) bool { return !strings.HasPrefix(p, "//") }
	s := State{Settings: proto.SharedSettings{Git: f.Git, Access: f.Agents.Access, WorktreeRoot: f.Worktree.Root}}
	for _, p := range f.Projects {
		if local(p) && s.index(p) < 0 {
			s.Projects = append(s.Projects, proto.RegistryProject{ID: newID(), Path: p, Repo: f.Repos[p]})
		}
	}
	s.Tracked = slices.DeleteFunc(slices.Clone(f.Tracked), func(p string) bool { return !local(p) })
	return s
}
