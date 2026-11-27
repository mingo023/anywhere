package config

import (
	"encoding/json"
	"io/fs"
	"net"
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"testing"
)

// tree lists every path under dir with a regular file's contents, so two homes compare whole.
func tree(t *testing.T, dir string) []string {
	t.Helper()
	var out []string
	filepath.WalkDir(dir, func(path string, d fs.DirEntry, err error) error {
		if err != nil || path == dir {
			return err
		}
		rel, _ := filepath.Rel(dir, path)
		if d.Type().IsRegular() {
			raw, _ := os.ReadFile(path)
			rel += "=" + string(raw)
		}
		out = append(out, rel)
		return nil
	})
	return out
}

func write(t *testing.T, root string, files ...string) {
	t.Helper()
	for _, f := range files {
		name, body, _ := strings.Cut(f, "=")
		path := filepath.Join(root, name)
		os.MkdirAll(filepath.Dir(path), 0o700)
		if err := os.WriteFile(path, []byte(body), 0o600); err != nil {
			t.Fatal(err)
		}
	}
}

func homes(t *testing.T) (legacy, home string) {
	if runtime.GOOS == "windows" {
		t.Skip("pocketd keeps its legacy home on Windows")
	}
	user := t.TempDir()
	return filepath.Join(user, ".coding-pocket"), filepath.Join(user, ".anywhere")
}

func linked(t *testing.T, legacy, home string) {
	t.Helper()
	if to, err := os.Readlink(legacy); err != nil || to != home {
		t.Fatalf("legacy is %q %v, not a link to %s", to, err, home)
	}
}

func TestADevHomeMovesWholeAndLeavesALinkBehind(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("pocketd keeps its legacy home on Windows")
	}
	user := t.TempDir()
	legacy, home := filepath.Join(user, ".coding-pocket-dev"), filepath.Join(user, ".anywhere", "dev")
	write(t, legacy, "config.json={}", "state/board.json=[]")
	if moved, err := migrate(legacy, home); !moved || err != nil {
		t.Fatal(moved, err)
	}
	linked(t, legacy, home)
	if got := tree(t, home); !slices.Equal(got, []string{"config.json={}", "state", "state/board.json=[]"}) {
		t.Fatal(got)
	}
	if moved, err := migrate(legacy, home); moved || err != nil {
		t.Fatal("a second run moves nothing", moved, err)
	}
}

func TestAReleaseHomeMovesBesideWhatAnywhereAlreadyKeepsThere(t *testing.T) {
	legacy, home := homes(t)
	write(t, home, "accounts/claude/a=1")
	write(t, legacy, "config.json={}", "pocketd.lock=1", "pocketd.sock=", "state/names.json={}")
	if moved, err := migrate(legacy, home); !moved || err != nil {
		t.Fatal(moved, err)
	}
	linked(t, legacy, home)
	want := []string{"accounts", "accounts/claude", "accounts/claude/a=1", "config.json={}", "pocketd.lock=1", "state", "state/names.json={}"}
	if got := tree(t, home); !slices.Equal(got, want) {
		t.Fatal(got)
	}
}

func TestAFolderInBothHomesStopsTheMoveBeforeAnythingMoves(t *testing.T) {
	legacy, home := homes(t)
	write(t, home, "state/x=new")
	write(t, legacy, "config.json={}", "state/x=old")
	if moved, err := migrate(legacy, home); moved || err == nil || !strings.Contains(err.Error(), "state") {
		t.Fatal(moved, err)
	}
	if got := tree(t, legacy); !slices.Equal(got, []string{"config.json={}", "state", "state/x=old"}) {
		t.Fatal(got)
	}
}

func TestAFailedRenamePutsBackWhatMoved(t *testing.T) {
	legacy, home := homes(t)
	write(t, home, "logs=a file where legacy has a folder")
	write(t, legacy, "config.json={}", "logs/pocketd.log=x")
	if moved, err := migrate(legacy, home); moved || err == nil {
		t.Fatal(moved, err)
	}
	if got := tree(t, legacy); !slices.Equal(got, []string{"config.json={}", "logs", "logs/pocketd.log=x"}) {
		t.Fatal(got)
	}
	if got := tree(t, home); !slices.Equal(got, []string{"logs=a file where legacy has a folder"}) {
		t.Fatal(got)
	}
}

