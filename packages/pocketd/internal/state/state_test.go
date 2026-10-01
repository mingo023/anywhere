package state

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func sample() File {
	return File{Version: Version, Terminals: []Terminal{
		{TerminalID: "t1", LaunchDir: "/w", Cols: 100, Rows: 30},
		{TerminalID: "t2", LaunchDir: "/w", Cols: 80, Rows: 24, Provider: "claude", ConversationID: "c1",
			TranscriptPath: "/p/c1.jsonl", Launch: &Launch{Access: "edits", Model: "opus"}, AgentID: "a1", CreatedAt: 7, Status: "working"},
	}}
}

func TestWhatIsSavedLoadsBackUnchanged(t *testing.T) {
	path := Path(t.TempDir())
	if err := Save(path, sample()); err != nil {
		t.Fatal(err)
	}
	got, err := Load(path)
	if err != nil || !reflect.DeepEqual(got, sample()) {
		t.Fatalf("Load = %+v, %v", got, err)
	}
}

func TestAMissingFileLoadsEmpty(t *testing.T) {
	got, err := Load(Path(t.TempDir()))
	if err != nil || got.Version != Version || len(got.Terminals) != 0 {
		t.Fatalf("Load = %+v, %v", got, err)
	}
}

func TestSaveIsAtomicAndPrivate(t *testing.T) {
	path := Path(t.TempDir())
	if err := Save(path, sample()); err != nil {
		t.Fatal(err)
	}
	if st, _ := os.Stat(filepath.Dir(path)); st.Mode().Perm() != 0o700 {
		t.Errorf("dir mode = %v", st.Mode().Perm())
	}
	if st, _ := os.Stat(path); st.Mode().Perm() != 0o600 {
		t.Errorf("file mode = %v", st.Mode().Perm())
	}
	if entries, _ := os.ReadDir(filepath.Dir(path)); len(entries) != 1 {
		t.Errorf("dir holds %d entries, want only the file", len(entries))
	}
}

func TestACorruptFileLoadsEmptyAndIsKept(t *testing.T) {
	for name, body := range map[string]string{"garbage": "{nope", "future": `{"version":2,"terminals":[]}`} {
		t.Run(name, func(t *testing.T) {
			path := Path(t.TempDir())
			os.MkdirAll(filepath.Dir(path), 0o700)
			os.WriteFile(path, []byte(body), 0o600)
			got, err := Load(path)
			if err == nil || len(got.Terminals) != 0 || got.Version != Version {
				t.Fatalf("Load = %+v, %v", got, err)
			}
			if kept, _ := os.ReadFile(path + ".bad"); string(kept) != body {
				t.Fatalf(".bad holds %q", kept)
			}
			if !strings.Contains(err.Error(), path) {
				t.Fatalf("err = %v", err)
			}
		})
	}
}

func TestACorruptFileThatCantBeSetAsideSaysSo(t *testing.T) {
	path := Path(t.TempDir())
	os.MkdirAll(filepath.Dir(path), 0o700)
	os.WriteFile(path, []byte("{nope"), 0o600)
	os.Chmod(filepath.Dir(path), 0o500)
	t.Cleanup(func() { os.Chmod(filepath.Dir(path), 0o700) })
	if _, err := Load(path); err == nil || !strings.Contains(err.Error(), path+".bad") {
		t.Fatalf("err = %v", err)
	}
}
