package hosts

import (
	"context"
	"crypto/sha256"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"slices"
	"strconv"
	"strings"
	"testing"
	"time"

	"pocketd/internal/config"
)

func TestBootstrapsLinesBecomeProgressAndTheRestIsLog(t *testing.T) {
	for line, want := range map[string]Progress{
		"ANYWHERE step detect":                            {Kind: ProgressStep, Step: StepDetect},
		"ANYWHERE step service\n":                         {Kind: ProgressStep, Step: StepService},
		"ANYWHERE step START":                             {Kind: ProgressStep, Step: StepStart},
		"ANYWHERE warn linger is off":                     {Kind: ProgressWarn, Text: "linger is off"},
		"ANYWHERE warn":                                   {Kind: ProgressWarn},
		"ANYWHERE error checksum mismatch: got a, want b": {Kind: ProgressError, Text: "checksum mismatch: got a, want b"},
		"ANYWHERE running":                                {Kind: ProgressRunning},
		"ANYWHERE done":                                   {Kind: ProgressDone},
	} {
		if got, ok := parseProgress(line); !ok || got != want {
			t.Errorf("%q: %+v %v", line, got, ok)
		}
	}
	for _, log := range []string{`{"label":"x","installed":true}`, "ANYWHERE step teleport", "ANYWHERE done now", "ANYWHERE running fast", "ANYWHERE"} {
		if got, ok := parseProgress(log); ok {
			t.Errorf("%q: %+v", log, got)
		}
	}
}

func TestEveryStepBootstrapReportsIsKnown(t *testing.T) {
	for _, step := range []string{"detect", "download", "verify", "unpack", "link", "service", "start"} {
		if !strings.Contains(bootstrapSh, "say step "+step) {
			t.Errorf("bootstrap.sh never says %s", step)
		}
		if !strings.Contains(bootstrapPs1, "Step '"+step+"'") {
			t.Errorf("bootstrap.ps1 never says %s", step)
		}
		if _, ok := parseProgress("ANYWHERE step " + step); !ok {
			t.Errorf("%s isn't known", step)
		}
	}
}

func TestUnameNamesTheArchivePastAnyLoginNoise(t *testing.T) {
	for out, want := range map[string]Platform{
		"Welcome!\nLinux\naarch64\n":      {"linux", "arm64"},
		"Darwin\narm64\n":                 {"darwin", "arm64"},
		"Linux\nx86_64\n":                 {"linux", "amd64"},
		"MINGW64_NT-10.0-22631\nx86_64\n": {"windows", "amd64"},
	} {
		if got, err := parseUname(out); err != nil || got != want {
			t.Errorf("%q: %+v %v", out, got, err)
		}
	}
	for out, want := range map[string]string{
		"FreeBSD\namd64\n": "Anywhere doesn't run on FreeBSD",
		"Linux\nriscv64\n": "Anywhere doesn't run on riscv64",
		"":                 "uname printed nothing",
		"Linux\n":          "uname printed nothing",
	} {
		if _, err := parseUname(out); err == nil || err.Error() != want {
			t.Errorf("%q: %v, want %s", out, err, want)
		}
	}
}

func TestAnArchiveOnThisMacWinsOverTheRelease(t *testing.T) {
	dist := t.TempDir()
	archive := filepath.Join(dist, "pocketd-linux-arm64.tar.gz")
	os.WriteFile(archive, nil, 0o644)
	if got, err := source(Platform{"linux", "arm64"}, "1.2.0", dist); err != nil || got != (Source{Archive: archive}) {
		t.Errorf("%+v %v", got, err)
	}
	release := Source{Version: "1.2.0", URL: "https://github.com/mingo023/anywhere/releases/download/v1.2.0"}
	if got, err := source(Platform{"darwin", "arm64"}, "1.2.0", dist); err != nil || got != release {
		t.Errorf("%+v %v", got, err)
	}
}

func TestAnInstallRunsAsThePlatformTheProbeFound(t *testing.T) {
	host := New(alias("pc"), nil)
	dist := t.TempDir()
	archive := filepath.Join(dist, "pocketd-windows-amd64.zip")
	os.WriteFile(archive, nil, 0o644)
	on, src, err := installPlan(host, Platform{"windows", "amd64"}, "dev", dist)
	if err != nil || !on.Windows || src != (Source{Archive: archive}) {
		t.Errorf("%+v %+v %v", on, src, err)
	}
	host.Windows = true
	if on, _, err := installPlan(host, Platform{"linux", "arm64"}, "1.2.0", ""); err != nil || on.Windows {
		t.Errorf("%+v %v", on, err)
	}
}

