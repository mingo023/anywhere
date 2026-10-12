package service

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"time"

	"pocketd/internal/atomicfile"
	"pocketd/internal/lock"
)

func plistPath(label string) (string, error) {
	user, err := os.UserHomeDir()
	return filepath.Join(user, "Library", "LaunchAgents", label+".plist"), err
}

// domains: the GUI session when the user is logged in at the screen, else the
// background one an SSH login gets.
func domains() []string {
	uid := os.Getuid()
	return []string{fmt.Sprintf("gui/%d", uid), fmt.Sprintf("user/%d", uid)}
}

// loadedIn is the domain launchd has label in, or ""; `launchctl print` exits 113 for one it doesn't know.
func loadedIn(label string) (string, error) {
	for _, d := range domains() {
		err := exec.Command("launchctl", "print", d+"/"+label).Run()
		var exit *exec.ExitError
		switch {
		case err == nil:
			return d, nil
		case errors.As(err, &exit) && exit.ExitCode() == 113:
		default:
			return "", err
		}
	}
	return "", nil
}

func launchctl(args ...string) error {
	if out, err := exec.Command("launchctl", args...).CombinedOutput(); err != nil {
		return fmt.Errorf("launchctl %s: %v: %s", args[0], err, bytes.TrimSpace(out))
	}
	return nil
}

func Install(s Spec) error {
	path, err := plistPath(s.Label)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(logPath(s)), 0o700); err != nil {
		return err
	}
	// launchd would create its log world-readable.
	f, err := os.OpenFile(logPath(s), os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		return err
	}
	f.Close()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	if err := atomicfile.Write(path, plist(s), 0o644); err != nil {
		return err
	}
	domain, err := loadedIn(s.Label)
	if err != nil {
		return err
	}
	if domain != "" {
		if _, running := lock.Holder(s.Home); running {
			return nil
		}
		// Nothing to stop: reload so launchd starts the plist just written.
		if err := launchctl("bootout", domain+"/"+s.Label); err != nil {
			return err
		}
	}
	for _, d := range domains() {
		if err = launchctl("bootstrap", d, path); err == nil {
			return nil
		}
	}
	return err
}

func Uninstall(s Spec) error {
	path, err := plistPath(s.Label)
	if err != nil {
		return err
	}
	domain, err := loadedIn(s.Label)
	if err != nil {
		return err
	}
	if domain != "" {
		if err := launchctl("bootout", domain+"/"+s.Label); err != nil {
			return err
		}
		// bootout can return while pocketd is still shutting down.
		for range 50 {
			if _, held := lock.Holder(s.Home); !held {
				break
			}
			time.Sleep(100 * time.Millisecond)
		}
	}
	if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
		return err
	}
	return nil
}

func Status(s Spec) (installed, running bool, err error) {
	path, err := plistPath(s.Label)
	if err != nil {
		return false, false, err
	}
	_, err = os.Stat(path)
	_, running = lock.Holder(s.Home)
	return err == nil, running, nil
}
