package service

import (
	"os"
	"path/filepath"
	"testing"
)

func TestRetireRemovesOnlyAServiceRunningTheHomesOwnPocketd(t *testing.T) {
	user := t.TempDir()
	t.Setenv("HOME", user)
	real := filepath.Join(user, ".anywhere")
	legacy := filepath.Join(user, ".coding-pocket")
	if err := os.MkdirAll(filepath.Join(real, "bin"), 0o700); err != nil {
		t.Fatal(err)
	}
	os.WriteFile(filepath.Join(real, "bin", "pocketd"), []byte("#!/bin/sh\n"), 0o755)
	if err := os.Symlink(real, legacy); err != nil {
		t.Fatal(err)
	}
	const label = "dev.mingo.anywhere.test.retire.pocketd"
	for name, c := range map[string]struct {
		exe     string
		retired bool
	}{
		"the home's bin/pocketd, named through the legacy home": {filepath.Join(legacy, "bin", "pocketd"), true},
		"a pocketd elsewhere": {filepath.Join(user, "other", "pocketd"), false},
	} {
		path, _ := plistPath(label)
		os.MkdirAll(filepath.Dir(path), 0o755)
		if err := os.WriteFile(path, plist(Spec{Exe: c.exe, Home: legacy, Label: label}), 0o644); err != nil {
			t.Fatal(err)
		}
		retired, err := Retire(label, real)
		_, stat := os.Stat(path)
		if err != nil || retired != c.retired || os.IsNotExist(stat) != c.retired {
			t.Errorf("%s: retired %v, err %v, plist left %v", name, retired, err, stat == nil)
		}
		os.Remove(path)
	}
}
