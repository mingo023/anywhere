package hosts

import (
	"cmp"
	"context"
	"database/sql"
	"errors"
	"fmt"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"sync"
	"time"

	"pocketd/internal/hostop"
	"pocketd/internal/proto"
	"pocketd/internal/terminal"
)

var (
	ErrUnknown = errors.New("That host isn't added")
	ErrBusy    = errors.New("An install is already under way on that host")
)

const wakeCheckEvery = 5 * time.Second

type Entry struct {
	Host
	Socket string
	Link   Link
}

type Manager struct {
	db    *sql.DB
	paths Paths
	// env is what ssh runs with: the login shell's, so the user's agent, config and PATH apply.
	env     func() []string
	version string
	// name is this machine's, which a Windows host's pocketd names its owner token after.
	name string
	// Mirror, set before Start, keeps here the sessions this Mac starts on its hosts.
	Mirror Mirror

	mu sync.Mutex
	// ctx is nil until Start, and no link runs before.
	ctx      context.Context
	hosts    []Host
	links    map[string]Link
	sups     map[string]*supervisor
	installs map[string]bool
	// work is what runs ssh to each host: its installs and sign-ins, and its removal until the tunnel is down.
	work    map[string]*work
	changed chan struct{}
	// remotes is each online host's conn to its pocketd, last the latest one each host had, and summaries the
	// sessions mirrored from them.
	remotes   map[string]*remote
	last      map[string]*remote
	summaries map[string]proto.AgentSummary
}

func Open(db *sql.DB, paths Paths, env func() []string, version, name string) (*Manager, error) {
	saved, err := Load(db)
	if err != nil {
		return nil, err
	}
	m := &Manager{db: db, paths: paths, env: env, version: version, name: name, hosts: saved,
		links: map[string]Link{}, sups: map[string]*supervisor{}, installs: map[string]bool{}, work: map[string]*work{}, changed: make(chan struct{}),
		remotes: map[string]*remote{}, last: map[string]*remote{}, summaries: map[string]proto.AgentSummary{}}
	for _, h := range saved {
		m.links[h.ID] = Link{State: StateConnecting}
	}
	return m, nil
}

func (m *Manager) Start(ctx context.Context) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.ctx = ctx
	for _, h := range m.hosts {
		m.supervise(h.ID)
	}
	ticker := time.NewTicker(wakeCheckEvery)
	go func() {
		defer ticker.Stop()
		m.retryAfterWake(ctx, ticker.C, wakeCheckEvery)
	}()
}

// retryAfterWake retries every host waiting out a backoff when a tick comes in much later than it was due, as after the
// machine slept: its network is back, but the links still wait. A failed host waits for the user, as a refused key does.
func (m *Manager) retryAfterWake(ctx context.Context, tick <-chan time.Time, every time.Duration) {
	var last time.Time
	for {
		select {
		case <-ctx.Done():
			return
		case now := <-tick:
			// Round(0) drops the monotonic reading, which stands still while the machine sleeps.
			if !last.IsZero() && now.Round(0).Sub(last.Round(0)) > 2*every {
				m.retryWaiting()
			}
			last = now
		}
	}
}

func (m *Manager) retryWaiting() {
	m.mu.Lock()
	defer m.mu.Unlock()
	for id, s := range m.sups {
		if m.links[id].State == StateRetrying {
			s.retry()
		}
	}
}

func (m *Manager) List() []Entry {
	m.mu.Lock()
	defer m.mu.Unlock()
	out := make([]Entry, len(m.hosts))
	for i, h := range m.hosts {
		out[i] = m.entry(h)
	}
	return out
}

func (m *Manager) Endpoint(id string) (socket, token string, ok bool) {
	if _, ok := m.host(id); !ok {
		return "", "", false
	}
	owner, _ := loadOwner(m.paths.Token(id))
	return m.paths.Socket(id), owner.Token, true
}

func (m *Manager) Changed() <-chan struct{} {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.changed
}

