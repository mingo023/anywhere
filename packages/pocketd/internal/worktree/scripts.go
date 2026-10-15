package worktree

import (
	"encoding/json"
	"errors"
	"io/fs"
	"log"
	"os"
	"path/filepath"
	"strings"
)

// ConfigFile is where a repo keeps its own worktree scripts, committed with
// its code: {"setup": ["…"], "teardown": ["…"]}, each a list of commands.
const ConfigFile = ".anywhere/config.json"

type repoConfig struct {
	Setup []string `json:"setup"`
}

// RepoSetup is the setup the repo itself names for the new Worktree at tree:
// from tree's ConfigFile, else from the project's own checkout, the first that
// lists one. Its commands run as one script, a line each, like a setup typed
// into the project's settings. "" when neither names one; a file that doesn't
// parse is skipped.
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
	return ""
}
