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
	h, ok := m.host(id)
	if !ok {
		return ErrUnknown
	}
	if f := m.signIn(h, password); f != "" {
		return &SSHError{f}
	}
	return m.Retry(id)
}

func (m *Manager) signIn(h Host, password string) Failure {
	dir := filepath.Join(m.paths.Home, "hosts")
	if os.MkdirAll(dir, 0o700) != nil {
		return FailOther
	}
	key := m.paths.Key(h.ID)
	if _, err := os.Stat(key); err != nil && !m.keygen(key) {
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
	stderr, ok := m.passwordSignIn(h, askpass, password, public, methodPassword)
	if !ok && !takesPasswords(stderr) {
		stderr, ok = m.passwordSignIn(h, askpass, password, public, methodKeyboardInteractive)
	}
	if ok {
		return ""
	}
	return classify(stderr)
}

// passwordSignIn runs the key install over one sign-in with method; ssh's stderr when it fails.
func (m *Manager) passwordSignIn(h Host, askpass, password string, public []byte, method passwordMethod) (string, bool) {
	ctx, cancel := context.WithTimeout(context.Background(), signInTimeout)
	defer cancel()
	cmd, err := m.command(ctx, passwordArgs(h, m.paths.KnownHosts(h.ID), method, installKey))
	if err != nil {
		return err.Error(), false
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
		return "Operation timed out", false
	}
	return stderr.String(), err == nil
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

func (m *Manager) keygen(key string) bool {
	code, _, _, err := hostop.Run(context.Background(), m.env(), "", []string{"ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-C", "anywhere", "-f", key}, nil, 0)
	return err == nil && code == 0
}
