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
