package hosts

import (
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"testing"
)

func TestHostsRoundTripThroughAFileOnlyTheUserReads(t *testing.T) {
	home := t.TempDir()
	path := filepath.Join(home, "state", "hosts.json")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("old"), 0o644); err != nil {
		t.Fatal(err)
	}
	devbox := New(alias("devbox"), nil)
	box := New(Target{Kind: KindAddress, Addr: "10.0.4.12", Port: 22, User: "u", KeyFile: "~/.ssh/k"}, []Host{devbox})
	box.Windows = true
	want := []Host{devbox, box}
	if err := Save(home, want); err != nil {
		t.Fatal(err)
	}
	got, err := Load(home)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("%+v %v", got, err)
	}
	if st, _ := os.Stat(path); runtime.GOOS != "windows" && st.Mode().Perm() != 0o600 {
		t.Fatalf("mode %v", st.Mode())
	}
}

func TestNoHostsFileMeansNoHosts(t *testing.T) {
	got, err := Load(t.TempDir())
	if len(got) != 0 || err != nil {
		t.Fatalf("%+v %v", got, err)
	}
}

func TestAHostsFileItCannotUseIsAnErrorAndStaysWhereItIs(t *testing.T) {
	home := t.TempDir()
	path := filepath.Join(home, "state", "hosts.json")
	os.MkdirAll(filepath.Dir(path), 0o700)
	for _, content := range []string{"{", `{"version":2,"hosts":[]}`} {
		os.WriteFile(path, []byte(content), 0o600)
		if _, err := Load(home); err == nil {
			t.Errorf("%s loaded", content)
		}
		if got, _ := os.ReadFile(path); string(got) != content {
			t.Errorf("%s became %s", content, got)
		}
	}
}

func TestHostsFromDesktopJsonAreImportedOnceAndTheFileIsLeftAlone(t *testing.T) {
	home := t.TempDir()
	// Saved by the desktop's Store before hosts moved to pocketd.
	sample, err := os.ReadFile("testdata/desktop.json")
	if err != nil {
		t.Fatal(err)
	}
	desktop := filepath.Join(home, "desktop.json")
	if err := os.WriteFile(desktop, sample, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := ImportDesktop(home); err != nil {
		t.Fatal(err)
	}
	want := []Host{
		{ID: "devbox", Name: "devbox", Target: alias("devbox")},
		{ID: "10-0-4-12", Name: "10.0.4.12", Target: Target{Kind: KindAddress, Addr: "10.0.4.12", Port: 2222, User: "dev", KeyFile: "~/.ssh/id_ed25519"}, Windows: true},
		{ID: "gpu-1", Name: "gpu-1", Target: Target{Kind: KindAddress, Addr: "gpu-1.lab.example.com", Port: 22}},
	}
	if got, err := Load(home); err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("imported %+v %v", got, err)
	}
	if err := Save(home, want[1:]); err != nil {
		t.Fatal(err)
	}
	if err := ImportDesktop(home); err != nil {
		t.Fatal(err)
	}
	if got, _ := Load(home); !reflect.DeepEqual(got, want[1:]) {
		t.Fatalf("a removed host came back: %+v", got)
	}
	if got, _ := os.ReadFile(desktop); string(got) != string(sample) {
		t.Fatal("desktop.json changed")
	}
}

func TestADesktopJsonItCannotReadImportsNothing(t *testing.T) {
	home := t.TempDir()
	os.WriteFile(filepath.Join(home, "desktop.json"), []byte("{"), 0o600)
	if err := ImportDesktop(home); err == nil {
		t.Fatal("imported")
	}
	if _, err := os.Stat(filepath.Join(home, "state", "hosts.json")); !os.IsNotExist(err) {
		t.Fatalf("hosts.json written: %v", err)
	}
}
