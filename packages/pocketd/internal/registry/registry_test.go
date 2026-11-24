package registry

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"testing"
	"time"

	"pocketd/internal/proto"
)

func home(t *testing.T, desktop string) string {
	t.Helper()
	dir := t.TempDir()
	if desktop != "" {
		if err := os.WriteFile(filepath.Join(dir, "desktop.json"), []byte(desktop), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

func marshal(v any) string {
	raw, _ := json.Marshal(v)
	return string(raw)
}

func folders(t *testing.T, names ...string) []string {
	t.Helper()
	root := t.TempDir()
	var out []string
	for _, n := range names {
		p := filepath.Join(root, n)
		if err := os.MkdirAll(p, 0o755); err != nil {
			t.Fatal(err)
		}
		out = append(out, p)
	}
	return out
}

func published(t *testing.T, ch <-chan []byte) proto.RegistryList {
	t.Helper()
	select {
	case raw := <-ch:
		var l proto.RegistryList
		if err := json.Unmarshal(raw, &l); err != nil {
			t.Fatal(err)
		}
		return l
	case <-time.After(time.Second):
		t.Fatal("nothing published")
	}
	return proto.RegistryList{}
}

func quiet(t *testing.T, ch <-chan []byte) {
	t.Helper()
	select {
	case raw := <-ch:
		t.Fatalf("published %s", raw)
	default:
	}
}

func TestADesktopJSONIsImportedWithoutTheOtherHostsProjects(t *testing.T) {
	desktop := `{"projects":["/w/pocket","//box/w/app"],"repos":{"/w/pocket":{"name":"Pocket","color":7,"base":"dev","setup":"make","teardown":"make clean","launch":{"provider":"claude","model":"opus"},"dev_url":"http://localhost:3000","icon":{"emoji":"P"},"gh_account":"work","accounts":{"claude":"w"},"hidden":["codex"]},"//box/w/app":{"name":"App"}},` +
		`"tracked":["/wt/a","//box/wt/b"],"git":{"remote":"fork","commit_all":false,"force_with_lease":true},"agents":{"access":"edits"},"worktree":{"root":"/wt"},"sidebar":{"external_worktrees":true},"collapsed":["/w/pocket"]}`
	l := Open(home(t, desktop)).List()
	if len(l.Projects) != 1 || l.Projects[0].Path != "/w/pocket" || len(l.Projects[0].ID) != 16 {
		t.Fatalf("%+v", l.Projects)
	}
	r := l.Projects[0].Repo
	if r.Name != "Pocket" || r.Color != 7 || r.Setup != "make" || r.Teardown != "make clean" || r.DevURL != "http://localhost:3000" || r.GhAccount != "work" ||
		string(r.Launch) != `{"provider":"claude","model":"opus"}` || string(r.Icon) != `{"emoji":"P"}` ||
		!reflect.DeepEqual(r.Accounts, map[string]string{"claude": "w"}) || !reflect.DeepEqual(r.Hidden, []string{"codex"}) {
		t.Fatalf("%+v", r)
	}
	if !reflect.DeepEqual(l.Tracked, []string{"/wt/a"}) {
		t.Fatalf("tracked %v", l.Tracked)
	}
	s := l.Settings
	if s.Git.Remote != "fork" || s.Git.CommitsAll() || !s.Git.ForceWithLease || s.Access != "edits" || s.WorktreeRoot != "/wt" || !s.ExternalWorktrees {
		t.Fatalf("%+v", s)
	}
}

func TestTwoClonesNamedAlikeAreTwoProjects(t *testing.T) {
	l := Open(home(t, `{"projects":["/a/app","/b/app"],"repos":{"/a/app":{"name":"app"},"/b/app":{"name":"app"}}}`)).List()
	if len(l.Projects) != 2 || l.Projects[0].ID == l.Projects[1].ID {
		t.Fatalf("%+v", l.Projects)
	}
}

func TestAMissingDesktopJSONImportsAnEmptyRegistry(t *testing.T) {
	r := Open(t.TempDir())
	if f := r.Load(); len(f.Projects) != 0 || len(f.Repos) != 0 || len(f.Tracked) != 0 {
		t.Fatalf("%+v", f)
	}
	if l := r.List(); len(l.Projects) != 0 {
		t.Fatalf("%+v", l)
	}
}

func TestARestartKeepsTheIdsAndEditingDesktopJSONAfterwardsChangesNothing(t *testing.T) {
	dir := home(t, `{"projects":["/a/app","/b/app"],"worktree":{"root":"/wt"}}`)
	first := Open(dir).List()
	if err := os.WriteFile(filepath.Join(dir, "desktop.json"), []byte(`{"projects":["/c"],"worktree":{"root":"/other"}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	again := Open(dir)
	if got, want := marshal(again.List()), marshal(first); got != want {
		t.Fatalf("got %s, want %s", got, want)
	}
	if got := again.Load().Projects; !slices.Equal(got, []string{"/a/app", "/b/app"}) {
		t.Fatalf("%v", got)
	}
}

func TestLoadServesTheProjectsAndEachOnesIdInItsRepo(t *testing.T) {
	r := Open(home(t, `{"projects":["/w/pocket","/w/bare"],"repos":{"/w/pocket":{"name":"Pocket","setup":"make"}},"worktree":{"root":"/wt"},"tracked":["/wt/a"],"agents":{"access":"edits"},"sidebar":{"external_worktrees":true}}`))
	f, l := r.Load(), r.List()
	if !slices.Equal(f.Projects, []string{"/w/pocket", "/w/bare"}) || f.Name("/w/pocket") != "Pocket" || f.Repos["/w/pocket"].Setup != "make" ||
		f.Repos["/w/pocket"].ID != l.Projects[0].ID || f.Repos["/w/bare"].ID != l.Projects[1].ID || f.Repos["/w/bare"].ID == "" {
		t.Fatalf("%+v", f)
	}
	if f.Worktree.Root != "/wt" || f.Agents.Access != "edits" || !f.Sidebar.ExternalWorktrees || !slices.Equal(f.Tracked, []string{"/wt/a"}) {
		t.Fatalf("%+v", f)
	}
}

func TestAddingAProjectSavesPublishesAndSurvivesARestart(t *testing.T) {
	dir, paths := t.TempDir(), folders(t, "app")
	r := Open(dir)
	ch, stop := r.Subscribe()
	defer stop()
	if err := r.AddProject(paths[0], &Repo{Name: "App", Color: 3, Launch: json.RawMessage(`{"model":"opus"}`)}); err != nil {
		t.Fatal(err)
	}
	l := published(t, ch)
	if len(l.Projects) != 1 || l.Projects[0].Path != paths[0] || l.Projects[0].Repo.Name != "App" {
		t.Fatalf("%+v", l)
	}
	again := Open(dir).List()
	if len(again.Projects) != 1 || again.Projects[0].ID != l.Projects[0].ID || again.Projects[0].Repo.Color != 3 || string(again.Projects[0].Repo.Launch) != `{"model":"opus"}` {
		t.Fatalf("%+v", again)
	}
}

func TestAddingAListedPathAgainChangesAndPublishesNothing(t *testing.T) {
	paths := folders(t, "app")
	r := Open(t.TempDir())
	r.AddProject(paths[0], nil)
	id := r.Load().Repos[paths[0]].ID
	ch, stop := r.Subscribe()
	defer stop()
	if err := r.AddProject(paths[0], &Repo{Name: "Other"}); err != nil {
		t.Fatal(err)
	}
	if got := r.Load().Repos[paths[0]]; got.ID != id || got.Name != "" {
		t.Fatalf("%+v", got)
	}
	quiet(t, ch)
}

func TestAProjectIsAnAbsolutePathToAFolderThatExists(t *testing.T) {
	paths := folders(t, "app")
	file := filepath.Join(paths[0], "f")
	os.WriteFile(file, nil, 0o600)
	r := Open(t.TempDir())
	for _, p := range []string{"relative/app", filepath.Join(paths[0], "gone"), file} {
		if err := r.AddProject(p, nil); err != ErrNotAFolder {
			t.Errorf("%q: %v", p, err)
		}
	}
	if len(r.Load().Projects) != 0 {
		t.Fatal("added")
	}
}

func TestRemovingAProjectIsForgottenAfterARestartAndRemovingAgainIsFine(t *testing.T) {
	dir, paths := t.TempDir(), folders(t, "a", "b")
	r := Open(dir)
	r.AddProject(paths[0], nil)
	r.AddProject(paths[1], nil)
	ch, stop := r.Subscribe()
	defer stop()
	if err := r.RemoveProject(paths[0]); err != nil {
		t.Fatal(err)
	}
	if l := published(t, ch); len(l.Projects) != 1 || l.Projects[0].Path != paths[1] {
		t.Fatalf("%+v", l)
	}
	if err := r.RemoveProject(paths[0]); err != nil {
		t.Fatal(err)
	}
	quiet(t, ch)
	if got := Open(dir).Load().Projects; !slices.Equal(got, []string{paths[1]}) {
		t.Fatalf("%v", got)
	}
}

func TestSettingAProjectReplacesItsRepoAndKeepsItsId(t *testing.T) {
	dir, paths := t.TempDir(), folders(t, "app")
	r := Open(dir)
	r.AddProject(paths[0], &Repo{Name: "Old", Setup: "make"})
	id := r.Load().Repos[paths[0]].ID
	ch, stop := r.Subscribe()
	defer stop()
	if err := r.SetRepo(paths[0], Repo{Name: "New", Hidden: []string{"codex"}}); err != nil {
		t.Fatal(err)
	}
	l := published(t, ch)
	if got := l.Projects[0]; got.ID != id || got.Repo.Name != "New" || got.Repo.Setup != "" || !slices.Equal(got.Repo.Hidden, []string{"codex"}) {
		t.Fatalf("%+v", got)
	}
	if got := Open(dir).Load().Repos[paths[0]]; got.ID != id || got.Name != "New" {
		t.Fatalf("%+v", got)
	}
	if err := r.SetRepo("/nowhere", Repo{}); err != ErrUnknownProject {
		t.Fatalf("%v", err)
	}
	quiet(t, ch)
}

func TestTrackingAWorktreeIsIdempotentBothWays(t *testing.T) {
	dir := t.TempDir()
	r := Open(dir)
	ch, stop := r.Subscribe()
	defer stop()
	r.Track("/wt/a", true)
	if l := published(t, ch); !slices.Equal(l.Tracked, []string{"/wt/a"}) {
		t.Fatalf("%+v", l)
	}
	r.Track("/wt/a", true)
	quiet(t, ch)
	if got := Open(dir).Load().Tracked; !slices.Equal(got, []string{"/wt/a"}) {
		t.Fatalf("%v", got)
	}
	r.Track("/wt/a", false)
	if l := published(t, ch); len(l.Tracked) != 0 {
		t.Fatalf("%+v", l)
	}
	r.Track("/wt/a", false)
	quiet(t, ch)
}

func TestSharedSettingsAreReplacedWholeAndSurviveARestart(t *testing.T) {
	dir := home(t, `{"worktree":{"root":"/old"},"agents":{"access":"edits"}}`)
	r := Open(dir)
	ch, stop := r.Subscribe()
	defer stop()
	want := proto.SharedSettings{Git: proto.GitSettings{Remote: "fork", ForceWithLease: true}, WorktreeRoot: "/new"}
	if err := r.SetSettings(want); err != nil {
		t.Fatal(err)
	}
	if l := published(t, ch); !reflect.DeepEqual(l.Settings, want) {
		t.Fatalf("%+v", l.Settings)
	}
	f := Open(dir).Load()
	if f.Worktree.Root != "/new" || f.Agents.Access != "" || f.Git.PushRemote() != "fork" || !f.Git.ForceWithLease {
		t.Fatalf("%+v", f)
	}
}

func TestAFailedWriteChangesNothingAndPublishesNothing(t *testing.T) {
	dir := t.TempDir()
	r := Open(dir)
	r.Track("/wt/a", true)
	ch, stop := r.Subscribe()
	defer stop()
	state := filepath.Join(dir, "state")
	os.Chmod(state, 0o500)
	t.Cleanup(func() { os.Chmod(state, 0o700) })
	if err := r.Track("/wt/b", true); err == nil {
		t.Skip("the folder stayed writable")
	}
	if got := r.Load().Tracked; !slices.Equal(got, []string{"/wt/a"}) {
		t.Fatalf("%v", got)
	}
	quiet(t, ch)
}

func TestTheListWritesEmptyCollectionsAndAnUnsetLaunchAbsent(t *testing.T) {
	paths := folders(t, "app")
	r := Open(t.TempDir())
	r.AddProject(paths[0], nil)
	raw, _ := json.Marshal(r.List())
	want := `{"type":"registry.list","projects":[{"id":"` + r.Load().Repos[paths[0]].ID + `","path":"` + paths[0] + `","repo":{"name":"","color":0,"setup":"","teardown":"","dev_url":"","icon":null,"gh_account":"","accounts":{},"hidden":[]}}],"tracked":[],"settings":{"git":{"remote":"","commit_all":null,"force_with_lease":false},"access":"","worktree_root":"","external_worktrees":false}}`
	if string(raw) != want {
		t.Fatalf("got %s", raw)
	}
}

func TestGitSettingsLoadAndUnsetOnesTakeTheDesktopsDefaults(t *testing.T) {
	if g := Open(home(t, `{"git":{"remote":"fork","commit_all":false,"force_with_lease":true}}`)).Load().Git; g.PushRemote() != "fork" || g.CommitsAll() {
		t.Fatalf("got %+v", g)
	}
	if g := (Git{}); g.PushRemote() != "origin" || !g.CommitsAll() {
		t.Fatalf("defaults %+v", g)
	}
}

func TestAProjectIsNamedByItsRepoElseItsFolder(t *testing.T) {
	f := File{Repos: map[string]Repo{"/w/pocket": {Name: "Pocket"}}}
	if a, b := f.Name("/w/pocket"), f.Name("/w/other"); a != "Pocket" || b != "other" {
		t.Fatal(a, b)
	}
}

func TestABrokenStateFileIsKeptAsideAndMadeAgainFromDesktopJSON(t *testing.T) {
	app := folders(t, "app")[0]
	dir := home(t, marshal(map[string]any{"projects": []string{app}}))
	os.MkdirAll(filepath.Join(dir, "state"), 0o700)
	os.WriteFile(filepath.Join(dir, "state", "registry.json"), []byte("{"), 0o600)
	if got := Open(dir).Load().Projects; !slices.Equal(got, []string{app}) {
		t.Fatalf("%v", got)
	}
	if raw, _ := os.ReadFile(filepath.Join(dir, "state", "registry.json.broken")); string(raw) != "{" {
		t.Fatalf("kept aside: %q", raw)
	}
}
