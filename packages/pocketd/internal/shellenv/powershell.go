package shellenv

import "strings"

// PowerShell reports whether shell is pwsh or Windows PowerShell. shell may
// be a Windows path on any OS.
func PowerShell(shell string) bool {
	name := strings.TrimSuffix(strings.ToLower(shell[strings.LastIndexAny(shell, `/\`)+1:]), ".exe")
	return name == "pwsh" || name == "powershell"
}

// LoginArgs start shell as a terminal's interactive shell.
func LoginArgs(shell string) []string {
	if PowerShell(shell) {
		return []string{"-NoLogo"}
	}
	return []string{"-l"}
}

// PSQuote is s as a PowerShell literal string. PowerShell also takes the
// typographic single quotes as quotes, and prompts often hold them.
func PSQuote(s string) string {
	var b strings.Builder
	b.WriteByte('\'')
	for _, r := range s {
		if strings.ContainsRune("'‘’‚‛", r) {
			b.WriteRune(r)
		}
		b.WriteRune(r)
	}
	b.WriteByte('\'')
	return b.String()
}

// PSCommand runs argv as PowerShell would from its prompt, so a function or
// script named argv[0] runs too.
func PSCommand(argv []string) string {
	q := make([]string, len(argv))
	for i, a := range argv {
		q[i] = PSQuote(a)
	}
	return "& " + strings.Join(q, " ")
}

// PSCommandIn is PSCommand for shell. Windows PowerShell drops the double quotes inside an argument it passes to a
// program, so there each one goes escaped as the program's argv parser reads it: \" after its backslashes doubled.
// It wraps an argument holding a space in quotes, so the backslashes that end one are doubled too.
func PSCommandIn(shell string, argv []string) string {
	name := strings.ToLower(shell[strings.LastIndexAny(shell, `/\`)+1:])
	if name != "powershell.exe" && name != "powershell" {
		return PSCommand(argv)
	}
	escaped := make([]string, len(argv))
	for i, a := range argv {
		var b strings.Builder
		slashes := 0
		for _, r := range a {
			switch r {
			case '\\':
				slashes++
			case '"':
				b.WriteString(strings.Repeat(`\`, slashes+1))
				slashes = 0
			default:
				slashes = 0
			}
			b.WriteRune(r)
		}
		if strings.ContainsAny(a, " \t") {
			b.WriteString(strings.Repeat(`\`, slashes))
		}
		escaped[i] = b.String()
	}
	return PSCommand(escaped)
}
