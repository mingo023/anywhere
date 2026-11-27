package hosts

import (
	"bufio"
	"context"
	_ "embed"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"unicode"

	"pocketd/internal/config"
	"pocketd/internal/ops"
)

//go:embed bootstrap.sh
var bootstrapSh string

//go:embed bootstrap.ps1
var bootstrapPs1 string

const (
	releases  = "https://github.com/mingo023/zoes/releases/download"
	upload    = "upload.tar.gz"
	uploadZip = "upload.zip"
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

type Platform struct{ OS, Arch string }

func (p Platform) windows() bool { return p.OS == "windows" }

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

type Source struct {
	Archive string
	Version string
	URL     string
}

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

// label names one service per app folder, so a test install never takes over the real one.
func label(remoteDir string) string {
	if remoteDir == config.HomeName() || slices.Contains(config.LegacyHomeNames(), remoteDir) {
		return config.HostLabel()
	}
	return config.HostLabel() + "." + strings.TrimLeft(remoteDir, ".")
}

// fill refuses values holding anything but safe characters rather than escape them: the placeholders sit in quoted literals.
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

// installPlan is the host to install on once its platform is known, and where its pocketd comes from.
func installPlan(h Host, platform Platform, version, dist string) (Host, Source, error) {
	src, err := source(platform, version, dist)
	if err != nil {
		return Host{}, Source{}, err
	}
	h.Windows = platform.windows()
	return h, src, nil
}

func (m *Manager) install(ctx context.Context, h Host, update bool, dist string, on func(Progress)) error {
	platform, err := m.platform(ctx, h)
	if err != nil {
		return err
	}
	h, src, err := installPlan(h, platform, m.version, dist)
	if err != nil {
		return err
	}
	m.setWindows(h.ID, h.Windows)
	return m.bootstrap(ctx, h, src, update, on)
}

// platform is what the host runs. Windows' shells have no uname, so a failed one asks PowerShell.
func (m *Manager) platform(ctx context.Context, h Host) (Platform, error) {
	stdout, stderr, code, err := m.run(ctx, h, "uname -s; uname -m", nil)
	if err != nil {
		return Platform{}, err
	}
	if code == 0 {
		return parseUname(string(stdout))
	}
	ps, _, _, err := m.run(ctx, h, windowsArchCommand(), nil)
	if err != nil {
		return Platform{}, err
	}
	if p, found, err := parseWindowsArch(string(ps)); found {
		return p, err
	}
	return Platform{}, fmt.Errorf("Couldn't tell what %s runs: %s", h.Name, lastLine(stderr))
}

// bootstrap installs src on h and starts it, or finds pocketd running and leaves it be. With force it installs over
// a running pocketd, which hands off to it in place; on Windows it restarts instead.
func (m *Manager) bootstrap(ctx context.Context, h Host, src Source, force bool, on func(Progress)) error {
	dir, err := m.remoteDir(ctx, h, h.Windows)
	if err != nil {
		return err
	}
	version, url, sum := src.Version, src.URL, ""
	if src.Archive != "" {
		var ok bool
		if sum, ok = checksum(src.Archive); !ok {
			return fmt.Errorf("No checksum beside %s", src.Archive)
		}
		version = m.version
	}
	script := bootstrapSh
	if h.Windows {
		script = bootstrapPs1
	}
	script, err = fill(script, version, url, dir, label(dir))
	if err != nil {
		return err
	}
	uploaded := src.Archive != ""
	if uploaded {
		if err := m.upload(ctx, h, src.Archive, dir); err != nil {
			return err
		}
	}
	var env []string
	var command string
	if h.Windows {
		if uploaded {
			env = append(env, "$env:ANYWHERE_ARCHIVE = Join-Path $HOME "+psQuote(dir+`\`+uploadZip))
		}
		if sum != "" {
			env = append(env, "$env:ANYWHERE_SHA256 = '"+sum+"'")
		}
		if force {
			env = append(env, "$env:ANYWHERE_FORCE = '1'")
		}
		for i := len(env) - 1; i >= 0; i-- {
			script = env[i] + "\r\n" + script
		}
		// -Command - runs stdin line by line, which breaks the script's multi-line blocks.
		command = powershell("& ([ScriptBlock]::Create([Console]::In.ReadToEnd()))")
	} else {
		if uploaded {
			env = append(env, fmt.Sprintf(`ANYWHERE_ARCHIVE="$HOME/%s/%s"`, dir, upload))
		}
		if sum != "" {
			env = append(env, "ANYWHERE_SHA256="+sum)
		}
		if force {
			env = append(env, "ANYWHERE_FORCE=1")
		}
		command = bootstrapCommand(dir, env, uploaded)
	}
	cmd, err := m.command(ctx, m.sshArgs(h, nil, command))
	if err != nil {
		return fmt.Errorf("Couldn't run ssh: %w", err)
	}
	cmd.Stdin = strings.NewReader(script)
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return err
	}
	var stderr strings.Builder
	cmd.Stderr = &stderr
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("Couldn't run ssh: %w", err)
	}
	var last Progress
	lines := bufio.NewScanner(stdout)
	for lines.Scan() {
		if p, ok := parseProgress(lines.Text()); ok {
			if p.Kind == ProgressDone || p.Kind == ProgressRunning || p.Kind == ProgressError {
				last = p
			}
			on(p)
		}
	}
	cmd.Wait()
	switch {
	case last.Kind == ProgressDone || last.Kind == ProgressRunning:
		return nil
	case last.Kind == ProgressError:
		return errors.New(last.Text)
	case cmd.ProcessState.ExitCode() == 255:
		return &SSHError{classify(stderr.String())}
	}
	return fmt.Errorf("The install stopped before it finished: %s", lastLine([]byte(stderr.String())))
}

func (m *Manager) upload(ctx context.Context, h Host, archive, dir string) error {
	data, err := os.ReadFile(archive)
	if err != nil {
		return fmt.Errorf("Can't read %s: %v", archive, err)
	}
	command := fmt.Sprintf(`sh -c 'umask 077; mkdir -p "$HOME/%s" && cat > "$HOME/%s/%s"'`, dir, dir, upload)
	if h.Windows {
		command = powershell(fmt.Sprintf("$d = Join-Path $HOME %s; New-Item -ItemType Directory -Force -Path $d | Out-Null; $f = [IO.File]::Create((Join-Path $d '%s')); try { [Console]::OpenStandardInput().CopyTo($f) } finally { $f.Close() }", psQuote(dir), uploadZip))
	}
	_, stderr, code, err := m.run(ctx, h, command, data)
	if err != nil {
		return err
	}
	if code != 0 {
		return fmt.Errorf("Couldn't upload pocketd: %s", lastLine(stderr))
	}
	return nil
}

// uninstall stops pocketd's service on the host and removes it; pocketd's files and the host's projects stay.
// A Windows host's pocketd first forgets this machine's owner token, through the link that's still up.
func (m *Manager) uninstall(ctx context.Context, h Host) error {
	// A host never reached since it was added may be Windows without this machine knowing yet.
	windows := h.Windows
	if !windows {
		platform, err := m.platform(ctx, h)
		if err != nil {
			return err
		}
		windows = platform.windows()
	}
	dir, err := m.remoteDir(ctx, h, windows)
	if err != nil {
		return err
	}
	command := fmt.Sprintf(`sh -c 'p="$HOME/%s/bin/pocketd"; [ -x "$p" ] || exit 0; POCKET_HOME="$HOME/%s" "$p" service uninstall --label %s'`, dir, dir, label(dir))
	if windows {
		owner, _ := loadOwner(m.paths.Token(h.ID))
		request(m.paths.Socket(h.ID), owner.Token, ops.Msg{Op: "owner.revokeSelf"}, replyWithin)
		command = powershell(fmt.Sprintf("$p = Join-Path $HOME %s; if (Test-Path $p) { & $p service uninstall --label %s; exit $LASTEXITCODE }", psQuote(dir+`\bin\pocketd.cmd`), psQuote(label(dir))))
	}
	_, stderr, code, err := m.run(ctx, h, command, nil)
	if err != nil {
		return err
	}
	if code != 0 {
		return fmt.Errorf("Couldn't uninstall pocketd: %s", lastLine(stderr))
	}
	if windows {
		os.Remove(m.paths.Token(h.ID))
	}
	return nil
}

// remoteDir is the folder pocketd lives in on h, as config.DefaultHome finds it there: the oldest legacy one that
// leads neither into RemoteDir nor, as the link pocketd leaves once it moved its home, into a newer one; else RemoteDir.
func (m *Manager) remoteDir(ctx context.Context, h Host, windows bool) (string, error) {
	legacy, current := m.paths.LegacyDirs, m.paths.RemoteDir
	if len(legacy) == 0 {
		return current, nil
	}
	command := fmt.Sprintf(`sh -c 'real() { (cd "$HOME/$1" 2>/dev/null && pwd -P); }; cur=$(real %[2]s); set -- %[1]s; `+
		`while [ $# -gt 0 ]; do d=$1; shift; r=$(real "$d") || continue; [ "$r" = "$cur" ] && continue; `+
		`if [ -L "$HOME/$d" ]; then for n; do [ "$r" = "$(real "$n")" ] && continue 2; done; fi; echo "$d"; exit; done; echo %[2]s'`,
		strings.Join(legacy, " "), current)
	if windows {
		// pocketd never moves its home on Windows, so a legacy folder there is still the home.
		script := "$d = " + psQuote(current)
		for _, l := range slices.Backward(legacy) {
			script += fmt.Sprintf("; if (Test-Path (Join-Path $HOME %s)) { $d = %[1]s }", psQuote(l))
		}
		command = powershell(script + "; $d")
	}
	stdout, stderr, _, err := m.run(ctx, h, command, nil)
	if err != nil {
		return "", err
	}
	if dir := lastLine(stdout); dir == current || slices.Contains(legacy, dir) {
		return dir, nil
	}
	return "", fmt.Errorf("Couldn't tell where pocketd lives on %s: %s", h.Name, lastLine(stderr))
}

func lastLine(text []byte) string {
	if lines, ok := lastLines(string(text), 1); ok {
		return lines[0]
	}
	return "no output"
}
