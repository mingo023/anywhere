// Package launchagent installs pocketd as a per-user launchd service.
package launchagent

import (
	"bytes"
	"encoding/xml"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

	"pocketd/internal/atomicfile"
)

const Label = "dev.mingo.pocket.pocketd"

type Spec struct{ Exe, LogPath string }

// KeepAlive restarts only a failed exit, so a clean stop stays stopped.
// Interactive keeps launchd from throttling the PTYs' CPU and I/O.
const plist = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>%[1]s</string>
	<key>ProgramArguments</key>
	<array>
		<string>%[2]s</string>
		<string>serve</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<dict>
		<key>SuccessfulExit</key>
		<false/>
	</dict>
	<key>ThrottleInterval</key>
	<integer>30</integer>
	<key>ProcessType</key>
	<string>Interactive</string>
	<key>StandardOutPath</key>
	<string>%[3]s</string>
	<key>StandardErrorPath</key>
	<string>%[3]s</string>
</dict>
</plist>
`

func escape(s string) string {
	var b bytes.Buffer
	xml.EscapeText(&b, []byte(s))
	return b.String()
}

func Plist(s Spec) []byte {
	return fmt.Appendf(nil, plist, Label, escape(s.Exe), escape(s.LogPath))
}

// Path is where the plist lives for the user whose home directory is home.
func Path(home string) string {
	return filepath.Join(home, "Library", "LaunchAgents", Label+".plist")
}

func target(uid int) string { return fmt.Sprintf("gui/%d/%s", uid, Label) }

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

// Uninstall stops the service and removes its plist.
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
