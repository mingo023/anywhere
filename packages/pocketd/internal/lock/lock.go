// Package lock keeps one pocketd per home.
package lock

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
)

type Lock struct{ f *os.File }

type ErrRunning struct {
	PID  int
	Home string
}

func (e ErrRunning) Error() string {
	return fmt.Sprintf("pocketd is already running (pid %d) on %s. Stop it first, or use it.", e.PID, e.Home)
}

func path(home string) string { return filepath.Join(home, "pocketd.lock") }

// Acquire holds home's lock until Close or exit; the kernel drops it when the process dies.
func Acquire(home string) (*Lock, error) {
	if err := os.MkdirAll(home, 0o700); err != nil {
		return nil, err
	}
	f, err := os.OpenFile(path(home), os.O_RDWR|os.O_CREATE, 0o600)
	if err != nil {
		return nil, err
	}
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		f.Close()
		if errors.Is(err, syscall.EWOULDBLOCK) {
			pid, _ := Holder(home)
			return nil, ErrRunning{PID: pid, Home: home}
		}
		return nil, err
	}
	if err := f.Truncate(0); err != nil {
		f.Close()
		return nil, err
	}
	if _, err := f.WriteAt([]byte(strconv.Itoa(os.Getpid())+"\n"), 0); err != nil {
		f.Close()
		return nil, err
	}
	return &Lock{f}, nil
}

func (l *Lock) Close() error { return l.f.Close() }

// Holder is the pid of the pocketd holding home's lock, if one does.
func Holder(home string) (int, bool) {
	f, err := os.Open(path(home))
	if err != nil {
		return 0, false
	}
	defer f.Close()
	if syscall.Flock(int(f.Fd()), syscall.LOCK_SH|syscall.LOCK_NB) == nil {
		return 0, false
	}
	raw, err := os.ReadFile(path(home))
	if err != nil {
		return 0, false
	}
	pid, err := strconv.Atoi(strings.TrimSpace(string(raw)))
	return pid, err == nil && pid > 0
}
