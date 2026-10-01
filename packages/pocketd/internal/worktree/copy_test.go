package worktree

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"pocketd/internal/registry"
)

func TestTheCopySetIsTheRepoListElseRootEnvFilesPlusWorktreeinclude(t *testing.T) {
	p := t.TempDir()
	for _, name := range []string{".env", ".env.local", "README.md"} {
		os.WriteFile(filepath.Join(p, name), nil, 0o600)
	}
	os.WriteFile(filepath.Join(p, ".worktreeinclude"), []byte("# local config\n\nconfig/local.json\n./.env\n"), 0o600)
	if got, want := copySet(registry.File{}, p), []string{".env", ".env.local", "config/local.json"}; !reflect.DeepEqual(got, want) {
		t.Errorf("default: %v", got)
	}
	f := registered(p, registry.Repo{Copy: []string{"secrets.json"}})
	if got, want := copySet(f, p), []string{"secrets.json", "config/local.json", ".env"}; !reflect.DeepEqual(got, want) {
		t.Errorf("listed: %v", got)
	}
}

func TestCopyNeverOverwritesAndTakesOnlyPlainFilesInsideTheProject(t *testing.T) {
	p, dst, outside := t.TempDir(), t.TempDir(), t.TempDir()
	os.WriteFile(filepath.Join(p, ".env"), []byte("A=1"), 0o600)
	os.WriteFile(filepath.Join(p, ".env.local"), []byte("B=2"), 0o644)
	os.Symlink(filepath.Join(p, ".env"), filepath.Join(p, ".env.link"))
	os.Mkdir(filepath.Join(p, ".env.d"), 0o755)
	os.MkdirAll(filepath.Join(p, "sub"), 0o755)
	os.WriteFile(filepath.Join(p, "sub", "deep.txt"), []byte("deep"), 0o644)
	os.WriteFile(filepath.Join(outside, "secret"), []byte("s"), 0o600)
	os.Symlink(outside, filepath.Join(p, "linked"))
	os.WriteFile(filepath.Join(p, ".worktreeinclude"), []byte("sub/deep.txt\n../x\n/etc/hosts\nlinked/secret\ngone.txt\n"), 0o600)
	os.WriteFile(filepath.Join(dst, ".env.local"), []byte("mine"), 0o600)

	copied, skipped := CopyInto(registry.File{}, p, dst)
	if want := []string{".env", "sub/deep.txt"}; !reflect.DeepEqual(copied, want) {
		t.Errorf("copied %v", copied)
	}
	want := []string{".env.d: not a regular file", ".env.link: not a regular file", ".env.local: exists",
		"../x: outside the project", "/etc/hosts: outside the project", "linked/secret: outside the project", "gone.txt: missing"}
	if !reflect.DeepEqual(skipped, want) {
		t.Errorf("skipped %q", skipped)
	}
	if raw, _ := os.ReadFile(filepath.Join(dst, ".env.local")); string(raw) != "mine" {
		t.Errorf(".env.local = %q", raw)
	}
	if fi, err := os.Stat(filepath.Join(dst, ".env")); err != nil || fi.Mode().Perm() != 0o600 {
		t.Errorf(".env: %v, %v", fi, err)
	}
}

func TestCopyWritesNothingThroughASymlinkedFolderInTheWorktree(t *testing.T) {
	p, dst, outside := t.TempDir(), t.TempDir(), t.TempDir()
	os.MkdirAll(filepath.Join(p, "sub", "deeper"), 0o755)
	os.WriteFile(filepath.Join(p, "sub", "deeper", "deep.txt"), []byte("deep"), 0o644)
	os.Symlink(outside, filepath.Join(dst, "sub"))
	copied, skipped := CopyInto(registered(p, registry.Repo{Copy: []string{"sub/deeper/deep.txt"}}), p, dst)
	entries, _ := os.ReadDir(outside)
	if len(copied) != 0 || !reflect.DeepEqual(skipped, []string{"sub/deeper/deep.txt: outside the project"}) || len(entries) != 0 {
		t.Fatalf("copied %v, skipped %q, outside holds %d", copied, skipped, len(entries))
	}
}
