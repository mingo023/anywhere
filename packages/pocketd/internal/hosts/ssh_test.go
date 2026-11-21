package hosts

import (
	"encoding/base64"
	"encoding/binary"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"runtime"
	"slices"
	"strings"
	"testing"
	"time"
	"unicode/utf16"
)

func addressHost(port int, user, keyFile string) Host {
	return New(Target{Kind: KindAddress, Addr: "10.0.4.12", Port: port, User: user, KeyFile: keyFile}, nil)
}

func decoded(t *testing.T, command string) string {
	t.Helper()
	b64, ok := strings.CutPrefix(command, "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand ")
	if !ok {
		t.Fatalf("not an encoded command: %s", command)
	}
	raw, err := base64.StdEncoding.DecodeString(b64)
	if err != nil {
		t.Fatal(err)
	}
	units := make([]uint16, len(raw)/2)
	for i := range units {
		units[i] = binary.LittleEndian.Uint16(raw[2*i:])
	}
	return string(utf16.Decode(units))
}

func pathTooLong(err error) bool {
	var e *SSHError
	return errors.As(err, &e) && e.Failure == FailPathTooLong
}

func TestSshNeverPromptsAndKeepsNewHostKeysOutOfTheUsersFile(t *testing.T) {
	got := args(addressHost(22, "", ""), "/h/known_hosts", "", nil, "true")
	for _, opt := range []string{"BatchMode=yes", "StrictHostKeyChecking=accept-new", "ControlMaster=no", `UserKnownHostsFile="/h/known_hosts" ~/.ssh/known_hosts`} {
		if !slices.Contains(got, opt) {
			t.Errorf("no %s in %v", opt, got)
		}
	}
	if !slices.Equal(got[len(got)-2:], []string{"10.0.4.12", "true"}) {
		t.Fatalf("%v", got)
	}
}

func TestAnAddressPassesOnlyThePortUserAndKeyItWasGiven(t *testing.T) {
	tail := func(h Host) []string {
		got := args(h, "/k", "", nil, "true")
		return got[slices.IndexFunc(got, func(a string) bool { return strings.HasPrefix(a, "UserKnownHostsFile") })+1:]
	}
	if got := tail(addressHost(22, "", "")); !slices.Equal(got, []string{"--", "10.0.4.12", "true"}) {
		t.Errorf("%v", got)
	}
	want := []string{"-p", "2222", "-l", "dev", "-i", "/k/id", "-o", "IdentitiesOnly=yes", "--", "10.0.4.12", "true"}
	if got := tail(addressHost(2222, "dev", "/k/id")); !slices.Equal(got, want) {
		t.Errorf("%v", got)
	}
	if got := tail(New(alias("-oProxyCommand=x"), nil)); !slices.Equal(got, []string{"--", "-oProxyCommand=x", "true"}) {
		t.Errorf("a saved name became an option: %v", got)
	}
}

func TestTheAppsOwnKeyGoesFirstAndTheUsersAgentKeysNotAtAll(t *testing.T) {
	got := args(addressHost(22, "", "/k/id"), "/k", "/h/devbox.key", nil, "true")
	appKey, hostKey := slices.Index(got, "/h/devbox.key"), slices.Index(got, "/k/id")
	if appKey < 0 || hostKey < 0 || appKey > hostKey {
		t.Errorf("the host's key file comes first: %v", got)
	}
	if noKeyFile := args(addressHost(22, "", ""), "/k", "/h/devbox.key", nil, "true"); !slices.Contains(noKeyFile, "IdentitiesOnly=yes") {
		t.Errorf("the agent's keys are offered: %v", noKeyFile)
	}
}

func TestInstallingAKeyAddsItOnceOnItsOwnLine(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("needs sh")
	}
	home := t.TempDir()
	authorized := filepath.Join(home, ".ssh", "authorized_keys")
	os.MkdirAll(filepath.Dir(authorized), 0o700)
	os.WriteFile(authorized, []byte("ssh-ed25519 AAAAold me"), 0o600)
	for range 2 {
		sh := exec.Command("sh", "-c", installKey)
		sh.Env = append(os.Environ(), "HOME="+home)
		sh.Stdin = strings.NewReader("ssh-ed25519 AAAAnew anywhere\n")
		if out, err := sh.CombinedOutput(); err != nil {
			t.Fatalf("%v: %s", err, out)
		}
	}
	got, _ := os.ReadFile(authorized)
	if string(got) != "ssh-ed25519 AAAAold me\nssh-ed25519 AAAAnew anywhere\n" {
		t.Fatalf("%q", got)
	}
}

