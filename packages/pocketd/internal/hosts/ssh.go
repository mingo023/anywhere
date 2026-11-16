package hosts

import (
	"encoding/base64"
	"encoding/binary"
	"fmt"
	"strconv"
	"strings"
	"time"
	"unicode/utf16"
)

// maxSocketPath is what both ends of a forwarded unix socket must fit: sun_path on macOS, which ssh checks before connecting.
const maxSocketPath = 103

const (
	// tunnelCommand holds the session open after the forward is up, so READY on stdout means the socket is ready.
	tunnelCommand = "echo READY; exec cat"

	// installKey adds the public key on stdin to the host's authorized_keys unless it's there, on a line of its own
	// even when the file doesn't end in a newline. Wrapped in sh -c because the host's login shell may not be POSIX.
	installKey = `sh -c 'umask 077; mkdir -p ~/.ssh; chmod 700 ~/.ssh; touch ~/.ssh/authorized_keys; chmod 600 ~/.ssh/authorized_keys; key=$(cat); grep -qxF "$key" ~/.ssh/authorized_keys && exit; [ -z "$(tail -c1 ~/.ssh/authorized_keys)" ] || echo >> ~/.ssh/authorized_keys; printf "%s\n" "$key" >> ~/.ssh/authorized_keys'`
)

// Failure is why a connection failed, by what the user can do about it. Its value is its name on the wire.
type Failure string

const (
	// FailAuth is a wrong key, no key, a key with a passphrase and no agent, or a wrong user: ssh can't tell them apart.
	FailAuth        Failure = "auth"
	FailUnknownKey  Failure = "unknown_key"
	FailKeyChanged  Failure = "key_changed"
	FailNoSuchHost  Failure = "no_such_host"
	FailRefused     Failure = "refused"
	FailTimedOut    Failure = "timed_out"
	FailDropped     Failure = "dropped"
	FailNotRunning  Failure = "not_running"
	FailUnpaired    Failure = "unpaired"
	FailPathTooLong Failure = "path_too_long"
	FailOther       Failure = "other"
	// FailThrottled is sshd's penalty for failed logins, which also blocks good ones for about 20 seconds.
	FailThrottled Failure = "throttled"
)

// SSHError is ssh failing to reach or sign in to a host.
type SSHError struct{ Failure Failure }

func (e *SSHError) Error() string { return string(e.Failure) }

// retryAfter is how long to wait before trying again after attempt failures in a row; false waits for the user.
func (f Failure) retryAfter(attempt int) (time.Duration, bool) {
	backoff := func(capSecs int) time.Duration { return time.Duration(min(1<<min(attempt, 6), capSecs)) * time.Second }
	switch f {
	case FailAuth, FailUnknownKey, FailKeyChanged, FailUnpaired, FailPathTooLong:
		return 0, false
	case FailThrottled:
		return 20 * time.Second, true
	case FailNotRunning:
		return max(backoff(15), 2*time.Second), true
	case FailOther:
		return backoff(15), attempt < 3
	default:
		return backoff(15), true
	}
}

// Message is what went wrong on name, and what to do about it.
func (f Failure) Message(name string) string {
	switch f {
	case FailAuth:
		return fmt.Sprintf("%s didn't accept your SSH keys. Check you can run ssh %s in Terminal.", name, name)
	case FailUnknownKey:
		return fmt.Sprintf("%s's SSH key isn't trusted yet. Connect once with ssh %s in Terminal.", name, name)
	case FailKeyChanged:
		return fmt.Sprintf("%s's SSH key changed. Check with whoever runs it before continuing.", name)
	case FailNoSuchHost:
		return fmt.Sprintf("Can't find %s. Check the address, or your connection.", name)
	case FailRefused:
		return fmt.Sprintf("%s refused the connection. Check that SSH is on there.", name)
	case FailTimedOut:
		return fmt.Sprintf("Can't reach %s. Check it's on and on your network.", name)
	case FailDropped:
		return fmt.Sprintf("Lost the connection to %s.", name)
	case FailThrottled:
		return fmt.Sprintf("%s is limiting sign-ins after failed attempts. Trying again shortly.", name)
	case FailNotRunning:
		return fmt.Sprintf("Anywhere is stopped on %s.", name)
	case FailUnpaired:
		return fmt.Sprintf("Anywhere on %s didn't accept this Mac. Reconnect to pair again.", name)
	case FailPathTooLong:
		return fmt.Sprintf("The path to Anywhere on %s is too long to forward.", name)
	}
	return fmt.Sprintf("Couldn't connect to %s.", name)
}

