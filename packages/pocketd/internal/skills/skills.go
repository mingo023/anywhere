// Package skills lists the slash commands an agent CLI offers in a project
// without running it, by reading the folders it keeps skills and commands in.
// Each harness only says where those folders are.
package skills

import (
	"io/fs"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/providers"
)

// Root is a folder a harness reads slash commands from.
type Root struct {
	Dir string
	// Skills: each <dir>/<name>/SKILL.md is one; otherwise each .md file below Dir is one, named by its stem.
	Skills bool
	// Prefix leads each name, as a plugin's "<plugin>:".
	Prefix string
}

// Harness is an agent CLI whose slash commands can be read off disk.
type Harness struct {
	// Roots are the folders for an account's config dir, the user's home and a project's cwd, in the order the menu
	// lists them; a name seen earlier wins.
	Roots func(config, home, cwd string) []Root
	// ByDir names a skill by its folder, as its CLI invokes it, rather than by its frontmatter's name.
	ByDir bool
}

var harnesses = map[string]Harness{providers.Claude.ID: claude, providers.Codex.ID: codex}

var codex = Harness{Roots: func(_, home, cwd string) []Root {
	var roots []Root
	for _, dir := range upToRepo(cwd) {
		roots = append(roots, Root{Dir: filepath.Join(dir, ".agents", "skills"), Skills: true})
	}
	return append(roots, Root{Dir: filepath.Join(home, ".agents", "skills"), Skills: true}, Root{Dir: "/etc/codex/skills", Skills: true})
}}

// Scan is the slash commands provider offers in cwd on the account whose config dir is config; none for a provider
// with no harness.
func Scan(provider, config, home, cwd string) []proto.Command {
	h, ok := harnesses[provider]
	if !ok {
		return nil
	}
	var out []proto.Command
	seen := map[string]bool{}
	for _, r := range h.Roots(config, home, cwd) {
		for _, c := range read(r, h.ByDir) {
			if !seen[c.Name] {
				seen[c.Name] = true
				out = append(out, c)
			}
		}
	}
	return out
}

func read(r Root, byDir bool) []proto.Command {
	if !r.Skills {
		return commands(r)
	}
	entries, _ := os.ReadDir(r.Dir)
	var out []proto.Command
	for _, e := range entries {
		raw, err := os.ReadFile(filepath.Join(r.Dir, e.Name(), "SKILL.md"))
		if err != nil {
			continue
		}
		fm := frontmatter(string(raw))
		name := fm["name"]
		if byDir || name == "" {
			name = e.Name()
		}
		if fm["user-invocable"] != "false" {
			out = append(out, proto.Command{Name: r.Prefix + name, Description: fm["description"], ArgumentHint: fm["argument-hint"]})
		}
	}
	return out
}

func commands(r Root) []proto.Command {
	var out []proto.Command
	filepath.WalkDir(r.Dir, func(path string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() || filepath.Ext(path) != ".md" {
			return nil
		}
		raw, err := os.ReadFile(path)
		if err != nil {
			return nil
		}
		fm := frontmatter(string(raw))
		name := strings.TrimSuffix(d.Name(), ".md")
		out = append(out, proto.Command{Name: r.Prefix + name, Description: fm["description"], ArgumentHint: fm["argument-hint"]})
		return nil
	})
	return out
}

// upToRepo is cwd and each folder above it up to its git repo's root; cwd alone outside a repo.
func upToRepo(cwd string) []string {
	var dirs []string
	for dir := cwd; ; dir = filepath.Dir(dir) {
		dirs = append(dirs, dir)
		if _, err := os.Stat(filepath.Join(dir, ".git")); err == nil {
			return dirs
		}
		if filepath.Dir(dir) == dir {
			return dirs[:1]
		}
	}
}

// frontmatter is the top-level scalar keys of md's leading YAML block; a folded or literal value is joined into one
// line.
func frontmatter(md string) map[string]string {
	out := map[string]string{}
	lines := strings.Split(strings.ReplaceAll(md, "\r\n", "\n"), "\n")
	if len(lines) == 0 || strings.TrimSpace(lines[0]) != "---" {
		return out
	}
	block := ""
	for _, line := range lines[1:] {
		if strings.TrimSpace(line) == "---" {
			break
		}
		if line != "" && (line[0] == ' ' || line[0] == '\t') {
			if block != "" {
				out[block] = strings.TrimSpace(out[block] + " " + strings.TrimSpace(line))
			}
			continue
		}
		block = ""
		key, value, ok := strings.Cut(line, ":")
		if !ok {
			continue
		}
		key, value = strings.TrimSpace(key), strings.TrimSpace(value)
		switch value {
		case ">", "|", ">-", "|-", ">+", "|+":
			block, value = key, ""
		}
		out[key] = unquote(value)
	}
	return out
}

func unquote(v string) string {
	if len(v) >= 2 && v[0] == '"' && v[len(v)-1] == '"' {
		if s, err := strconv.Unquote(v); err == nil {
			return s
		}
	}
	if len(v) >= 2 && v[0] == '\'' && v[len(v)-1] == '\'' {
		return strings.ReplaceAll(v[1:len(v)-1], "''", "'")
	}
	return v
}