func TestAPasswordSignInAsksOnceAndOnlyAHostAlreadyTrusted(t *testing.T) {
	got := passwordArgs(addressHost(2222, "dev", "/k/id"), "/h/known_hosts", methodPassword, installKey)
	for _, opt := range []string{"BatchMode=no", "PubkeyAuthentication=no", "NumberOfPasswordPrompts=1", "PreferredAuthentications=password", "StrictHostKeyChecking=yes", `UserKnownHostsFile="/h/known_hosts" ~/.ssh/known_hosts`} {
		if !slices.Contains(got, opt) {
			t.Errorf("no %s in %v", opt, got)
		}
	}
	for _, opt := range []string{"StrictHostKeyChecking=accept-new", "BatchMode=yes", "PreferredAuthentications=keyboard-interactive"} {
		if slices.Contains(got, opt) {
			t.Errorf("%s in %v", opt, got)
		}
	}
	if !slices.Equal(got[len(got)-3:], []string{"--", "10.0.4.12", installKey}) {
		t.Errorf("%v", got[len(got)-3:])
	}
}

func TestKeyboardInteractiveIsTriedOnlyOnAHostWithoutPlainPasswords(t *testing.T) {
	if !takesPasswords("dev@10.0.4.12: Permission denied (publickey,password,keyboard-interactive).") {
		t.Error("a host offering passwords")
	}
	if takesPasswords("dev@10.0.4.12: Permission denied (publickey,keyboard-interactive).") {
		t.Error("a host not offering passwords")
	}
	if !takesPasswords("ssh: connect to host 10.0.4.12 port 22: Connection refused") {
		t.Error("a failure before sign-in isn't about the method")
	}
}

func TestTheForwardGoesToPocketdsSocketUnderTheHostsAbsoluteHome(t *testing.T) {
	opts, err := forward("/l/devbox.sock", "/home/dev/", ".coding-pocket")
	if err != nil || opts[len(opts)-1] != "/l/devbox.sock:/home/dev/.coding-pocket/pocketd.sock" {
		t.Fatalf("%v %v", opts, err)
	}
	if !slices.Contains(opts, "StreamLocalBindUnlink=yes") {
		t.Errorf("%v", opts)
	}
}

func TestAHomeTooLongForASocketPathFailsBeforeSshRuns(t *testing.T) {
	_, err := forward("/l/a.sock", "/home/"+strings.Repeat("x", 90), ".coding-pocket")
	if !pathTooLong(err) {
		t.Fatalf("%v", err)
	}
}

func TestTheProbeReadsHomeAndWhetherPocketdRunsPastAnyLoginNoise(t *testing.T) {
	for out, want := range map[string]Probe{
		"Welcome!\n/home/dev\nrunning\n": {"/home/dev", ".cp", true},
		"/Users/m\nstopped\n":            {"/Users/m", ".cp", false},
	} {
		if got, ok := parseProbe(out, []string{".cp"}); !ok || got != want {
			t.Errorf("%q: %+v %v", out, got, ok)
		}
	}
	for _, out := range []string{"stopped\n", "Welcome\nstopped\n"} {
		if got, ok := parseProbe(out, []string{".cp"}); ok {
			t.Errorf("%q: %+v", out, got)
		}
	}
}

func TestTheProbeFindsPocketdInTheLegacyFolderOnAHostThatHasntMovedIt(t *testing.T) {
	dirs := []string{".anywhere", ".coding-pocket"}
	for out, want := range map[string]Probe{
		"/home/dev\nstopped\nrunning\n": {"/home/dev", ".coding-pocket", true},
		"/home/dev\nrunning\nrunning\n": {"/home/dev", ".anywhere", true},
		"/home/dev\nstopped\nstopped\n": {"/home/dev", ".anywhere", false},
	} {
		if got, ok := parseProbe(out, dirs); !ok || got != want {
			t.Errorf("%q: %+v %v", out, got, ok)
		}
	}
	want := `printf '%s\n' "$HOME"; test -S "$HOME/.anywhere/pocketd.sock" && echo running || echo stopped; test -S "$HOME/.coding-pocket/pocketd.sock" && echo running || echo stopped`
	if got := probeCommand(dirs); got != want {
		t.Errorf("%s", got)
	}
}

func TestPowershellGetsTheScriptAsUTF16SoCmdsQuotingNeverTouchesIt(t *testing.T) {
	command := windowsProbeCommand([]string{".it's"})
	for _, b := range []byte(command) {
		if !(b >= 'a' && b <= 'z' || b >= 'A' && b <= 'Z' || b >= '0' && b <= '9' || strings.IndexByte(" -+/=", b) >= 0) {
			t.Fatalf("%q in %s", b, command)
		}
	}
	want := `Write-Output $HOME; if (Test-Path (Join-Path $HOME '.it''s\bin\pocketd.cmd')) { 'installed' } else { 'missing' }`
	if got := decoded(t, command); got != want {
		t.Errorf("%s", got)
	}
	if !strings.Contains(decoded(t, windowsTunnelCommand()), "ReadToEnd()") {
		t.Error("the tunnel doesn't hold stdin")
	}
}

