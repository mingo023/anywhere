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
	f := registry.File{Projects: []string{plain, p}, Repos: map[string]registry.Repo{p: {ID: "p1", Name: "Pocket"}, plain: {ID: "p2"}}}
	for cwd, want := range map[string]Place{
		p:                           {Project: "Pocket", ProjectID: "p1", Tree: p, Worktree: "pocket", Branch: "main", Main: true},
		linked:                      {Project: "Pocket", ProjectID: "p1", Tree: linked, Worktree: "calm-otter", Branch: "calm-otter"},
		filepath.Join(plain, "src"): {Project: filepath.Base(plain), ProjectID: "p2", Tree: plain, Worktree: filepath.Base(plain), Main: true},
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

func TestResolvePlacesAFolderInItsProjectHoweverItIsSpelled(t *testing.T) {
	p := repo(t)
	plainReal := filepath.Join(filepath.Dir(p), "plain")
	os.MkdirAll(filepath.Join(plainReal, "src"), 0o755)
	links := t.TempDir()
	realRoot := t.TempDir()
	for name, to := range map[string]string{"repos": filepath.Dir(p), "root": realRoot} {
		if err := os.Symlink(to, filepath.Join(links, name)); err != nil {
			symlinkFailed(t, err)
		}
	}
	plain := filepath.Join(links, "repos", "plain")
	root := filepath.Join(links, "root")
	f := registry.File{Projects: []string{plain, p}, Repos: map[string]registry.Repo{p: {Name: "Pocket"}}, Worktree: registry.Worktree{Root: root}}
	linked := filepath.Join(realRoot, "Pocket", "calm-otter")
	git(t, p, "worktree", "add", "-q", "-b", "calm-otter", linked)
	gone := filepath.Join(realRoot, "Pocket", "gone")
	git(t, p, "worktree", "add", "-q", "-b", "gone", gone)
	os.RemoveAll(gone)
	for folder, want := range map[string]string{
		p + "/":                                 p,
		plain:                                   plain,
		filepath.Join(links, "repos", "pocket"): p,
		linked:                                  p,
		gone:                                    p,
		filepath.Join(root, "Pocket", "never-made"): p,
		filepath.Join(plainReal, "src"):             plain,
	} {
		if got, ok := Resolve(f, folder); !ok || got != want {
			t.Errorf("%s: %q %v, want %q", folder, got, ok, want)
		}
	}
	for _, folder := range []string{t.TempDir(), "", "pocket"} {
		if got, ok := Resolve(f, folder); ok {
			t.Errorf("%q: %q", folder, got)
		}
	}
}
