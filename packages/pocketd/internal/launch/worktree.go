package launch

import (
	"os/exec"
	"path/filepath"
	"slices"
	"strconv"
	"strings"

	"pocketd/internal/proto"
	"pocketd/internal/shellenv"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
)

// NewWorktree is a worktree.create: a Worktree named Name from Base, or on
// Branch, or on pull request PR; at most one of the three.
type NewWorktree struct {
	Project, Name, Base, Branch string
	PR                          int
	// Setup runs the project's setup in a new Terminal of the Worktree; nil runs it.
	Setup *bool
}

type Made struct {
	Path, Branch string
	// Terminal runs the setup; "" when there was none to run.
	Terminal string
}

// Worktree makes a Worktree as agent.create's checkout.new does, with the
// same steps, copy and setup, but no agent. The setup runs in a login shell
// in a new Terminal there, which stays open once it ends; Worktree doesn't
// wait for it. A Worktree already on Branch is returned as it is.
func (l *Launcher) Worktree(w Who, n NewWorktree, progress func(step, note string)) (Made, *Failure) {
	progress(proto.StepPrepare, "")
	f := l.reg.Load()
	if !slices.Contains(f.Projects, n.Project) {
		return Made{}, fail("unknown_project", "This project isn't on your Mac anymore")
	}
	spec := proto.LaunchSpec{Project: n.Project, Checkout: proto.Checkout{New: &proto.NewWorktree{Name: n.Name, Base: n.Base, Branch: n.Branch, Setup: n.Setup}}}
	if n.PR != 0 {
		spec.Checkout.New.PR = strconv.Itoa(n.PR)
	}
	env := l.d.LoginEnv()
	path, setup, fl := l.checkout(w, f, spec, env, progress)
	if fl != nil {
		return Made{}, fl
	}
	if n.Branch == "" && n.PR == 0 && l.Names != nil {
		// A deleted Worktree's name would otherwise stick to the new one at its path.
		l.Names.Forget(path)
	}
	m := Made{Path: path, Branch: currentBranch(path)}
	if setup == "" {
		return m, nil
	}
	progress(proto.StepSetup, "")
	origin := "desktop"
	if !w.Owner {
		origin = "phone"
	}
	id := terminal.NewID()
	shell := shellOf(env)
	t, err := l.d.Terminals.Spawn(terminal.Spec{ID: id, Cmd: shell, Args: SetupArgs(shell, setup), Cwd: path,
		Env: worktree.ScriptEnv(l.d.Env(l.d.GhEnv(env, path), id, ""), n.Project, path), Cols: 100, Rows: 30, Origin: origin})
	if err != nil {
		progress(proto.StepSetup, "Couldn't start the setup: "+err.Error())
		return m, nil
	}
	m.Terminal = t.Info().ID
	return m, nil
}

// SetupArgs are shell's arguments that run setup, then stay a login shell
// whatever setup did, so its output can be read.
func SetupArgs(shell, setup string) []string {
	if shellenv.PowerShell(shell) {
		return []string{"-NoLogo", "-NoExit", "-Command", setup}
	}
	if filepath.Base(shell) == "fish" {
		return []string{"-l", "-c", "begin\n" + setup + "\nend\nexec " + quote(shell) + " -l"}
	}
	return []string{"-l", "-c", "(\n" + setup + "\n)\nexec " + quote(shell) + " -l"}
}

func currentBranch(path string) string {
	out, _ := exec.Command("git", "-C", path, "branch", "--show-current").Output()
	return strings.TrimSpace(string(out))
}
