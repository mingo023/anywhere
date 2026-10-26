package fsview

import (
	"context"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"testing"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/registry"
)

func gitEnv() []string {
	return append(os.Environ(), "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_NOSYSTEM=1",
		"GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@example.com", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@example.com")
}

func run(t *testing.T, dir string, argv ...string) {
	t.Helper()
	cmd := exec.Command(argv[0], argv[1:]...)
	cmd.Dir, cmd.Env = dir, gitEnv()
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("%v: %v\n%s", argv, err, out)
	}
}

func put(t *testing.T, root, rel, text string) {
	t.Helper()
	p := filepath.Join(root, rel)
	os.MkdirAll(filepath.Dir(p), 0o755)
	if err := os.WriteFile(p, []byte(text), 0o644); err != nil {
		t.Fatal(err)
	}
}

// repo is a registered project with a commit, then a change, a new file,
// an ignored folder and a link, served by a Service registered over it.
func repo(t *testing.T) (*Service, string) {
	t.Helper()
	base, _ := filepath.EvalSymlinks(t.TempDir())
	root := filepath.Join(base, "app")
	put(t, root, "src/main.rs", "fn main() {}\n")
	put(t, root, "src/lib.rs", "pub fn a() {}\n")
	put(t, root, "README.md", "# app\n")
	put(t, root, ".gitignore", "target/\n")
	run(t, root, "git", "init", "-q", "-b", "main")
	run(t, root, "git", "add", ".")
	run(t, root, "git", "commit", "-q", "-m", "init")
	put(t, root, "src/main.rs", "fn main() { run() }\n")
	put(t, root, "src/new/mod.rs", "mod x;\n")
	put(t, root, "target/debug/app", "bin")
	put(t, root, ".env", "KEY=1\n")
	os.Symlink("README.md", filepath.Join(root, "readme"))
	desktop := filepath.Join(base, "desktop.json")
	raw, _ := json.Marshal(registry.File{Projects: []string{root}})
	os.WriteFile(desktop, raw, 0o644)
	return &Service{Registry: registry.New(desktop), Env: gitEnv, Attachments: filepath.Join(base, "home", "attachments")}, root
}

func names(entries []proto.FsEntry) []string {
	var out []string
	for _, e := range entries {
		out = append(out, e.Name)
	}
	return out
}

func TestAListingPutsFoldersFirstAndCarriesIgnoredFlagsAndGitLetters(t *testing.T) {
	s, root := repo(t)
	entries, err := s.List(context.Background(), root, "")
	if err != nil {
		t.Fatal(err)
	}
	if got := names(entries); !slices.Equal(got, []string{"src", "target", ".env", ".gitignore", "README.md", "readme"}) {
		t.Fatal(got)
	}
	by := map[string]proto.FsEntry{}
	for _, e := range entries {
		by[e.Name] = e
	}
	if by["src"].Git != "M" || !by["target"].Ignored || by["target"].Git != "" || by[".env"].Git != "?" || by["README.md"].Git != "" ||
		!by["readme"].Link || by["readme"].Dir || by["README.md"].Size != 6 || by["README.md"].Mtime == 0 {
		t.Fatalf("%+v", entries)
	}
	src, _ := s.List(context.Background(), root, "src")
	letters := map[string]string{}
	for _, e := range src {
		letters[e.Name] = e.Git
	}
	if got := names(src); !slices.Equal(got, []string{"new", "lib.rs", "main.rs"}) || letters["new"] != "?" || letters["main.rs"] != "M" || letters["lib.rs"] != "" {
		t.Fatal(got, letters)
	}
}

func TestAListingOfATreeOutsideAProjectOrAPathOutOfItIsRefused(t *testing.T) {
	s, root := repo(t)
	if _, err := s.List(context.Background(), filepath.Dir(root), ""); codeOf(err) != proto.CodeNotATree {
		t.Fatal(err)
	}
	if _, err := s.List(context.Background(), root, "../"); codeOf(err) != proto.CodeOutsideTree {
		t.Fatal(err)
	}
	if _, err := s.Read(root, "../desktop.json", 0); codeOf(err) != proto.CodeOutsideTree {
		t.Fatal(err)
	}
	if _, err := s.Read(root, "", 0); codeOf(err) != proto.CodeOutsideTree {
		t.Fatal(err)
	}
}

