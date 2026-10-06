package config

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func refuse(string) error {
	panic("a config without a token has nothing to adopt")
}

func TestLoadCreatesPrivateConfigOnce(t *testing.T) {
	t.Setenv("POCKET_HOME", t.TempDir())
	first, err := Load(refuse)
	if err != nil || first.Port != DefaultPort() || first.Listen != "auto" {
		t.Fatalf("%+v %v", first, err)
	}
	st, _ := os.Stat(filepath.Join(Home(), "config.json"))
	if st.Mode().Perm() != 0o600 {
		t.Fatalf("mode %v", st.Mode())
	}
	again, _ := Load(refuse)
	if again != first {
		t.Fatal("config changed on second load")
	}
}

func TestAConfigWithATokenMovesItIntoDevicesAndForgetsIt(t *testing.T) {
	t.Setenv("POCKET_HOME", t.TempDir())
	path := filepath.Join(Home(), "config.json")
	os.WriteFile(path, []byte(`{"token":"t","port":1,"profiles":[]}`), 0o600)
	var adopted []string
	adopt := func(tok string) error { adopted = append(adopted, tok); return nil }
	c, err := Load(adopt)
	if err != nil || c.Port != 1 || c.Listen != "auto" || len(adopted) != 1 || adopted[0] != "t" {
		t.Fatalf("%+v %v %v", c, err, adopted)
	}
	raw, _ := os.ReadFile(path)
	if strings.Contains(string(raw), "token") || !strings.Contains(string(raw), "profiles") {
		t.Fatalf("config.json = %s", raw)
	}
	if st, _ := os.Stat(path); st.Mode().Perm() != 0o600 {
		t.Fatalf("mode %v", st.Mode())
	}
	if _, err := Load(adopt); err != nil || len(adopted) != 1 {
		t.Fatalf("second load adopted again: %v %v", adopted, err)
	}
}

func TestLoadRejectsBrokenConfig(t *testing.T) {
	t.Setenv("POCKET_HOME", t.TempDir())
	os.WriteFile(filepath.Join(Home(), "config.json"), []byte(`{"token":"t"}`), 0o600)
	if _, err := Load(refuse); err == nil {
		t.Fatal("want error")
	}
}

func TestLoadRejectsAnUnknownListenMode(t *testing.T) {
	t.Setenv("POCKET_HOME", t.TempDir())
	keep := func(string) error { return nil }
	os.WriteFile(filepath.Join(Home(), "config.json"), []byte(`{"port":1,"listen":"0.0.0.0"}`), 0o600)
	if _, err := Load(keep); err == nil {
		t.Fatal("want error")
	}
	os.WriteFile(filepath.Join(Home(), "config.json"), []byte(`{"port":1,"listen":"loopback"}`), 0o600)
	if c, err := Load(keep); err != nil || c.Listen != "loopback" {
		t.Fatalf("%+v %v", c, err)
	}
}

func release(t *testing.T) {
	channel = "release"
	t.Cleanup(func() { channel = "" })
}

func TestADevBuildKeepsOutOfTheInstalledAppsHomeServiceAndPort(t *testing.T) {
	t.Setenv("POCKET_HOME", "")
	t.Setenv("HOME", "/h")
	if Release() || Home() != "/h/.coding-pocket-dev" || Label() != "dev.mingo.anywhere.dev.pocketd" || DefaultPort() != 4518 {
		t.Fatal(Home(), Label(), DefaultPort())
	}
}

func TestAReleaseBuildKeepsTheNamesInstalledCopiesUse(t *testing.T) {
	release(t)
	t.Setenv("POCKET_HOME", "")
	t.Setenv("HOME", "/h")
	if !Release() || Home() != "/h/.coding-pocket" || Label() != "dev.mingo.anywhere.pocketd" || DefaultPort() != 4517 {
		t.Fatal(Home(), Label(), DefaultPort())
	}
}
