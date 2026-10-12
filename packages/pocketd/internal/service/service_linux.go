package service

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

	"pocketd/internal/atomicfile"
	"pocketd/internal/lock"
)

func unitPath(label string) (string, error) {
	dir, err := os.UserConfigDir()
	return filepath.Join(dir, "systemd", "user", label+".service"), err
}

func systemctl(args ...string) error {
	cmd := exec.Command("systemctl", append([]string{"--user"}, args...)...)
	// Some non-interactive logins leave it unset, and systemctl --user can't find the user manager without it.
	if os.Getenv("XDG_RUNTIME_DIR") == "" {
		cmd.Env = append(os.Environ(), fmt.Sprintf("XDG_RUNTIME_DIR=/run/user/%d", os.Getuid()))
	}
	if out, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("systemctl --user %s: %v: %s", args[0], err, bytes.TrimSpace(out))
	}
	return nil
}

// Install leaves an active unit running: enable --now only starts a stopped one.
func Install(s Spec) error {
	path, err := unitPath(s.Label)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	if err := atomicfile.Write(path, unit(s), 0o644); err != nil {
		return err
	}
	if err := systemctl("daemon-reload"); err != nil {
		return err
	}
	if err := systemctl("enable", "--now", s.Label+".service"); err != nil {
		return err
	}
	// Without linger the unit stops at logout. Some distros need root for it; the caller reports that.
	exec.Command("loginctl", "enable-linger").Run()
	return nil
}

func Uninstall(s Spec) error {
	path, err := unitPath(s.Label)
	if err != nil {
		return err
	}
	if _, err := os.Stat(path); os.IsNotExist(err) {
		return nil
	}
	if err := systemctl("disable", "--now", s.Label+".service"); err != nil {
		return err
	}
	if err := os.Remove(path); err != nil {
		return err
	}
	return systemctl("daemon-reload")
}

func Status(s Spec) (installed, running bool, err error) {
	path, err := unitPath(s.Label)
	if err != nil {
		return false, false, err
	}
	_, err = os.Stat(path)
	_, running = lock.Holder(s.Home)
	return err == nil, running, nil
}