func TestADevBuildWithoutAnArchiveSaysHowToMakeOne(t *testing.T) {
	_, err := source(Platform{"linux", "amd64"}, "dev", "")
	if err == nil || !strings.Contains(err.Error(), "scripts/package-pocketd.sh dev linux-amd64") || !strings.Contains(err.Error(), "in packages/pocketd/dist.") {
		t.Errorf("%v", err)
	}
	_, err = source(Platform{"windows", "arm64"}, "dev", "")
	if err == nil || !strings.Contains(err.Error(), "No pocketd-windows-arm64.zip") || !strings.Contains(err.Error(), "scripts/package-pocketd-windows.sh dev arm64") {
		t.Errorf("%v", err)
	}
}

func TestPowershellNamesAWindowsHostsCPUAndAUnixShellSaysNothing(t *testing.T) {
	if got, found, err := parseWindowsArch("Welcome\r\nwindows AMD64\r\n"); !found || err != nil || got != (Platform{"windows", "amd64"}) {
		t.Errorf("%+v %v %v", got, found, err)
	}
	if _, found, err := parseWindowsArch("windows x86\n"); !found || err == nil || err.Error() != "Anywhere doesn't run on x86" {
		t.Errorf("%v %v", found, err)
	}
	if _, found, err := parseWindowsArch("sh: powershell: command not found\n"); found || err != nil {
		t.Errorf("%v %v", found, err)
	}
	if !strings.Contains(decoded(t, windowsArchCommand()), "PROCESSOR_ARCHITEW6432") {
		t.Error("a 32-bit shell isn't handled")
	}
}

func filled(t *testing.T, script string) string {
	t.Helper()
	got, err := fill(script, "1.2.0", "https://x/v1.2.0", ".coding-pocket", "dev.mingo.anywhere.host.pocketd")
	if err != nil {
		t.Fatal(err)
	}
	for _, placeholder := range []string{"@@VERSION@@", "@@URL@@", "@@HOME_DIR@@", "@@LABEL@@", "@@PORT@@"} {
		if strings.Contains(got, placeholder) {
			t.Errorf("%s is left", placeholder)
		}
	}
	return got
}

func TestTheWindowsScriptIsAsciiAndFilledIn(t *testing.T) {
	for _, c := range []byte(bootstrapPs1) {
		if c >= 0x80 {
			t.Fatal("PowerShell reads stdin in the console's code page")
		}
	}
	script := filled(t, bootstrapPs1)
	for _, want := range []string{"$Version = '1.2.0'", "$HomeDir = Join-Path $HOME '.coding-pocket'", "return " + strconv.Itoa(config.DefaultOwnerPort())} {
		if !strings.Contains(script, want) {
			t.Errorf("no %q", want)
		}
	}
}

func TestTheRenderedScriptHasNoPlaceholderLeft(t *testing.T) {
	script := filled(t, bootstrapSh)
	for _, want := range []string{"VERSION='1.2.0'", `DIR="$HOME/.coding-pocket"`} {
		if !strings.Contains(script, want) {
			t.Errorf("no %q", want)
		}
	}
}

func TestAValueThatCouldBreakOutOfItsQuotesIsRefused(t *testing.T) {
	for _, c := range []struct{ script, version, url, dir, label string }{
		{bootstrapSh, "1.2.0'; rm -rf ~; '", "https://x", ".cp", "l"},
		{bootstrapPs1, "1.2.0", "https://x", ".cp", "l' + (iex 'calc') + '"},
		{bootstrapSh, "1.2.0", "https://x", "$(id)", "l"},
		{bootstrapSh, "1.2.0", "https://x", "$HOME", "l"},
		{bootstrapSh, "1.2.0", "https://x", ".cp", "\u00e9"},
	} {
		if _, err := fill(c.script, c.version, c.url, c.dir, c.label); err == nil {
			t.Errorf("%+v was filled in", c)
		}
	}
	if _, err := fill(bootstrapSh, "0.0.0-e2e+1", "https://github.com/a/b/v0.0.0", ".coding-pocket-e2e", "dev.mingo.anywhere.host.pocketd"); err != nil {
		t.Error(err)
	}
}

