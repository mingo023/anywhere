package fsview

import (
	"errors"
	"os"
	"path/filepath"
	"testing"

	"pocketd/internal/proto"
	"pocketd/internal/registry"
)

func codeOf(err error) string {
	if r, ok := errors.AsType[*Refusal](err); ok {
		return r.Code
	}
	return ""
}

// project makes a registered project folder holding src/a.rs, and a folder
// outside it holding secret.
func project(t *testing.T) (registry.File, string, string) {
	t.Helper()
	base, _ := filepath.EvalSymlinks(t.TempDir())
	root, outside := filepath.Join(base, "app"), filepath.Join(base, "elsewhere")
	for _, d := range []string{filepath.Join(root, "src"), outside} {
		if err := os.MkdirAll(d, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	os.WriteFile(filepath.Join(root, "src", "a.rs"), []byte("fn a() {}\n"), 0o644)
	os.WriteFile(filepath.Join(outside, "secret"), []byte("key"), 0o644)
	return registry.File{Projects: []string{root}, Worktree: registry.Worktree{Root: filepath.Join(base, "trees")}}, root, outside
}

func TestATreeIsAFolderOfARegisteredProject(t *testing.T) {
	f, root, outside := project(t)
	for _, tree := range []string{root, filepath.Join(root, "src")} {
		if got, err := Tree(f, tree); err != nil || got != tree {
			t.Errorf("%s: %q %v", tree, got, err)
		}
	}
	for _, tree := range []string{"", "app", outside, root + "/", root + "/src/..", filepath.Join(root, "src", "a.rs"), filepath.Join(root, "missing")} {
		if _, err := Tree(f, tree); codeOf(err) != proto.CodeNotATree {
			t.Errorf("%q: %v", tree, err)
		}
	}
}

func TestAWorktreeInTheProjectsWorktreesFolderIsATree(t *testing.T) {
	f, _, _ := project(t)
	wt := filepath.Join(f.Worktree.Root, "app", "calm-otter")
	os.MkdirAll(wt, 0o755)
	if got, err := Tree(f, wt); err != nil || got != wt {
		t.Fatalf("%q %v", got, err)
	}
}

func TestALinkInsideAProjectCantNameATreeElsewhere(t *testing.T) {
	f, root, outside := project(t)
	os.Symlink(outside, filepath.Join(root, "out"))
	if _, err := Tree(f, filepath.Join(root, "out")); codeOf(err) != proto.CodeNotATree {
		t.Fatal(err)
	}
}

func TestAPathMayNotClimbOutOfItsTree(t *testing.T) {
	_, root, _ := project(t)
	for _, rel := range []string{"..", "../elsewhere/secret", "src/../../elsewhere", "/etc/passwd", "src/\x00"} {
		if _, err := Join(root, rel); codeOf(err) != proto.CodeOutsideTree {
			t.Errorf("%q: %v", rel, err)
		}
	}
	for rel, want := range map[string]string{"": root, "src/a.rs": filepath.Join(root, "src", "a.rs"), "./src": filepath.Join(root, "src"), "new/file": filepath.Join(root, "new", "file")} {
		if got, err := Join(root, rel); err != nil || got != want {
			t.Errorf("%q: %q %v", rel, got, err)
		}
	}
}

func TestALinkOutOfTheTreeIsOutsideItAndOneWithinIsFollowed(t *testing.T) {
	_, root, outside := project(t)
	os.Symlink(filepath.Join(outside, "secret"), filepath.Join(root, "secret"))
	os.Symlink(outside, filepath.Join(root, "out"))
	os.Symlink("src/a.rs", filepath.Join(root, "a.rs"))
	for _, rel := range []string{"secret", "out", "out/secret", "out/missing"} {
		if _, err := Join(root, rel); codeOf(err) != proto.CodeOutsideTree {
			t.Errorf("%q: %v", rel, err)
		}
	}
	if got, err := Join(root, "a.rs"); err != nil || got != filepath.Join(root, "src", "a.rs") {
		t.Fatalf("%q %v", got, err)
	}
}
