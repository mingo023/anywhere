package worktree

import (
	"encoding/json"
	"errors"
	"io/fs"
	"log"
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
)

// ConfigFile is where a repo keeps its own worktree scripts, committed with
// its code: {"setup": ["…"], "teardown": ["…"]}, each a list of commands.
const ConfigFile = ".anywhere/config.json"

// SetupScript is the script a repo can keep instead of naming its setup in
// ConfigFile. It may be gitignored: new Worktrees then lack it, and the one
// in the project's own checkout runs.
const SetupScript = ".anywhere/setup.sh"

type repoConfig struct {
	Setup []string `json:"setup"`
}

// RepoSetup is the setup the repo itself names for the new Worktree at tree:
// from tree's ConfigFile, else from the project's own checkout, the first that
// lists one. Its commands run as one script, a line each, like a setup typed
// into the project's settings. A file that doesn't parse is skipped. When
// neither lists one, it runs the SetupScript with bash, tree's before the
// checkout's; never on Windows, where sessions run PowerShell. "" when there's
// none of these.
func RepoSetup(project, tree string) string {
	for _, dir := range []string{tree, project} {
		raw, err := os.ReadFile(filepath.Join(dir, ConfigFile))
		if errors.Is(err, fs.ErrNotExist) {
			continue
		}
		var c repoConfig
		if err == nil {
			err = json.Unmarshal(raw, &c)
		}
		if err != nil {
			log.Printf("worktree: skipping %s: %v", filepath.Join(dir, ConfigFile), err)
			continue
		}
		if len(c.Setup) > 0 {
			return strings.Join(c.Setup, "\n")
		}
	}
	if runtime.GOOS == "windows" {
		return ""
	}
	for _, dir := range []string{tree, project} {
		script := filepath.Join(dir, SetupScript)
		if fi, err := os.Stat(script); err == nil && fi.Mode().IsRegular() {
			return "bash '" + strings.ReplaceAll(script, "'", `'\''`) + "'"
		}
	}
	return ""
}

// The variables a Session's terminal and its project's scripts get, like
// Superset's SUPERSET_ROOT_PATH, SUPERSET_WORKSPACE_PATH and
// SUPERSET_WORKSPACE_NAME: a gitignored script lives only in the project's
// checkout, and finds it and the tree it runs for from these.
const (
	RootPathVar = "ANYWHERE_ROOT_PATH"
	TreePathVar = "ANYWHERE_WORKSPACE_PATH"
	TreeNameVar = "ANYWHERE_WORKSPACE_NAME"
)

// ScriptEnv is env for a Session in tree, one of project's Worktrees or its
// own checkout, with RootPathVar, TreePathVar and TreeNameVar set for them in
// place of any it already held.
func ScriptEnv(env []string, project, tree string) []string {
	vars := []string{RootPathVar + "=" + project, TreePathVar + "=" + tree, TreeNameVar + "=" + filepath.Base(tree)}
	out := slices.DeleteFunc(slices.Clone(env), func(kv string) bool {
		k, _, _ := strings.Cut(kv, "=")
		return k == RootPathVar || k == TreePathVar || k == TreeNameVar
	})
	return append(out, vars...)
}
