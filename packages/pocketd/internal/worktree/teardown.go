package worktree

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	"pocketd/internal/shellenv"
)

// TeardownScript is the script a repo can keep instead of naming its teardown in ConfigFile. It may be gitignored,
// so only the project's own checkout has it.
const TeardownScript = ".anywhere/teardown.sh"

// Teardown is the script deleting the Worktree at tree runs first: own, the project settings' teardown, else the one
// the repo names in ConfigFile, else its TeardownScript run with bash (never on Windows); each tree's before the
// project's own checkout's. "" when there's none.
func Teardown(own, project, tree string) string {
	if own != "" {
		return own
	}
	for _, dir := range []string{tree, project} {
		raw, err := os.ReadFile(filepath.Join(dir, ConfigFile))
		var c struct {
			Teardown []string `json:"teardown"`
		}
		if err == nil && json.Unmarshal(raw, &c) == nil && len(c.Teardown) > 0 {
			return strings.Join(c.Teardown, "\n")
		}
	}
	if runtime.GOOS == "windows" {
		return ""
	}
	for _, dir := range []string{tree, project} {
		script := filepath.Join(dir, TeardownScript)
		if fi, err := os.Stat(script); err == nil && fi.Mode().IsRegular() {
			return "bash '" + strings.ReplaceAll(script, "'", `'\''`) + "'"
		}
	}
	return ""
}

// TeardownFailed holds the end of a teardown's output.
type TeardownFailed struct{ Tail string }

func (TeardownFailed) Error() string { return "Its teardown script failed" }

// RunTeardown runs script in tree with shell, told the project and tree as a session is (ScriptEnv), for limit at
// most. It skips a tree whose folder is already gone.
func RunTeardown(shell, script, project, tree string, env []string, limit time.Duration) error {
	if fi, err := os.Stat(tree); script == "" || err != nil || !fi.IsDir() {
		return nil
	}
	ctx, cancel := context.WithTimeout(context.Background(), limit)
	defer cancel()
	args := []string{"-l", "-c", script}
	if shellenv.PowerShell(shell) {
		args = []string{"-NoLogo", "-Command", script}
	}
	cmd := exec.CommandContext(ctx, shell, args...)
	cmd.Dir, cmd.Env, cmd.WaitDelay = tree, ScriptEnv(env, project, tree), time.Second
	out, err := cmd.CombinedOutput()
	if err == nil {
		return nil
	}
	if errors.Is(ctx.Err(), context.DeadlineExceeded) {
		out = append(out, []byte("\nTimed out after "+limit.String())...)
	}
	lines := strings.Split(strings.TrimSpace(string(out)), "\n")
	return TeardownFailed{strings.Join(lines[max(0, len(lines)-20):], "\n")}
}