func TestAReadNamesTheTreeAndPathItWasAskedFor(t *testing.T) {
	s, root := repo(t)
	r, err := s.Read(root, "readme", 0)
	if err != nil || r.Tree != root || r.Path != "readme" || r.Kind != "text" || *r.Text != "# app\n" {
		t.Fatalf("%+v %v", r, err)
	}
}

func TestSearchFindsTrackedAndUntrackedFilesButNotIgnoredOnes(t *testing.T) {
	s, root := repo(t)
	got, err := s.Search(context.Background(), root, "", 50)
	if err != nil {
		t.Fatal(err)
	}
	// Changed files lead an empty query.
	lead := slices.Sorted(slices.Values(got[:4]))
	if !slices.Equal(lead, []string{".env", "readme", "src/main.rs", "src/new/mod.rs"}) || slices.Contains(got, "target/debug/app") || len(got) != 7 {
		t.Fatal(got)
	}
	if got, _ := s.Search(context.Background(), root, "mod", 50); !slices.Equal(got, []string{"src/new/mod.rs"}) {
		t.Fatal(got)
	}
}

func TestSearchSeesANewFileOnceItsIndexHasRefreshed(t *testing.T) {
	s, root := repo(t)
	ctx := context.Background()
	s.Search(ctx, root, "x", 50)
	put(t, root, "src/fresh.rs", "")
	c := s.caches(root)
	c.files.mu.Lock()
	c.files.at = time.Now().Add(-time.Minute)
	c.files.mu.Unlock()
	// The stale index answers at once and re-reads behind it.
	if got, _ := s.Search(ctx, root, "fresh", 50); len(got) != 0 {
		t.Fatal(got)
	}
	deadline := time.Now().Add(5 * time.Second)
	for {
		if got, _ := s.Search(ctx, root, "fresh", 50); slices.Equal(got, []string{"src/fresh.rs"}) {
			return
		}
		if time.Now().After(deadline) {
			t.Fatal("the index never refreshed")
		}
		time.Sleep(10 * time.Millisecond)
	}
}

func TestAFolderOutsideARepositoryListsWithoutLetters(t *testing.T) {
	base, _ := filepath.EvalSymlinks(t.TempDir())
	root := filepath.Join(base, "notes")
	put(t, root, "a.md", "a")
	desktop := filepath.Join(base, "desktop.json")
	raw, _ := json.Marshal(registry.File{Projects: []string{root}})
	os.WriteFile(desktop, raw, 0o644)
	s := &Service{Registry: registry.New(desktop), Env: gitEnv}
	entries, err := s.List(context.Background(), root, "")
	if err != nil || len(entries) != 1 || entries[0].Git != "" || entries[0].Ignored {
		t.Fatal(entries, err)
	}
	if got, err := s.Search(context.Background(), root, "a", 5); err != nil || len(got) != 0 {
		t.Fatal(got, err)
	}
}

func TestAReadOfTheAttachmentsFolderServesWhatAnAgentShared(t *testing.T) {
	s, _ := repo(t)
	put(t, s.Attachments, "0a1b2c3d4e5f-chart.png", "\x89PNG\r\n\x1a\n")
	r, err := s.Read(s.Attachments, "0a1b2c3d4e5f-chart.png", 0)
	if err != nil || r.Kind != "image" || r.Tree != s.Attachments || r.Path != "0a1b2c3d4e5f-chart.png" {
		t.Fatalf("%+v %v", r, err)
	}
	if _, err := s.Read(s.Attachments, "../../desktop.json", 0); codeOf(err) != proto.CodeOutsideTree {
		t.Fatalf("a path out of the folder: %v", err)
	}
	if _, err := s.Read(filepath.Dir(s.Attachments), "attachments/0a1b2c3d4e5f-chart.png", 0); codeOf(err) != proto.CodeNotATree {
		t.Fatalf("pocketd's home: %v", err)
	}
}

func TestTheAttachmentsFolderServesThroughALinkToIt(t *testing.T) {
	s, _ := repo(t)
	put(t, s.Attachments, "0a1b2c3d4e5f-notes.md", "# notes\n")
	link := filepath.Join(t.TempDir(), "attachments")
	if err := os.Symlink(s.Attachments, link); err != nil {
		t.Fatal(err)
	}
	s.Attachments = link
	if r, err := s.Read(link, "0a1b2c3d4e5f-notes.md", 0); err != nil || r.Kind != "text" || r.Tree != link {
		t.Fatalf("%+v %v", r, err)
	}
}
