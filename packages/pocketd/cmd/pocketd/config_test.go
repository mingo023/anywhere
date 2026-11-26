package main

import (
	"bytes"
	"errors"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"

	"pocketd/internal/config"
	"pocketd/internal/config/configtest"
	"pocketd/internal/daemon"
	"pocketd/internal/launch"
	"pocketd/internal/lock"
	"pocketd/internal/reach"
	"pocketd/internal/statedb"
)

func TestAnUnknownConfigKeyIsNamedAsUnknown(t *testing.T) {
	settings := configtest.Settings(t)
	set := configSetter(launch.New(&daemon.Daemon{}, nil, settings, nil), settings, func() {})
	err := set("phone.maxAcess", "ask")
	if err == nil || errors.Is(err, config.ErrInvalid) || !strings.Contains(err.Error(), `unknown key "phone.maxAcess"`) {
		t.Fatalf("err = %v", err)
	}
}

func TestAKeepAwakeChangeWakesTheKeeperAndARefusedOneDoesNot(t *testing.T) {
	settings := configtest.Settings(t)
	kicks := 0
	set := configSetter(launch.New(&daemon.Daemon{}, nil, settings, nil), settings, func() { kicks++ })
	if err := set("awake.enabled", "false"); err != nil || kicks != 1 || settings.KeepAwake() {
		t.Fatalf("err %v, kicks %d", err, kicks)
	}
	if err := set("awake.lingerMinutes", "90"); err == nil || kicks != 1 {
		t.Fatalf("err %v, kicks %d", err, kicks)
	}
}

func TestAProviderCommandThatIsntOnTheMacIsRefused(t *testing.T) {
	settings := configtest.Settings(t)
	set := configSetter(launch.New(&daemon.Daemon{}, nil, settings, nil), settings, func() {})
	missing, real := "/nowhere/claude", "/bin/sh"
	if runtime.GOOS == "windows" {
		missing, real = `C:\nowhere\claude.exe`, os.Args[0]
	}
	if err := set("claude.command", missing); err == nil || !strings.Contains(err.Error(), "isn't an executable") || settings.AgentCommand("claude") != "claude" {
		t.Fatalf("err %v, command %q", err, settings.AgentCommand("claude"))
	}
	if err := set("claude.command", real); err != nil || settings.AgentCommand("claude") != real {
		t.Fatalf("err %v", err)
	}
}

func TestPocketdConfigSetsPhoneTerminalsAndThePushRelay(t *testing.T) {
	settings := configtest.Settings(t)
	set := configSetter(launch.New(&daemon.Daemon{}, nil, settings, nil), settings, func() {})
	if err := set("phone.terminals", "off"); err != nil || settings.PhoneTerminals() {
		t.Fatalf("phone.terminals: %v", err)
	}
	if err := set("push.relay", "https://relay.example.com"); err != nil || settings.PushRelay() != "https://relay.example.com" {
		t.Fatalf("push.relay: %v %q", err, settings.PushRelay())
	}
}

func freePort(t *testing.T) int {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()
	return ln.Addr().(*net.TCPAddr).Port
}

func TestAPortChangeMovesThePhoneListenerAndATakenPortKeepsIt(t *testing.T) {
	settings := configtest.Settings(t)
	first, err := reach.Listen(freePort(t), "loopback", http.NotFoundHandler())
	if err != nil {
		t.Fatal(err)
	}
	var phones atomic.Pointer[reach.Listener]
	phones.Store(first)
	t.Cleanup(func() { phones.Load().Close() })
	moves := 0
	set := networkSetter(&phones, settings, func() { moves++ }, func(string, string) error { return errors.New("rest") })

	next := freePort(t)
	if err := set("port", strconv.Itoa(next)); err != nil || phones.Load().Port() != next || settings.Port() != next || moves != 1 {
		t.Fatalf("err %v, port %d, saved %d, moves %d", err, phones.Load().Port(), settings.Port(), moves)
	}
	if _, err := net.Dial("tcp", "127.0.0.1:"+strconv.Itoa(next)); err != nil {
		t.Fatalf("new port not served: %v", err)
	}

	taken, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer taken.Close()
	busy := taken.Addr().(*net.TCPAddr).Port
	if err := set("port", strconv.Itoa(busy)); err == nil || phones.Load().Port() != next || settings.Port() != next || moves != 1 {
		t.Fatalf("taken port: err %v, port %d, saved %d", err, phones.Load().Port(), settings.Port())
	}
	if err := set("port", "80"); err == nil {
		t.Fatal("privileged port accepted")
	}
}

