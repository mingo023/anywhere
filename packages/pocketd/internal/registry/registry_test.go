package registry

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
	"time"
)

func write(t *testing.T, path, body string, mtime time.Time) {
	t.Helper()
	if err := os.WriteFile(path, []byte(body), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Chtimes(path, mtime, mtime); err != nil {
		t.Fatal(err)
	}
}

func TestADesktopJSONLoadsAndItsUnknownKeysAreIgnored(t *testing.T) {
	path := filepath.Join(t.TempDir(), "desktop.json")
	write(t, path, `{"projects":["/w/pocket"],"repos":{"/w/pocket":{"name":"Pocket","color":1,"base":"dev","worktrees":"/wt","setup":"make","copy":[".env"]}},"collapsed":["/w/pocket"],"sounds":{"done":false}}`, time.Now())
	want := File{Projects: []string{"/w/pocket"}, Repos: map[string]Repo{"/w/pocket": {Name: "Pocket", Base: "dev", Worktrees: "/wt", Setup: "make", Copy: []string{".env"}}}}
	if got := New(path).Load(); !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v", got)
	}
}

func TestARewriteWithANewMtimeReloads(t *testing.T) {
	path := filepath.Join(t.TempDir(), "desktop.json")
	at := time.Now()
	write(t, path, `{"projects":["/a"]}`, at)
	r := New(path)
	r.Load()
	write(t, path, `{"projects":["/b"]}`, at.Add(time.Second))
	if got := r.Load().Projects; !reflect.DeepEqual(got, []string{"/b"}) {
		t.Fatalf("got %v", got)
	}
}

func TestABrokenFileKeepsTheLastGoodCopyUntilItIsFixed(t *testing.T) {
	path := filepath.Join(t.TempDir(), "desktop.json")
	at := time.Now()
	write(t, path, `{"projects":["/a"]}`, at)
	r := New(path)
	r.Load()
	write(t, path, `{"projects":["/b"]]`, at.Add(time.Second))
	if got := r.Load().Projects; !reflect.DeepEqual(got, []string{"/a"}) {
		t.Fatalf("broken: got %v", got)
	}
	write(t, path, `{"projects":["/c"]}`, at.Add(time.Second))
	if got := r.Load().Projects; !reflect.DeepEqual(got, []string{"/c"}) {
		t.Fatalf("fixed: got %v", got)
	}
}

func TestAFileThatCantBeStattedKeepsTheLastGoodCopy(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "desktop.json")
	write(t, path, `{"projects":["/a"]}`, time.Now())
	r := New(path)
	r.Load()
	os.Chmod(dir, 0)
	t.Cleanup(func() { os.Chmod(dir, 0o700) })
	if got := r.Load().Projects; !reflect.DeepEqual(got, []string{"/a"}) {
		t.Fatalf("got %v", got)
	}
}

func TestAMissingFileIsAnEmptyRegistry(t *testing.T) {
	if got := New(filepath.Join(t.TempDir(), "desktop.json")).Load(); got.Projects != nil || got.Repos != nil {
		t.Fatalf("got %+v", got)
	}
}

func TestTheFileSitsNextToTheSocket(t *testing.T) {
	t.Setenv("POCKETD_SOCK", "/tmp/scratch/pocketd.sock")
	if got := Path(); got != "/tmp/scratch/desktop.json" {
		t.Fatal(got)
	}
}

func TestAProjectIsNamedByItsRepoElseItsFolder(t *testing.T) {
	f := File{Repos: map[string]Repo{"/w/pocket": {Name: "Pocket"}}}
	if a, b := f.Name("/w/pocket"), f.Name("/w/other"); a != "Pocket" || b != "other" {
		t.Fatal(a, b)
	}
}