func (m *Manager) Add(t Target, name string) (Entry, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	taken := slices.Clone(m.hosts)
	for id, w := range m.work {
		if w.removed {
			// Its files go once its work ends, and mustn't be another host's by then.
			taken = append(taken, Host{ID: id})
		}
	}
	h, err := Add(taken, t)
	if err != nil {
		return Entry{}, err
	}
	h.Name = cmp.Or(name, h.Name)
	list := append(slices.Clone(m.hosts), h)
	if err := Save(m.db, m.hosts, list); err != nil {
		return Entry{}, err
	}
	m.hosts = list
	m.links[h.ID] = Link{State: StateConnecting}
	m.supervise(h.ID)
	m.notify()
	return m.entry(h), nil
}

// Edit points the host at t, keeping its id and files, and reconnects it; the same target changes nothing.
func (m *Manager) Edit(id string, t Target) (Entry, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	i := m.index(id)
	switch {
	case i < 0:
		return Entry{}, ErrUnknown
	case m.hosts[i].Target == t:
		return m.entry(m.hosts[i]), nil
	case m.installs[id]:
		return Entry{}, ErrBusy
	}
	h, err := Edit(m.hosts, m.hosts[i], t)
	if err != nil {
		return Entry{}, err
	}
	list := slices.Clone(m.hosts)
	list[i] = h
	if err := Save(m.db, m.hosts, list); err != nil {
		return Entry{}, err
	}
	m.hosts = list
	// The supervisor reads the target on each connect, so a retry drops the old tunnel for one to t.
	if s := m.sups[id]; s != nil {
		s.retry()
	}
	m.notify()
	return m.entry(h), nil
}

// Remove closes the host's tunnel and forgets it with its files here. With uninstall it first removes pocketd's
// service there; if that fails the host stays.
func (m *Manager) Remove(id string, uninstall bool) error {
	h, ok := m.host(id)
	if !ok {
		return ErrUnknown
	}
	if uninstall {
		if err := m.uninstall(context.Background(), h); err != nil {
			return err
		}
	}
	m.mu.Lock()
	i := m.index(id)
	if i < 0 {
		m.mu.Unlock()
		return ErrUnknown
	}
	list := slices.Delete(slices.Clone(m.hosts), i, i+1)
	if err := Save(m.db, m.hosts, list); err != nil {
		m.mu.Unlock()
		return err
	}
	s := m.sups[id]
	m.hosts = list
	delete(m.sups, id)
	delete(m.links, id)
	_, done := m.begin(id)
	m.work[id].removed = true
	m.work[id].cancel()
	m.follow(id, Link{})
	delete(m.last, id)
	m.notify()
	m.mu.Unlock()
	if s != nil {
		s.stop()
	}
	done()
	if m.Mirror != nil {
		m.Mirror.Removed(id, h.Name)
	}
	return nil
}

type work struct {
	ctx     context.Context
	cancel  context.CancelFunc
	n       int
	removed bool
}

// begin counts work on id under way, on ctx, until done; the last done of a removed host forgets its files, which
// what ran ssh there could have written until then. m.mu is held.
func (m *Manager) begin(id string) (ctx context.Context, done func()) {
	w := m.work[id]
	if w == nil {
		ctx, cancel := context.WithCancel(context.Background())
		w = &work{ctx: ctx, cancel: cancel}
		m.work[id] = w
	}
	w.n++
	return w.ctx, func() {
		m.mu.Lock()
		w.n--
		last := w.n == 0
		if last {
			delete(m.work, id)
			w.cancel()
		}
		m.mu.Unlock()
		if last && w.removed {
			for _, f := range []string{m.paths.KnownHosts(id), m.paths.Token(id), m.paths.Key(id), m.paths.Key(id) + ".pub"} {
				os.Remove(f)
			}
		}
	}
}

func (m *Manager) Retry(id string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.index(id) < 0 {
		return ErrUnknown
	}
	if s := m.sups[id]; s != nil {
		s.retry()
	}
	return nil
}

// Install puts pocketd on the host, or with update installs over the one there, in its own goroutine; on gets
// each step. Once pocketd runs there the link reconnects rather than wait out its backoff.
func (m *Manager) Install(id string, update bool, archiveDir string, on func(Progress)) error {
	m.mu.Lock()
	h, ok := m.hostLocked(id)
	busy := m.installs[id]
	var ctx context.Context
	var done func()
	if ok && !busy {
		m.installs[id] = true
		ctx, done = m.begin(id)
	}
	m.mu.Unlock()
	switch {
	case !ok:
		return ErrUnknown
	case busy:
		return ErrBusy
	}
	go func() {
		// Finding the platform and uploading the archive come before bootstrap says it checks the host.
		on(Progress{Kind: ProgressStep, Step: StepDetect})
		err := m.install(ctx, h, update, archiveDir, on)
		m.mu.Lock()
		delete(m.installs, id)
		m.mu.Unlock()
		done()
		if err != nil {
			_, final := errors.AsType[FinalError](err)
			on(Progress{Kind: ProgressError, Text: message(err, h.Name), Final: final})
			return
		}
		m.Retry(id)
	}()
	return nil
}

