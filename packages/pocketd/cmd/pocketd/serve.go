package main

import (
	"cmp"
	"context"
	"errors"
	"fmt"
	"io/fs"
	"log"
	"net"
	"net/netip"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"pocketd/internal/accounts"
	"runtime"
	"runtime/debug"
	"slices"
	"strings"
	"sync/atomic"
	"syscall"
	"time"

	"pocketd/internal/agent"
	"pocketd/internal/automation"
	"pocketd/internal/awake"
	"pocketd/internal/board"
	"pocketd/internal/broker"
	"pocketd/internal/cliskill"
	"pocketd/internal/config"
	"pocketd/internal/daemon"
	"pocketd/internal/devices"
	"pocketd/internal/events"
	"pocketd/internal/fsview"
	"pocketd/internal/github"
	"pocketd/internal/gitview"
	"pocketd/internal/handoff"
	"pocketd/internal/holder"
	"pocketd/internal/host"
	"pocketd/internal/hub"
	"pocketd/internal/launch"
	"pocketd/internal/launchagent"
	"pocketd/internal/locals"
	"pocketd/internal/lock"
	"pocketd/internal/logfile"
	"pocketd/internal/names"
	"pocketd/internal/ops"
	"pocketd/internal/owners"
	"pocketd/internal/pairing"
	"pocketd/internal/proc"
	"pocketd/internal/proto"
	"pocketd/internal/push"
	"pocketd/internal/reach"
	"pocketd/internal/registry"
	"pocketd/internal/shellenv"
	"pocketd/internal/skills"
	"pocketd/internal/state"
	"pocketd/internal/terminal"
	"pocketd/internal/workspace"
	"pocketd/internal/worktree"
	"pocketd/internal/wsserver"
)

