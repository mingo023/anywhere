package config

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"strconv"
	"testing"
)

func refuse(string) error {
	panic("a config without a token has nothing to adopt")
}

func TestLoadFillsTheTableWithTheDefaultsOnceAndTouchesNoFile(t *testing.T) {
	home := t.TempDir()
	t.Setenv("POCKET_HOME", home)
	db := newDB(t, nil)
	first, err := Load(db, refuse)
	if err != nil || first.Port != DefaultPort() || first.Listen != "auto" {
		t.Fatalf("%+v %v", first, err)
	}
	if rows, want := configRows(t, db), map[string]string{"port": strconv.Itoa(DefaultPort()), "listen": `"auto"`}; !reflect.DeepEqual(rows, want) {
		t.Fatalf("rows %v", rows)
	}
	again, _ := Load(db, refuse)
	if again != first {
		t.Fatal("config changed on second load")
	}
	if entries, _ := os.ReadDir(home); len(entries) != 0 {
		t.Fatalf("Load wrote %v", entries)
	}
}

func TestAConfigWithATokenMovesItIntoDevicesAndForgetsIt(t *testing.T) {
	db := newDB(t, map[string]string{"token": `"t"`, "port": "1", "profiles.work": `{"cmd":"x"}`})
	var adopted []string
	adopt := func(tok string) error { adopted = append(adopted, tok); return nil }
	c, err := Load(db, adopt)
	if err != nil || c.Port != 1 || c.Listen != "auto" || len(adopted) != 1 || adopted[0] != "t" {
		t.Fatalf("%+v %v %v", c, err, adopted)
	}
	if rows := configRows(t, db); rows["token"] != "" || rows["profiles.work"] == "" {
		t.Fatalf("config = %v", rows)
	}
	if _, err := Load(db, adopt); err != nil || len(adopted) != 1 {
		t.Fatalf("second load adopted again: %v %v", adopted, err)
	}
}

func TestATokenThatCantBeAdoptedStaysInTheTable(t *testing.T) {
	db := newDB(t, map[string]string{"token": `"t"`, "port": "1"})
	if _, err := Load(db, func(string) error { return errors.New("no") }); err == nil {
		t.Fatal("want error")
	}
	if configRows(t, db)["token"] != `"t"` {
		t.Fatal("the token was dropped before anything held it")
	}
}

func TestLoadRejectsBrokenConfig(t *testing.T) {
	if _, err := Load(newDB(t, map[string]string{"token": `"t"`}), refuse); err == nil {
		t.Fatal("want error")
	}
}

func TestLoadRejectsAnUnknownListenMode(t *testing.T) {
	keep := func(string) error { return nil }
	if _, err := Load(newDB(t, map[string]string{"port": "1", "listen": `"0.0.0.0"`}), keep); err == nil {
		t.Fatal("want error")
	}
	if _, err := Load(newDB(t, map[string]string{"port": "1", "listen": "false"}), keep); err == nil {
		t.Fatal("a listen that isn't a string: want error")
	}
	if c, err := Load(newDB(t, map[string]string{"port": "1", "listen": `"loopback"`}), keep); err != nil || c.Listen != "loopback" {
		t.Fatalf("%+v %v", c, err)
	}
}

func TestOwnerListenTakesOnlyALoopbackAddress(t *testing.T) {
	for v, ok := range map[string]bool{`"off"`: true, `"127.0.0.1:4519"`: true, `"[::1]:4519"`: true, `"0.0.0.0:4519"`: false, `"localhost:4519"`: false, `"4519"`: false, `4519`: false} {
		db := newDB(t, map[string]string{"port": "1", "ownerListen": v})
		if c, err := Load(db, refuse); (err == nil) != ok || ok && `"`+c.OwnerListen+`"` != v {
			t.Errorf("%s: %+v %v", v, c, err)
		}
		if got, err := OwnerListen(db); (err == nil) != ok || ok && `"`+got+`"` != v {
			t.Errorf("%s: OwnerListen %q %v", v, got, err)
		}
	}
}

func TestOwnerListenReadsAsOffWithoutARow(t *testing.T) {
	if got, err := OwnerListen(newDB(t, nil)); err != nil || got != "off" && runtime.GOOS != "windows" {
		t.Fatalf("%q %v", got, err)
	}
}

func TestAHomeNoDaemonImportedYetHasItsOwnerListenInConfigJSONAndNothingIsWritten(t *testing.T) {
	home := t.TempDir()
	if got, err := LegacyOwnerListen(home); err != nil || got != "off" && runtime.GOOS != "windows" {
		t.Fatalf("no file: %q %v", got, err)
	}
	path := filepath.Join(home, "config.json")
	original := []byte(`{"port":1,"ownerListen":"127.0.0.1:4519"}`)
	os.WriteFile(path, original, 0o600)
	if got, err := LegacyOwnerListen(home); err != nil || got != "127.0.0.1:4519" {
		t.Fatalf("%q %v", got, err)
	}
	if now, _ := os.ReadFile(path); string(now) != string(original) {
		t.Fatalf("config.json became %s", now)
	}
	os.WriteFile(path, []byte(`{"ownerListen":"0.0.0.0:1"}`), 0o600)
	if _, err := LegacyOwnerListen(home); err == nil {
		t.Fatal("a public address was accepted")
	}
}

func release(t *testing.T) {
	channel = "release"
	t.Cleanup(func() { channel = "" })
}

func TestADevBuildKeepsOutOfTheInstalledAppsHomeServiceAndPort(t *testing.T) {
	t.Setenv("POCKET_HOME", "")
	t.Setenv("HOME", "/h")
	if Release() || Home() != "/h/.anywhere/dev" || Label() != "dev.mingo.anywhere.dev.pocketd" || DefaultPort() != 4518 {
		t.Fatal(Home(), Label(), DefaultPort())
	}
}

func TestAReleaseBuildKeepsTheNamesInstalledCopiesUse(t *testing.T) {
	release(t)
	t.Setenv("POCKET_HOME", "")
	t.Setenv("HOME", "/h")
	if !Release() || Home() != "/h/.anywhere" || Label() != "dev.mingo.anywhere.pocketd" || DefaultPort() != 4517 {
		t.Fatal(Home(), Label(), DefaultPort())
	}
}

func TestTheLegacyHomeStaysTheHomeUntilItMoves(t *testing.T) {
	user := t.TempDir()
	t.Setenv("HOME", user)
	t.Setenv("POCKET_HOME", "")
	legacy, home := filepath.Join(user, ".coding-pocket-dev"), filepath.Join(user, ".anywhere", "dev")
	os.Mkdir(legacy, 0o700)
	if Home() != legacy {
		t.Fatal(Home())
	}
	t.Setenv("POCKET_HOME", legacy+"/")
	if Home() != legacy {
		t.Fatal("a host service's POCKET_HOME", Home())
	}
	os.Remove(legacy)
	os.MkdirAll(home, 0o700)
	os.Symlink(home, legacy)
	if Home() != home || DefaultHome() != home {
		t.Fatal("moved", Home(), DefaultHome())
	}
	t.Setenv("POCKET_HOME", "/scratch")
	if Home() != "/scratch" {
		t.Fatal(Home())
	}
}
