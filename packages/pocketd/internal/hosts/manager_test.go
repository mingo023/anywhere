package hosts

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"sync"
	"testing"
	"time"
)

func TestEditingAHostSavesItsNewTargetUnderTheSameID(t *testing.T) {
	m, err := openManager(t, Paths{Home: t.TempDir(), RemoteDir: ".cp"}, func() []string { return nil }, "1.2.0", "Mac")
	if err != nil {
		t.Fatal(err)
	}
	devbox, err := m.Add(alias("devbox"), "")
	if err != nil {
		t.Fatal(err)
	}
	other, err := m.Add(address("10.0.4.12"), "")
	if err != nil {
		t.Fatal(err)
	}
	over := Target{Kind: KindAlias, Name: "devbox", Port: 2222, User: "ops"}
	e, err := m.Edit(devbox.ID, over)
	if err != nil || e.ID != devbox.ID || e.Target != over {
		t.Fatalf("%+v %v", e, err)
	}
	saved, err := Load(m.db)
	if err != nil || len(saved) != 2 || saved[0].ID != devbox.ID || saved[0].Target != over {
		t.Fatalf("saved %+v %v", saved, err)
	}
	if _, err := m.Edit(devbox.ID, other.Target); !errors.As(err, new(ExistsError)) {
		t.Fatalf("another host's target: %v", err)
	}
	if _, err := m.Edit("nope", over); !errors.Is(err, ErrUnknown) {
		t.Fatalf("an unknown host: %v", err)
	}
	m.installs[devbox.ID] = true
	if _, err := m.Edit(devbox.ID, alias("devbox")); !errors.Is(err, ErrBusy) {
		t.Fatalf("mid-install: %v", err)
	}
	if e, err := m.Edit(devbox.ID, over); err != nil || e.Target != over {
		t.Fatalf("the same target mid-install changes nothing: %+v %v", e, err)
	}
}