var patterns = []struct {
	text    string
	failure Failure
}{
	{"REMOTE HOST IDENTIFICATION HAS CHANGED", FailKeyChanged},
	{"forwarding disabled due to host key", FailKeyChanged},
	{"has changed and you have requested strict checking", FailKeyChanged},
	{"Host key verification failed", FailUnknownKey},
	{"Permission denied (", FailAuth},
	{"Bad local forwarding specification", FailPathTooLong},
	{"too long for Unix domain socket", FailPathTooLong},
	{"Could not resolve hostname", FailNoSuchHost},
	{"Connection refused", FailRefused},
	{"timed out", FailTimedOut},
	{"not responding", FailDropped},
	{"closed by remote host", FailDropped},
	{"Connection closed by", FailThrottled},
}

// classify is what a failed ssh's stderr says went wrong, checked most specific first.
func classify(stderr string) Failure {
	for _, p := range patterns {
		if strings.Contains(stderr, p.text) {
			return p.failure
		}
	}
	return FailOther
}

// probeCommand prints the host's $HOME, then whether pocketd's socket is there; the forward needs an absolute path.
func probeCommand(homeDirName string) string {
	return fmt.Sprintf(`printf '%%s\n' "$HOME"; test -S "$HOME/%s/pocketd.sock" && echo running || echo stopped`, homeDirName)
}

// Probe is what the probe found: the host's home folder and whether pocketd runs there.
type Probe struct {
	Home    string
	Running bool
}

// lastLines is the last n non-empty lines of out, trimmed, oldest first; false when there are fewer.
func lastLines(out string, n int) ([]string, bool) {
	var lines []string
	for _, l := range strings.Split(out, "\n") {
		if l = strings.TrimSpace(l); l != "" {
			lines = append(lines, l)
		}
	}
	if len(lines) < n {
		return nil, false
	}
	return lines[len(lines)-n:], true
}

func parseProbe(stdout string) (Probe, bool) {
	lines, ok := lastLines(stdout, 2)
	if !ok || !strings.HasPrefix(lines[0], "/") {
		return Probe{}, false
	}
	return Probe{lines[0], lines[1] == "running"}, true
}

// args is ssh's arguments for command on host, never prompting, with extra options before the destination.
// App options come first because ssh keeps the first value it reads for an option.
// key is the app's own key for the host, or empty; with it, keys ssh wasn't named aren't offered,
// so the user's agent can't spend MaxAuthTries first.
func args(host Host, knownHosts, key string, extra []string, command string) []string {
	out := options(
		"BatchMode=yes",
		"ConnectTimeout=10",
		"ServerAliveInterval=5",
		"ServerAliveCountMax=3",
		"StrictHostKeyChecking=accept-new",
		// Overrides a QUIET or ERROR in the user's config, which would hide the lines classify reads.
		"LogLevel=INFO",
		// A user's `ControlMaster auto` would hand the forward to a background master and exit at once.
		"ControlMaster=no",
		"ControlPath=none",
	)
	out = append(out, knownHostsOption(knownHosts)...)
	if key != "" {
		out = append(out, "-i", key, "-o", "IdentitiesOnly=yes")
	}
	out = append(out, extra...)
	return append(out, destination(host, command)...)
}

type passwordMethod string

// One method per connection: after a wrong password, OpenSSH 9.6 on Ubuntu never answers a
// keyboard-interactive attempt in the same connection, so ssh hangs until the server's login grace ends.
const (
	methodPassword            passwordMethod = "PreferredAuthentications=password"
	methodKeyboardInteractive passwordMethod = "PreferredAuthentications=keyboard-interactive"
)

// passwordArgs is ssh's arguments for command on host, signing in by method with the password askpass gives it.
// Its own list rather than args', whose BatchMode=yes would win and refuse to ask.
func passwordArgs(host Host, knownHosts string, method passwordMethod, command string) []string {
	out := options(
		"BatchMode=no",
		"PubkeyAuthentication=no",
		string(method),
		// Each wrong password adds to sshd's penalty, which also locks out good keys.
		"NumberOfPasswordPrompts=1",
		// The password goes only to the host key the key sign-in recorded, never to a new one.
		"StrictHostKeyChecking=yes",
		"ConnectTimeout=10",
		"LogLevel=INFO",
		"ControlMaster=no",
		"ControlPath=none",
	)
	out = append(out, knownHostsOption(knownHosts)...)
	return append(out, destination(host, command)...)
}