func TestOnlyTheAppsOwnFolderGetsTheAppsServiceName(t *testing.T) {
	if got := label(config.HomeName()); got != config.HostLabel() {
		t.Errorf("%s", got)
	}
	if got := label(".coding-pocket-e2e"); got != config.HostLabel()+".coding-pocket-e2e" {
		t.Errorf("%s", got)
	}
	if got := label(config.LegacyHomeName()); got != config.HostLabel() {
		t.Errorf("an update on a host that hasn't moved its home names a second service: %s", got)
	}
}

func TestAHostKeepsUsingTheLegacyFolderUntilItsPocketdMovesIt(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("the stand-in ssh is a shell script")
	}
	bin, home := t.TempDir(), t.TempDir()
	script := "#!/bin/sh\nfor a; do last=$a; done\nHOME='" + home + "' exec /bin/sh -c \"$last\"\n"
	if err := os.WriteFile(filepath.Join(bin, "ssh"), []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	m, err := openManager(t, Paths{Home: t.TempDir(), RemoteDir: ".anywhere", LegacyDir: ".coding-pocket"}, func() []string { return []string{"PATH=" + bin + ":/bin:/usr/bin"} }, "1.2.0", "Mac")
	if err != nil {
		t.Fatal(err)
	}
	e, err := m.Add(alias("devbox"), "")
	if err != nil {
		t.Fatal(err)
	}
	legacy, current := filepath.Join(home, ".coding-pocket"), filepath.Join(home, ".anywhere")
	dir := func() string {
		t.Helper()
		d, err := m.remoteDir(context.Background(), e.Host, false)
		if err != nil {
			t.Fatal(err)
		}
		return d
	}
	if got := dir(); got != ".anywhere" {
		t.Errorf("a new host: %s", got)
	}
	os.Mkdir(legacy, 0o700)
	if got := dir(); got != ".coding-pocket" {
		t.Errorf("a host that hasn't moved: %s", got)
	}
	os.Remove(legacy)
	os.Mkdir(current, 0o700)
	os.Symlink(current, legacy)
	if got := dir(); got != ".anywhere" {
		t.Errorf("a host that has moved: %s", got)
	}
	elsewhere := filepath.Join(home, "disk")
	os.Mkdir(elsewhere, 0o700)
	os.Remove(legacy)
	os.Symlink(elsewhere, legacy)
	if got := dir(); got != ".coding-pocket" {
		t.Errorf("a legacy home the user linked to another disk: %s", got)
	}
}

func TestAnUploadedArchiveIsCheckedThenRemovedWhateverTheLoginShell(t *testing.T) {
	got := bootstrapCommand(".cp", []string{`ANYWHERE_ARCHIVE="$HOME/.cp/upload.tar.gz"`, "ANYWHERE_FORCE=1"}, true)
	want := `sh -c 'ANYWHERE_ARCHIVE="$HOME/.cp/upload.tar.gz" ANYWHERE_FORCE=1 sh -s; rc=$?; rm -f "$HOME/.cp/upload.tar.gz"; exit $rc'`
	if got != want {
		t.Errorf("%s", got)
	}
	if got := bootstrapCommand(".cp", nil, false); got != "sh -c 'sh -s'" {
		t.Errorf("%s", got)
	}
}

func TestTheChecksumComesFromTheFileBesideTheArchive(t *testing.T) {
	dir := t.TempDir()
	archive := filepath.Join(dir, "pocketd-linux-arm64.tar.gz")
	sums := archive + ".sha256"
	sum := strings.Repeat("ab", 32)
	os.WriteFile(sums, []byte(sum+"  pocketd-linux-arm64.tar.gz\n"), 0o644)
	if got, ok := checksum(archive); !ok || got != sum {
		t.Errorf("%q %v", got, ok)
	}
	for _, bad := range []string{"nope\n", "", "abcd\n", strings.Repeat("zz", 32)} {
		os.WriteFile(sums, []byte(bad), 0o644)
		if got, ok := checksum(archive); ok {
			t.Errorf("%q gave %q", bad, got)
		}
	}
	if _, ok := checksum(filepath.Join(dir, "none.tar.gz")); ok {
		t.Error("a missing file has a checksum")
	}
}