func TestARerunAfterACrashAnywhereFinishesTheSameMove(t *testing.T) {
	files := []string{"config.json={}", "desktop.json=d", "pocketd.lock=", "state/a=1"}
	want := []string{"accounts", "config.json={}", "desktop.json=d", "pocketd.lock=", "state", "state/a=1"}
	for crashAfter := range len(files) + 1 {
		legacy, home := homes(t)
		write(t, legacy, files...)
		os.MkdirAll(filepath.Join(home, "accounts"), 0o700)
		list, _ := os.ReadDir(legacy)
		for _, e := range list[:min(crashAfter, len(list))] {
			os.Rename(filepath.Join(legacy, e.Name()), filepath.Join(home, e.Name()))
		}
		// The next start's lock.Acquire makes the lock file again if it had moved.
		write(t, legacy, "pocketd.lock=")
		if moved, err := migrate(legacy, home); !moved || err != nil {
			t.Fatal(crashAfter, moved, err)
		}
		linked(t, legacy, home)
		if got := tree(t, home); !slices.Equal(got, want) {
			t.Fatal(crashAfter, got)
		}
	}
}

func TestAFileTheDesktopWroteToTheLegacyHomeAfterACrashWins(t *testing.T) {
	legacy, home := homes(t)
	write(t, home, "desktop.json=before", "config.json={}")
	write(t, legacy, "desktop.json=after")
	if moved, err := migrate(legacy, home); !moved || err != nil {
		t.Fatal(moved, err)
	}
	if got := tree(t, home); !slices.Equal(got, []string{"config.json={}", "desktop.json=after"}) {
		t.Fatal(got)
	}
}

func TestALegacyFolderThatWontEmptyIsKeptInsideTheNewHome(t *testing.T) {
	legacy, home := homes(t)
	write(t, home, "state/a=1")
	write(t, legacy, "state/b=late")
	if err := replace(legacy, home); err != nil {
		t.Fatal(err)
	}
	linked(t, legacy, home)
	if kept, _ := filepath.Glob(filepath.Join(home, "legacy-*", "state", "b")); len(kept) != 1 {
		t.Fatal(tree(t, home))
	}
}

func TestTheNextStartMakesTheLinkACrashLeftUnmade(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("pocketd keeps its legacy home on Windows")
	}
	user := t.TempDir()
	t.Setenv("HOME", user)
	legacy, home := filepath.Join(user, LegacyHomeNames()[0]), filepath.Join(user, HomeName())
	write(t, legacy, "config.json={}", moving+"="+legacy)
	os.MkdirAll(filepath.Dir(home), 0o700)
	os.Rename(legacy, home)
	if got, err := MoveLegacyHome(DefaultHome()); got != home || err != nil {
		t.Fatal(got, err)
	}
	linked(t, legacy, home)
	if got := tree(t, home); !slices.Equal(got, []string{"config.json={}"}) {
		t.Fatal(got)
	}
	os.Remove(legacy)
	if _, err := MoveLegacyHome(home); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Lstat(legacy); err == nil {
		t.Fatal("a home that never moved got a legacy link")
	}
}

func TestAHoldersSocketStillAnswersAtBothPathsAfterTheMove(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("pocketd keeps its legacy home on Windows")
	}
	// A socket path must fit in 104 bytes, which t.TempDir's on macOS doesn't leave room for.
	user, err := os.MkdirTemp("/tmp", "home")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(user) })
	legacy, home := filepath.Join(user, ".coding-pocket"), filepath.Join(user, ".anywhere")
	write(t, legacy, "config.json={}")
	os.Mkdir(filepath.Join(legacy, "holders"), 0o700)
	ln, err := net.Listen("unix", filepath.Join(legacy, "holders", "ab.sock"))
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()
	go func() {
		for {
			c, err := ln.Accept()
			if err != nil {
				return
			}
			c.Write([]byte("hi"))
			c.Close()
		}
	}()
	if moved, err := migrate(legacy, home); !moved || err != nil {
		t.Fatal(moved, err)
	}
	for _, dir := range []string{home, legacy} {
		c, err := net.Dial("unix", filepath.Join(dir, "holders", "ab.sock"))
		if err != nil {
			t.Fatal(dir, err)
		}
		buf := make([]byte, 2)
		if n, _ := c.Read(buf); string(buf[:n]) != "hi" {
			t.Fatal(dir, string(buf[:n]))
		}
		c.Close()
	}
}

