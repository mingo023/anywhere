package main

import (
	"errors"
	"net"
	"net/http"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"

	"pocketd/internal/config"
	"pocketd/internal/daemon"
	"pocketd/internal/launch"
	"pocketd/internal/reach"
)

func TestAnUnknownConfigKeyIsNamedAsUnknown(t *testing.T) {
	settings := config.NewSettings(t.TempDir())
	set := configSetter(launch.New(&daemon.Daemon{}, nil, settings, nil), settings, func() {})
	err := set("phone.maxAcess", "ask")
	if err == nil || errors.Is(err, config.ErrInvalid) || !strings.Contains(err.Error(), `unknown key "phone.maxAcess"`) {
		t.Fatalf("err = %v", err)
	}
}

func TestAKeepAwakeChangeWakesTheKeeperAndARefusedOneDoesNot(t *testing.T) {
	settings := config.NewSettings(t.TempDir())
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
	settings := config.NewSettings(t.TempDir())
	set := configSetter(launch.New(&daemon.Daemon{}, nil, settings, nil), settings, func() {})
	if err := set("claude.command", "/nowhere/claude"); err == nil || !strings.Contains(err.Error(), "isn't an executable") || settings.AgentCommand("claude") != "claude" {
		t.Fatalf("err %v, command %q", err, settings.AgentCommand("claude"))
	}
	if err := set("claude.command", "/bin/sh"); err != nil || settings.AgentCommand("claude") != "/bin/sh" {
		t.Fatalf("err %v", err)
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
	settings := config.NewSettings(t.TempDir())
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
	settings := config.NewSettings(t.TempDir())
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
