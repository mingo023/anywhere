//go:build unix

package lock

import (
	"errors"
	"testing"
)

func TestALockKeptForExecStaysHeldAfterTheOriginalCloses(t *testing.T) {
	home := t.TempDir()
	l, err := Acquire(home)
	if err != nil {
		t.Fatal(err)
	}
	fd, err := l.Keep()
	if err != nil {
		t.Fatal(err)
	}
	l.Close()
	if _, err := Acquire(home); !errors.As(err, &ErrRunning{}) {
		t.Fatalf("took a lock an exec kept: %v", err)
	}
	Adopt(fd, home).Close()
	again, err := Acquire(home)
	if err != nil {
		t.Fatalf("the lock stayed after its kept fd closed: %v", err)
	}
	again.Close()
}