// m.mu is held.
func (m *Manager) supervise(id string) {
	if m.ctx == nil {
		return
	}
	ctx, cancel := context.WithCancel(m.ctx)
	s := &supervisor{m: m, id: id, events: make(chan event, 16), cancel: cancel, done: make(chan struct{}), every: checkEvery, within: replyWithin}
	m.sups[id] = s
	go s.run(ctx)
}

// report records s's link, unless s no longer supervises its host.
func (m *Manager) report(s *supervisor, l Link) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.sups[s.id] != s {
		return
	}
	m.follow(s.id, l)
	if m.links[s.id] != l {
		m.links[s.id] = l
		m.notify()
	}
}

// setWindows saves what the host runs, once an install's probe or its pocketd said.
func (m *Manager) setWindows(id string, windows bool) {
	m.mu.Lock()
	defer m.mu.Unlock()
	i := m.index(id)
	if i < 0 || m.hosts[i].Windows == windows {
		return
	}
	next := slices.Clone(m.hosts)
	next[i].Windows = windows
	if err := Save(m.db, m.hosts, next); err != nil {
		log.Printf("hosts: %v", err)
		return
	}
	m.hosts = next
	m.notify()
}

func (m *Manager) notify() {
	close(m.changed)
	m.changed = make(chan struct{})
}

func (m *Manager) entry(h Host) Entry { return Entry{h, m.paths.Socket(h.ID), m.links[h.ID]} }

func (m *Manager) index(id string) int {
	return slices.IndexFunc(m.hosts, func(h Host) bool { return h.ID == id })
}

func (m *Manager) host(id string) (Host, bool) {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.hostLocked(id)
}

func (m *Manager) hostLocked(id string) (Host, bool) {
	if i := m.index(id); i >= 0 {
		return m.hosts[i], true
	}
	return Host{}, false
}

func (m *Manager) command(ctx context.Context, args []string) (*exec.Cmd, error) {
	env := m.env()
	path, err := terminal.LookPath("ssh", env)
	if err != nil {
		return nil, err
	}
	cmd := exec.CommandContext(ctx, path, args...)
	cmd.Env = env
	// ssh's children on the host side of a fake, or a ProxyCommand, can hold its pipes past the kill.
	cmd.WaitDelay = time.Second
	return cmd, nil
}

func (m *Manager) sshArgs(h Host, extra []string, command string) []string {
	known := m.paths.KnownHosts(h.ID)
	// Before ssh runs, or it can't record a host key it accepts, and a password sign-in then won't trust it.
	os.MkdirAll(filepath.Dir(known), 0o700)
	key := m.paths.Key(h.ID)
	if _, err := os.Stat(key); err != nil {
		key = ""
	}
	return args(h, known, key, extra, command)
}

// run is command's output on the host; an *SSHError only when ssh itself failed, which it says by exiting 255.
func (m *Manager) run(ctx context.Context, h Host, command string, stdin []byte) (stdout, stderr []byte, code int, err error) {
	code, stdout, stderr, err = hostop.Run(ctx, m.env(), "", append([]string{"ssh"}, m.sshArgs(h, nil, command)...), stdin, 0)
	switch {
	case err != nil:
		err = fmt.Errorf("Couldn't run ssh: %w", err)
	case code == 255:
		err = &SSHError{classify(string(stderr))}
	}
	return stdout, stderr, code, err
}

func failureOf(err error) Failure {
	if e, ok := errors.AsType[*SSHError](err); ok {
		return e.Failure
	}
	if err != nil {
		return FailOther
	}
	return ""
}

func message(err error, name string) string {
	if e, ok := errors.AsType[*SSHError](err); ok {
		return e.Failure.Message(name)
	}
	return err.Error()
}
