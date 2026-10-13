package service

import (
	"encoding/binary"
	"fmt"
	"os"
	"os/exec"
	"os/user"
	"path/filepath"
	"strings"
	"unicode/utf16"

	"pocketd/internal/lock"
)

// Install registers s as a scheduled task of the current user and starts it.
// A pocketd already running on s.Home keeps running.
func Install(s Spec) error {
	u, err := user.Current()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(s.Home, 0o700); err != nil {
		return err
	}
	path := filepath.Join(s.Home, "task.xml")
	if err := os.WriteFile(path, utf16File(taskXML(s, u.Username)), 0o600); err != nil {
		return err
	}
	defer os.Remove(path)
	if err := schtasks("/Create", "/TN", s.Label, "/XML", path, "/F"); err != nil {
		return err
	}
	return schtasks("/Run", "/TN", s.Label)
}

// Uninstall ends pocketd, and its terminals with it, then deletes the task.
func Uninstall(s Spec) error {
	if installed, _, err := Status(s); err != nil || !installed {
		return err
	}
	schtasks("/End", "/TN", s.Label)
	return schtasks("/Delete", "/TN", s.Label, "/F")
}

// Status reads running from home's lock, since schtasks prints its states
// in the system's language.
func Status(s Spec) (installed, running bool, err error) {
	_, running = lock.Holder(s.Home)
	err = exec.Command("schtasks", "/Query", "/TN", s.Label).Run()
	if _, ok := err.(*exec.ExitError); ok {
		return false, running, nil
	}
	return err == nil, running, err
}

func schtasks(args ...string) error {
	out, err := exec.Command("schtasks", args...).CombinedOutput()
	if err != nil {
		return fmt.Errorf("schtasks %s: %v: %s", args[0], err, strings.TrimSpace(string(out)))
	}
	return nil
}

// utf16File is s as UTF-16LE with a BOM: schtasks /XML rejects UTF-8.
func utf16File(s string) []byte {
	out := []byte{0xff, 0xfe}
	for _, u := range utf16.Encode([]rune(s)) {
		out = binary.LittleEndian.AppendUint16(out, u)
	}
	return out
}
