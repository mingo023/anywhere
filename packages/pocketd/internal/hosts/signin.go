package hosts

import (
	"bytes"
	"context"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	"pocketd/internal/hostop"
)

// passwordVar is where askpass reads the password: an inherited fd doesn't reach it, and argv shows in ps.
const passwordVar = "POCKET_SSH_PW"

// signInTimeout bounds what ssh's ConnectTimeout doesn't: a server that stops answering mid sign-in.
const signInTimeout = 30 * time.Second

// SignIn puts the app's own key in the host's authorized_keys over one password sign-in, then reconnects with it.
// The host's key must already be trusted, so the password never goes to a host nobody checked. The password
// lives only in this call and in ssh's environment.
func (m *Manager) SignIn(id, password string) error {
	m.mu.Lock()
	h, ok := m.hostLocked(id)
	var ctx context.Context
	var done func()
	if ok {
		ctx, done = m.begin(id)
	}
	m.mu.Unlock()
	if !ok {
		return ErrUnknown
	}
	f := m.signIn(ctx, h, password)
	done()
	if f != "" {
		return &SSHError{f}
	}
	return m.Retry(id)
}

func (m *Manager) signIn(ctx context.Context, h Host, password string) Failure {
	dir := filepath.Join(m.paths.Home, "hosts")
	if os.MkdirAll(dir, 0o700) != nil {
		return FailOther
	}
	key := m.paths.Key(h.ID)
	if _, err := os.Stat(key); err != nil && !m.keygen(ctx, key) {
		return FailOther
	}
	public, err := os.ReadFile(key + ".pub")
	if err != nil {
		return FailOther
	}
	askpass, err := writeAskpass(dir)
	if err != nil {
		return FailOther
	}
	install := installKey
	if h.Windows {
		install = windowsInstallKey
	}
	method := methodPassword
	stderr, code := m.passwordSignIn(ctx, h, askpass, password, public, method, install)
	if code != 0 && !takesPasswords(stderr) {
		method = methodKeyboardInteractive
		stderr, code = m.passwordSignIn(ctx, h, askpass, password, public, method, install)
	}
	if code == 0 {
		return ""
	}
	// ssh exits 255 only when it fails itself, so the password worked and sh didn't: maybe a Windows host not yet probed.
	if code > 0 && code != 255 && install == installKey {
		if _, code = m.passwordSignIn(ctx, h, askpass, password, public, method, windowsInstallKey); code == 0 {
			return ""
		}
	}
	return classify(stderr)
}

// passwordSignIn runs install over one sign-in with method: ssh's stderr and exit code, or -1 when ssh didn't finish.
func (m *Manager) passwordSignIn(ctx context.Context, h Host, askpass, password string, public []byte, method passwordMethod, install string) (string, int) {
	ctx, cancel := context.WithTimeout(ctx, signInTimeout)
	defer cancel()
	cmd, err := m.command(ctx, passwordArgs(h, m.paths.KnownHosts(h.ID), method, install))
	if err != nil {
		return err.Error(), -1
	}
	cmd.Env = append(cmd.Env, "SSH_ASKPASS="+askpass,
		// Askpass even when pocketd was started from a terminal, whose tty ssh would otherwise prompt on.
		"SSH_ASKPASS_REQUIRE=force",
		passwordVar+"="+password)
	ownGroup(cmd)
	cmd.Stdin = bytes.NewReader(public)
	var stderr strings.Builder
	cmd.Stderr = &stderr
	err = cmd.Run()
	if ctx.Err() != nil {
		// ssh's own wording, so classify calls it timed_out.
		return "Operation timed out", -1
	}
	if cmd.ProcessState == nil {
		return stderr.String(), -1
	}
	return stderr.String(), cmd.ProcessState.ExitCode()
}

func writeAskpass(dir string) (string, error) {
	if runtime.GOOS == "windows" {
		// Delayed expansion reads the password after cmd has parsed the line, so its & or | stay text.
		path := filepath.Join(dir, "askpass.cmd")
		return path, os.WriteFile(path, []byte("@setlocal EnableDelayedExpansion\r\n@echo(!"+passwordVar+"!\r\n"), 0o600)
	}
	path := filepath.Join(dir, "askpass")
	if err := os.WriteFile(path, []byte("#!/bin/sh\nprintf '%s\\n' \"$"+passwordVar+"\"\n"), 0o700); err != nil {
		return "", err
	}
	return path, os.Chmod(path, 0o700)
}

func (m *Manager) keygen(ctx context.Context, key string) bool {
	code, _, _, err := hostop.Run(ctx, m.env(), "", []string{"ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-C", "anywhere", "-f", key}, nil, 0)
	return err == nil && code == 0
}