func serve(sock, handed string) error {
	home := config.Home()
	var inherited handoff.File
	var err error
	if handed != "" {
		if inherited, err = handedDown(home, handed); err != nil {
			return err
		}
	}
	var held *lock.Lock
	if inherited.LockFD > 0 {
		held = lock.Adopt(inherited.LockFD, home)
	} else if held, err = lock.Acquire(home); err != nil {
		return err
	}
	defer held.Close()
	legacy := home
	home, moveErr := config.MoveLegacyHome(home)
	if home != legacy && os.Getenv("POCKETD_SOCK") == "" {
		sock = config.Sock()
	}
	// Read under the lock, so a start that loses it leaves the file to the one that holds it.
	if handed == "" {
		inherited, _ = handedDown(home, "")
	}
	logPath := filepath.Join(home, "logs", "pocketd.log")
	logs, err := logfile.Open(logPath, 5<<20, 1)
	if err != nil {
		return err
	}
	defer logs.Close()
	log.SetOutput(logs)
	detachConsole()
	switch {
	case moveErr != nil:
		log.Printf("home: moving %s to %s: %v", legacy, home, moveErr)
	case home != legacy:
		log.Printf("home: moved %s to %s", legacy, home)
	}
	started := time.Now()
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer stop()

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
	if user, err := os.UserHomeDir(); err == nil {
		if err := cliskill.Refresh(user); err != nil {
			log.Printf("skill: %v", err)
		}
	}
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
	if runtime.GOOS != "windows" {
		d.Terminals.Holder = &holder.Starter{Exe: exe, Dir: holder.Dir(home)}
	}
	d.Capture = func() shellenv.Result {
		shell := shellenv.LoginShell()
		return shellenv.Capture(ctx, shell, shellenv.Base(os.Environ(), shell), 5*time.Second)
	}
	mon := host.NewMonitor()
	kick := make(chan struct{}, 1)
	hostname, _ := os.Hostname()
	pushes, err := push.Open(push.Path(home))
	if err != nil {
		log.Printf("push.json: %v; no phone is registered for pushes", err)
	}
	notifier := &push.Notifier{Store: pushes, Agents: d.Agents, Broker: d.Broker, Mac: hostname, Events: evs}
	settings := config.NewSettings(home)
	o, err := openOrchestrator(home, settings, d.Agents, h, inherited.MCPFD)
	if err != nil {
		return fmt.Errorf("orchestrator.db: %w", err)
	}
	d.Broker.OnAsk = func(req proto.PermissionRequest) {
		notifier.Ask(req)
		o.ask(req)
	}
	d.Broker.OnClose = o.asked
	d.Agents.OnStatus = func(id, provider, from, to string) {
		evs.Emit(events.Event{Kind: "status", Agent: id, Provider: provider, From: from, To: to})
		notifier.Kick(id, from, to)
		select {
		case kick <- struct{}{}:
		default:
		}
	}
	host.FollowTerminals(mon, settings)
	notifier.Relay = &push.Client{URL: settings.PushRelay, HTTP: push.NewHTTP()}
	keeper := &awake.Keeper{S: awake.System(), Enabled: settings.KeepAwake, Linger: settings.AwakeLinger, After: time.After,
		OnHeld: func(on bool) { mon.Set(func(s *proto.HostState) { s.KeepingAwake = on }) }}
	go keeper.Run(ctx, kick, d.Agents.Busy)
	go tick(ctx, mon, evs)
	if err := d.WritePlugin(); err != nil {
		return err
	}
	d.Terminals.OnInput = d.Input
	d.Terminals.OnReport = d.Report
	hostID, err := state.HostID(home)
	if err != nil {
		return err
	}
	userHome, _ := os.UserHomeDir()
	// The same folder for dev and release builds, so both see one set of accounts.
	d.Accounts = &accounts.Store{Root: filepath.Join(userHome, ".anywhere", "accounts"), Home: userHome}
	if runtime.GOOS == "darwin" {
		d.Accounts.Secret = accounts.Keychain
	}
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
	o.host.Broker, o.host.Launcher, o.host.LoginEnv, o.host.Projects, o.host.Planning = d.Broker, l, d.LoginEnv, reg, d.Planning
	l.MCPFlags = o.host.ClaudeFlags
	o.host.CloseTerminal = func(id string) {
		if t := d.Terminals.Get(id); t != nil {
			t.Close()
		}
	}
	d.MCPCodex = o.host.CodexConfig
	d.MCPServers = o.host.ACPServers
	localStore := locals.Open(home)
	l.Locals = localStore
	d.OnPrompt = l.Prompted
	if settings.ResumeAgents() {
		d.Resume = l.ResumeCmd
	}
	d.AgentTab = l.AgentTab
	d.OnRestore = func(id string, ok bool, ms int64, outcome, reason string) {
		evs.Emit(events.Event{Kind: "restore", Agent: id, OK: &ok, MS: ms, Outcome: outcome, Reason: reason})
	}
	up := &upgrader{exe: exe, home: home, d: d, lock: held, starting: l.Starting, evs: evs, mon: mon, handover: o.handover, resume: o.engine.Resume, mcp: o.ln}
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
		Post: postBound(d.Agents, o.engine),
		Turn: func(after int64, prompt, entry string) automation.Watcher {
			return automation.TurnWatcher(d.Agents, after, prompt, entry, queuedEntry(o.store))
		},
		Start: func(runID string, a proto.Automation) launch.Result {
			return l.Create(launch.Who{Owner: true, Key: "automation:" + a.ID}, runID, automation.Spec(a, runID), func(string, string) {}, func(launch.Creating) {})
		},
		OnFinish: func(r proto.Run) {
			a, _ := autos.Automation(r.AutomationID)
			notifier.Automation(r, a.Name)
		}}
	tickets := board.Open(home)
	boardScheduler := &board.Scheduler{Store: tickets, Now: time.Now, Watch: board.AgentWatcher(d.Agents),
		Start: func(requestID string, t proto.Ticket, created func(string)) launch.Result {
			return l.Create(launch.Who{Owner: true, Key: "board:" + t.ID}, requestID, board.Spec(t, requestID), func(string, string) {}, func(c launch.Creating) { created(c.Cwd) })
		}}
	ownerTokens := &owners.Store{Path: owners.Path(home)}
	skillStore, err := skills.Open(home)
	if err != nil {
		log.Printf("skills: %v", err)
	}
	o.host.Sched = schedules{store: autos, scheduler: scheduler}
	o.server.Hooks = hooks(schedules{store: autos, scheduler: scheduler})
	ws := &wsserver.Server{Orch: o.engine, Owners: ownerTokens, Devices: devs, Pairing: pairs, Host: pairHost, MacName: computerName(hostname), Hostname: hostname, Agents: d.Agents, Broker: d.Broker, Hub: h, Monitor: mon, Events: evs, AskOpen: d.AskOpen, Projects: func() []proto.Project { return worktree.Projects(reg.Load()) }, Launch: l, Names: worktreeNames, Version: versionString(), Automations: autos, Scheduler: scheduler, Board: tickets, Locals: localStore, Registry: reg, Terminals: d.Terminals, Retitle: d.Retitle, Settings: settings, TerminalHost: d, Git: gitview.New(d.LoginEnv), Skills: skillStore}
	if hm := openHosts(ctx, home, d.LoginEnv, ws.MacName, mirror{o}); hm != nil {
		ws.Hosts = &wsserver.Hosts{Manager: hm}
		o.host.Hosts, o.host.Envs = hm, environments{hm, o.host.Registry}
	}
	ws.OpenSessions = o.openSessions
	// Off Recapture's caller, which may be a spawn: listing providers runs a shell per agent.
	d.OnEnv = func() { go ws.PublishProviders() }
	projectList := workspace.NewProjects(reg)
	go projectList.Run(ctx)
	ws.Workspace = wsserver.WorkspaceService{Projects: projectList, Accounts: &workspace.AccountList{Store: d.Accounts, Running: d.Running}, Switch: d.Switch, SubagentTrees: o.subagentTrees}
	if ws.Hosts != nil {
		ws.Workspace.HostDelete = ws.Hosts.Manager.DeleteWorktree
	}
	gh := github.New(func(dir string) []string { return d.GhEnv(d.LoginEnv(), dir) }, reg)
	ws.GitHub, o.host.PRs = gh, gh.Cached
	boardScheduler.Merged = func(ctx context.Context, tree string) bool {
		pr, _ := ws.GitHub.PR(ctx, tree)
		return pr != nil && pr.State == "MERGED"
	}
	ws.Files = &fsview.Service{Registry: reg, Env: d.LoginEnv, Attachments: filepath.Join(home, "attachments")}
	notifier.MacName, notifier.Foreground, ws.Push = ws.MacName, ws.Foreground, notifier
	go notifier.Run(ctx)
	// A revoked phone's sockets close and its push registration goes with it.
	unpair := func(id, reason string) {
		ws.CloseDevice(id, reason)
		notifier.Forget(id)
	}
	if err := endGrace(devs, unpair); err != nil {
		return err
	}
	statePath := state.Path(d.Home)
	// Found before Adopt starts holders of its own.
	var clients []*holder.Client
	if d.Terminals.Holder != nil {
		clients = holder.Find(d.Terminals.Holder.Dir)
	}
	reap(clients)
	var ptys []state.Terminal
	for _, e := range inherited.Terminals {
		if e.FD > 0 {
			ptys = append(ptys, e.Saved)
		}
	}
	if len(ptys) > 0 {
		handoff.Claim(inherited)
		o.takeIn(ptys)
		d.Adopt(inherited)
	}
	saved := o.restoring(statePath)
	saved.Terminals = slices.DeleteFunc(saved.Terminals, func(e state.Terminal) bool {
		return slices.ContainsFunc(ptys, func(p state.Terminal) bool { return p.TerminalID == e.TerminalID })
	})
	saved.Terminals = d.Reattach(clients, inherited.Terminals, saved.Terminals)
	d.Restore(saved, cmp.Or(os.Getenv("POCKETD_RESTORE_SHELL"), shellenv.LoginShell()))
	o.host.Rehold(len(ptys)+len(clients) > 0)
	// The awake assertion died with the pocketd before.
	select {
	case kick <- struct{}{}:
	default:
	}
	o.closeAbsent(d.Agents)
	go d.Watch(context.Background())
	go func() {
		if d.WaitRestored(ctx) == nil {
			claimWorktrees(home, reg.Load(), usedFolders(worktreeNames, d.Terminals, tickets, autos))
		}
	}()
	if err := autos.Reconcile(time.Now()); err != nil {
		log.Printf("automations: %v", err)
	}
	scheduler.Resume()
	go scheduler.Run(ctx)
	go o.run(ctx)
	go boardScheduler.Run(ctx)
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
	var ownerLn net.Listener
	if cfg.OwnerListen != "off" {
		if ownerLn, err = net.Listen("tcp", cfg.OwnerListen); err != nil && handed == "" {
			ln.Close()
			return fmt.Errorf("ownerListen: %w", err)
		} else if err != nil {
			// An upgrade goes on without the owner port: exiting would start again into the same taken port.
			log.Printf("ownerListen: %v", err)
		}
	}
	if watch != nil {
		go watch.run(ctx, up.run)
	}
	// Closing the listener ends Serve, so a stop signal exits 0 and launchd leaves pocketd stopped.
	go func() {
		<-ctx.Done()
		ln.Close()
		if ownerLn != nil {
			ownerLn.Close()
		}
	}()
	writer := state.NewWriter(statePath, d.Snapshot)
	writer.Saved = o.saved
	go writer.Run(ctx)
	fmt.Println("pocketd listening on", sock)
	if ownerLn != nil {
		fmt.Println("owner: tcp://" + ownerLn.Addr().String())
	}
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
		if user, _ := os.UserHomeDir(); home == config.DefaultHome() {
			service = launchagent.Service(os.Getuid(), launchagent.Path(user))
		}
		h := mon.State()
		return ops.Status{PID: os.Getpid(), Version: versionString(), Home: home, Sock: sock, Log: logPath,
			Uptime: int64(time.Since(started).Seconds()), Listen: addrs(live.Load().Addrs()), Terminals: len(d.Terminals.List()),
			Agents: agents, KeepingAwake: h.KeepingAwake, Tailnet: h.Tailnet, ShellEnv: d.ShellEnv(), Service: service,
			Config: settings.Values(), Providers: l.Paths(),
			HostID: hostID, OS: runtime.GOOS, Arch: runtime.GOARCH, UserHome: userHome, Shell: shellenv.LoginShell(), Name: ws.MacName, Caps: ops.HostCaps}
	}
	srv := &ops.Server{
		Terminals: d.Terminals, Spawn: d.Spawn, Hook: d.Hook, Pipe: d.Pipe,
		Devices: devs, Kick: unpair,
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
		Upgrade:  up.run,
		LoginEnv: d.LoginEnv,
		TrashDir: filepath.Join(home, "trash"),
		Owners:   ownerTokens,
		Accounts: l.Accounts,
	}
	if ownerLn != nil {
		go srv.Serve(ownerLn)
	}
	err = srv.Serve(ln)
	if ctx.Err() != nil {
		writer.Freeze()
		defer o.close()
		if d.Terminals.Holder == nil {
			d.Terminals.CloseAll(terminal.CloseGrace)
			return nil
		}
		// The holders keep the Terminals running for the next pocketd. The tails stop before the store's last
		// flush, so no line they read is carried past without its items stored.
		terms := d.Carry()
		o.handover()
		if err := handoff.Write(handoff.Path(home), handoff.File{Format: handoff.Format, Terminals: terms}); err != nil {
			log.Printf("handoff: %v", err)
		}
		return nil
	}
	return err
}

