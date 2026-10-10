package main

import (
	"cmp"
	"context"
	"fmt"
	"log"
	"net/netip"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"runtime/debug"
	"strings"
	"sync/atomic"
	"syscall"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/automation"
	"pocketd/internal/awake"
	"pocketd/internal/broker"
	"pocketd/internal/config"
	"pocketd/internal/daemon"
	"pocketd/internal/devices"
	"pocketd/internal/events"
	"pocketd/internal/handoff"
	"pocketd/internal/host"
	"pocketd/internal/hub"
	"pocketd/internal/launch"
	"pocketd/internal/launchagent"
	"pocketd/internal/locals"
	"pocketd/internal/lock"
	"pocketd/internal/logfile"
	"pocketd/internal/names"
	"pocketd/internal/ops"
	"pocketd/internal/pairing"
	"pocketd/internal/proc"
	"pocketd/internal/proto"
	"pocketd/internal/reach"
	"pocketd/internal/registry"
	"pocketd/internal/shellenv"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
	"pocketd/internal/worktree"
	"pocketd/internal/wsserver"
)

func serve(sock, handed string) error {
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
	reap()

	evs, err := events.Open(home)
	if err != nil {
		log.Printf("events: %v; serving without them", err)
	}
	defer evs.Close()
	launchd := os.Getenv("XPC_SERVICE_NAME") == config.Label()
	if launchd && config.Release() {
		// The bundled plist can't name a log path under ~, so launchd drops stderr.
		if f, err := os.OpenFile(filepath.Join(home, "logs", "launchd.log"), os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600); err == nil {
			debug.SetCrashOutput(f, debug.CrashOptions{})
			f.Close()
		}
	}
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
	reg := registry.New(registry.Path())
	h := hub.New()
	d := &daemon.Daemon{
		Terminals: terminal.NewManager(),
		Agents:    agent.NewRegistry(h),
		Broker:    broker.New(h),
		Home:      config.Home(),
		Exe:       exe,
		Sock:      sock,
		Registry:  reg,
	}
	d.Capture = func() shellenv.Result {
		shell := shellenv.LoginShell()
		return shellenv.Capture(ctx, shell, shellenv.Base(os.Environ(), shell), 5*time.Second)
	}
	mon := host.NewMonitor()
	kick := make(chan struct{}, 1)
	d.Agents.OnStatus = func(id, provider, from, to string) {
		evs.Emit(events.Event{Kind: "status", Agent: id, Provider: provider, From: from, To: to})
		select {
		case kick <- struct{}{}:
		default:
		}
	}
	settings := config.NewSettings(home)
	keeper := &awake.Keeper{S: &awake.IOKit{}, Enabled: settings.KeepAwake, Linger: settings.AwakeLinger, After: time.After,
		OnHeld: func(on bool) { mon.Set(func(s *proto.HostState) { s.KeepingAwake = on }) }}
	go keeper.Run(ctx, kick, func() bool { return agent.Busy(d.Agents.List()) })
	go tick(ctx, mon, evs)
	if err := d.WritePlugin(); err != nil {
		return err
	}
	d.Terminals.OnInput = d.Input
	d.Terminals.OnReport = d.Report
	hostname, _ := os.Hostname()
	pairs := pairing.New(time.Now)
	var live atomic.Pointer[reach.Listener]
	pairHost := func() (string, bool) {
		if l := live.Load(); l != nil {
			return l.PairHost()
		}
		return "", false
	}
	worktreeNames := names.Open(home)
	l := launch.New(d, reg, settings, evs)
	l.Names = worktreeNames
	localStore := locals.Open(home)
	l.Locals = localStore
	if settings.ResumeAgents() {
		d.Resume = l.ResumeCmd
	}
	d.OnRestore = func(id string, ok bool, ms int64, outcome, reason string) {
		evs.Emit(events.Event{Kind: "restore", Agent: id, OK: &ok, MS: ms, Outcome: outcome, Reason: reason})
	}
	up := &upgrader{exe: exe, home: home, d: d, starting: l.Starting, evs: evs, mon: mon}
	// Taken before restoring, so a binary swapped meanwhile still counts as a change.
	var watch *selfWatch
	if st, err := os.Stat(exe); err == nil {
		if sum, err := fileSum(exe); err == nil {
			watch = &selfWatch{exe: exe, sum: sum, seen: st, now: time.Now}
		}
	}
	autos := automation.Open(home)
	autos.Grace, autos.History = settings.AutomationGrace, settings.AutomationHistory
	scheduler := &automation.Scheduler{Store: autos, Now: time.Now, Watch: automation.AgentWatcher(d.Agents),
		Start: func(runID string, a proto.Automation) launch.Result {
			return l.Create(launch.Who{Owner: true, Key: "automation:" + a.ID}, runID, automation.Spec(a, runID), func(string, string) {}, func(launch.Creating) {})
		}}
	ws := &wsserver.Server{Devices: devs, Pairing: pairs, Host: pairHost, MacName: computerName(hostname), Hostname: hostname, Agents: d.Agents, Broker: d.Broker, Hub: h, Monitor: mon, Events: evs, AskOpen: d.AskOpen, Projects: func() []proto.Project { return worktree.Projects(reg.Load()) }, Launch: l, Names: worktreeNames, Version: versionString(), Automations: autos, Scheduler: scheduler, Locals: localStore, Registry: reg, Terminals: d.Terminals}
	if err := endGrace(devs, ws.CloseDevice); err != nil {
		return err
	}
	statePath := state.Path(d.Home)
	if handed != "" {
		f, err := handoff.Read(handed)
		os.Remove(handed)
		if err != nil {
			return fmt.Errorf("handoff: %w", err)
		}
		d.Adopt(f)
		// The awake assertion died with the old image.
		select {
		case kick <- struct{}{}:
		default:
		}
	} else {
		saved, err := state.Load(statePath)
		if err != nil {
			log.Print(err)
		}
		d.Restore(saved, cmp.Or(os.Getenv("POCKETD_RESTORE_SHELL"), shellenv.LoginShell()))
	}
	go d.Watch(context.Background())
	if err := autos.Reconcile(time.Now()); err != nil {
		log.Printf("automations: %v", err)
	}
	scheduler.Resume()
	go scheduler.Run(ctx)
	phones, err := reach.Listen(cfg.Port, cfg.Listen, ws)
	if err != nil {
		return err
	}
	live.Store(phones)
	go watchTailnet(ctx, live.Load, mon)

	ln, err := ops.Listen(sock)
	if err != nil {
		return err
	}
	if watch != nil {
		go watch.run(ctx, up.run)
	}
	// Closing the listener ends Serve, so a stop signal exits 0 and launchd leaves pocketd stopped.
	go func() {
		<-ctx.Done()
		ln.Close()
	}()
	writer := state.NewWriter(statePath, d.Snapshot)
	go writer.Run(ctx)
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
		if user, _ := os.UserHomeDir(); home == filepath.Join(user, config.HomeName()) {
			service = launchagent.Service(os.Getuid(), launchagent.Path(user))
		}
		h := mon.State()
		return ops.Status{PID: os.Getpid(), Version: versionString(), Home: home, Sock: sock, Log: logPath,
			Uptime: int64(time.Since(started).Seconds()), Listen: addrs(live.Load().Addrs()), Terminals: len(d.Terminals.List()),
			Agents: agents, KeepingAwake: h.KeepingAwake, Tailnet: h.Tailnet, ShellEnv: d.ShellEnv(), Service: service,
			Config: settings.Values(), Providers: l.Paths()}
	}
	err = (&ops.Server{
		Terminals: d.Terminals, Spawn: d.Spawn, Hook: d.Hook,
		Devices: devs, Kick: ws.CloseDevice,
		Pairing: pairs, Host: pairHost, MacName: ws.MacName,
		WS: ws, AskOpen: d.AskOpen,
		Status:     status,
		LaunchExit: l.Exited,
		ConfigSet: networkSetter(&live, settings, func() {
			mon.Set(func(s *proto.HostState) { s.Tailnet = live.Load().Tailnet() })
		}, configSetter(l, settings, func() {
			select {
			case kick <- struct{}{}:
			default:
			}
		})),
		Upgrade: up.run,
	}).Serve(ln)
	if ctx.Err() != nil {
		writer.Freeze()
		d.Terminals.CloseAll(terminal.CloseGrace)
		return nil
	}
	return err
}

