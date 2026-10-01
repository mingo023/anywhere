package worktree

import (
	"os"
	"path/filepath"
	"testing"

	"pocketd/internal/registry"
)

func TestLocateFindsTheCheckoutAndItsSharedRepo(t *testing.T) {
	p := repo(t)
	linked := filepath.Join(filepath.Dir(p), "calm-otter")
	git(t, p, "worktree", "add", "-q", "-b", "calm-otter", linked)
	os.MkdirAll(filepath.Join(linked, "sub", "deep"), 0o755)
	git(t, p, "worktree", "add", "-q", "--detach", filepath.Join(filepath.Dir(p), "loose"))
	common := filepath.Join(p, ".git")
	for cwd, want := range map[string]Location{
		p:                                       {Root: p, Common: common, Branch: "main", Main: true},
		filepath.Join(linked, "sub", "deep"):    {Root: linked, Common: common, Branch: "calm-otter"},
		filepath.Join(filepath.Dir(p), "loose"): {Root: filepath.Join(filepath.Dir(p), "loose"), Common: common},
	} {
		if got, ok := Locate(cwd); !ok || got != want {
			t.Errorf("%s: %+v, want %+v", cwd, got, want)
		}
	}
	if got, ok := Locate(t.TempDir()); ok {
		t.Errorf("not a repo: %+v", got)
	}
}

func TestFindNamesTheProjectAndWorktreeAnAgentWorksIn(t *testing.T) {
	p := repo(t)
	linked := filepath.Join(filepath.Dir(p), "calm-otter")
	git(t, p, "worktree", "add", "-q", "-b", "calm-otter", linked)
	plain := t.TempDir()
	os.Mkdir(filepath.Join(plain, "src"), 0o755)
	f := registry.File{Projects: []string{plain, p}, Repos: map[string]registry.Repo{p: {Name: "Pocket"}}}
	for cwd, want := range map[string]Place{
		p:                           {Project: "Pocket", Worktree: "pocket", Branch: "main", Main: true},
		linked:                      {Project: "Pocket", Worktree: "calm-otter", Branch: "calm-otter"},
		filepath.Join(plain, "src"): {Project: filepath.Base(plain), Worktree: filepath.Base(plain), Main: true},
	} {
		if got, ok := Find(f, cwd); !ok || got != want {
			t.Errorf("%s: %+v, want %+v", cwd, got, want)
		}
	}
	for _, cwd := range []string{t.TempDir(), "", "pocket"} {
		if got, ok := Find(f, cwd); ok {
			t.Errorf("%q: %+v", cwd, got)
		}
	}
}