// handedDown is what the pocketd before this one left: the file an upgrade's
// exec named, with the fds it kept open, or the one a pocketd that stopped
// wrote, whose fds died with it. Either is read once.
func handedDown(home, handed string) (handoff.File, error) {
	path := cmp.Or(handed, handoff.Path(home))
	f, err := handoff.Read(path)
	os.Remove(path)
	switch {
	case errors.Is(err, fs.ErrNotExist) && handed == "":
		return handoff.File{}, nil
	case err != nil && handed == "":
		log.Printf("handoff: %v", err)
		return handoff.File{}, nil
	case err != nil:
		return handoff.File{}, fmt.Errorf("handoff: %w", err)
	}
	if handed == "" {
		f.MCPFD, f.LockFD = 0, 0
		for i := range f.Terminals {
			f.Terminals[i].FD = 0
		}
	}
	return f, nil
}

// reap ends what a crashed pocketd left running (proc.Reap has the rule).
func reap(held []*holder.Client) {
	ps, err := proc.Orphans(os.Getuid())
	if err != nil {
		log.Printf("reap: %v", err)
		return
	}
	sessions := map[int]bool{}
	for _, c := range held {
		sessions[c.Pid()] = true
	}
	if pids := proc.Reap(os.Getpid(), proc.Alive, sessions, ps); len(pids) > 0 {
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
	if runtime.GOOS != "darwin" {
		return fallback
	}
	out, err := exec.Command("scutil", "--get", "ComputerName").Output()
	if name := strings.TrimSpace(string(out)); err == nil && name != "" {
		return name
	}
	return fallback
}