// reap ends what a crashed pocketd left running (proc.Reap has the rule).
func reap() {
	ps, err := proc.Orphans(os.Getuid())
	if err != nil {
		log.Printf("reap: %v", err)
		return
	}
	if pids := proc.Reap(os.Getpid(), proc.Alive, ps); len(pids) > 0 {
		log.Printf("reap: ending %d processes of a dead pocketd", len(pids))
		go proc.Kill(pids, terminal.CloseGrace)
	}
}

// watchTailnet follows phones, which a port change replaces.
func watchTailnet(ctx context.Context, phones func() *reach.Listener, mon *host.Monitor) {
	for {
		mon.Set(func(s *proto.HostState) { s.Tailnet = phones().Tailnet() })
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

// endGrace starts the legacy device's grace and, once it is over, forgets the
// device and closes its sockets. It reads the wall clock each minute because
// timers stall while the Mac sleeps.
func endGrace(store *devices.Store, closeDevice func(id, reason string)) error {
	end, err := store.StartGrace(time.Now())
	if err != nil || end.IsZero() {
		return err
	}
	over := func() bool {
		ended, err := store.EndGrace(time.Now())
		if err != nil {
			log.Printf("devices: forgetting the legacy device: %v", err)
		}
		if ended {
			closeDevice(devices.LegacyID, "revoked")
		}
		return ended
	}
	if over() {
		return nil
	}
	go func() {
		for range time.Tick(time.Minute) {
			if over() {
				return
			}
		}
	}()
	return nil
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
