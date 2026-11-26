package registry

import (
	"database/sql"
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"slices"
	"strconv"
	"testing"
	"time"

	"pocketd/internal/proto"
	"pocketd/internal/statedb/dbtest"
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

func newDB(t *testing.T) *sql.DB {
	t.Helper()
	return dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db"))
}

func open(t *testing.T, db *sql.DB) *Registry {
	t.Helper()
	r, err := Open(db)
	if err != nil {
		t.Fatal(err)
	}
	return r
}

// imported is a registry holding what desktop.json held, as the import into pocketd.db leaves it.
func imported(t *testing.T, desktop string) (*Registry, *sql.DB) {
	t.Helper()
	s, err := ReadLegacy(home(t, desktop))
	if err != nil {
		t.Fatal(err)
	}
	db := newDB(t)
	if err := open(t, db).write(State{}, s); err != nil {
		t.Fatal(err)
	}
	return open(t, db), db
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
	r, _ := imported(t, desktop)
	l := r.List()
	if len(l.Projects) != 1 || l.Projects[0].Path != "/w/pocket" || len(l.Projects[0].ID) != 16 {
		t.Fatalf("%+v", l.Projects)
	}
	repo := l.Projects[0].Repo
	if repo.Name != "Pocket" || repo.Color != 7 || repo.Setup != "make" || repo.Teardown != "make clean" || repo.DevURL != "http://localhost:3000" || repo.GhAccount != "work" ||
		string(repo.Launch) != `{"provider":"claude","model":"opus"}` || string(repo.Icon) != `{"emoji":"P"}` ||
		!reflect.DeepEqual(repo.Accounts, map[string]string{"claude": "w"}) || !reflect.DeepEqual(repo.Hidden, []string{"codex"}) {
		t.Fatalf("%+v", repo)
	}
	if !reflect.DeepEqual(l.Tracked, []string{"/wt/a"}) {
		t.Fatalf("tracked %v", l.Tracked)
	}
	s := l.Settings
	if s.Git.Remote != "fork" || s.Git.CommitsAll() || !s.Git.ForceWithLease || s.Access != "edits" || s.WorktreeRoot != "/wt" {
		t.Fatalf("%+v", s)
	}
}

func TestTwoClonesNamedAlikeAreTwoProjects(t *testing.T) {
	r, _ := imported(t, `{"projects":["/a/app","/b/app"],"repos":{"/a/app":{"name":"app"},"/b/app":{"name":"app"}}}`)
	if l := r.List(); len(l.Projects) != 2 || l.Projects[0].ID == l.Projects[1].ID {
		t.Fatalf("%+v", l.Projects)
	}
}

func TestAMissingDesktopJSONImportsAnEmptyRegistry(t *testing.T) {
	r, _ := imported(t, "")
	if f := r.Load(); len(f.Projects) != 0 || len(f.Repos) != 0 || len(f.Tracked) != 0 {
		t.Fatalf("%+v", f)
	}
	if l := r.List(); len(l.Projects) != 0 {
		t.Fatalf("%+v", l)
	}
}

func TestAnEmptyDatabaseIsAnEmptyRegistry(t *testing.T) {
	r := open(t, newDB(t))
	if f := r.Load(); len(f.Projects) != 0 || len(f.Tracked) != 0 || f.Git.CommitAll != nil {
		t.Fatalf("%+v", f)
	}
}

func TestARestartKeepsTheIds(t *testing.T) {
	r, db := imported(t, `{"projects":["/a/app","/b/app"],"worktree":{"root":"/wt"}}`)
	again := open(t, db)
	if got, want := marshal(again.List()), marshal(r.List()); got != want {
		t.Fatalf("got %s, want %s", got, want)
	}
	if got := again.Load().Projects; !slices.Equal(got, []string{"/a/app", "/b/app"}) {
		t.Fatalf("%v", got)
	}
}

func TestLoadServesTheProjectsAndEachOnesIdInItsRepo(t *testing.T) {
	r, _ := imported(t, `{"projects":["/w/pocket","/w/bare"],"repos":{"/w/pocket":{"name":"Pocket","setup":"make"}},"worktree":{"root":"/wt"},"tracked":["/wt/a"],"agents":{"access":"edits"},"sidebar":{"external_worktrees":true}}`)
	f, l := r.Load(), r.List()
	if !slices.Equal(f.Projects, []string{"/w/pocket", "/w/bare"}) || f.Name("/w/pocket") != "Pocket" || f.Repos["/w/pocket"].Setup != "make" ||
		f.Repos["/w/pocket"].ID != l.Projects[0].ID || f.Repos["/w/bare"].ID != l.Projects[1].ID || f.Repos["/w/bare"].ID == "" {
		t.Fatalf("%+v", f)
	}
	if f.Worktree.Root != "/wt" || f.Agents.Access != "edits" || !slices.Equal(f.Tracked, []string{"/wt/a"}) {
		t.Fatalf("%+v", f)
	}
}

func TestAddingAProjectSavesPublishesAndSurvivesARestart(t *testing.T) {
	db, paths := newDB(t), folders(t, "app")
	r := open(t, db)
	ch, stop := r.Subscribe()
	defer stop()
	if err := r.AddProject(paths[0], &Repo{Name: "App", Color: 3, Launch: json.RawMessage(`{"model":"opus"}`)}); err != nil {
		t.Fatal(err)
	}
	l := published(t, ch)
	if len(l.Projects) != 1 || l.Projects[0].Path != paths[0] || l.Projects[0].Repo.Name != "App" {
		t.Fatalf("%+v", l)
	}
	again := open(t, db).List()
	if len(again.Projects) != 1 || again.Projects[0].ID != l.Projects[0].ID || again.Projects[0].Repo.Color != 3 || string(again.Projects[0].Repo.Launch) != `{"model":"opus"}` {
		t.Fatalf("%+v", again)
	}
}

func TestAddingAListedPathAgainChangesAndPublishesNothing(t *testing.T) {
	paths := folders(t, "app")
	r := open(t, newDB(t))
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
	r := open(t, newDB(t))
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
	db, paths := newDB(t), folders(t, "a", "b", "c")
	r := open(t, db)
	for _, p := range paths {
		r.AddProject(p, nil)
	}
	ch, stop := r.Subscribe()
	defer stop()
	if err := r.RemoveProject(paths[0]); err != nil {
		t.Fatal(err)
	}
	if l := published(t, ch); len(l.Projects) != 2 || l.Projects[0].Path != paths[1] {
		t.Fatalf("%+v", l)
	}
	if err := r.RemoveProject(paths[0]); err != nil {
		t.Fatal(err)
	}
	quiet(t, ch)
	if got := open(t, db).Load().Projects; !slices.Equal(got, paths[1:]) {
		t.Fatalf("%v", got)
	}
}

func TestOnRemoveHearsTheRemovedProjectOnceItIsSavedAndMayReadTheRegistry(t *testing.T) {
	db, paths := newDB(t), folders(t, "a", "b")
	r := open(t, db)
	r.AddProject(paths[0], nil)
	r.AddProject(paths[1], nil)
	id := r.Load().Repos[paths[0]].ID
	var heard []string
	r.OnRemove(func(projectID string) {
		var n int
		db.QueryRow(`SELECT COUNT(*) FROM projects WHERE id = ?`, projectID).Scan(&n)
		heard = append(heard, projectID+":"+strconv.Itoa(n)+":"+strconv.Itoa(len(r.Load().Projects)))
	})
	r.SetRepo(paths[1], Repo{Name: "B"})
	r.RemoveProject(paths[0])
	r.RemoveProject(paths[0])
	if want := []string{id + ":0:1"}; !slices.Equal(heard, want) {
		t.Fatalf("heard %v, want %v", heard, want)
	}
}

func TestARemovalThatFailedToSaveKeepsTheProjectAndTellsNoHook(t *testing.T) {
	db, paths := newDB(t), folders(t, "a")
	r := open(t, db)
	r.AddProject(paths[0], nil)
	heard := false
	r.OnRemove(func(string) { heard = true })
	db.Close()
	if err := r.RemoveProject(paths[0]); err == nil {
		t.Fatal("removed with a closed database")
	}
	if heard || !slices.Equal(r.Load().Projects, paths) {
		t.Fatalf("heard %v, projects %v", heard, r.Load().Projects)
	}
}

func TestSettingAProjectReplacesItsRepoAndKeepsItsId(t *testing.T) {
	db, paths := newDB(t), folders(t, "app")
	r := open(t, db)
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
	if got := open(t, db).Load().Repos[paths[0]]; got.ID != id || got.Name != "New" {
		t.Fatalf("%+v", got)
	}
	if err := r.SetRepo("/nowhere", Repo{}); err != ErrUnknownProject {
		t.Fatalf("%v", err)
	}
	quiet(t, ch)
}

func TestTrackingAWorktreeIsIdempotentBothWays(t *testing.T) {
	db := newDB(t)
	r := open(t, db)
	ch, stop := r.Subscribe()
	defer stop()
	r.Track("/wt/a", true)
	if l := published(t, ch); !slices.Equal(l.Tracked, []string{"/wt/a"}) {
		t.Fatalf("%+v", l)
	}
	r.Track("/wt/a", true)
	quiet(t, ch)
	if got := open(t, db).Load().Tracked; !slices.Equal(got, []string{"/wt/a"}) {
		t.Fatalf("%v", got)
	}
	r.Track("/wt/a", false)
	if l := published(t, ch); len(l.Tracked) != 0 {
		t.Fatalf("%+v", l)
	}
	r.Track("/wt/a", false)
	quiet(t, ch)
}

func TestAPathTrackedAndMadeForAnAgentIsKeptInBothListsInOrder(t *testing.T) {
	db := newDB(t)
	r := open(t, db)
	r.Track("/wt/b", true)
	r.Track("/wt/a", true)
	r.AgentWorktree("/wt/a", true)
	r.AgentWorktree("/wt/c", true)
	r.AgentWorktrees(func(p []string) []string { return []string{p[1], p[0]} })
	f := open(t, db).Load()
	if !slices.Equal(f.Tracked, []string{"/wt/b", "/wt/a"}) || !slices.Equal(f.AgentWorktrees, []string{"/wt/c", "/wt/a"}) {
		t.Fatalf("%v %v", f.Tracked, f.AgentWorktrees)
	}
}

func TestSharedSettingsAreReplacedWholeAndSurviveARestart(t *testing.T) {
	r, db := imported(t, `{"worktree":{"root":"/old"},"agents":{"access":"edits"}}`)
	ch, stop := r.Subscribe()
	defer stop()
	off := false
	want := proto.SharedSettings{Git: proto.GitSettings{Remote: "fork", CommitAll: &off, ForceWithLease: true}, WorktreeRoot: "/new"}
	if err := r.SetSettings(want); err != nil {
		t.Fatal(err)
	}
	if l := published(t, ch); !reflect.DeepEqual(l.Settings, want) {
		t.Fatalf("%+v", l.Settings)
	}
	f := open(t, db).Load()
	if f.Worktree.Root != "/new" || f.Agents.Access != "" || f.Git.PushRemote() != "fork" || f.Git.CommitsAll() || !f.Git.ForceWithLease {
		t.Fatalf("%+v", f)
	}
}

func TestAFailedWriteChangesNothingAndPublishesNothing(t *testing.T) {
	db := newDB(t)
	r := open(t, db)
	r.Track("/wt/a", true)
	ch, stop := r.Subscribe()
	defer stop()
	db.Close()
	if err := r.Track("/wt/b", true); err == nil {
		t.Fatal("wrote into a closed database")
	}
	if got := r.Load().Tracked; !slices.Equal(got, []string{"/wt/a"}) {
		t.Fatalf("%v", got)
	}
	quiet(t, ch)
}

func TestTheListWritesEmptyCollectionsAndAnUnsetLaunchAbsent(t *testing.T) {
	paths := folders(t, "app")
	r := open(t, newDB(t))
	r.AddProject(paths[0], nil)
	raw, _ := json.Marshal(r.List())
	path, _ := json.Marshal(paths[0])
	want := `{"type":"registry.list","projects":[{"id":"` + r.Load().Repos[paths[0]].ID + `","path":` + string(path) + `,"repo":{"name":"","color":0,"setup":"","teardown":"","dev_url":"","icon":null,"gh_account":"","accounts":{},"hidden":[]}}],"tracked":[],"settings":{"git":{"remote":"","commit_all":null,"force_with_lease":false},"access":"","worktree_root":""}}`
	if string(raw) != want {
		t.Fatalf("got %s", raw)
	}
}

func TestGitSettingsLoadAndUnsetOnesTakeTheDesktopsDefaults(t *testing.T) {
	if r, _ := imported(t, `{"git":{"remote":"fork","commit_all":false,"force_with_lease":true}}`); r.Load().Git.PushRemote() != "fork" || r.Load().Git.CommitsAll() {
		t.Fatalf("got %+v", r.Load().Git)
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

func TestABrokenStateFileReadsAsDesktopJSONAndIsLeftAsItIs(t *testing.T) {
	dir := home(t, `{"projects":["/w/app"]}`)
	os.MkdirAll(filepath.Join(dir, "state"), 0o700)
	os.WriteFile(filepath.Join(dir, "state", "registry.json"), []byte("{"), 0o600)
	s, err := ReadLegacy(dir)
	if err != nil || len(s.Projects) != 1 || s.Projects[0].Path != "/w/app" {
		t.Fatalf("%+v %v", s, err)
	}
	if raw, _ := os.ReadFile(filepath.Join(dir, "state", "registry.json")); string(raw) != "{" {
		t.Fatalf("written over: %q", raw)
	}
	if _, err := os.Stat(filepath.Join(dir, "state", "registry.json.broken")); !os.IsNotExist(err) {
		t.Fatalf("renamed: %v", err)
	}
}

func TestAStateFileIsReadOverDesktopJSON(t *testing.T) {
	dir := home(t, `{"projects":["/w/desktop"]}`)
	os.MkdirAll(filepath.Join(dir, "state"), 0o700)
	os.WriteFile(filepath.Join(dir, "state", "registry.json"), []byte(`{"projects":[{"id":"p1","path":"/w/state","repo":{"name":"S"}}],"agent_worktrees":["/wt/x"]}`), 0o600)
	s, err := ReadLegacy(dir)
	if err != nil || len(s.Projects) != 1 || s.Projects[0].ID != "p1" || s.Projects[0].Repo.Name != "S" || !slices.Equal(s.AgentWorktrees, []string{"/wt/x"}) {
		t.Fatalf("%+v %v", s, err)
	}
}

func TestAKeyOfDesktopJSONInAShapeItCantReadLosesOnlyItself(t *testing.T) {
	s, err := ReadLegacy(home(t, `{"projects":["/w/app"],"git":{"remote":7},"worktree":{"root":"/wt"}}`))
	if f := s.File(); err != nil || !slices.Equal(f.Projects, []string{"/w/app"}) || f.Worktree.Root != "/wt" {
		t.Fatalf("%+v %v", f, err)
	}
}

func TestAStateFileThatCantBeReadIsAnError(t *testing.T) {
	if os.Getuid() == 0 {
		t.Skip("root reads any file")
	}
	if runtime.GOOS == "windows" {
		t.Skip("mode 0 only marks a file read-only on Windows; it stays readable")
	}
	dir := t.TempDir()
	path := filepath.Join(dir, "state", "registry.json")
	os.MkdirAll(filepath.Dir(path), 0o700)
	os.WriteFile(path, []byte(`{}`), 0)
	if _, err := ReadLegacy(dir); err == nil {
		t.Fatal("read")
	}
}
