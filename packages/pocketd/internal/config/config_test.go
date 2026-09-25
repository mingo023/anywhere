package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestLoadCreatesPrivateConfigOnce(t *testing.T) {
	t.Setenv("POCKET_HOME", t.TempDir())
	first, err := Load()
	if err != nil || first.Port != 4517 || len(first.Token) != 32 {
		t.Fatalf("%+v %v", first, err)
	}
	st, _ := os.Stat(filepath.Join(Home(), "config.json"))
	if st.Mode().Perm() != 0o600 {
		t.Fatalf("mode %v", st.Mode())
	}
	again, _ := Load()
	if again != first {
		t.Fatal("token changed on second load")
	}
}

func TestLoadKeepsOldConfigWorking(t *testing.T) {
	t.Setenv("POCKET_HOME", t.TempDir())
	os.WriteFile(filepath.Join(Home(), "config.json"), []byte(`{"token":"t","port":1,"profiles":[]}`), 0o600)
	c, err := Load()
	if err != nil || c.Token != "t" || c.Port != 1 {
		t.Fatalf("%+v %v", c, err)
	}
}

func TestLoadRejectsBrokenConfig(t *testing.T) {
	t.Setenv("POCKET_HOME", t.TempDir())
	os.WriteFile(filepath.Join(Home(), "config.json"), []byte(`{"port":1}`), 0o600)
	if _, err := Load(); err == nil {
		t.Fatal("want error")
	}
}