func TestListenSwitchesModeInPlaceAndOtherKeysPassThrough(t *testing.T) {
	settings := configtest.Settings(t)
	ln, err := reach.Listen(freePort(t), "auto", http.NotFoundHandler())
	if err != nil {
		t.Fatal(err)
	}
	var phones atomic.Pointer[reach.Listener]
	phones.Store(ln)
	t.Cleanup(func() { phones.Load().Close() })
	var passed string
	set := networkSetter(&phones, settings, func() {}, func(key, _ string) error { passed = key; return nil })
	if err := set("listen", "loopback"); err != nil || ln.Mode() != "loopback" || settings.Listen() != "loopback" {
		t.Fatalf("err %v, mode %s, saved %s", err, ln.Mode(), settings.Listen())
	}
	if err := set("listen", "lan"); err == nil || settings.Listen() != "loopback" {
		t.Fatalf("lan accepted: %v", err)
	}
	if set("awake.enabled", "false"); passed != "awake.enabled" {
		t.Fatalf("passed %q", passed)
	}
}

func TestAPortSetByTheDaemonsSetterIsThePortAfterARestartAndConfigJSONIsUntouched(t *testing.T) {
	home := t.TempDir()
	original := []byte("{\n  \"port\": 4517,\n  \"listen\": \"auto\"\n}\n")
	if err := os.WriteFile(filepath.Join(home, "config.json"), original, 0o600); err != nil {
		t.Fatal(err)
	}
	db, err := statedb.Open(home)
	if err != nil {
		t.Fatal(err)
	}
	settings, err := config.NewSettings(db)
	if err != nil {
		t.Fatal(err)
	}
	first, err := reach.Listen(freePort(t), "loopback", http.NotFoundHandler())
	if err != nil {
		t.Fatal(err)
	}
	var phones atomic.Pointer[reach.Listener]
	phones.Store(first)
	t.Cleanup(func() { phones.Load().Close() })
	port := freePort(t)
	set := networkSetter(&phones, settings, func() {}, func(string, string) error { return errors.New("rest") })
	if err := set("port", strconv.Itoa(port)); err != nil {
		t.Fatal(err)
	}
	db.Close()

	db, err = statedb.Open(home)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	c, err := config.Load(db, func(string) error { return nil })
	if err != nil || c.Port != port {
		t.Fatalf("port %d after the restart, want %d: %v", c.Port, port, err)
	}
	if now, _ := os.ReadFile(filepath.Join(home, "config.json")); !bytes.Equal(now, original) {
		t.Fatalf("config.json became %s", now)
	}
}

func TestConfigSetWithNoPocketdRunningWritesTheDatabaseSoABadPortCanBeRepaired(t *testing.T) {
	home := t.TempDir()
	original := []byte(`{"port": 0}`)
	if err := os.WriteFile(filepath.Join(home, "config.json"), original, 0o600); err != nil {
		t.Fatal(err)
	}
	db, err := statedb.Open(home)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := config.Load(db, func(string) error { return nil }); err == nil {
		t.Fatal("port 0 loaded")
	}
	db.Close()

	var out bytes.Buffer
	if err := configSet(filepath.Join(home, "none.sock"), home, "port", "4600", &out); err != nil {
		t.Fatal(err)
	}
	db, err = statedb.Open(home)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	if c, err := config.Load(db, func(string) error { return nil }); err != nil || c.Port != 4600 {
		t.Fatalf("%+v %v", c, err)
	}
	if now, _ := os.ReadFile(filepath.Join(home, "config.json")); !bytes.Equal(now, original) {
		t.Fatalf("config.json became %s", now)
	}
}

func TestConfigSetRefusesToWriteBesideAPocketdThatHoldsTheLock(t *testing.T) {
	home := t.TempDir()
	held, err := lock.Acquire(home)
	if err != nil {
		t.Fatal(err)
	}
	defer held.Close()
	var out bytes.Buffer
	if err := configSet(filepath.Join(home, "none.sock"), home, "port", "4600", &out); !errors.As(err, new(lock.ErrRunning)) {
		t.Fatalf("err = %v", err)
	}
	if _, err := os.Stat(statedb.Path(home)); err == nil {
		t.Fatal("pocketd.db written beside a running pocketd")
	}
}
