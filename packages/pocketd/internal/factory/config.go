// Package factory takes a project's Tickets through the Factory's stations.
package factory

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"log"
	"maps"
	"os"
	"path/filepath"
	"sync"

	"pocketd/internal/atomicfile"
	"pocketd/internal/proto"
)

const configVersion = 1

var ErrInvalid = errors.New("These Factory settings aren't valid")

type configFile struct {
	Version  int                            `json:"version"`
	Projects map[string]proto.FactoryConfig `json:"projects"`
}

// Configs is each project's Factory settings by its folder, saved whole to
// Home/state/factory.json on every change.
type Configs struct {
	path string

	mu sync.Mutex
	by map[string]proto.FactoryConfig
}

// OpenConfigs reads home/state/factory.json. A file it can't use is renamed
// to .bad, so the next save doesn't erase it, and every project starts off.
func OpenConfigs(home string) *Configs {
	c := &Configs{path: filepath.Join(home, "state", "factory.json"), by: map[string]proto.FactoryConfig{}}
	raw, err := os.ReadFile(c.path)
	if errors.Is(err, fs.ErrNotExist) {
		return c
	}
	var f configFile
	if err == nil {
		err = json.Unmarshal(raw, &f)
	}
	if err == nil && f.Version != configVersion {
		err = fmt.Errorf("version %d, want %d", f.Version, configVersion)
	}
	if err != nil {
		log.Printf("factory %s: %v; every project starts off", c.path, err)
		if rerr := os.Rename(c.path, c.path+".bad"); rerr != nil {
			log.Print(rerr)
		}
		return c
	}
	if f.Projects != nil {
		c.by = f.Projects
	}
	return c
}

// Get is project's settings, the defaults when none were saved.
func (c *Configs) Get(project string) proto.FactoryConfig {
	c.mu.Lock()
	defer c.mu.Unlock()
	if cfg, ok := c.by[project]; ok {
		return cfg
	}
	return proto.DefaultFactoryConfig()
}

// Save keeps cfg for project; the store changes only if the write worked.
func (c *Configs) Save(project string, cfg proto.FactoryConfig) error {
	if project == "" || !cfg.Valid() {
		return ErrInvalid
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	next := maps.Clone(c.by)
	next[project] = cfg
	raw, err := json.MarshalIndent(configFile{Version: configVersion, Projects: next}, "", "  ")
	if err == nil {
		err = os.MkdirAll(filepath.Dir(c.path), 0o700)
	}
	if err == nil {
		err = atomicfile.Write(c.path, raw, 0o600)
	}
	if err != nil {
		return err
	}
	c.by = next
	return nil
}

// Limit is how many of project's tickets may hold a slot at once; 0 is no
// limit of its own, as while its Factory is off.
func (c *Configs) Limit(project string) int {
	if cfg := c.Get(project); cfg.Enabled {
		return cfg.Slots
	}
	return 0
}
