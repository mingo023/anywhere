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

// ConfigDir is the folder a repo keeps its own worktree scripts in, committed
// with its code. LegacyConfigDir is the one repos used before the rename.
const (
	ConfigDir       = ".zoes"
	LegacyConfigDir = ".anywhere"
)

// ConfigFile, SetupScript and TeardownScript are the files in a config folder.
// ConfigFile is {"setup": ["…"], "teardown": ["…"]}, each a list of commands.
// The scripts can stand in for naming the commands there. They may be
// gitignored: new Worktrees then lack them, and the ones in the project's own
// checkout run.
const (
	ConfigFile     = "config.json"
	SetupScript    = "setup.sh"
	TeardownScript = "teardown.sh"
)

var configFiles = []string{ConfigFile, SetupScript, TeardownScript}

// ConfigDirIn is the config folder of the checkout at root: ConfigDir when it
// holds any of the config files, else LegacyConfigDir when it does, else
// ConfigDir. A folder is read whole: files in LegacyConfigDir are ignored once
// ConfigDir holds any.
func ConfigDirIn(root string) string {
	for _, name := range []string{ConfigDir, LegacyConfigDir} {
		dir := filepath.Join(root, name)
		for _, f := range configFiles {
			if fi, err := os.Stat(filepath.Join(dir, f)); err == nil && fi.Mode().IsRegular() {
				return dir
			}
		}
	}
	return filepath.Join(root, ConfigDir)
}

// ConfigPath is where file (one of ConfigFile, SetupScript, TeardownScript)
// lives, or goes, in the checkout at root.
func ConfigPath(root, file string) string {
	return filepath.Join(ConfigDirIn(root), file)
}

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
		raw, err := os.ReadFile(ConfigPath(dir, ConfigFile))
		if errors.Is(err, fs.ErrNotExist) {
			continue
		}
		var c repoConfig
		if err == nil {
			err = json.Unmarshal(raw, &c)
		}
		if err != nil {
			log.Printf("worktree: skipping %s: %v", ConfigPath(dir, ConfigFile), err)
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
		script := ConfigPath(dir, SetupScript)
		if fi, err := os.Stat(script); err == nil && fi.Mode().IsRegular() {
			return "bash '" + strings.ReplaceAll(script, "'", `'\''`) + "'"
		}
	}
	return ""
}

// The variables a Session's terminal and its project's scripts get, like
// Superset's SUPERSET_ROOT_PATH, SUPERSET_WORKSPACE_PATH and
// SUPERSET_WORKSPACE_NAME: a gitignored script lives only in the project's
// checkout, and finds it and the tree it runs for from these. Each is set
// under both names with one value; a reader takes the ANYWHERE_ one when set
// and not empty, else the ZOES_ one.
const (
	RootPathVar = "ANYWHERE_ROOT_PATH"
	TreePathVar = "ANYWHERE_WORKSPACE_PATH"
	TreeNameVar = "ANYWHERE_WORKSPACE_NAME"

	ZoesRootPathVar = "ZOES_ROOT_PATH"
	ZoesTreePathVar = "ZOES_WORKSPACE_PATH"
	ZoesTreeNameVar = "ZOES_WORKSPACE_NAME"
)

// ScriptEnv is env for a Session in tree, one of project's Worktrees or its
// own checkout, with the variables above set for them in place of any it
// already held.
func ScriptEnv(env []string, project, tree string) []string {
	name := filepath.Base(tree)
	set := [][2]string{
		{RootPathVar, project}, {ZoesRootPathVar, project},
		{TreePathVar, tree}, {ZoesTreePathVar, tree},
		{TreeNameVar, name}, {ZoesTreeNameVar, name},
	}
	out := slices.DeleteFunc(slices.Clone(env), func(kv string) bool {
		k, _, _ := strings.Cut(kv, "=")
		return slices.ContainsFunc(set, func(v [2]string) bool { return v[0] == k })
	})
	for _, v := range set {
		out = append(out, v[0]+"="+v[1])
	}
	return out
}
