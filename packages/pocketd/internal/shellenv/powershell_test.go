package shellenv

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"slices"
	"testing"
)

func TestPSQuoteDoublesEveryQuotePowerShellEndsAStringAt(t *testing.T) {
	if got, want := PSQuote("it's ‘x’ ‚y‛ \"$z\" `n"), "'it''s ‘‘x’’ ‚‚y‛‛ \"$z\" `n'"; got != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestPSCommandQuotesEachArgument(t *testing.T) {
	if got, want := PSCommand([]string{`C:\Program Files\claude.exe`, "fix it's bug"}), `& 'C:\Program Files\claude.exe' 'fix it''s bug'`; got != want {
		t.Errorf("got %s, want %s", got, want)
	}
}

func TestPowerShellIsKnownByNameFromEitherOSPath(t *testing.T) {
	for shell, want := range map[string]bool{
		`C:\Program Files\PowerShell\7\pwsh.exe`:                    true,
		`C:\Windows\System32\WindowsPowerShell\v1.0\PowerShell.EXE`: true,
		"/usr/local/bin/pwsh":                                       true,
		"/bin/zsh":                                                  false,
		`C:\Windows\System32\cmd.exe`:                               false,
		"powershell-preview":                                        false,
	} {
		if got := PowerShell(shell); got != want {
			t.Errorf("%s: got %v", shell, got)
		}
	}
}

func TestPSCommandInWindowsPowerShellEscapesTheQuotesItWouldStrip(t *testing.T) {
	shell := `C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`
	if got, want := PSCommandIn(shell, []string{"sh", "-c", `echo "a\" b\\"`}), `& 'sh' '-c' 'echo \"a\\\" b\\\\\"'`; got != want {
		t.Errorf("got %s, want %s", got, want)
	}
	if got, want := PSCommandIn(`C:\Program Files\PowerShell\7\pwsh.exe`, []string{"sh", "-c", `echo "a"`}), `& 'sh' '-c' 'echo "a"'`; got != want {
		t.Errorf("pwsh passes quotes itself: got %s, want %s", got, want)
	}
}

func TestWindowsPowerShellHandsAProgramEachArgumentWhole(t *testing.T) {
	if os.Getenv("PSARGV_HELPER") == "1" {
		return
	}
	shell, err := exec.LookPath("powershell.exe")
	if err != nil {
		t.Skip("no Windows PowerShell")
	}
	want := []string{`C:\my dir\`, `say "hi"`, `a b\\`, `no\space\`, "last"}
	argv := append([]string{os.Args[0], "-test.run=^TestPrintsItsArgv$", "--"}, want...)
	cmd := exec.Command(shell, "-NoProfile", "-NonInteractive", "-Command", PSCommandIn(shell, argv))
	cmd.Env = append(os.Environ(), "PSARGV_HELPER=1")
	out, err := cmd.Output()
	if err != nil {
		t.Fatalf("%v: %s", err, out)
	}
	var got []string
	if err := json.Unmarshal(bytes.TrimSpace(out[:bytes.IndexByte(out, '\n')+1]), &got); err != nil {
		t.Fatalf("%v: %s", err, out)
	}
	if !slices.Equal(got, want) {
		t.Fatalf("got %q, want %q", got, want)
	}
}

func TestPrintsItsArgv(t *testing.T) {
	if os.Getenv("PSARGV_HELPER") != "1" {
		t.Skip("run by TestWindowsPowerShellHandsAProgramEachArgumentWhole")
	}
	args := os.Args[slices.Index(os.Args, "--")+1:]
	raw, _ := json.Marshal(args)
	fmt.Println(string(raw))
}
