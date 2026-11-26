package shellenv

import (
	"os"
	"path/filepath"
	"testing"
)

func TestAWindowsFileIsExecutableByAnExtensionInPathext(t *testing.T) {
	t.Setenv("PATHEXT", ".COM;.EXE;.BAT;.CMD")
	dir := t.TempDir()
	for name, want := range map[string]bool{"claude.exe": true, "claude.CMD": true, "notes.txt": false, "claude": false} {
		path := filepath.Join(dir, name)
		if err := os.WriteFile(path, nil, 0o644); err != nil {
			t.Fatal(err)
		}
		if got := IsExecutable(path); got != want {
			t.Errorf("IsExecutable(%s) = %v, want %v", name, got, want)
		}
	}
	if IsExecutable(dir + `\sub.exe`) {
		t.Error("a missing file is executable")
	}
	os.Mkdir(filepath.Join(dir, "d.exe"), 0o755)
	if IsExecutable(filepath.Join(dir, "d.exe")) {
		t.Error("a folder is executable")
	}
}
