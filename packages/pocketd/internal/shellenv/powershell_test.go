package shellenv

import "testing"

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
