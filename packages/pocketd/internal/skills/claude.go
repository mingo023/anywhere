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
	var roots []Root
	if cwd != "" {
		project := filepath.Join(cwd, ".claude")
		roots = append(roots, Root{Dir: filepath.Join(project, "skills"), Skills: true}, Root{Dir: filepath.Join(project, "commands")})
	}
	roots = append(roots, Root{Dir: filepath.Join(config, "skills"), Skills: true}, Root{Dir: filepath.Join(config, "commands")})
	for _, p := range plugins(config, cwd) {
		roots = append(roots, Root{Dir: filepath.Join(p.dir, "skills"), Skills: true, Prefix: p.name + ":"}, Root{Dir: filepath.Join(p.dir, "commands"), Prefix: p.name + ":"})
	}
	return roots
}}

type plugin struct{ name, dir string }

// plugins are those installed for the user or for cwd and enabled by the user's settings or cwd's, the project's
// overriding; only the user's with no cwd.
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
	settings := []string{filepath.Join(config, "settings.json")}
	if cwd != "" {
		settings = append(settings, filepath.Join(cwd, ".claude", "settings.json"), filepath.Join(cwd, ".claude", "settings.local.json"))
	}
	for _, path := range settings {
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
			if in.Scope == "user" || (cwd != "" && in.ProjectPath == cwd) {
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