func TestOnlyTheLegacyHomeMoves(t *testing.T) {
	user := t.TempDir()
	t.Setenv("HOME", user)
	scratch := filepath.Join(user, "scratch")
	write(t, scratch, "config.json={}")
	if got, err := MoveLegacyHome(scratch); got != scratch || err != nil {
		t.Fatal(got, err)
	}
	if _, err := os.Stat(filepath.Join(scratch, "config.json")); err != nil {
		t.Fatal(err)
	}
}

func TestMoveLegacyHomeHandsBackTheNewHome(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("pocketd keeps its legacy home on Windows")
	}
	user := t.TempDir()
	t.Setenv("HOME", user)
	legacy := filepath.Join(user, LegacyHomeNames()[0])
	write(t, legacy, "config.json={}")
	if got, err := MoveLegacyHome(legacy); got != filepath.Join(user, ".zoes", "dev") || err != nil {
		t.Fatal(got, err)
	}
}

// lay makes each entry of layout under user: "a/b/" a folder, "a -> b" a link to user/b.
func lay(t *testing.T, user string, layout ...string) {
	t.Helper()
	for _, e := range layout {
		if from, to, ok := strings.Cut(e, " -> "); ok {
			if err := os.Symlink(filepath.Join(user, to), filepath.Join(user, from)); err != nil {
				t.Fatal(err)
			}
		} else if err := os.MkdirAll(filepath.Join(user, e), 0o700); err != nil {
			t.Fatal(err)
		}
	}
}

// at points the user's home at a fresh folder and the build at channel.
func at(t *testing.T, ch string) string {
	if runtime.GOOS == "windows" {
		t.Skip("pocketd keeps its legacy home on Windows")
	}
	user, _ := filepath.EvalSymlinks(t.TempDir())
	t.Setenv("HOME", user)
	t.Setenv("POCKET_HOME", "")
	channel = ch
	t.Cleanup(func() { channel = "" })
	return user
}

// The desktop reads the same table in daemon.rs, so both find the same home.
func TestEachLayoutHasTheHomeTheDesktopFindsToo(t *testing.T) {
	raw, err := os.ReadFile(filepath.Join("testdata", "homes.json"))
	if err != nil {
		t.Fatal(err)
	}
	var cases []struct {
		Name         string
		Layout       []string
		Release, Dev string
	}
	if err := json.Unmarshal(raw, &cases); err != nil {
		t.Fatal(err)
	}
	for _, c := range cases {
		for ch, want := range map[string]string{"release": c.Release, "": c.Dev} {
			user := at(t, ch)
			lay(t, user, c.Layout...)
			if got := DefaultHome(); got != filepath.Join(user, want) {
				t.Errorf("%s, %q build: %s, want %s", c.Name, ch, got, want)
			}
		}
	}
}

func TestAnAnywhereHomeMovesToZoesAndOldPathsStillLeadThere(t *testing.T) {
	user := at(t, "release")
	lay(t, user, ".anywhere/dev/", ".coding-pocket -> .anywhere")
	write(t, filepath.Join(user, ".anywhere"), "state/pocketd.db=db", "dev/desktop.json=d")
	home := DefaultHome()
	got, err := MoveLegacyHome(home)
	zoes := filepath.Join(user, ".zoes")
	if got != zoes || err != nil {
		t.Fatal(got, err)
	}
	linked(t, home, zoes)
	for _, old := range []string{".anywhere/state/pocketd.db", ".coding-pocket/state/pocketd.db", ".anywhere/dev/desktop.json"} {
		if _, err := os.Stat(filepath.Join(user, old)); err != nil {
			t.Error(err)
		}
	}
	if DefaultHome() != zoes || SharedHome() != zoes {
		t.Fatal(DefaultHome(), SharedHome())
	}
	want := []Move{{filepath.Join(user, ".coding-pocket"), zoes}, {filepath.Join(user, ".anywhere"), zoes}}
	if got := Moved(); !slices.Equal(got, want) {
		t.Fatal(got)
	}
	channel = ""
	if DefaultHome() != filepath.Join(zoes, "dev") {
		t.Fatal("the dev home moved with it", DefaultHome())
	}
}

