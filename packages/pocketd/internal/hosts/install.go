package hosts

import (
	_ "embed"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"unicode"

	"pocketd/internal/config"
)

//go:embed bootstrap.sh
var bootstrapSh string

//go:embed bootstrap.ps1
var bootstrapPs1 string

const (
	releases = "https://github.com/mingo023/anywhere/releases/download"
	// upload is where an uploaded archive waits, in the app's folder on the host.
	upload = "upload.tar.gz"
)

// Step is a line of the install checklist, in the order bootstrap.sh and bootstrap.ps1 report them.
type Step string

const (
	StepDetect   Step = "detect"
	StepDownload Step = "download"
	StepVerify   Step = "verify"
	StepUnpack   Step = "unpack"
	StepLink     Step = "link"
	StepService  Step = "service"
	StepStart    Step = "start"
)

var steps = []Step{StepDetect, StepDownload, StepVerify, StepUnpack, StepLink, StepService, StepStart}

type ProgressKind string

const (
	ProgressStep ProgressKind = "step"
	ProgressWarn ProgressKind = "warn"
	// ProgressRunning is a pocketd already running there, reused as it is.
	ProgressRunning ProgressKind = "running"
	ProgressDone    ProgressKind = "done"
	ProgressError   ProgressKind = "error"
)

// Progress is what an install reports; Step is set for a step, Text for a warn or an error.
type Progress struct {
	Kind ProgressKind
	Step Step
	Text string
}

// parseProgress is what an `ANYWHERE …` line from bootstrap.sh reports; false for its other output.
func parseProgress(line string) (Progress, bool) {
	rest, ok := strings.CutPrefix(strings.TrimRightFunc(line, unicode.IsSpace), "ANYWHERE ")
	if !ok {
		return Progress{}, false
	}
	word, text, _ := strings.Cut(rest, " ")
	switch {
	case word == "step":
		for _, s := range steps {
			if strings.EqualFold(string(s), text) {
				return Progress{Kind: ProgressStep, Step: s}, true
			}
		}
	case word == "warn":
		return Progress{Kind: ProgressWarn, Text: text}, true
	case word == "error":
		return Progress{Kind: ProgressError, Text: text}, true
	case word == "running" && text == "":
		return Progress{Kind: ProgressRunning}, true
	case word == "done" && text == "":
		return Progress{Kind: ProgressDone}, true
	}
	return Progress{}, false
}

// Platform is what a host runs, named as pocketd's archives name it.
type Platform struct{ OS, Arch string }

func (p Platform) windows() bool { return p.OS == "windows" }

// parseUname is the platform `uname -s` then `uname -m` printed, past any login noise.
func parseUname(stdout string) (Platform, error) {
	lines, ok := lastLines(stdout, 2)
	if !ok {
		return Platform{}, errors.New("uname printed nothing")
	}
	system, machine := lines[0], lines[1]
	var goos string
	switch {
	case strings.HasPrefix(system, "MINGW"), strings.HasPrefix(system, "MSYS"), strings.HasPrefix(system, "CYGWIN"):
		goos = "windows"
	case system == "Darwin":
		goos = "darwin"
	case system == "Linux":
		goos = "linux"
	default:
		return Platform{}, fmt.Errorf("Anywhere doesn't run on %s", system)
	}
	arch, ok := archOf(machine)
	if !ok {
		return Platform{}, fmt.Errorf("Anywhere doesn't run on %s", machine)
	}
	return Platform{goos, arch}, nil
}

func archOf(machine string) (string, bool) {
	switch machine {
	case "arm64", "aarch64":
		return "arm64", true
	case "x86_64", "amd64":
		return "amd64", true
	}
	return "", false
}

// windowsArchCommand prints `windows <CPU>` from PowerShell; a 32-bit shell on a 64-bit CPU names the real one in the W6432 variable.
func windowsArchCommand() string {
	return powershell(`$cpu = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }; Write-Output "windows $cpu"`)
}

