package launchagent

import (
	"bytes"
	"flag"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

var update = flag.Bool("update", false, "rewrite testdata")

var spec = Spec{Exe: "/Users/me/bin/pocketd", LogPath: "/Users/me/.coding-pocket/logs/launchd.log"}

func TestPlistMatchesGolden(t *testing.T) {
	golden := filepath.Join("testdata", "pocketd.plist")
	if *update {
		os.WriteFile(golden, Plist(spec), 0o644)
	}
	want, err := os.ReadFile(golden)
	if err != nil {
		t.Fatal(err)
	}
	if got := Plist(spec); !bytes.Equal(got, want) {
		t.Fatalf("got\n%s", got)
	}
	if out, err := exec.Command("plutil", "-lint", golden).CombinedOutput(); err != nil {
		t.Fatalf("%v: %s", err, out)
	}
}

func TestPlistCarriesNoEnvironment(t *testing.T) {
	if bytes.Contains(Plist(spec), []byte("EnvironmentVariables")) {
		t.Fatal("the service must build its env from the login shell, not the plist")
	}
}

func TestPlistEscapesThePath(t *testing.T) {
	p := Plist(Spec{Exe: "/Users/a&b/<pocketd>", LogPath: "/l"})
	if !bytes.Contains(p, []byte("<string>/Users/a&amp;b/&lt;pocketd&gt;</string>")) {
		t.Fatalf("%s", p)
	}
}