func TestAnUpdateLeavesAloneThePocketdOfAnywhereDevOrAnUploadedBuild(t *testing.T) {
	for name, c := range map[string]struct{ app, id, url string }{
		"Anywhere Dev":          {"Anywhere Dev.app", "dev.mingo.anywhere.dev", "https://x/v1.2.0"},
		"an uploaded dev build": {"Anywhere.app", "dev.mingo.anywhere", ""},
	} {
		var exe string
		out, linked := forceUpdate(t, func(dir string) string {
			exe = anywhereApp(t, filepath.Join(dir, "..", "..", c.app), c.id, "#!/bin/sh\n")
			return exe
		}, c.url)
		if runtime.GOOS != "darwin" {
			if !linked {
				t.Errorf("%s: only a Mac runs the app, yet the update stopped:\n%s", name, out)
			}
			continue
		}
		if linked || !strings.Contains(out, "ANYWHERE error pocketd here runs from "+exe+", which an update can't replace") {
			t.Errorf("%s: linked %v:\n%s", name, linked, out)
		}
	}
}

func TestAnUpdateReplacesTheAnywhereAppWhosePocketdRunsThere(t *testing.T) {
	if runtime.GOOS != "darwin" {
		t.Skip("only a Mac runs the app")
	}
	const newer = "#!/bin/sh\n# 1.2.0\necho '{\"version\": \"1.2.0\"}'\n"
	dmg := releaseDMG(t, newer)
	var app string
	out, linked := forceUpdate(t, func(dir string) string {
		app = filepath.Join(filepath.Dir(filepath.Dir(dir)), "Applications", "Anywhere.app")
		return anywhereApp(t, app, "dev.mingo.anywhere", "#!/bin/sh\n")
	}, serveRelease(t, dmg, fmt.Sprintf("%x  Anywhere.dmg\n", sha256.Sum256(dmg))), "ANYWHERE_ALLOW_HTTP=1")
	now, _ := os.ReadFile(filepath.Join(app, appPocketd))
	if linked || string(now) != newer || !strings.HasSuffix(out, "ANYWHERE done\n") {
		t.Errorf("linked %v, app's pocketd %q:\n%s", linked, now, out)
	}
	if left, _ := filepath.Glob(app + ".*"); len(left) > 0 {
		t.Errorf("left beside the app: %v", left)
	}
}

func TestAnAppUpdateWhoseDMGFailsItsChecksumKeepsTheAppThere(t *testing.T) {
	if runtime.GOOS != "darwin" {
		t.Skip("only a Mac runs the app")
	}
	dmg := releaseDMG(t, "#!/bin/sh\n# 1.2.0\n")
	var app string
	out, _ := forceUpdate(t, func(dir string) string {
		app = filepath.Join(filepath.Dir(filepath.Dir(dir)), "Applications", "Anywhere.app")
		return anywhereApp(t, app, "dev.mingo.anywhere", "#!/bin/sh\n# old\n")
	}, serveRelease(t, dmg, strings.Repeat("ab", 32)+"  Anywhere.dmg\n"), "ANYWHERE_ALLOW_HTTP=1")
	now, _ := os.ReadFile(filepath.Join(app, appPocketd))
	if string(now) != "#!/bin/sh\n# old\n" || !strings.Contains(out, "ANYWHERE error checksum mismatch") {
		t.Errorf("app's pocketd %q:\n%s", now, out)
	}
}

func TestAnAppUpdateThatCantMoveTheAppLeavesItThere(t *testing.T) {
	if runtime.GOOS != "darwin" {
		t.Skip("only a Mac runs the app")
	}
	dmg := releaseDMG(t, "#!/bin/sh\n# 1.2.0\necho '{\"version\": \"1.2.0\"}'\n")
	var app string
	out, _ := forceUpdate(t, func(dir string) string {
		app = filepath.Join(filepath.Dir(filepath.Dir(dir)), "Applications", "Anywhere.app")
		exe := anywhereApp(t, app, "dev.mingo.anywhere", "#!/bin/sh\n# old\n")
		if b, err := exec.Command("chflags", "uchg", app).CombinedOutput(); err != nil {
			t.Fatalf("%v: %s", err, b)
		}
		t.Cleanup(func() { exec.Command("chflags", "nouchg", app).Run() })
		return exe
	}, serveRelease(t, dmg, fmt.Sprintf("%x  Anywhere.dmg\n", sha256.Sum256(dmg))), "ANYWHERE_ALLOW_HTTP=1")
	now, _ := os.ReadFile(filepath.Join(app, appPocketd))
	if string(now) != "#!/bin/sh\n# old\n" || !strings.Contains(out, "ANYWHERE error can't replace "+app) {
		t.Errorf("app's pocketd %q:\n%s", now, out)
	}
	if left, _ := filepath.Glob(app + ".*"); len(left) > 0 {
		t.Errorf("left beside the app: %v", left)
	}
}

