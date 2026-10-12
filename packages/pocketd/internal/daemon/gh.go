package daemon

import (
	"os/exec"
	"strings"

	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
)

// GhEnv is env for a terminal in cwd, with GH_TOKEN set to the token of the
// github.com account its Project picked in the desktop: gh prefers it to the
// account it has active. The token is asked of gh at each spawn, so one
// renewed by signing in again is never stale.
func (d *Daemon) GhEnv(env []string, cwd string) []string {
	if d.Registry == nil {
		return env
	}
	f := d.Registry.Load()
	p, ok := worktree.Project(f, cwd)
	if !ok || f.Repos[p].GhAccount == "" {
		return env
	}
	token := ghToken(f.Repos[p].GhAccount, d.LoginEnv())
	if token == "" {
		return env
	}
	out := make([]string, 0, len(env)+1)
	for _, kv := range env {
		if !strings.HasPrefix(kv, "GH_TOKEN=") {
			out = append(out, kv)
		}
	}
	return append(out, "GH_TOKEN="+token)
}

// ghToken is the token gh keeps for account, or "" when it has none.
func ghToken(account string, env []string) string {
	gh, err := terminal.LookPath("gh", env)
	if err != nil {
		return ""
	}
	cmd := exec.Command(gh, "auth", "token", "--hostname", "github.com", "--user", account)
	cmd.Env, cmd.Dir = withoutTokens(env), "/"
	out, err := cmd.Output()
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(out))
}

// withoutTokens drops the variables that would make gh print their token
// instead of the account's.
func withoutTokens(env []string) []string {
	var out []string
	for _, kv := range env {
		if !strings.HasPrefix(kv, "GH_TOKEN=") && !strings.HasPrefix(kv, "GITHUB_TOKEN=") {
			out = append(out, kv)
		}
	}
	return out
}
