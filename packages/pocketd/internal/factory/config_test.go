package factory

import (
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"pocketd/internal/proto"
)

func on(slots int) proto.FactoryConfig {
	c := proto.DefaultFactoryConfig()
	c.Enabled, c.Slots = true, slots
	return c
}

func TestAProjectWithNothingSavedHasTheDefaults(t *testing.T) {
	c := OpenConfigs(t.TempDir())
	if got := c.Get("/app"); got.Enabled || !reflect.DeepEqual(got, proto.DefaultFactoryConfig()) {
		t.Fatalf("got %+v", got)
	}
}

func TestSavedSettingsSurviveARestartAndStayWithTheirProject(t *testing.T) {
	home := t.TempDir()
	want := on(2)
	want.Checks = []string{"go test ./..."}
	if err := OpenConfigs(home).Save("/app", want); err != nil {
		t.Fatal(err)
	}
	again := OpenConfigs(home)
	if got := again.Get("/app"); !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
	if got := again.Get("/other"); !reflect.DeepEqual(got, proto.DefaultFactoryConfig()) {
		t.Fatalf("another project took the settings: %+v", got)
	}
}

func TestSettingsOutOfRangeAreRefusedAndNothingIsSaved(t *testing.T) {
	home := t.TempDir()
	c := OpenConfigs(home)
	if err := c.Save("/app", on(proto.MaxBoardLimit+1)); !errors.Is(err, ErrInvalid) {
		t.Fatalf("got %v", err)
	}
	if err := c.Save("", on(1)); !errors.Is(err, ErrInvalid) {
		t.Fatalf("a config with no project: %v", err)
	}
	if c.Get("/app").Enabled {
		t.Fatal("the refused settings were kept")
	}
	if _, err := os.Stat(filepath.Join(home, "state", "factory.json")); !errors.Is(err, fs.ErrNotExist) {
		t.Fatalf("a file was written: %v", err)
	}
}

func TestAProjectLimitCountsOnlyWhileItsFactoryIsOn(t *testing.T) {
	c := OpenConfigs(t.TempDir())
	if n := c.Limit("/app"); n != 0 {
		t.Fatalf("a project with nothing saved had a limit of %d", n)
	}
	off := on(2)
	off.Enabled = false
	c.Save("/app", off)
	if n := c.Limit("/app"); n != 0 {
		t.Fatalf("a project with the Factory off had a limit of %d", n)
	}
	c.Save("/app", on(2))
	if n := c.Limit("/app"); n != 2 {
		t.Fatalf("got %d, want 2", n)
	}
	c.Save("/app", on(0))
	if n := c.Limit("/app"); n != 0 {
		t.Fatalf("got %d, want the Board's limit (0)", n)
	}
}

func TestAnUnreadableFileIsKeptAsideAndEveryProjectStartsOff(t *testing.T) {
	home := t.TempDir()
	path := filepath.Join(home, "state", "factory.json")
	os.MkdirAll(filepath.Dir(path), 0o700)
	os.WriteFile(path, []byte("{not json"), 0o600)
	c := OpenConfigs(home)
	if c.Get("/app").Enabled {
		t.Fatal("a project was on")
	}
	if _, err := os.Stat(path + ".bad"); err != nil {
		t.Fatalf("the file wasn't kept: %v", err)
	}
	if err := c.Save("/app", on(1)); err != nil {
		t.Fatal(err)
	}
}
