package lock

import (
	"errors"
	"os"
	"testing"
)

func TestSecondAcquireReportsTheHolderPid(t *testing.T) {
	home := t.TempDir()
	l, err := Acquire(home)
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	_, err = Acquire(home)
	var running ErrRunning
	if !errors.As(err, &running) || running.PID != os.Getpid() || running.Home != home {
		t.Fatalf("%v", err)
	}
	if pid, ok := Holder(home); !ok || pid != os.Getpid() {
		t.Fatalf("holder %d %v", pid, ok)
	}
}

func TestLockIsFreeAfterTheHolderCloses(t *testing.T) {
	home := t.TempDir()
	l, err := Acquire(home)
	if err != nil {
		t.Fatal(err)
	}
	l.Close()
	if _, ok := Holder(home); ok {
		t.Fatal("a closed lock still has a holder")
	}
	l, err = Acquire(home)
	if err != nil {
		t.Fatal(err)
	}
	l.Close()
}
