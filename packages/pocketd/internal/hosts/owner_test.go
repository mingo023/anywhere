package hosts

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func TestTheOwnerTokenFileIsOnlyTheUsers(t *testing.T) {
	path := filepath.Join(t.TempDir(), "hosts", "pc.token")
	os.MkdirAll(filepath.Dir(path), 0o755)
	os.WriteFile(path, []byte("old"), 0o644)
	owner := Owner{Token: "t1", Port: 4519}
	if err := saveOwner(path, owner); err != nil {
		t.Fatal(err)
	}
	if st, _ := os.Stat(path); runtime.GOOS != "windows" && st.Mode().Perm() != 0o600 {
		t.Fatalf("mode %v", st.Mode())
	}
	if got, ok := loadOwner(path); !ok || got != owner {
		t.Fatalf("%+v %v", got, ok)
	}
}

func TestAnOwnerFileWithoutATokenAndPortIsNotAnOwner(t *testing.T) {
	path := filepath.Join(t.TempDir(), "pc.token")
	for _, content := range []string{"", "{", `{}`, `{"token":"t"}`, `{"port":4519}`} {
		os.WriteFile(path, []byte(content), 0o600)
		if got, ok := loadOwner(path); ok {
			t.Errorf("%q loaded as %+v", content, got)
		}
	}
	if _, ok := loadOwner(filepath.Join(t.TempDir(), "none")); ok {
		t.Error("a missing file loaded")
	}
}

func TestPairReadsTheTokenAndPortPastAnyLoginNoise(t *testing.T) {
	out := "Welcome to the box\r\n{\"token\":\"abc\",\"id\":\"45bd\",\"port\":4519}\r\n"
	if got, ok := parsePair(out); !ok || got != (Owner{Token: "abc", Port: 4519}) {
		t.Fatalf("%+v %v", got, ok)
	}
	for _, noise := range []string{"ownerListen is off; set it in config.json\n", `{"error":"no owner"}` + "\n", ""} {
		if got, ok := parsePair(noise); ok {
			t.Errorf("%q paired as %+v", noise, got)
		}
	}
}

func TestThePairCommandNamesThisMacInAsciiForCmd(t *testing.T) {
	got := decoded(t, pairCommand(".coding-pocket", "Mingo’s é \"Mac\""))
	want := `& (Join-Path $HOME '.coding-pocket\bin\pocketd.cmd') pair --json --name 'Mingo''s ? ?Mac?'; exit $LASTEXITCODE`
	if got != want {
		t.Fatalf("%s", got)
	}
}