func TestTheWindowsProbeWantsADriveHome(t *testing.T) {
	for out, want := range map[string]Probe{
		"Welcome\r\nC:\\Users\\me\r\ninstalled\r\n": {`C:\Users\me`, ".cp", true},
		"C:\\Users\\me\nmissing\n":                  {`C:\Users\me`, ".cp", false},
	} {
		if got, ok := parseWindowsProbe(out, []string{".cp"}); !ok || got != want {
			t.Errorf("%q: %+v %v", out, got, ok)
		}
	}
	legacy := Probe{`C:\Users\me`, ".old", true}
	if got, ok := parseWindowsProbe("C:\\Users\\me\nmissing\ninstalled\n", []string{".cp", ".old"}); !ok || got != legacy {
		t.Errorf("%+v %v", got, ok)
	}
	for _, out := range []string{"/home/dev\nmissing\n", "C:/Users/me\nmissing\n", "'powershell' is not recognized\n"} {
		if got, ok := parseWindowsProbe(out, []string{".cp"}); ok {
			t.Errorf("%q: %+v", out, got)
		}
	}
}

func TestAWindowsForwardGoesToTheOwnerPortOnTheHostsLoopback(t *testing.T) {
	opts, err := tcpForward("/l/pc.sock", 4519)
	want := []string{"-o", "ExitOnForwardFailure=yes", "-o", "StreamLocalBindUnlink=yes", "-L", "/l/pc.sock:127.0.0.1:4519"}
	if err != nil || !slices.Equal(opts, want) {
		t.Fatalf("%v %v", opts, err)
	}
	if _, err := tcpForward("/"+strings.Repeat("x", 110), 4519); !pathTooLong(err) {
		t.Fatalf("%v", err)
	}
}

func TestStderrTellsWhatWentWrong(t *testing.T) {
	for _, c := range []struct {
		stderr string
		want   Failure
	}{
		{"dev@10.0.4.12: Permission denied (publickey,password,keyboard-interactive).", FailAuth},
		{"No ED25519 host key is known for [127.0.0.1]:2230 and you have requested strict checking.\nHost key verification failed.", FailUnknownKey},
		{"@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @\nHost key verification failed.", FailKeyChanged},
		{"Host key for [127.0.0.1]:2250 has changed and you have requested strict checking.\nHost key verification failed.", FailKeyChanged},
		{"ssh: Could not resolve hostname nosuch.invalid: nodename nor servname provided, or not known", FailNoSuchHost},
		{"ssh: connect to host 127.0.0.1 port 2230: Connection refused", FailRefused},
		{"ssh: connect to host 10.255.255.1 port 22: Operation timed out", FailTimedOut},
		{"Connection timed out during banner exchange", FailTimedOut},
		{"ssh: connect to host 192.168.1.20 port 22: Network is unreachable", FailTimedOut},
		{"ssh: connect to host 192.168.1.20 port 22: No route to host", FailTimedOut},
		{"client_loop: send disconnect: Broken pipe", FailDropped},
		{"read: Connection reset by peer", FailDropped},
		{"Timeout, server 127.0.0.1 not responding.", FailDropped},
		{"Connection to 127.0.0.1 closed by remote host.", FailDropped},
		{"Connection closed by 127.0.0.1 port 2230", FailThrottled},
		{"Bad local forwarding specification '/l:/r'", FailPathTooLong},
		{"", FailOther},
	} {
		if got := classify(c.stderr); got != c.want {
			t.Errorf("%q is %s, want %s", c.stderr, got, c.want)
		}
	}
}

func TestARefusedLoginWaitsForTheUserAndALostLinkBacksOff(t *testing.T) {
	if _, retry := FailAuth.retryAfter(0); retry {
		t.Error("a refused login retries")
	}
	if _, retry := FailKeyChanged.retryAfter(3); retry {
		t.Error("a changed key retries")
	}
	if wait, retry := FailThrottled.retryAfter(0); !retry || wait != 20*time.Second {
		t.Errorf("%v %v", wait, retry)
	}
	var waits []time.Duration
	for n := range 7 {
		wait, retry := FailDropped.retryAfter(n)
		if !retry {
			t.Fatalf("attempt %d", n)
		}
		waits = append(waits, wait/time.Second)
	}
	if !reflect.DeepEqual(waits, []time.Duration{1, 2, 4, 8, 15, 15, 15}) {
		t.Errorf("%v", waits)
	}
	if wait, retry := FailNotRunning.retryAfter(0); !retry || wait != 2*time.Second {
		t.Errorf("%v %v", wait, retry)
	}
}

func TestAnUnknownSshFailureGetsThreeTries(t *testing.T) {
	var tries []bool
	for n := range 4 {
		_, retry := FailOther.retryAfter(n)
		tries = append(tries, retry)
	}
	if !slices.Equal(tries, []bool{true, true, true, false}) {
		t.Fatalf("%v", tries)
	}
}

func TestEveryFailureTellsTheUserWhichHostItIsAbout(t *testing.T) {
	for _, f := range []Failure{FailAuth, FailUnknownKey, FailKeyChanged, FailNoSuchHost, FailRefused, FailTimedOut, FailDropped, FailThrottled, FailNotRunning, FailUnpaired, FailPathTooLong, FailOther} {
		if !strings.Contains(f.Message("devbox"), "devbox") {
			t.Errorf("%s: %s", f, f.Message("devbox"))
		}
	}
}