// parseWindowsArch is what windowsArchCommand printed; found is false when it wasn't Windows that ran it.
func parseWindowsArch(stdout string) (p Platform, found bool, err error) {
	lines := strings.Split(stdout, "\n")
	for i := len(lines) - 1; i >= 0; i-- {
		cpu, ok := strings.CutPrefix(strings.TrimSpace(lines[i]), "windows ")
		if !ok {
			continue
		}
		switch cpu {
		case "AMD64":
			return Platform{"windows", "amd64"}, true, nil
		case "ARM64":
			return Platform{"windows", "arm64"}, true, nil
		}
		return Platform{}, true, fmt.Errorf("Anywhere doesn't run on %s", cpu)
	}
	return Platform{}, false, nil
}

// Source is where pocketd comes from: an archive on this Mac that's uploaded first, or the release's download folder.
type Source struct {
	Archive string
	Version string
	URL     string
}

// source is the source for platform: an archive in dist, when there is one, else the release version.
// A dev build has no release, so it needs the archive.
func source(platform Platform, version, dist string) (Source, error) {
	name, build := fmt.Sprintf("pocketd-%s-%s.tar.gz", platform.OS, platform.Arch), fmt.Sprintf("package-pocketd.sh dev %s-%s", platform.OS, platform.Arch)
	if platform.windows() {
		name, build = fmt.Sprintf("pocketd-windows-%s.zip", platform.Arch), "package-pocketd-windows.sh dev "+platform.Arch
	}
	if dist != "" {
		if st, err := os.Stat(filepath.Join(dist, name)); err == nil && st.Mode().IsRegular() {
			return Source{Archive: filepath.Join(dist, name)}, nil
		}
	}
	if version == "dev" {
		dir := dist
		if dir == "" {
			dir = "packages/pocketd/dist"
		}
		return Source{}, fmt.Errorf("No %s in %s. Build it with scripts/%s", name, dir, build)
	}
	return Source{Version: version, URL: releases + "/v" + version}, nil
}

// checksum is the sha256 package-pocketd.sh wrote beside archive.
func checksum(archive string) (string, bool) {
	text, err := os.ReadFile(archive + ".sha256")
	if err != nil {
		return "", false
	}
	sum := strings.Fields(string(text))
	if len(sum) == 0 || len(sum[0]) != 64 || strings.Trim(sum[0], "0123456789abcdefABCDEF") != "" {
		return "", false
	}
	return sum[0], true
}

// bootstrapCommand runs bootstrap.sh from stdin with env, under sh whatever the login shell is.
func bootstrapCommand(dir string, env []string, uploaded bool) string {
	inner := ""
	for _, e := range env {
		inner += e + " "
	}
	inner += "sh -s"
	if uploaded {
		inner += fmt.Sprintf(`; rc=$?; rm -f "$HOME/%s/%s"; exit $rc`, dir, upload)
	}
	return "sh -c '" + inner + "'"
}

// label is the service's name for the app's folder remoteDir. One per folder, so a test install never
// takes over the real one.
func label(remoteDir string) string {
	if remoteDir == config.HomeName() {
		return config.HostLabel()
	}
	return config.HostLabel() + "." + strings.TrimLeft(remoteDir, ".")
}

// fill is script with its placeholders filled. They sit inside quoted shell and PowerShell literals, so a value
// holding anything but these characters is refused rather than escaped.
func fill(script, version, url, dir, label string) (string, error) {
	safe := func(v string) bool {
		return strings.IndexFunc(v, func(c rune) bool {
			return !(c < unicode.MaxASCII && (unicode.IsLetter(c) || unicode.IsDigit(c) || strings.ContainsRune("._-+/:", c)))
		}) < 0
	}
	for _, v := range []string{version, url, dir, label} {
		if !safe(v) {
			return "", fmt.Errorf("Can't install with %q: it has characters a script can't quote", v)
		}
	}
	return strings.NewReplacer(
		"@@VERSION@@", version,
		"@@URL@@", url,
		"@@HOME_DIR@@", dir,
		"@@LABEL@@", label,
		"@@PORT@@", strconv.Itoa(config.DefaultOwnerPort()),
	).Replace(script), nil
}
