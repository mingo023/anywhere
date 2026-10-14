package launch

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"time"

	"pocketd/internal/ops"
	"pocketd/internal/shellenv"
)

// Accounts answers an accounts.* op; Key is the provider and ID the account.
func (l *Launcher) Accounts(ctx context.Context, m ops.Msg) ops.Msg {
	s := l.d.Accounts
	if s == nil {
		return ops.Msg{Ev: "error", Error: "accounts unsupported"}
	}
	var err error
	switch m.Op {
	case "accounts.usage":
		s.Refresh(ctx, m.ID)
	case "accounts.signin":
		return l.signIn(m.Key, m.ID)
	case "accounts.rename":
		err = s.Rename(m.Key, m.ID, m.Text)
	case "accounts.default":
		if err = s.SetDefault(m.Key, m.ID); err == nil {
			go l.d.Follow(m.Key)
		}
	case "accounts.signout":
		var id string
		if id, err = s.Resolve(m.Key, m.ID); err == nil {
			l.signOut(m.Key, id)
		}
	case "accounts.remove":
		if !s.Dismiss(m.ID) {
			err = s.Remove(m.Key, m.ID, l.d.Running(m.Key, m.ID), func() { l.signOut(m.Key, m.ID) })
		}
	}
	if err != nil {
		return ops.Msg{Ev: "error", Error: err.Error()}
	}
	return ops.Msg{Ev: "accounts", Accounts: s.List(l.d.Running)}
}

// signIn opens a Terminal running provider's login for account id, or for a
// new account when id is "". The account is kept once the Terminal ends.
func (l *Launcher) signIn(provider, id string) ops.Msg {
	s := l.d.Accounts
	env := l.d.LoginEnv()
	exe, ok := l.find(provider, env)
	if !ok {
		return ops.Msg{Ev: "error", Error: provider + " isn't installed on this machine"}
	}
	var err error
	if id == "" {
		id, err = s.Add(provider)
	} else {
		_, err = s.Resolve(provider, id)
	}
	if err != nil {
		return ops.Msg{Ev: "error", Error: err.Error()}
	}
	home, _ := os.UserHomeDir()
	account := s.Env(provider, id)
	argv := inAccount(shellenv.LoginShell(), loginArgv(provider, exe), account)
	t, err := l.d.Spawn(ops.Msg{Cmd: argv[0], Args: argv[1:], Cwd: home, Env: slices.Concat(env, account), Cols: 100, Rows: 30})
	if err != nil {
		s.Finish(provider, id, func() {})
		return ops.Msg{Ev: "error", Error: err.Error()}
	}
	go func() {
		<-t.Done()
		s.Finish(provider, id, func() { l.signOut(provider, id) })
	}()
	return ops.Msg{Ev: "spawned", ID: t.Info().ID, Text: id}
}

func loginArgv(provider, exe string) []string {
	if provider == "codex" {
		// A device code works on a host whose browser is elsewhere.
		return []string{exe, "login", "--device-auth"}
	}
	return []string{exe, "auth", "login"}
}

// signOut runs provider's own logout on account id, which also drops its Keychain item.
func (l *Launcher) signOut(provider, id string) {
	env := l.d.LoginEnv()
	exe, ok := l.find(provider, env)
	if !ok {
		return
	}
	argv := []string{exe, "logout"}
	if provider == "claude" {
		argv = []string{exe, "auth", "logout"}
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	account := l.d.Accounts.Env(provider, id)
	argv = inAccount(shellenv.LoginShell(), argv, account)
	cmd := exec.CommandContext(ctx, argv[0], argv[1:]...)
	cmd.Env = slices.Concat(env, account)
	cmd.Run()
}

// inAccount is InShell with account's env set by the shell itself, after
// the rc files it reads, which could point the CLI at another folder.
func inAccount(shell string, argv, account []string) []string {
	pre := preamble(shell, account)
	if shellenv.PowerShell(shell) {
		return []string{shell, "-NoLogo", "-Command", pre + shellenv.PSCommand(argv)}
	}
	if filepath.Base(shell) == "fish" {
		return append([]string{shell, "-c", pre + "$argv"}, argv...)
	}
	return append([]string{shell, "-c", pre + `"$@"`, shell}, argv...)
}
