package service

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

var spec = Spec{Exe: "/Users/me/.coding-pocket/runtime/1.2.0/pocketd", Home: "/Users/me/.coding-pocket", Label: "dev.mingo.anywhere.host.pocketd"}

func TestPlistRunsServeWithTheHomeAndLogsUnderIt(t *testing.T) {
	got := string(plist(spec))
	for _, want := range []string{
		"<string>dev.mingo.anywhere.host.pocketd</string>",
		"<string>/Users/me/.coding-pocket/runtime/1.2.0/pocketd</string>\n\t\t<string>serve</string>",
		"<key>POCKET_HOME</key>\n\t\t<string>/Users/me/.coding-pocket</string>",
		"<string>/Users/me/.coding-pocket/logs/launchd.log</string>",
		"<key>SuccessfulExit</key>\n\t\t<false/>",
	} {
		if !strings.Contains(got, want) {
			t.Fatalf("missing %q in\n%s", want, got)
		}
	}
}

func TestPlistEscapesPathsAndLints(t *testing.T) {
	s := Spec{Exe: "/Users/a&b/<x>/pocketd", Home: "/Users/a&b/.coding-pocket", Label: "l"}
	got := string(plist(s))
	if strings.Contains(got, "a&b") || !strings.Contains(got, "/Users/a&amp;b/&lt;x&gt;/pocketd") {
		t.Fatalf("not escaped:\n%s", got)
	}
	if _, err := exec.LookPath("plutil"); err != nil {
		return
	}
	path := filepath.Join(t.TempDir(), "p.plist")
	os.WriteFile(path, []byte(got), 0o644)
	if out, err := exec.Command("plutil", "-lint", path).CombinedOutput(); err != nil {
		t.Fatalf("%v: %s", err, out)
	}
}

func TestUnitRestartsServeForeverWithTheHome(t *testing.T) {
	got := string(unit(spec))
	for _, want := range []string{
		`ExecStart="/Users/me/.coding-pocket/runtime/1.2.0/pocketd" serve`,
		`Environment="POCKET_HOME=/Users/me/.coding-pocket"`,
		"Restart=always",
		"StartLimitIntervalSec=0",
		"WantedBy=default.target",
	} {
		if !strings.Contains(got, want) {
			t.Fatalf("missing %q in\n%s", want, got)
		}
	}
}

func TestUnitQuotesSpacesQuotesAndSpecifiers(t *testing.T) {
	got := string(unit(Spec{Exe: `/home/a b/"q"/100%/pocketd`, Home: `/home/a b`}))
	if !strings.Contains(got, `ExecStart="/home/a b/\"q\"/100%%/pocketd" serve`) {
		t.Fatalf("got\n%s", got)
	}
	if !strings.Contains(got, `Environment="POCKET_HOME=/home/a b"`) {
		t.Fatalf("got\n%s", got)
	}
}
