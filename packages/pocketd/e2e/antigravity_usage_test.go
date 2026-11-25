//go:build unix

package e2e

import (
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"pocketd/internal/accounts"
	"pocketd/internal/ops"
)

// onLoginPath puts dir first on the PATH of the login shell pocketd reads its env from.
func onLoginPath(dir string) func(*Harness) {
	return func(h *Harness) {
		sh := fmt.Sprintf("export PATH='%s':$PATH\n", dir)
		for _, name := range []string{".zshenv", ".bash_profile", ".profile"} {
			os.WriteFile(filepath.Join(h.Home, name), []byte(sh), 0o644)
		}
	}
}

func (h *Harness) antigravity() accounts.Account {
	h.t.Helper()
	c := h.Ops()
	c.Send(ops.Msg{Op: "accounts.usage"})
	r, err := c.Recv()
	if err != nil || r.Ev != "accounts" {
		h.t.Fatalf("%+v %v", r, err)
	}
	for _, a := range r.Accounts {
		if a.Provider == "antigravity" {
			return a
		}
	}
	h.t.Fatalf("no Antigravity row in %+v", r.Accounts)
	return accounts.Account{}
}

func TestAntigravitysUsageComesFromAgysOwnReport(t *testing.T) {
	bin := t.TempDir()
	report, err := os.ReadFile("../internal/accounts/testdata/agy-usage.json")
	if err != nil {
		t.Fatal(err)
	}
	os.WriteFile(filepath.Join(bin, "report.json"), report, 0o600)
	script := `#!/bin/sh
if [ "$1" = --version ]; then echo "agy 1.5.16"; exit 0; fi
for a in "$@"; do echo "$a"; done > "` + bin + `/argv"
cat "` + bin + `/report.json"
`
	os.WriteFile(filepath.Join(bin, "agy"), []byte(script), 0o755)
	h := Start(t, onLoginPath(bin))
	a := h.antigravity()
	if a.ID != accounts.BuiltIn || !a.SignedIn || a.Usage == nil || a.Usage.Session != nil || a.Usage.Week == nil {
		t.Fatalf("%+v %+v", a, a.Usage)
	}
	if a.Usage.Week.Percent != 0 || a.Usage.Week.ResetsAt != 1796189468000 {
		t.Fatalf("week %+v", a.Usage.Week)
	}
	raw, _ := os.ReadFile(filepath.Join(bin, "argv"))
	argv := strings.Split(strings.TrimSpace(string(raw)), "\n")
	want := []string{"--print", "/usage", "--output-format", "json", "--print-timeout", "10s", "--settings", `{"disableSlashCommands":false}`, "--disable-slash-commands=false", "--log-file"}
	if len(argv) != len(want)+1 || !reflect.DeepEqual(argv[:len(want)], want) || filepath.Base(argv[len(want)]) != "agy.log" {
		t.Fatalf("ran agy with %q", argv)
	}
}

func TestWithoutAgyAntigravityIsListedSignedOut(t *testing.T) {
	h := Start(t)
	if a := h.antigravity(); a.ID != accounts.BuiltIn || a.SignedIn || a.Error != "" || a.Usage == nil || a.Usage.Session != nil {
		t.Fatalf("%+v %+v", a, a.Usage)
	}
}
