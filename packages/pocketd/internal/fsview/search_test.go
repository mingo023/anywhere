package fsview

import (
	"fmt"
	"slices"
	"strings"
	"testing"
)

func index(paths ...string) Index { return NewIndex(strings.Join(paths, "\x00")) }

func TestAQueryMatchesItsLettersInOrderAnywhereInThePath(t *testing.T) {
	ix := index("packages/desktop/crates/pocket/src/explorer.rs", "README.md", "docs/mobile/plan.md")
	if got := ix.Search("pcexp", 10, nil); !slices.Equal(got, []string{"packages/desktop/crates/pocket/src/explorer.rs"}) {
		t.Fatal(got)
	}
	if got := ix.Search("xyz", 10, nil); len(got) != 0 {
		t.Fatal(got)
	}
	if got := ix.Search("READ me", 10, nil); !slices.Equal(got, []string{"README.md"}) {
		t.Fatal(got)
	}
}

func TestABaseNameMatchBeatsOneSpreadOverFolders(t *testing.T) {
	ix := index("src/main/actions/time.go", "src/mat.go", "lib/format.go")
	if got := ix.Search("mat", 3, nil); got[0] != "src/mat.go" {
		t.Fatal(got)
	}
}

func scoreOf(query, path string) int {
	s, _ := Score(query, path, strings.ToLower(path))
	return s
}

func TestMatchesAtWordStartsCamelHumpsAndInRunsScoreHigher(t *testing.T) {
	for _, c := range []struct{ query, better, worse string }{
		{"sl", "a/status_letters.go", "a/xsxxxlx.go"},
		{"sr", "a/StatusRow.go", "a/xsxxrxxxx.go"},
		{"abc", "x/abcd.go", "x/axbxc.go"},
	} {
		if b, w := scoreOf(c.query, c.better), scoreOf(c.query, c.worse); b <= w {
			t.Errorf("%s: %s %d <= %s %d", c.query, c.better, b, c.worse, w)
		}
	}
}

func TestAnExactBaseNameComesFirst(t *testing.T) {
	ix := index("docs/plan-old.md", "a/b/c/plan.md", "planner/main.go")
	if got := ix.Search("plan", 3, nil); got[0] != "a/b/c/plan.md" {
		t.Fatal(got)
	}
}

func TestChangedFilesWinTiesAndShorterPathsNext(t *testing.T) {
	ix := index("bb/view.rs", "a/view.rs", "c/view.rs", "long/folder/view.rs")
	changed := func(p string) bool { return p == "long/folder/view.rs" }
	if got := ix.Search("view", 4, changed); !slices.Equal(got, []string{"long/folder/view.rs", "a/view.rs", "c/view.rs", "bb/view.rs"}) {
		t.Fatal(got)
	}
}

func TestAnEmptyQueryListsChangedFilesThenTheRestInGitsOrder(t *testing.T) {
	ix := index("z.go", "b.go", "a.go", "c.go")
	changed := func(p string) bool { return p == "a.go" || p == "c.go" }
	if got := ix.Search("", 3, changed); !slices.Equal(got, []string{"a.go", "c.go", "z.go"}) {
		t.Fatal(got)
	}
}

func TestSearchStopsAtTheLimit(t *testing.T) {
	var paths []string
	for i := range 100 {
		paths = append(paths, fmt.Sprintf("src/file%d.go", i))
	}
	if got := index(paths...).Search("file", 7, nil); len(got) != 7 {
		t.Fatal(len(got))
	}
}

func TestAnIndexReadsNewlineOrNulSeparatedOutput(t *testing.T) {
	if NewIndex("a\nb\n").Len() != 2 || NewIndex("a b\x00c\nd\x00").Len() != 2 || NewIndex("").Len() != 0 {
		t.Fatal()
	}
}

// fiftyThousand is a repo-sized list of paths.
func fiftyThousand() Index {
	dirs := []string{"packages/desktop/crates/pocket/src/chat", "packages/desktop/crates/git/src", "packages/ios/Packages/AnywhereKit/Sources/Stores",
		"packages/pocketd/internal/wsserver", "node_modules/.pnpm/react@19/node_modules/react/cjs", "docs/mobile", "scripts"}
	names := []string{"view", "model", "store", "row", "lanes", "status", "golden", "explorer", "preview", "markdown"}
	exts := []string{"rs", "swift", "go", "json", "md", "ts", "toml"}
	var b strings.Builder
	for i := range 50_000 {
		fmt.Fprintf(&b, "%s/%d/%s_%d.%s\x00", dirs[i%len(dirs)], i%97, names[i%len(names)], i, exts[i%len(exts)])
	}
	return NewIndex(b.String())
}

func BenchmarkSearchFiftyThousandPaths(b *testing.B) {
	ix := fiftyThousand()
	for _, q := range []string{"e", "expl", "stsw", "pocketdwsserverstatus"} {
		b.Run(q, func(b *testing.B) {
			for b.Loop() {
				ix.Search(q, 50, nil)
			}
		})
	}
}