const appPocketd = "Contents/Library/Helpers/AnywhereDaemon.app/Contents/MacOS/pocketd"

func anywhereApp(t *testing.T, app, id, script string) string {
	exe := filepath.Join(app, appPocketd)
	if err := os.MkdirAll(filepath.Dir(exe), 0o755); err != nil {
		t.Fatal(err)
	}
	plist := `<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>` + id + `</string></dict></plist>
`
	if err := os.WriteFile(filepath.Join(app, "Contents", "Info.plist"), []byte(plist), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(exe, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	exe, err := filepath.EvalSymlinks(exe)
	if err != nil {
		t.Fatal(err)
	}
	return exe
}

func releaseDMG(t *testing.T, script string) []byte {
	dir := t.TempDir()
	anywhereApp(t, filepath.Join(dir, "stage", "Anywhere.app"), "dev.mingo.anywhere", script)
	dmg := filepath.Join(dir, "Anywhere.dmg")
	if b, err := exec.Command("hdiutil", "create", "-quiet", "-volname", "Anywhere", "-srcfolder", filepath.Join(dir, "stage"), "-format", "UDZO", dmg).CombinedOutput(); err != nil {
		t.Fatalf("%v: %s", err, b)
	}
	b, err := os.ReadFile(dmg)
	if err != nil {
		t.Fatal(err)
	}
	return b
}

func serveRelease(t *testing.T, dmg []byte, sums string) string {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/Anywhere.dmg":
			w.Write(dmg)
		case "/Anywhere.dmg.sha256":
			io.WriteString(w, sums)
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(srv.Close)
	return srv.URL
}

func TestAnUpdateReplacesThePocketdItsOwnServiceRuns(t *testing.T) {
	out, linked := forceUpdate(t, func(dir string) string { return filepath.Join(dir, "bin", "pocketd") }, "https://x/v1.2.0")
	if !linked || !strings.HasSuffix(out, "ANYWHERE done\n") {
		t.Errorf("linked %v:\n%s", linked, out)
	}
}

// forceUpdate runs bootstrap.sh with ANYWHERE_FORCE in a scratch home reached through a symlink, as launchd resolves
// it, against a pocketd that answers on its socket with the pid of a process whose argv[0] is running(dir), dir being
// the resolved path. An earlier update's bin/pocketd is already there. Every pocketd it runs is a stub, so nothing
// reaches launchd or systemd.
func forceUpdate(t *testing.T, running func(dir string) string, releaseURL string, env ...string) (out string, linked bool) {
	if runtime.GOOS == "windows" {
		t.Skip("bootstrap.sh runs on unix hosts")
	}
	base, err := os.MkdirTemp("", "bs")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(base) })
	if base, err = filepath.EvalSymlinks(base); err != nil {
		t.Fatal(err)
	}
	home := filepath.Join(base, "home")
	dir := filepath.Join(base, "real", ".cp")
	stub := filepath.Join(base, "src", "pocketd-test")
	for _, d := range []string{filepath.Join(dir, "bin"), stub} {
		if err := os.MkdirAll(d, 0o700); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.Symlink(filepath.Join(base, "real"), home); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "bin", "pocketd"), []byte("#!/bin/sh\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	script := "#!/bin/sh\ncase $1 in\nversion) echo '{\"version\": \"1.2.0\"}' ;;\nstatus) printf '{\\n  \"pid\": %s,\\n  \"version\": \"1.2.0\"\\n}\\n' \"$STUB_PID\" ;;\nservice) echo '{\"linger\":true}' ;;\nesac\n"
	if err := os.WriteFile(filepath.Join(stub, "pocketd"), []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	archive := filepath.Join(base, "pocketd.tar.gz")
	if b, err := exec.Command("tar", "-czf", archive, "-C", filepath.Dir(stub), filepath.Base(stub)).CombinedOutput(); err != nil {
		t.Fatalf("%v: %s", err, b)
	}
	b, _ := os.ReadFile(archive)
	ln, err := net.Listen("unix", filepath.Join(dir, "pocketd.sock"))
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()
	sleep, err := exec.LookPath("sleep")
	if err != nil {
		t.Fatal(err)
	}
	holder := &exec.Cmd{Path: sleep, Args: []string{running(dir), "30"}}
	if err := holder.Start(); err != nil {
		t.Fatal(err)
	}
	defer func() { holder.Process.Kill(); holder.Wait() }()
	rendered, err := fill(bootstrapSh, "1.2.0", releaseURL, ".cp", "dev.mingo.anywhere.test.host.pocketd")
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	defer cancel()
	cmd := exec.CommandContext(ctx, "sh", "-s")
	cmd.Stdin = strings.NewReader(rendered)
	cmd.Env = []string{"HOME=" + home, "PATH=" + os.Getenv("PATH"), "STUB_PID=" + strconv.Itoa(holder.Process.Pid), "ANYWHERE_FORCE=1",
		"ANYWHERE_ARCHIVE=" + archive, fmt.Sprintf("ANYWHERE_SHA256=%x", sha256.Sum256(b))}
	cmd.Env = append(cmd.Env, env...)
	got, _ := cmd.CombinedOutput()
	now, _ := os.ReadFile(filepath.Join(dir, "bin", "pocketd"))
	return string(got), string(now) == script
}

// Installs for real on the Linux host ANYWHERE_TEST_HOST names, an ssh alias that signs in with keys, from the archive
// `scripts/package-pocketd.sh` puts in dist for it, under a home and service of its own that it removes after.
func TestPocketdInstallsOnARealLinuxHostThenIsFoundRunning(t *testing.T) {
	name := os.Getenv("ANYWHERE_TEST_HOST")
	if name == "" {
		t.Skip("ANYWHERE_TEST_HOST names no host")
	}
	const dir = ".coding-pocket-e2e2"
	m, err := openManager(t, Paths{Home: t.TempDir(), RemoteDir: dir}, os.Environ, "dev", "Mac")
	if err != nil {
		t.Fatal(err)
	}
	e, err := m.Add(alias(name), "")
	if err != nil {
		t.Fatal(err)
	}
	h, ctx := e.Host, context.Background()
	sh := func(command string) (string, int) {
		out, _, code, err := m.run(ctx, h, command, nil)
		if err != nil {
			t.Fatal(err)
		}
		return string(out), code
	}
	pocketd := fmt.Sprintf(`POCKET_HOME="$HOME/%s" "$HOME/%s/bin/pocketd"`, dir, dir)
	cleanup := func() {
		sh(fmt.Sprintf(`%s service uninstall --label %s; rm -rf "$HOME/%s"`, pocketd, label(dir), dir))
	}
	cleanup()
	install := func(update bool) []Progress {
		var seen []Progress
		if err := m.install(ctx, h, update, filepath.Join("..", "..", "dist"), func(p Progress) { seen = append(seen, p) }); err != nil {
			t.Fatalf("%v, after %+v", err, seen)
		}
		return seen
	}

	if p, err := m.platform(ctx, h); err != nil || p.OS != "linux" {
		t.Fatalf("%+v %v", p, err)
	}
	seen := install(false)
	if seen[len(seen)-1].Kind != ProgressDone || !slices.Contains(seen, Progress{Kind: ProgressStep, Step: StepVerify}) || slices.Contains(seen, Progress{Kind: ProgressStep, Step: StepDownload}) {
		t.Fatalf("%+v", seen)
	}
	if out, _ := sh(pocketd + " status --json"); !strings.Contains(out, `"os": "linux"`) {
		t.Errorf("status: %s", out)
	}
	if _, code := sh(fmt.Sprintf(`test -e "$HOME/%s/%s"`, dir, upload)); code == 0 {
		t.Error("the upload is still there")
	}
	if again := install(false); !slices.Equal(again, []Progress{{Kind: ProgressStep, Step: StepDetect}, {Kind: ProgressRunning}}) {
		t.Errorf("installing again: %+v", again)
	}
	if updated := install(true); updated[len(updated)-1].Kind != ProgressDone {
		t.Errorf("updating: %+v", updated)
	}
	if _, code := sh(pocketd + " status"); code != 0 {
		t.Error("pocketd doesn't answer after the update")
	}
	cleanup()
	if _, code := sh("systemctl --user is-active " + label(dir)); code == 0 {
		t.Error("its service still runs")
	}
}
