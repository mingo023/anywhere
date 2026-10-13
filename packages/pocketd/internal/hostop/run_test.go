package hostop

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestRunGivesTheExitCodeAndBothOutputs(t *testing.T) {
	code, out, errOut, err := Run(context.Background(), os.Environ(), t.TempDir(), []string{"sh", "-c", "cat; echo oops >&2; exit 3"}, []byte("hi"), 0)
	if err != nil || code != 3 || string(out) != "hi" || string(errOut) != "oops\n" {
		t.Fatalf("%d %q %q %v", code, out, errOut, err)
	}
}

func TestRunRunsInCwd(t *testing.T) {
	dir, _ := filepath.EvalSymlinks(t.TempDir())
	_, out, _, err := Run(context.Background(), os.Environ(), dir, []string{"pwd"}, nil, 0)
	if err != nil || strings.TrimSpace(string(out)) != dir {
		t.Fatalf("%q %v", out, err)
	}
}

func TestRunFindsTheProgramOnTheGivenPath(t *testing.T) {
	bin := t.TempDir()
	os.WriteFile(filepath.Join(bin, "hello"), []byte("#!/bin/sh\necho from-path\n"), 0o755)
	env := append(os.Environ(), "PATH="+bin+":/usr/bin:/bin")
	_, out, _, err := Run(context.Background(), env, bin, []string{"hello"}, nil, 0)
	if err != nil || string(out) != "from-path\n" {
		t.Fatalf("%q %v", out, err)
	}
}

func TestRunGivesMinusOneWhenTheTimeoutKillsIt(t *testing.T) {
	start := time.Now()
	code, _, _, err := Run(context.Background(), os.Environ(), t.TempDir(), []string{"sleep", "10"}, nil, 100*time.Millisecond)
	if err != nil || code != -1 || time.Since(start) > 5*time.Second {
		t.Fatalf("%d %v after %s", code, err, time.Since(start))
	}
}

func TestRunRefusesAMissingProgram(t *testing.T) {
	if _, _, _, err := Run(context.Background(), os.Environ(), t.TempDir(), []string{"no-such-program-xyz"}, nil, 0); err == nil {
		t.Fatal("ran")
	}
	if _, _, _, err := Run(context.Background(), os.Environ(), t.TempDir(), nil, nil, 0); err == nil {
		t.Fatal("ran no argv")
	}
}

func TestCappedKeepsTheFirstBytesAndSwallowsTheRest(t *testing.T) {
	c := capped{limit: 5}
	for _, s := range []string{"abc", "defg", "hij"} {
		if n, err := c.Write([]byte(s)); n != len(s) || err != nil {
			t.Fatalf("%d %v", n, err)
		}
	}
	if string(c.b) != "abcde" {
		t.Fatalf("%q", c.b)
	}
}
