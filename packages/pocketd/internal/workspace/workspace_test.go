package workspace

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"pocketd/internal/accounts"
	"pocketd/internal/proto"
	"pocketd/internal/registry"
)

func git(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	cmd.Env = append(os.Environ(), "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_NOSYSTEM=1",
		"GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@t", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@t")
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
	return strings.TrimSpace(string(out))
}

// repo is a fresh repo named name on branch main, at its real path.
func repo(t *testing.T, name string) string {
	t.Helper()
	root, err := filepath.EvalSymlinks(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	dir := filepath.Join(root, name)
	os.Mkdir(dir, 0o755)
	git(t, dir, "init", "-q", "-b", "main")
	git(t, dir, "commit", "-q", "--allow-empty", "-m", "init")
	return dir
}

// registryOf lists every worktree, as the desktop does with external worktrees on.
func registryOf(t *testing.T, projects ...string) *registry.Registry {
	t.Helper()
	path := filepath.Join(t.TempDir(), "desktop.json")
	write(path, registry.File{Projects: projects, Sidebar: registry.Sidebar{ExternalWorktrees: true}})
	return registry.New(path)
}

func write(path string, f registry.File) {
	raw, _ := json.Marshal(f)
	os.WriteFile(path, raw, 0o600)
}

// pushed is the next list published, or nil when none comes.
func pushed(msgs <-chan []byte) []proto.Project {
	select {
	case raw := <-msgs:
		var m proto.ProjectList
		json.Unmarshal(raw, &m)
		return m.Projects
	case <-time.After(100 * time.Millisecond):
		return nil
	}
}

func paths(p proto.Project) []string {
	var out []string
	for _, w := range p.Worktrees {
		out = append(out, w.Path+"@"+w.Branch)
	}
	return out
}

func TestTheProjectListIsPushedOnceThenOnlyWhenAWorktreeComesGoesOrSwitches(t *testing.T) {
	p := repo(t, "pocket")
	ps := NewProjects(registryOf(t, p))
	msgs, stop := ps.Subscribe()
	defer stop()
	if _, ok := ps.List(); ok {
		t.Fatal("a list before the first scan")
	}
	ps.Scan()
	if got := pushed(msgs); len(got) != 1 || !reflect.DeepEqual(paths(got[0]), []string{p + "@main"}) {
		t.Fatalf("first scan: %v", got)
	}
	ps.Scan()
	if got := pushed(msgs); got != nil {
		t.Fatalf("an unchanged scan pushed %v", got)
	}
	wt := filepath.Join(filepath.Dir(p), "calm-otter")
	git(t, p, "worktree", "add", "-q", "-b", "calm-otter", wt)
	ps.Scan()
	if got := pushed(msgs); len(got) != 1 || !reflect.DeepEqual(paths(got[0]), []string{p + "@main", wt + "@calm-otter"}) {
		t.Fatalf("after add: %v", got)
	}
	time.Sleep(10 * time.Millisecond)
	git(t, wt, "switch", "-q", "-c", "fix-login")
	ps.Scan()
	if got := pushed(msgs); len(got) != 1 || !reflect.DeepEqual(paths(got[0]), []string{p + "@main", wt + "@fix-login"}) {
		t.Fatalf("after switch: %v", got)
	}
	git(t, p, "worktree", "remove", wt)
	ps.Scan()
	if got := pushed(msgs); len(got) != 1 || len(got[0].Worktrees) != 1 {
		t.Fatalf("after remove: %v", got)
	}
	if list, ok := ps.List(); !ok || len(list) != 1 {
		t.Fatalf("cache %v %v", list, ok)
	}
}

func TestTrackingAWorktreeOnTheDesktopPushesTheListWithIt(t *testing.T) {
	p := repo(t, "pocket")
	wt := filepath.Join(filepath.Dir(p), "calm-otter")
	git(t, p, "worktree", "add", "-q", "-b", "calm-otter", wt)
	path := filepath.Join(t.TempDir(), "desktop.json")
	write(path, registry.File{Projects: []string{p}})
	ps := NewProjects(registry.New(path))
	msgs, stop := ps.Subscribe()
	defer stop()
	ps.Scan()
	if got := pushed(msgs); len(got) != 1 || !reflect.DeepEqual(paths(got[0]), []string{p + "@main"}) {
		t.Fatalf("before tracking: %v", got)
	}
	time.Sleep(10 * time.Millisecond)
	write(path, registry.File{Projects: []string{p}, Tracked: []string{wt}})
	ps.Scan()
	if got := pushed(msgs); len(got) != 1 || !reflect.DeepEqual(paths(got[0]), []string{p + "@main", wt + "@calm-otter"}) {
		t.Fatalf("after tracking: %v", got)
	}
}

func TestAProjectWhoseFolderIsGoneLeavesTheListAndAPlainFolderIsItsOwnWorktree(t *testing.T) {
	plain := t.TempDir()
	ps := NewProjects(registryOf(t, filepath.Join(t.TempDir(), "gone"), plain))
	ps.Scan()
	list, _ := ps.List()
	if len(list) != 1 || list[0].Path != plain || len(list[0].Worktrees) != 1 || !list[0].Worktrees[0].IsMain {
		t.Fatalf("%+v", list)
	}
}

func TestRunScansAtOnceAndAgainWhenToldOfAChange(t *testing.T) {
	p := repo(t, "pocket")
	ps := NewProjects(registryOf(t, p))
	msgs, stop := ps.Subscribe()
	defer stop()
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go ps.Run(ctx)
	if got := pushed(msgs); got == nil {
		t.Fatal("no first scan")
	}
	git(t, p, "worktree", "add", "-q", "-b", "x", filepath.Join(filepath.Dir(p), "x"))
	ps.Changed()
	if got := pushed(msgs); got == nil || len(got[0].Worktrees) != 2 {
		t.Fatalf("%v", got)
	}
}

func TestBranchesComeNewestFirstWithTheCurrentOneAndRemotesWithoutTheirHead(t *testing.T) {
	p := repo(t, "pocket")
	git(t, p, "commit", "-q", "--allow-empty", "-m", "two", "--date", "@1790000000 +0000")
	git(t, p, "branch", "old", "HEAD~1")
	git(t, p, "update-ref", "refs/remotes/origin/main", "HEAD")
	git(t, p, "symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/main")
	current, local, remote, err := Branches(p)
	if err != nil || current != "main" {
		t.Fatalf("%q %v", current, err)
	}
	if len(local) != 2 || local[0].Name != "main" || local[1].Name != "old" || local[0].At < local[1].At {
		t.Fatalf("local %+v", local)
	}
	if len(remote) != 1 || remote[0].Name != "origin/main" {
		t.Fatalf("remote %+v", remote)
	}
	git(t, p, "switch", "-q", "--detach")
	if current, _, _, _ := Branches(p); current != "" {
		t.Fatalf("detached reads %q", current)
	}
}

func TestBranchesStopAtTwoHundredOnEachSide(t *testing.T) {
	var b strings.Builder
	for i := range 250 {
		fmt.Fprintf(&b, " \x00refs/heads/b%d\x00%d\x00\n \x00refs/remotes/origin/b%d\x00%d\x00\n", i, 2000-i, i, 2000-i)
	}
	b.WriteString("*\x00refs/heads/zz\x001\x00\n")
	current, local, remote := parseRefs([]byte(b.String()))
	if len(local) != MaxBranches || len(remote) != MaxBranches || current != "zz" || local[0] != (proto.Branch{Name: "b0", At: 2000}) {
		t.Fatalf("%d %d %q %+v", len(local), len(remote), current, local[0])
	}
}

func TestBranchesOfAFolderThatIsntARepoFailWithGitsReason(t *testing.T) {
	if _, _, _, err := Branches(t.TempDir()); err == nil || !strings.Contains(err.Error(), "not a git repository") {
		t.Fatalf("%v", err)
	}
}

func TestAnAccountIsNamedByItsLabelElseItsEmailAndOneSigningInIsLeftOut(t *testing.T) {
	list := []accounts.Account{
		{ID: accounts.BuiltIn, Provider: "claude", Email: "me@x.dev", Plan: "max", SignedIn: true, Default: true,
			Usage: &accounts.Usage{Session: &accounts.Window{Percent: 42.5, ResetsAt: 1790000000000}, Week: &accounts.Window{Percent: 120}}},
		{ID: "a1", Provider: "claude", Label: "Work", SignedIn: false},
		{ID: "a2", Provider: "codex"},
		{ID: "a3", Provider: "claude", Pending: true},
	}
	got := Accounts(list, "")
	want := []proto.Account{
		{ID: "builtin", Provider: "claude", Name: "me@x.dev", Plan: "max", Email: "me@x.dev", SignedIn: true, IsDefault: true,
			FiveHour: &proto.Usage{Pct: 42.5, ResetsAt: 1790000000}, Week: &proto.Usage{Pct: 100}},
		{ID: "a1", Provider: "claude", Name: "Work"},
		{ID: "a2", Provider: "codex", Name: "Not signed in"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
	if got := Accounts(list, "codex"); len(got) != 1 || got[0].ID != "a2" {
		t.Fatalf("codex: %+v", got)
	}
}
