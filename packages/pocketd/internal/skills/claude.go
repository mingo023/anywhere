package skills

import (
	"encoding/json"
	"maps"
	"os"
	"path/filepath"
	"slices"
	"strings"
)

var claude = Harness{ByDir: true, Roots: func(config, _, cwd string) []Root {
	project := filepath.Join(cwd, ".claude")
	roots := []Root{
		{Dir: filepath.Join(project, "skills"), Skills: true},
		{Dir: filepath.Join(project, "commands")},
		{Dir: filepath.Join(config, "skills"), Skills: true},
		{Dir: filepath.Join(config, "commands")},
	}
	for _, p := range plugins(config, cwd) {
		roots = append(roots, Root{Dir: filepath.Join(p.dir, "skills"), Skills: true, Prefix: p.name + ":"}, Root{Dir: filepath.Join(p.dir, "commands"), Prefix: p.name + ":"})
	}
	return roots
}}

type plugin struct{ name, dir string }

// plugins are those installed for the user or for cwd and enabled by the user's settings or cwd's, the project's
// overriding.
func plugins(config, cwd string) []plugin {
	var installed struct {
		Plugins map[string][]struct {
			Scope       string `json:"scope"`
			InstallPath string `json:"installPath"`
			ProjectPath string `json:"projectPath"`
		} `json:"plugins"`
	}
	readJSON(filepath.Join(config, "plugins", "installed_plugins.json"), &installed)
	enabled := map[string]bool{}
	for _, path := range []string{filepath.Join(config, "settings.json"), filepath.Join(cwd, ".claude", "settings.json"), filepath.Join(cwd, ".claude", "settings.local.json")} {
		var s struct {
			EnabledPlugins map[string]bool `json:"enabledPlugins"`
		}
		readJSON(path, &s)
		for id, on := range s.EnabledPlugins {
			enabled[id] = on
		}
	}
	var out []plugin
	for _, id := range slices.Sorted(maps.Keys(installed.Plugins)) {
		if !enabled[id] {
			continue
		}
		for _, in := range installed.Plugins[id] {
			if in.Scope == "user" || in.ProjectPath == cwd {
				name, _, _ := strings.Cut(id, "@")
				out = append(out, plugin{name, in.InstallPath})
				break
			}
		}
	}
	return out
}

func readJSON(path string, v any) {
	if raw, err := os.ReadFile(path); err == nil {
		json.Unmarshal(raw, v)
	}
}
