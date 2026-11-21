package config

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"time"
)

/**
 * MoveLegacyHome moves the legacy home into HomeName when home is it, and returns the home to use from then on.
 * Call it holding home's lock. It leaves a symlink at the legacy path, since running agents, holders and host
 * services hold paths under it. If moving fails it puts back what it moved and returns home with the error; once
 * everything has moved it returns the new home, with any error from replacing the legacy folder by the symlink.
 * Called with the new home, it makes the symlink a crash left unmade.
 * Windows keeps its legacy home: a symlink needs Developer Mode, and the held lock file can't be renamed.
 */
func MoveLegacyHome(home string) (string, error) {
	user, _ := os.UserHomeDir()
	to := filepath.Join(user, HomeName())
	if runtime.GOOS == "windows" {
		return home, nil
	}
	if filepath.Clean(home) == to {
		return home, relink(legacyHome(), to)
	}
	if filepath.Clean(home) != legacyHome() {
		return home, nil
	}
	moved, err := migrate(home, to)
	if !moved {
		return home, err
	}
	return to, err
}

/**
 * migrate moves legacy's entries into home, then replaces legacy with a symlink to home; moved says whether
 * everything is in home.
 * A rerun after a crash at any point finishes the move: what was moved is gone from legacy, and a file in both
 * places keeps legacy's copy, which is the one clients used while legacy resolved as the home.
 */
func migrate(legacy, home string) (moved bool, err error) {
	if fi, err := os.Lstat(legacy); err != nil || !fi.IsDir() {
		return false, nil
	}
	if _, err := os.Lstat(home); errors.Is(err, fs.ErrNotExist) {
		if err := os.MkdirAll(filepath.Dir(home), 0o700); err != nil {
			return false, err
		}
		if err := os.WriteFile(filepath.Join(legacy, moving), nil, 0o600); err != nil {
			return false, err
		}
		if err := os.Rename(legacy, home); err != nil {
			os.Remove(filepath.Join(legacy, moving))
			return false, err
		}
		return true, link(legacy, home)
	}
	// Stale: its listener died with the process that bound it, and the next one binds at the new path.
	os.Remove(filepath.Join(legacy, "pocketd.sock"))
	list, err := os.ReadDir(legacy)
	if err != nil {
		return false, err
	}
	for _, e := range list {
		if fi, err := os.Lstat(filepath.Join(home, e.Name())); err == nil && fi.IsDir() {
			return false, fmt.Errorf("%s is in both %s and %s", e.Name(), legacy, home)
		}
	}
	// Clients read these and a second pocketd takes the lock at the legacy path until it's a symlink.
	last := []string{"desktop.json", "config.json", "pocketd.lock"}
	slices.SortStableFunc(list, func(a, b fs.DirEntry) int { return slices.Index(last, a.Name()) - slices.Index(last, b.Name()) })
	if err := os.WriteFile(filepath.Join(home, moving), nil, 0o600); err != nil {
		return false, err
	}
	var done []string
	for _, e := range list {
		if err := os.Rename(filepath.Join(legacy, e.Name()), filepath.Join(home, e.Name())); err != nil {
			for _, name := range slices.Backward(done) {
				os.Rename(filepath.Join(home, name), filepath.Join(legacy, name))
			}
			os.Remove(filepath.Join(home, moving))
			return false, err
		}
		done = append(done, e.Name())
	}
	return true, replace(legacy, home)
}

// replace swaps the emptied legacy folder for a symlink to home. A client that resolved legacy as the home may
// have written there since it was listed.
func replace(legacy, home string) error {
	if late, err := os.ReadDir(legacy); err == nil {
		for _, e := range late {
			os.Rename(filepath.Join(legacy, e.Name()), filepath.Join(home, e.Name()))
		}
	}
	if err := os.Remove(legacy); err != nil {
		// Keep what wouldn't move inside home: a legacy folder left in place would stay the home.
		if err := os.Rename(legacy, filepath.Join(home, fmt.Sprintf("legacy-%d", time.Now().UnixNano()))); err != nil {
			return err
		}
	}
	return link(legacy, home)
}

// moving marks a home whose legacy folder is gone but may not be a symlink yet.
const moving = ".moving-from-legacy"

func link(legacy, home string) error {
	if err := os.Symlink(home, legacy); err != nil {
		return err
	}
	if err := os.Remove(filepath.Join(home, moving)); !errors.Is(err, fs.ErrNotExist) {
		return err
	}
	return nil
}

// relink finishes a move that stopped between taking legacy's folder away and putting the symlink there.
func relink(legacy, home string) error {
	if _, err := os.Stat(filepath.Join(home, moving)); err != nil {
		return nil
	}
	if _, err := os.Lstat(legacy); errors.Is(err, fs.ErrNotExist) {
		return link(legacy, home)
	}
	return os.Remove(filepath.Join(home, moving))
}
