package workspace

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"

	"pocketd/internal/accounts"
)

func TestASignedInAccountWithNoEmailIsNamedSignedIn(t *testing.T) {
	for want, a := range map[string]accounts.Account{
		"Signed in":     {SignedIn: true},
		"Not signed in": {},
		"a@x.com":       {SignedIn: true, Email: "a@x.com"},
		"Work":          {Label: "Work", Email: "a@x.com"},
	} {
		if got := name(a); got != want {
			t.Errorf("%+v: %q", a, got)
		}
	}
}

func TestAUsageReadSlowerThanTheListsWaitNeitherHoldsTheListNorIsLost(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("needs /bin/sh")
	}
	dir := t.TempDir()
	agy := filepath.Join(dir, "agy")
	report, err := filepath.Abs(filepath.Join("..", "accounts", "testdata", "agy-usage.json"))
	if err != nil {
		t.Fatal(err)
	}
	script := "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 1.5.16; exit 0; fi\nsleep 1\ncat '" + report + "'\n"
	if err := os.WriteFile(agy, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	s := &accounts.Store{Root: t.TempDir(), Home: t.TempDir()}
	s.Agy = func() (string, []string, bool) { return agy, []string{"PATH=/usr/bin:/bin"}, true }
	l := &AccountList{Store: s, Running: func(string, string) int { return 0 }}
	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()
	start := time.Now()
	l.List(ctx, "antigravity")
	if waited := time.Since(start); waited > 700*time.Millisecond {
		t.Fatalf("the list waited %v", waited)
	}
	for deadline := time.Now().Add(5 * time.Second); time.Now().Before(deadline); time.Sleep(50 * time.Millisecond) {
		if got := l.List(context.Background(), "antigravity"); len(got) == 1 && got[0].Week != nil {
			return
		}
	}
	t.Fatal("the usage read never landed")
}

func TestASlowLookupForAgyDoesNotHoldTheList(t *testing.T) {
	s := &accounts.Store{Root: t.TempDir(), Home: t.TempDir()}
	s.Agy = func() (string, []string, bool) {
		time.Sleep(time.Second)
		return "", nil, false
	}
	l := &AccountList{Store: s, Running: func(string, string) int { return 0 }}
	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()
	start := time.Now()
	l.List(ctx, "")
	if waited := time.Since(start); waited > 700*time.Millisecond {
		t.Fatalf("the list waited %v", waited)
	}
}