// takesPasswords is whether a refused ssh's stderr says the host takes plain passwords,
// so keyboard-interactive is only worth a try when it doesn't.
func takesPasswords(stderr string) bool {
	_, rest, found := strings.Cut(stderr, "Permission denied (")
	if !found {
		return true
	}
	methods, _, _ := strings.Cut(rest, ")")
	for _, m := range strings.Split(methods, ",") {
		if m == "password" {
			return true
		}
	}
	return false
}

func options(opts ...string) []string {
	out := make([]string, 0, 2*len(opts))
	for _, o := range opts {
		out = append(out, "-o", o)
	}
	return out
}

// knownHostsOption puts the app's file first, so a newly accepted key lands there and ~/.ssh/known_hosts stays as it is.
func knownHostsOption(knownHosts string) []string {
	return []string{"-o", fmt.Sprintf(`UserKnownHostsFile="%s" ~/.ssh/known_hosts`, knownHosts)}
}

func destination(host Host, command string) []string {
	var out []string
	dest := host.Target.Name
	if host.Target.Kind == KindAddress {
		t := host.Target
		if t.Port != 22 {
			out = append(out, "-p", strconv.Itoa(t.Port))
		}
		if t.User != "" {
			out = append(out, "-l", t.User)
		}
		if t.KeyFile != "" {
			out = append(out, "-i", t.KeyFile, "-o", "IdentitiesOnly=yes")
		}
		dest = t.Addr
	}
	// A saved host is read back from disk, so one starting with - must not become an option.
	return append(out, "--", dest, command)
}

// powershell runs script in Windows PowerShell whatever the account's ssh shell is: cmd.exe by default, whose
// quoting it never meets, encoded as -EncodedCommand wants it.
func powershell(script string) string {
	var raw []byte
	for _, u := range utf16.Encode([]rune(script)) {
		raw = binary.LittleEndian.AppendUint16(raw, u)
	}
	return "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand " + base64.StdEncoding.EncodeToString(raw)
}

func psQuote(text string) string { return "'" + strings.ReplaceAll(text, "'", "''") + "'" }

// windowsProbeCommand prints the host's home, then whether the app's pocketd is installed under it, from PowerShell.
func windowsProbeCommand(homeDirName string) string {
	shim := psQuote(homeDirName + `\bin\pocketd.cmd`)
	return powershell(fmt.Sprintf("Write-Output $HOME; if (Test-Path (Join-Path $HOME %s)) { 'installed' } else { 'missing' }", shim))
}

// parseWindowsProbe is the Windows probe's answer; a home that isn't X:\… means this wasn't Windows.
func parseWindowsProbe(stdout string) (Probe, bool) {
	lines, ok := lastLines(stdout, 2)
	if !ok {
		return Probe{}, false
	}
	home := lines[0]
	if len(home) < 3 || home[0]|0x20 < 'a' || home[0]|0x20 > 'z' || home[1] != ':' || home[2] != '\\' {
		return Probe{}, false
	}
	return Probe{home, lines[1] == "installed"}, true
}

// windowsTunnelCommand is tunnelCommand for Windows, where nothing like cat holds stdin.
func windowsTunnelCommand() string {
	return powershell("[Console]::Out.WriteLine('READY'); [Console]::Out.Flush(); $null = [Console]::In.ReadToEnd()")
}

// tcpForward is the options that forward local to pocketd's owner TCP port on the host: Windows' sshd can't forward to a socket.
func tcpForward(local string, port int) ([]string, error) {
	if len(local) > maxSocketPath {
		return nil, &SSHError{FailPathTooLong}
	}
	return forwardOptions(fmt.Sprintf("%s:127.0.0.1:%d", local, port)), nil
}

// forward is the options that forward local to pocketd's socket under the host's home.
func forward(local, home, homeDirName string) ([]string, error) {
	remote := strings.TrimRight(home, "/") + "/" + homeDirName + "/pocketd.sock"
	if len(local) > maxSocketPath || len(remote) > maxSocketPath {
		return nil, &SSHError{FailPathTooLong}
	}
	return forwardOptions(local + ":" + remote), nil
}

func forwardOptions(spec string) []string {
	return []string{
		"-o", "ExitOnForwardFailure=yes",
		// ssh never removes its socket file, so the next ssh can't bind without this.
		"-o", "StreamLocalBindUnlink=yes",
		"-L", spec,
	}
}
