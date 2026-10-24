package fsview

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"pocketd/internal/hostop"
)

func write(t *testing.T, name string, data []byte) string {
	t.Helper()
	p := filepath.Join(t.TempDir(), name)
	if err := os.WriteFile(p, data, 0o644); err != nil {
		t.Fatal(err)
	}
	return p
}

// sized is a sparse file of n bytes.
func sized(t *testing.T, name string, n int64) string {
	t.Helper()
	p := write(t, name, nil)
	if err := os.Truncate(p, n); err != nil {
		t.Fatal(err)
	}
	return p
}

func TestTextGoesWithItsSizeTimeAndLanguage(t *testing.T) {
	p := write(t, "main.rs", []byte("fn main() {}\n"))
	r := Read(p, "src/main.rs", 0)
	if r.Kind != "text" || *r.Text != "fn main() {}\n" || r.Language != "rust" || r.Size != 13 || r.Mtime == 0 || r.Truncated || r.Path != "src/main.rs" {
		t.Fatalf("%+v", r)
	}
}

func TestTellsTextFromBinary(t *testing.T) {
	for name, data := range map[string][]byte{
		"zip":      []byte("PK\x03\x04\x00\x00"),
		"latin1":   {0xff, 0xfe, 'a'},
		"late NUL": append(bytes.Repeat([]byte{'a'}, 9000), 0xff),
	} {
		if r := Read(write(t, "f", data), "f", 0); r.Kind != "binary" || r.Text != nil {
			t.Errorf("%s: %+v", name, r)
		}
	}
	// A NUL past the first 8 KiB in valid UTF-8 is still text, as the desktop reads it.
	if r := Read(write(t, "f.txt", append(bytes.Repeat([]byte{'a'}, 9000), 0)), "f.txt", 0); r.Kind != "text" {
		t.Fatalf("%+v", r)
	}
}

func TestTextOverMaxIsCutAtACharacterAndMarkedTruncated(t *testing.T) {
	p := write(t, "notes.md", []byte(strings.Repeat("é", 10)))
	r := Read(p, "notes.md", 5)
	if r.Kind != "text" || *r.Text != "éé" || !r.Truncated || r.Size != 20 || r.Language != "markdown" {
		t.Fatalf("%+v", r)
	}
	big := Read(write(t, "big.log", bytes.Repeat([]byte("a"), 600<<10)), "big.log", 0)
	if len(*big.Text) != TextMax || !big.Truncated {
		t.Fatalf("%d %v", len(*big.Text), big.Truncated)
	}
}

func TestTextOverTheHardReadMaxIsTooLarge(t *testing.T) {
	if r := Read(sized(t, "huge.log", hostop.ReadMax+1), "huge.log", 0); r.Kind != "tooLarge" || r.Size != hostop.ReadMax+1 || r.Text != nil {
		t.Fatalf("%+v", r)
	}
}

func TestImagesGoAsBytesUpTo20MBThenAreTooLarge(t *testing.T) {
	r := Read(write(t, "logo.PNG", []byte("\x89PNG\x00")), "logo.PNG", 0)
	if r.Kind != "image" || r.Mime != "image/png" || string(r.Data) != "\x89PNG\x00" || r.Text != nil {
		t.Fatalf("%+v", r)
	}
	if r := Read(sized(t, "photo.jpg", ImageMax), "photo.jpg", 0); r.Kind != "image" || len(r.Data) != ImageMax || r.Mime != "image/jpeg" {
		t.Fatalf("%s %d", r.Kind, len(r.Data))
	}
	if r := Read(sized(t, "photo.jpg", ImageMax+1), "photo.jpg", 0); r.Kind != "tooLarge" || r.Data != nil {
		t.Fatalf("%+v", r.Kind)
	}
}

func TestSVGGoesAsTextWithItsImageMime(t *testing.T) {
	r := Read(write(t, "icon.svg", []byte(`<svg xmlns="http://www.w3.org/2000/svg"/>`)), "icon.svg", 0)
	if r.Kind != "text" || r.Mime != "image/svg+xml" || r.Data != nil || r.Language != "text" {
		t.Fatalf("%+v", r)
	}
}

func TestAFolderOrAMissingFileIsUnreadable(t *testing.T) {
	dir := t.TempDir()
	for _, p := range []string{dir, filepath.Join(dir, "missing.rs")} {
		if r := Read(p, "x", 0); r.Kind != "unreadable" {
			t.Errorf("%s: %+v", p, r)
		}
	}
}

func TestLanguagesAreProtocolIds(t *testing.T) {
	for name, want := range map[string]string{
		"a.tsx": "tsx", "a.ts": "typescript", "a.mjs": "javascript", "Main.KT": "kotlin", "x.yml": "yaml", "run.zsh": "bash",
		"README.markdown": "markdown", "Makefile": "text", "index.htm": "html",
	} {
		if got := Language(name); got != want {
			t.Errorf("%s: %s, want %s", name, got, want)
		}
	}
}
