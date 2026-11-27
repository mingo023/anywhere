package hosts

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

// signInThrough signs in to devbox through a stand-in ssh: a sh case statement over the command it was given, in $last.
func signInThrough(t *testing.T, cases string) Failure {
	t.Helper()
	if runtime.GOOS == "windows" {
		t.Skip("the stand-in ssh is a shell script")
	}
	bin := t.TempDir()
	script := "#!/bin/sh\nfor last; do :; done\ncase \"$last\" in\n" + cases + "esac\nexit 255\n"
	if err := os.WriteFile(filepath.Join(bin, "ssh"), []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	m, err := openManager(t, Paths{Home: t.TempDir(), RemoteDir: ".cp"}, func() []string { return []string{"PATH=" + bin} }, "1.2.0", "Mac")
	if err != nil {
		t.Fatal(err)
	}
	devbox, err := m.Add(alias("devbox"), "")
	if err != nil {
		t.Fatal(err)
	}
	key := m.paths.Key(devbox.ID)
	os.MkdirAll(filepath.Dir(key), 0o700)
	os.WriteFile(key, []byte("private"), 0o600)
	os.WriteFile(key+".pub", []byte("ssh-ed25519 AAAAnew anywhere\n"), 0o600)
	return m.signIn(context.Background(), devbox.Host, "hunter2")
}

func TestAWindowsHostGetsTheKeyThroughPowerShellOnceThePasswordWorks(t *testing.T) {
	ran := filepath.Join(t.TempDir(), "ran")
	// As Windows' sshd under cmd.exe: the password works, sh isn't there, powershell is.
	f := signInThrough(t, `"sh -c"*) echo "'sh' is not recognized as an internal or external command," >&2; exit 1 ;;
powershell*) echo powershell >> '`+ran+`'; exit 0 ;;
`)
	if f != "" {
		t.Fatalf("sign-in failed: %s", f)
	}
	if got, _ := os.ReadFile(ran); string(got) != "powershell\n" {
		t.Fatalf("powershell ran %q", got)
	}
}

func TestAPosixHostWhoseKeyInstallFailsKeepsItsOwnError(t *testing.T) {
	// The password works, the install hits a dropped connection, and there is no powershell.
	f := signInThrough(t, `"sh -c"*) echo "Connection reset by peer" >&2; exit 1 ;;
powershell*) echo "sh: powershell: not found" >&2; exit 127 ;;
`)
	if f != FailDropped {
		t.Fatalf("got %q, want %q", f, FailDropped)
	}
}
