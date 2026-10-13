package hostop

import (
	"cmp"
	"errors"
	"fmt"
	"io/fs"
	"net/url"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"time"
)

// Trash moves path where the OS's file manager would put it, or into
// fallback where there is no such place, and returns where it went.
func Trash(path, fallback string) (string, error) {
	home, err := os.UserHomeDir()
	switch {
	case err != nil:
		return "", err
	case runtime.GOOS == "darwin":
		return trashInto(path, filepath.Join(home, ".Trash"))
	case runtime.GOOS == "linux":
		return trashXDG(path, cmp.Or(os.Getenv("XDG_DATA_HOME"), filepath.Join(home, ".local", "share")))
	}
	return trashInto(path, fallback)
}

func trashInto(path, dir string) (string, error) {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return "", err
	}
	for n := 1; ; n++ {
		dst := filepath.Join(dir, numbered(filepath.Base(path), n))
		if _, err := os.Lstat(dst); errors.Is(err, fs.ErrNotExist) {
			return dst, os.Rename(path, dst)
		}
	}
}

// trashXDG follows the freedesktop.org Trash spec: the .trashinfo, made
// exclusively, claims the name before the file moves.
func trashXDG(path, dataHome string) (string, error) {
	trash := filepath.Join(dataHome, "Trash")
	for _, d := range []string{"files", "info"} {
		if err := os.MkdirAll(filepath.Join(trash, d), 0o700); err != nil {
			return "", err
		}
	}
	for n := 1; ; n++ {
		name := numbered(filepath.Base(path), n)
		info := filepath.Join(trash, "info", name+".trashinfo")
		f, err := os.OpenFile(info, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
		if errors.Is(err, fs.ErrExist) {
			continue
		}
		if err != nil {
			return "", err
		}
		dst := filepath.Join(trash, "files", name)
		if _, err := os.Lstat(dst); err == nil {
			f.Close()
			continue
		}
		_, err = fmt.Fprintf(f, "[Trash Info]\nPath=%s\nDeletionDate=%s\n", (&url.URL{Path: path}).EscapedPath(), time.Now().Format("2006-01-02T15:04:05"))
		if cerr := f.Close(); err == nil {
			err = cerr
		}
		if err == nil {
			err = os.Rename(path, dst)
		}
		if err != nil {
			os.Remove(info)
			return "", err
		}
		return dst, nil
	}
}

// numbered is name for n 1, else "a 2.txt" style.
func numbered(name string, n int) string {
	if n == 1 {
		return name
	}
	ext := filepath.Ext(name)
	if ext == name {
		ext = ""
	}
	return fmt.Sprintf("%s %d%s", strings.TrimSuffix(name, ext), n, ext)
}
