package claude

import (
	"os"
	"path/filepath"
	"testing"
)

func TestRetitleAppendsTheTitleAsRenameWould(t *testing.T) {
	path := filepath.Join(t.TempDir(), "s1.jsonl")
	os.WriteFile(path, []byte(`{"type":"custom-title","customTitle":"calm-otter","sessionId":"s1"}`+"\n"), 0o600)
	if err := Retitle(path, " Fix <login> & signup "); err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(path)
	want := `{"type":"custom-title","customTitle":"calm-otter","sessionId":"s1"}` + "\n" +
		`{"type":"custom-title","customTitle":"Fix <login> & signup","sessionId":"s1"}` + "\n"
	if string(got) != want {
		t.Fatalf("%s", got)
	}
}

func TestRetitleLeavesABlankTitleAndAMissingTranscriptAlone(t *testing.T) {
	path := filepath.Join(t.TempDir(), "s1.jsonl")
	if err := Retitle(path, "  "); err != nil {
		t.Fatal(err)
	}
	if err := Retitle(path, "Fix login"); err == nil {
		t.Fatal("wrote a transcript claude hasn't started")
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Fatal("made a transcript")
	}
}
