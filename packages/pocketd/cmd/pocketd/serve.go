package main

import (
	"context"
	"fmt"
	"log"
	"net/netip"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"strings"
	"sync/atomic"
	"syscall"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/awake"
	"pocketd/internal/broker"
	"pocketd/internal/config"
	"pocketd/internal/daemon"
	"pocketd/internal/devices"
	"pocketd/internal/events"
	"pocketd/internal/host"
	"pocketd/internal/hub"
	"pocketd/internal/launchagent"
	"pocketd/internal/lock"
	"pocketd/internal/logfile"
	"pocketd/internal/ops"
	"pocketd/internal/pairing"
	"pocketd/internal/proto"
	"pocketd/internal/reach"
	"pocketd/internal/shellenv"
	"pocketd/internal/terminal"
	"pocketd/internal/wsserver"
)

func serve(sock string) error {
	home := config.Home()
	held, err := lock.Acquire(home)
	if err != nil {
		return err
	}
	defer held.Close()
	logPath := filepath.Join(home, "logs", "pocketd.log")
	logs, err := logfile.Open(logPath, 5<<20, 1)
	if err != nil {
		return err
	}
	defer logs.Close()
	log.SetOutput(logs)
	started := time.Now()
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer stop()

	evs, err := events.Open(home)
	if err != nil {
		log.Printf("events: %v; serving without them", err)
	}
	defer evs.Close()
	launchd := os.Getenv("XPC_SERVICE_NAME") == launchagent.Label
	evs.Emit(events.Event{Kind: "start", Version: versionString(), PID: os.Getpid(), Service: &launchd})
	defer func() {
		reason := "error"
		if ctx.Err() != nil {
			reason = "signal"
		}
		evs.Emit(events.Event{Kind: "stop", Reason: reason})
	}()

	devs, err := devices.Open(filepath.Join(home, "devices.json"), "")
	if err != nil {
		return fmt.Errorf("devices.json unreadable: %v. Move it away to reset pairing.", err)
	}
	cfg, err := config.Load(devs.EnsureLegacy)
	if err != nil {
		return err
	}
	exe, err := os.Executable()
	if err != nil {
		return err
	}
	h := hub.New()
	d := &daemon.Daemon{
		Terminals: terminal.NewManager(),
		Agents:    agent.NewRegistry(h),
		Broker:    broker.New(h),
		Home:      config.Home(),
		Exe:       exe,
		Sock:      sock,
	}
	d.Capture = func() shellenv.Result {
		shell := shellenv.LoginShell()
		return shellenv.Capture(ctx, shell, shellenv.Base(os.Environ(), shell), 5*time.Second)
	}
	go d.Recapture()
	mon := host.NewMonitor()
	kick := make(chan struct{}, 1)
	d.Agents.OnStatus = func(id, provider, from, to string) {
		evs.Emit(events.Event{Kind: "status", Agent: id, Provider: provider, From: from, To: to})
		select {
		case kick <- struct{}{}:
		default:
		}
	}
	keeper := &awake.Keeper{S: &awake.IOKit{}, Linger: 2 * time.Minute, After: time.After,
		OnHeld: func(on bool) { mon.Set(func(s *proto.HostState) { s.KeepingAwake = on }) }}
	go keeper.Run(ctx, kick, func() bool { return agent.Busy(d.Agents.List()) })
	go tick(ctx, mon, evs)
	if err := d.WritePlugin(); err != nil {
		return err
	}
	d.Terminals.OnInput = d.Input
	hostname, _ := os.Hostname()
	pairs := pairing.New(time.Now)
	var live atomic.Pointer[reach.Listener]
	pairHost := func() (string, bool) {
		if l := live.Load(); l != nil {
			return l.PairHost()
		}
		return "", false
	}
	ws := &wsserver.Server{Devices: devs, Pairing: pairs, Host: pairHost, MacName: computerName(hostname), Hostname: hostname, Agents: d.Agents, Broker: d.Broker, Hub: h, Monitor: mon, Events: evs, AskOpen: d.AskOpen}
	phones, err := reach.Listen(cfg.Port, cfg.Listen, ws)
	if err != nil {
		return err
	}
	live.Store(phones)
	go watchTailnet(ctx, phones, mon)
	go d.Watch(context.Background())

	ln, err := ops.Listen(sock)
	if err != nil {
		return err
	}
	// Closing the listener ends Serve, so a stop signal exits 0 and launchd leaves pocketd stopped.
	go func() {
		<-ctx.Done()
		ln.Close()
	}()
	fmt.Println("pocketd listening on", sock)
	for _, a := range phones.Addrs() {
		if !a.Addr().IsLoopback() {
			fmt.Println("phone: ws://" + a.String())
		}
	}
	if !phones.Tailnet() {
		fmt.Println("Phone access needs Tailscale")
	}
	status := func() ops.Status {
		agents := map[string]int{}
		for _, a := range d.Agents.List() {
			agents[a.Status]++
		}
		service := "none"
		if user, _ := os.UserHomeDir(); home == filepath.Join(user, ".coding-pocket") {
			service = launchagent.Service(os.Getuid(), launchagent.Path(user))
		}
		h := mon.State()
		return ops.Status{PID: os.Getpid(), Version: versionString(), Home: home, Sock: sock, Log: logPath,
			Uptime: int64(time.Since(started).Seconds()), Listen: addrs(phones.Addrs()), Terminals: len(d.Terminals.List()),
			Agents: agents, KeepingAwake: h.KeepingAwake, Tailnet: h.Tailnet, ShellEnv: d.ShellEnv(), Service: service}
	}
	err = (&ops.Server{
		Terminals: d.Terminals, Spawn: d.Spawn, Hook: d.Hook,
		Devices: devs, Kick: ws.CloseDevice,
		Pairing: pairs, Host: pairHost, MacName: ws.MacName,
		WS: ws, AskOpen: d.AskOpen,
		Status: status,
	}).Serve(ln)
	if ctx.Err() != nil {
		return nil
	}
	return err
}

func watchTailnet(ctx context.Context, ln *reach.Listener, mon *host.Monitor) {
	for {
		mon.Set(func(s *proto.HostState) { s.Tailnet = ln.Tailnet() })
		select {
		case <-ctx.Done():
			return
		case <-time.After(30 * time.Second):
		}
	}
}

// tick marks each minute pocketd is up while keeping the Mac awake; stats reads gaps as unreachable time.
func tick(ctx context.Context, mon *host.Monitor, evs *events.Log) {
	for {
		select {
		case <-ctx.Done():
			return
		case <-time.After(time.Minute):
			if mon.State().KeepingAwake {
				evs.Emit(events.Event{Kind: "tick"})
			}
		}
	}
}

func addrs(list []netip.AddrPort) []string {
	out := []string{}
	for _, a := range list {
		out = append(out, a.String())
	}
	return out
}

// computerName is the name the Mac shows in Finder and AirDrop, which the
// phone shows when asking to pair.
func computerName(fallback string) string {
	out, err := exec.Command("scutil", "--get", "ComputerName").Output()
	if name := strings.TrimSpace(string(out)); err == nil && name != "" {
		return name
	}
	return fallback
}
