package hosts

import (
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"
)

func TestAnInstallAlreadyUnderWayIsRefused(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("the stand-in ssh is a shell script")
	}
	// The only PATH ssh is looked up on, so the real one never runs.
	bin := t.TempDir()
	release := filepath.Join(bin, "release")
	script := "#!/bin/sh\nwhile [ ! -e '" + release + "' ]; do /bin/sleep 0.05; done\necho 'ssh: connect to host devbox port 22: Connection refused' >&2\nexit 255\n"
	if err := os.WriteFile(filepath.Join(bin, "ssh"), []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	m, err := Open(Paths{Home: t.TempDir(), RemoteDir: ".cp"}, func() []string { return []string{"PATH=" + bin} }, "1.2.0", "Mac")
	if err != nil {
		t.Fatal(err)
	}
	devbox, err := m.Add(alias("devbox"), "")
	if err != nil {
		t.Fatal(err)
	}
	progress := make(chan Progress, 8)
	if err := m.Install(devbox.ID, false, "", func(p Progress) { progress <- p }); err != nil {
		t.Fatal(err)
	}
	if err := m.Install(devbox.ID, true, "", func(Progress) {}); !errors.Is(err, ErrBusy) {
		t.Fatalf("a second install: %v", err)
	}
	if err := os.WriteFile(release, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	for {
		select {
		case p := <-progress:
			if p.Kind != ProgressError {
				continue
			}
			if want := FailRefused.Message("devbox"); p.Text != want {
				t.Fatalf("%q, want %q", p.Text, want)
			}
			return
		case <-time.After(10 * time.Second):
			t.Fatal("the first install never ended")
		}
	}
}
