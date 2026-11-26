//go:build unix

package e2e

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"testing"

	"pocketd/internal/handoff"
)

// jsonPocketd is the last commit whose pocketd kept its state in JSON files, before #397 moved it to pocketd.db.
const jsonPocketd = "8a9f4f61cba2a12be6607e73bbc73c329e43ac69"

type build struct {
	once sync.Once
	bin  string
	err  error
	skip string
	// format is the handoff format its pocketd writes and reads up to.
	format int
}

var (
	buildsMu sync.Mutex
	builds   = map[string]*build{}
)

// mainPocketd builds the pocketd at origin/main once a run.
func mainPocketd(t *testing.T) (string, int) {
	t.Helper()
	b := pocketdAt(t, "main", "origin/main", "+refs/heads/main:refs/remotes/origin/main")
	return b.bin, b.format
}

// pocketdAt builds the pocketd at rev once a run, fetching refspec first, from a detached worktree it removes again,
// into binDir.
func pocketdAt(t *testing.T, name, rev, refspec string) *build {
	t.Helper()
	buildsMu.Lock()
	b, ok := builds[name]
	if !ok {
		b = &build{bin: filepath.Join(binDir, "pocketd-"+name)}
		builds[name] = b
	}
	buildsMu.Unlock()
	b.once.Do(func() {
		git := func(args ...string) ([]byte, error) { return exec.Command("git", args...).CombinedOutput() }
		top, err := git("rev-parse", "--show-toplevel")
		if err != nil {
			b.err = fmt.Errorf("git rev-parse: %v: %s", err, top)
			return
		}
		repo := strings.TrimSpace(string(top))
		// A single-branch checkout, as CI's, gets another ref only by its full refspec. Offline, the one fetched
		// last stands in.
		fetch := []string{"-C", repo, "fetch", "-q", "origin", refspec}
		if shallow, _ := git("-C", repo, "rev-parse", "--is-shallow-repository"); strings.TrimSpace(string(shallow)) == "true" {
			fetch = append(fetch, "--depth=1")
		}
		git(fetch...)
		if _, err := git("-C", repo, "rev-parse", "--verify", "-q", rev+"^{commit}"); err != nil {
			b.skip = rev + " isn't here and couldn't be fetched, so there is no pocketd to upgrade from"
			return
		}
		src := filepath.Join(binDir, fmt.Sprintf("pocketd-%s-%d", name, os.Getpid()))
		if out, err := git("-C", repo, "worktree", "add", "-q", "--detach", src, rev); err != nil {
			b.err = fmt.Errorf("git worktree add: %v: %s", err, out)
			return
		}
		defer git("-C", repo, "worktree", "remove", "--force", src)
		code, err := os.ReadFile(filepath.Join(src, "packages", "pocketd", "internal", "handoff", "handoff.go"))
		m := regexp.MustCompile(`(?m)^\s*Format\s*=\s*(\d+)`).FindSubmatch(code)
		if err != nil || m == nil {
			b.err = fmt.Errorf("%s's handoff format: %v", name, err)
			return
		}
		b.format, _ = strconv.Atoi(string(m[1]))
		// Its terminal emulator links libghostty from the checkout's untracked third_party.
		libs, _ := filepath.Glob(filepath.Join(repo, "third_party", "*"))
		os.MkdirAll(filepath.Join(src, "third_party"), 0o755)
		for _, lib := range libs {
			if real, err := filepath.EvalSymlinks(lib); err == nil {
				os.Symlink(real, filepath.Join(src, "third_party", filepath.Base(lib)))
			}
		}
		build := exec.Command("go", "build", "-o", b.bin, "./cmd/pocketd")
		build.Dir = filepath.Join(src, "packages", "pocketd")
		if out, err := build.CombinedOutput(); err != nil {
			b.err = fmt.Errorf("build %s: %v: %s", name, err, out)
		}
	})
	if b.skip != "" {
		t.Skip(b.skip)
	}
	if b.err != nil {
		t.Fatal(b.err)
	}
	return b
}

func sessionsListed(h *Harness, p *Phone, ids ...string) {
	h.t.Helper()
	h.eventually(fmt.Sprintf("%q listed", ids), func() bool {
		got := listed(p)
		for _, id := range ids {
			if !slices.Contains(got, id) {
				return false
			}
		}
		return true
	})
}