func TestAHomeFromBeforeAnywhereMovesStraightToZoesWithWhatAnywhereKept(t *testing.T) {
	user := at(t, "release")
	write(t, filepath.Join(user, ".coding-pocket"), "state/pocketd.db=db")
	write(t, filepath.Join(user, ".anywhere"), "accounts/claude/a/x=1")
	home := DefaultHome()
	if home != filepath.Join(user, ".coding-pocket") {
		t.Fatal(home)
	}
	zoes := filepath.Join(user, ".zoes")
	if got, err := MoveLegacyHome(home); got != zoes || err != nil {
		t.Fatal(got, err)
	}
	linked(t, filepath.Join(user, ".coding-pocket"), zoes)
	linked(t, filepath.Join(user, ".anywhere"), zoes)
	want := []string{"accounts", "accounts/claude", "accounts/claude/a", "accounts/claude/a/x=1", "state", "state/pocketd.db=db"}
	if got := tree(t, zoes); !slices.Equal(got, want) {
		t.Fatal(got)
	}
	if DefaultHome() != zoes {
		t.Fatal(DefaultHome())
	}
}

func TestAHomeThatCantMoveStaysWhereDefaultHomeFindsIt(t *testing.T) {
	user := at(t, "release")
	write(t, filepath.Join(user, ".coding-pocket"), "state/pocketd.db=db")
	write(t, filepath.Join(user, ".anywhere"), "state/other=x")
	home := DefaultHome()
	if got, err := MoveLegacyHome(home); got != home || err == nil {
		t.Fatal(got, err)
	}
	if DefaultHome() != home {
		t.Fatal("the next start looks somewhere else:", DefaultHome())
	}
	if got := tree(t, home); !slices.Equal(got, []string{"state", "state/pocketd.db=db"}) {
		t.Fatal(got)
	}
}

func TestADevHomeWaitsInsideAnywhereForTheReleaseHomeToMove(t *testing.T) {
	user := at(t, "")
	lay(t, user, ".anywhere/dev/", ".anywhere/worktrees/")
	home := DefaultHome()
	if home != filepath.Join(user, ".anywhere", "dev") {
		t.Fatal(home)
	}
	if got, err := MoveLegacyHome(home); got != home || err != nil {
		t.Fatal(got, err)
	}
	if _, err := os.Lstat(filepath.Join(user, ".zoes")); err == nil {
		t.Fatal("a dev start made ~/.zoes before the release home moved there")
	}
	if SharedHome() != filepath.Join(user, ".anywhere") || len(Moved()) != 0 {
		t.Fatal(SharedHome(), Moved())
	}
}

func TestAPocketHomeNamingAnyLegacyHomeCountsAsUnset(t *testing.T) {
	user := at(t, "release")
	lay(t, user, ".zoes/", ".anywhere -> .zoes")
	for _, set := range []string{".anywhere", ".coding-pocket/"} {
		t.Setenv("POCKET_HOME", filepath.Join(user, set))
		if Home() != filepath.Join(user, ".zoes") {
			t.Error(set, Home())
		}
	}
}

func TestRehomeSwapsOnlyAWholeLeadingHome(t *testing.T) {
	moves := []Move{{"/u/.anywhere", "/u/.zoes"}}
	for in, want := range map[string]string{
		"/u/.anywhere":                 "/u/.zoes",
		"/u/.anywhere/worktrees/app/x": "/u/.zoes/worktrees/app/x",
		"/u/.anywhere-other/x":         "/u/.anywhere-other/x",
		"//m1/u/.anywhere/worktrees/x": "//m1/u/.anywhere/worktrees/x",
		"/w/app/.anywhere/config.json": "/w/app/.anywhere/config.json",
		"":                             "",
	} {
		if got := Rehome(moves, in); got != want {
			t.Errorf("%q: %q", in, got)
		}
	}
}

func TestAccountsStayWhereTheyWereMadeSoClaudeFindsItsKeychainItem(t *testing.T) {
	user := at(t, "release")
	if AccountsDir() != filepath.Join(user, ".zoes", "accounts") {
		t.Fatal("a fresh install", AccountsDir())
	}
	lay(t, user, ".zoes/accounts/", ".anywhere -> .zoes")
	if AccountsDir() != filepath.Join(user, ".anywhere", "accounts") {
		t.Fatal("after the move", AccountsDir())
	}
}
