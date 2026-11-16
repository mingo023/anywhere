package hosts

import (
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

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