func hasReply(p *Phone, id, text string) bool {
	return slices.Contains(timelineOf(p, id), "assistant echo: "+text)
}

// The release gate: a pocketd from main upgrades in place to this one with
// nothing lost, its sessions then outlive a restart, and main's binary put
// back orphans no holder: a main that can't read this handoff format is
// refused rather than exec'd into, and one that can takes over.
func TestAnUpgradeFromMainKeepsEverySessionWhichOutlivesARestartAndMainPutBackOrphansNoHolder(t *testing.T) {
	old, mainFormat := mainPocketd(t)
	h := Start(t, launchReady(""), func(h *Harness) {
		h.resume = true
		h.exe = filepath.Join(h.Home, "bin", "pocketd")
		copyFile(t, old, h.exe)
	})
	phone := h.Owner()
	create(phone, "r1", inRepo(h, map[string]any{"chat": true}))
	created := reply(phone, "agent.created", "error")
	if created.Type != "agent.created" {
		t.Fatalf("got %s", created.Raw)
	}
	chat, chatTerm := created.AgentID, created.TerminalID
	h.WaitScreen(chatTerm, "fake claude ready")
	create(phone, "r2", inRepo(h, map[string]any{}))
	created = reply(phone, "agent.created", "error")
	if created.Type != "agent.created" {
		t.Fatalf("got %s", created.Raw)
	}
	inTerm, term := created.AgentID, created.TerminalID
	h.WaitScreen(term, "fake claude ready")
	for _, id := range []string{chat, inTerm} {
		phone.Prompt(id, "before")
		h.eventually("the reply before the upgrade", func() bool { return hasReply(phone, id, "before") })
	}

	h.replaceBinary(filepath.Join(binDir, "pocketd-next"))
	h.eventually("pocketd e2e-next", func() bool { return h.Status().Version == "e2e-next" })

	again := h.Owner()
	sessionsListed(h, again, chat, inTerm)
	for _, id := range []string{chat, inTerm} {
		h.eventually("the history after the upgrade", func() bool { return hasReply(again, id, "before") })
		// The session is adopted again a moment after its history shows; until then a prompt is refused.
		h.eventually("the session takes a prompt", func() bool {
			again.Prompt(id, "after")
			return again.WaitFor("the prompt's answer", func(m Message) bool {
				return m.ID == "prompt-after" && (m.Type == "ack" || m.Type == "error")
			}).Type == "ack"
		})
		h.eventually("the reply after the upgrade", func() bool { return hasReply(again, id, "after") })
	}

	if code := h.Stop(syscall.SIGTERM); code != 0 {
		t.Fatalf("pocketd exited %d", code)
	}
	pids := heldPids(h)
	if pids[chatTerm] == 0 || pids[term] == 0 {
		t.Fatalf("held %v", pids)
	}
	h.up()
	last := h.Owner("host.v1")
	sessionsListed(h, last, chat, inTerm)
	for _, a := range last.agents() {
		if a.Restore != "" {
			t.Fatalf("agent %s was restored: %s", a.ID, a.Restore)
		}
	}

	h.replaceBinary(old)
	if mainFormat < handoff.Format {
		if out, code := h.Pocketd("upgrade"); code == 0 || !strings.Contains(out, fmt.Sprintf("format %d", handoff.Format)) {
			t.Fatalf("upgrade to main exited %d: %s", code, out)
		}
		last.WaitFor("upgradeFailed", func(m Message) bool { return m.Type == "host.changed" && m.Host.UpgradeFailed != "" })
		if v := h.Status().Version; v != "e2e-next" {
			t.Fatalf("pocketd %s after main was put back", v)
		}
	} else {
		if out, code := h.Pocketd("upgrade"); code != 0 {
			t.Fatalf("upgrade to main exited %d: %s", code, out)
		}
		h.eventually("main's pocketd", func() bool { return h.Status().Version != "e2e-next" })
		sessionsListed(h, h.Owner(), chat, inTerm)
	}
	for _, pid := range pids {
		if syscall.Kill(pid, 0) != nil {
			t.Fatalf("agent %d died", pid)
		}
	}
}
