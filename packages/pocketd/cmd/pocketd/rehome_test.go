package main

import (
	"os"
	"path/filepath"
	"slices"
	"testing"

	"pocketd/internal/config"
)

func TestAMoveUnderALinkedUserFolderAlsoRehomesItsRealPaths(t *testing.T) {
	real, err := filepath.EvalSymlinks(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	linked := filepath.Join(t.TempDir(), "me")
	if err := os.Symlink(real, linked); err != nil {
		t.Fatal(err)
	}
	moves := []config.Move{
		{From: filepath.Join(linked, ".anywhere"), To: filepath.Join(linked, ".zoes")},
		{From: filepath.Join(linked, ".coding-pocket-dev"), To: filepath.Join(linked, ".zoes", "dev")},
	}
	want := append(slices.Clone(moves),
		config.Move{From: filepath.Join(real, ".anywhere"), To: filepath.Join(real, ".zoes")},
		config.Move{From: filepath.Join(real, ".coding-pocket-dev"), To: filepath.Join(real, ".zoes", "dev")},
	)
	if got := withRealPaths(moves); !slices.Equal(got, want) {
		t.Errorf("got %v", got)
	}
	if got := withRealPaths(want[2:]); !slices.Equal(got, want[2:]) {
		t.Errorf("a real user folder adds nothing: %v", got)
	}
}
