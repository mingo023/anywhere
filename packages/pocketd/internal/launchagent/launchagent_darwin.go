package launchagent

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

	"pocketd/internal/atomicfile"
	"pocketd/internal/config"
)

func target(uid int) string { return fmt.Sprintf("gui/%d/%s", uid, config.Label()) }

// Loaded asks launchd; `launchctl print` exits 113 for a label it doesn't know.
func Loaded(uid int) (bool, error) {
	err := exec.Command("launchctl", "print", target(uid)).Run()
	var exit *exec.ExitError
	switch {
	case err == nil:
		return true, nil
	case errors.As(err, &exit) && exit.ExitCode() == 113:
		return false, nil
	}
	return false, err
}

// Service is "loaded", "installed" (plist only) or "none".
func Service(uid int, path string) string {
	if ok, _ := Loaded(uid); ok {
		return "loaded"
	}
	if _, err := os.Stat(path); err == nil {
		return "installed"
	}
	return "none"
}

// Install writes the plist and bootstraps it unless launchd already has the
// label. A loaded service is never booted out: that would close every Terminal.
func Install(s Spec, path string, uid int) (loaded bool, err error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return false, err
	}
	if err := atomicfile.Write(path, Plist(s), 0o644); err != nil {
		return false, err
	}
	if loaded, err = Loaded(uid); err != nil || loaded {
		return loaded, err
	}
	if out, err := exec.Command("launchctl", "bootstrap", fmt.Sprintf("gui/%d", uid), path).CombinedOutput(); err != nil {
		return false, fmt.Errorf("launchctl bootstrap: %v: %s", err, bytes.TrimSpace(out))
	}
	return false, nil
}

func Uninstall(path string, uid int) error {
	loaded, err := Loaded(uid)
	if err != nil {
		return err
	}
	if loaded {
		if out, err := exec.Command("launchctl", "bootout", target(uid)).CombinedOutput(); err != nil {
			return fmt.Errorf("launchctl bootout: %v: %s", err, bytes.TrimSpace(out))
		}
	}
	if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
		return err
	}
	return nil
}