func TestAnInstallAlreadyUnderWayIsRefused(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("the stand-in ssh is a shell script")
	}
	// The only PATH ssh is looked up on, so the real one never runs.
	bin := t.TempDir()
	release := filepath.Join(bin, "release")
	script := "#!/bin/sh\nwhile [ ! -e '" + release + "' ]; do /bin/sleep 0.05; done\necho 'ssh: connect to host devbox port 22: Connection refused' >&2\nexit 255\n"
	if err := os.WriteFile(filepath.Join(bin, "ssh"), []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	m, err := openManager(t, Paths{Home: t.TempDir(), RemoteDir: ".cp"}, func() []string { return []string{"PATH=" + bin} }, "1.2.0", "Mac")
	if err != nil {
		t.Fatal(err)
	}
	devbox, err := m.Add(alias("devbox"), "")
	if err != nil {
		t.Fatal(err)
	}
	progress := make(chan Progress, 8)
	if err := m.Install(devbox.ID, false, "", func(p Progress) { progress <- p }); err != nil {
		t.Fatal(err)
	}
	if err := m.Install(devbox.ID, true, "", func(Progress) {}); !errors.Is(err, ErrBusy) {
		t.Fatalf("a second install: %v", err)
	}
	if err := os.WriteFile(release, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	for {
		select {
		case p := <-progress:
			if p.Kind != ProgressError {
				continue
			}
			if want := FailRefused.Message("devbox"); p.Text != want {
				t.Fatalf("%q, want %q", p.Text, want)
			}
			return
		case <-time.After(10 * time.Second):
			t.Fatal("the first install never ended")
		}
	}
}

// standIn is an ssh, and an ssh-keygen, that hang until released, then record the host's key as accept-new does and
// fail; started closes once ssh runs. A child it leaves holds ssh's pipes past a kill, as a ProxyCommand's can.
func standIn(t *testing.T) (env func() []string, started <-chan struct{}, release func()) {
	t.Helper()
	if runtime.GOOS == "windows" {
		t.Skip("the stand-in ssh is a shell script")
	}
	bin := t.TempDir()
	mark, gate := filepath.Join(bin, "started"), filepath.Join(bin, "release")
	ssh := `#!/bin/sh
/bin/sleep 5 &
/usr/bin/touch '` + mark + `'
while [ ! -e '` + gate + `' ]; do /bin/sleep 0.02; done
for a in "$@"; do
	case "$a" in UserKnownHostsFile=*) f=${a#UserKnownHostsFile=\"}; /usr/bin/touch "${f%%\"*}";; esac
done
echo 'ssh: connect to host devbox port 22: Connection refused' >&2
exit 255
`
	keygen := "#!/bin/sh\nwhile [ \"$1\" != -f ]; do shift; done\necho key > \"$2\"; echo pub > \"$2.pub\"\n"
	for name, script := range map[string]string{"ssh": ssh, "ssh-keygen": keygen} {
		if err := os.WriteFile(filepath.Join(bin, name), []byte(script), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	ch := make(chan struct{})
	go func() {
		for {
			if _, err := os.Stat(mark); err == nil {
				close(ch)
				return
			}
			time.Sleep(10 * time.Millisecond)
		}
	}()
	return func() []string { return []string{"PATH=" + bin} }, ch, func() { os.WriteFile(gate, nil, 0o600) }
}

func filesOf(t *testing.T, home, id string) []string {
	t.Helper()
	found, _ := filepath.Glob(filepath.Join(home, "hosts", id+".*"))
	return found
}

func TestRemovingAHostMidInstallOrSignInLeavesNoneOfItsFiles(t *testing.T) {
	for name, start := range map[string]func(*Manager, string, chan<- struct{}){
		"install": func(m *Manager, id string, ended chan<- struct{}) {
			m.Install(id, false, "", func(p Progress) {
				if p.Kind == ProgressError {
					close(ended)
				}
			})
		},
		"sign-in": func(m *Manager, id string, ended chan<- struct{}) {
			go func() {
				m.SignIn(id, "hunter2")
				close(ended)
			}()
		},
	} {
		t.Run(name, func(t *testing.T) {
			env, started, release := standIn(t)
			home := t.TempDir()
			m, err := openManager(t, Paths{Home: home, RemoteDir: ".cp"}, env, "1.2.0", "Mac")
			if err != nil {
				t.Fatal(err)
			}
			devbox, err := m.Add(alias("devbox"), "")
			if err != nil {
				t.Fatal(err)
			}
			ended := make(chan struct{})
			start(m, devbox.ID, ended)
			<-started

			if err := m.Remove(devbox.ID, false); err != nil {
				t.Fatal(err)
			}
			release()
			select {
			case <-ended:
			case <-time.After(10 * time.Second):
				t.Fatal("it never ended")
			}

			if found := filesOf(t, home, devbox.ID); len(found) != 0 {
				t.Errorf("left %v", found)
			}
			if l := m.List(); len(l) != 0 {
				t.Errorf("listed %+v", l)
			}
		})
	}
}

func TestAHostAddedAgainBeforeItsRemovalFinishesKeepsItsFiles(t *testing.T) {
	env, started, release := standIn(t)
	defer release()
	home := t.TempDir()
	m, err := openManager(t, Paths{Home: home, RemoteDir: ".cp"}, env, "1.2.0", "Mac")
	if err != nil {
		t.Fatal(err)
	}
	devbox, err := m.Add(alias("devbox"), "")
	if err != nil {
		t.Fatal(err)
	}
	ended := make(chan struct{})
	// The desktop's conn holds a lock while it removes that its pushes of the install's progress take.
	var conn sync.Mutex
	m.Install(devbox.ID, false, "", func(p Progress) {
		conn.Lock()
		defer conn.Unlock()
		if p.Kind == ProgressError {
			close(ended)
		}
	})
	<-started
	removed := make(chan error, 1)
	conn.Lock()
	go func() { removed <- m.Remove(devbox.ID, false) }()
	select {
	case err := <-removed:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("the removal waited on the install's progress")
	}
	conn.Unlock()

	again, err := m.Add(alias("devbox"), "")
	if err != nil {
		t.Fatal(err)
	}
	known := m.paths.KnownHosts(again.ID)
	if err := os.WriteFile(known, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	select {
	case <-ended:
	case <-time.After(10 * time.Second):
		t.Fatal("the install never ended")
	}

	if found := filesOf(t, home, devbox.ID); again.ID != devbox.ID && len(found) != 0 {
		t.Errorf("the removed host left %v", found)
	}
	if _, err := os.Stat(known); err != nil {
		t.Errorf("the host added again lost its known_hosts: %v", err)
	}
	if l := m.List(); len(l) != 1 || l[0].ID != again.ID {
		t.Errorf("listed %+v", l)
	}
}

func TestAHostWaitingOutABackoffRetriesWhenTheClockJumpsPastASleep(t *testing.T) {
	m := &Manager{
		links: map[string]Link{"down": {State: StateRetrying}, "up": {State: StateOnline}, "refused": {State: StateFailed}},
		sups: map[string]*supervisor{
			"down":    {events: make(chan event, 16)},
			"up":      {events: make(chan event, 16)},
			"refused": {events: make(chan event, 16)},
		},
	}
	tick := make(chan time.Time)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go m.retryAfterWake(ctx, tick, time.Second)

	t0 := time.Now().Round(0)
	// A send is taken only once the one before it is handled.
	tick <- t0
	tick <- t0.Add(time.Second)
	tick <- t0.Add(2 * time.Second)
	if n := len(m.sups["down"].events); n != 0 {
		t.Fatalf("a steady clock retried %d times", n)
	}
	tick <- t0.Add(time.Hour)
	tick <- t0.Add(time.Hour + time.Second)
	select {
	case e := <-m.sups["down"].events:
		if e.kind != evRetry {
			t.Fatalf("%+v", e)
		}
	default:
		t.Fatal("a clock that jumped past a sleep retried nothing")
	}
	if n := len(m.sups["up"].events); n != 0 {
		t.Fatalf("an online host retried %d times", n)
	}
	if n := len(m.sups["refused"].events); n != 0 {
		t.Fatalf("a failed host retried %d times", n)
	}
}
