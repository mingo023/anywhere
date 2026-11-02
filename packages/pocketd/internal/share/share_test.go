package share

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func write(t *testing.T, path, text string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(text), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestKeepCopiesAFileUnderItsHashAndName(t *testing.T) {
	src := filepath.Join(t.TempDir(), "chart.png")
	write(t, src, "png bytes")
	dir := filepath.Join(t.TempDir(), "attachments")
	kept, err := Keep(dir, src)
	if err != nil {
		t.Fatal(err)
	}
	if filepath.Dir(kept) != dir || !strings.HasSuffix(kept, "-chart.png") || len(filepath.Base(kept)) != len("0123456789ab-chart.png") {
		t.Fatalf("kept = %q", kept)
	}
	if got, _ := os.ReadFile(kept); string(got) != "png bytes" {
		t.Fatalf("the copy holds %q", got)
	}
}

func TestAFileSharedAgainLandsOnItsCopyAndAChangedOneOnANewCopy(t *testing.T) {
	src := filepath.Join(t.TempDir(), "notes.md")
	write(t, src, "# one")
	dir := t.TempDir()
	first, _ := Keep(dir, src)
	again, _ := Keep(dir, src)
	write(t, src, "# two")
	changed, _ := Keep(dir, src)
	if first != again || changed == first {
		t.Fatalf("first %q, again %q, changed %q", first, again, changed)
	}
	if got, _ := os.ReadFile(first); string(got) != "# one" {
		t.Fatalf("the first copy now holds %q", got)
	}
}

func TestARelativePathIsFromTheFolderTheServerRunsIn(t *testing.T) {
	work := t.TempDir()
	write(t, filepath.Join(work, "notes.md"), "# hi")
	t.Chdir(work)
	kept, err := Keep(t.TempDir(), "notes.md")
	if err != nil || !strings.HasSuffix(kept, "-notes.md") {
		t.Fatalf("%q %v", kept, err)
	}
}

func TestKeepRefusesWhatIsntAFileItCanShare(t *testing.T) {
	dir := t.TempDir()
	big := filepath.Join(dir, "big.bin")
	f, _ := os.Create(big)
	f.Truncate(Max + 1)
	f.Close()
	for path, want := range map[string]string{
		filepath.Join(dir, "gone.png"): "There's no file at",
		dir:                            "is a folder",
		os.DevNull:                     "isn't a file",
		big:                            "over 20 MB",
		"":                             "Name the file",
	} {
		if _, err := Keep(t.TempDir(), path); err == nil || !strings.Contains(err.Error(), want) {
			t.Errorf("%q: %v", path, err)
		}
	}
}
