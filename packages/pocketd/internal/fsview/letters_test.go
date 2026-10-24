package fsview

import (
	"strings"
	"testing"
)

const sha = "0123456789012345678901234567890123456789"

func porcelain(records ...string) []byte { return []byte(strings.Join(records, "\x00") + "\x00") }

func ordinaryRecord(xy, path string) string {
	return "1 " + xy + " N... 100644 100644 100644 " + sha + " " + sha + " " + path
}

func TestAChangedFileShowsItsGitLetter(t *testing.T) {
	l := ParseStatus(porcelain(
		ordinaryRecord(".M", "src/a.rs"),
		ordinaryRecord("M.", "src/staged.rs"),
		ordinaryRecord("A.", "new.rs"),
		ordinaryRecord("AM", "new2.rs"),
		ordinaryRecord(".D", "gone.rs"),
		ordinaryRecord("D.", "gone2.rs"),
		ordinaryRecord("AD", "never.rs"),
		"2 R. N... 100644 100644 100644 "+sha+" "+sha+" R100 moved with space.rs", "old.rs",
		"u UU N... 100644 100644 100644 100644 "+sha+" "+sha+" "+sha+" conflict.rs",
		"? notes.txt",
	), "")
	for path, want := range map[string]string{
		"src/a.rs": "M", "src/staged.rs": "M", "new.rs": "A", "new2.rs": "A", "gone.rs": "D", "gone2.rs": "D", "never.rs": "",
		"moved with space.rs": "R", "old.rs": "", "conflict.rs": "U", "notes.txt": "?", "src/b.rs": "",
	} {
		if got := l.File(path); got != want {
			t.Errorf("%s: %q, want %q", path, got, want)
		}
	}
}

func TestAFolderTakesTheLetterItsChangesShareOrElseM(t *testing.T) {
	l := ParseStatus(porcelain(
		ordinaryRecord("A.", "new/a.rs"), ordinaryRecord("A.", "new/deep/b.rs"),
		ordinaryRecord("A.", "src/a.rs"), ordinaryRecord(".D", "src/b.rs"),
		ordinaryRecord(".M", "src2/a.rs"),
	), "")
	for dir, want := range map[string]string{"new": "A", "new/deep": "A", "src": "M", "src2": "M", "sr": "", "lib": ""} {
		if got := l.Folder(dir); got != want {
			t.Errorf("%s: %q, want %q", dir, got, want)
		}
	}
}

func TestAnUntrackedFolderMarksEverythingInsideIt(t *testing.T) {
	l := ParseStatus(porcelain("? scratch/", ordinaryRecord(".M", "scratch.md")), "")
	if l.Folder("scratch") != "?" || l.Folder("scratch/deep") != "?" || l.File("scratch/deep/x.go") != "?" || l.File("scratch.md") != "M" {
		t.Fatalf("%+v", l)
	}
}

func TestATreeBelowTheRepositoryRootSeesOnlyItsOwnChangesRelativeToIt(t *testing.T) {
	l := ParseStatus(porcelain(ordinaryRecord(".M", "web/src/app.ts"), ordinaryRecord(".M", "api/main.go")), "web/")
	if l.File("src/app.ts") != "M" || l.Folder("src") != "M" || l.File("api/main.go") != "" || l.Folder("api") != "" {
		t.Fatalf("%+v", l)
	}
}
