package launch

import (
	"path/filepath"
	"strings"

	"pocketd/internal/accounts"
	"pocketd/internal/shellenv"
)

// Wrap is the shell's argument list: setup, then argv, then a login shell.
// Either exit is reported through `pocketd hook exit` while the Terminal
// still shows its output; a Terminal's screen is gone once it closes. A setup
// that passes is reported too, before the agent starts. Setup runs in the
// shell itself, so what it exports reaches the agent; an exit trap reports a
// setup that calls exit. argv[0] may name a shell function (resolve), which
// the shell runs as the user's terminal would. account is a managed
// account's env, set after the user's rc files ran, with the keys that would
// sign the agent in as someone else unset; nil leaves the env alone.
func Wrap(shell, exe, setup string, argv, account []string) []string {
	if shellenv.PowerShell(shell) {
		return wrapPowerShell(exe, setup, argv, preamble(shell, account))
	}
	hook := quote(exe) + " hook exit"
	if filepath.Base(shell) == "fish" {
		script := ""
		if setup != "" {
			script = "function pocket_setup_exit --on-event fish_exit; " + hook + " setup $status; end\nbegin\n" + setup + "\nend; or exit\nfunctions -e pocket_setup_exit\n" + hook + " setup 0\n"
		}
		script += preamble(shell, account) + "$argv; set s $status; " + hook + " agent $s; exec " + quote(shell) + " -l"
		return append([]string{"-l", "-c", script}, argv...)
	}
	script := ""
	if setup != "" {
		script = "pocket_setup_exit() { " + hook + " setup $1; }\ntrap 'pocket_setup_exit $?' EXIT\n{\n" + setup + "\n} || exit\ntrap - EXIT\n" + hook + " setup 0\n"
	}
	script += preamble(shell, account) + `"$@"; s=$?; ` + hook + " agent $s; exec " + quote(shell) + " -l"
	return append([]string{"-l", "-c", script, shell}, argv...)
}

// wrapPowerShell is Wrap for PowerShell, whose -NoExit leaves the shell
// running after the script. A setup that calls exit goes unreported.
func wrapPowerShell(exe, setup string, argv []string, preamble string) []string {
	hook := "& " + shellenv.PSQuote(exe) + " hook exit"
	// $? is false after a native command exits non-zero; a cmdlet sets no code.
	status := "$s = if ($?) { 0 } elseif ($LASTEXITCODE) { $LASTEXITCODE } else { 1 }; "
	script := ""
	if setup != "" {
		script = setup + "\n" + status + "if ($s) { " + hook + " setup $s; exit $s }\n" + hook + " setup 0\n"
	}
	script += preamble + shellenv.PSCommand(argv) + "; " + status + hook + " agent $s"
	return []string{"-NoLogo", "-NoExit", "-Command", script}
}

// preamble is shell's script that unsets the keys in accounts.Unset and
// exports account's env; "" for nil.
func preamble(shell string, account []string) string {
	if account == nil {
		return ""
	}
	set := func(k, v string) string { return "export " + k + "=" + quote(v) + "\n" }
	script := "unset " + strings.Join(accounts.Unset, " ") + "\n"
	if shellenv.PowerShell(shell) {
		set = func(k, v string) string { return "$env:" + k + " = " + shellenv.PSQuote(v) + "\n" }
		script = "Remove-Item Env:" + strings.Join(accounts.Unset, ", Env:") + " -ErrorAction SilentlyContinue\n"
	} else if filepath.Base(shell) == "fish" {
		set = func(k, v string) string { return "set -gx " + k + " " + quote(v) + "\n" }
		script = "set -e " + strings.Join(accounts.Unset, " ") + "\n"
	}
	for _, kv := range account {
		k, v, _ := strings.Cut(kv, "=")
		script += set(k, v)
	}
	return script
}

// InShell runs argv through shell, so a shell function named argv[0] runs.
func InShell(shell string, argv []string) []string {
	if shellenv.PowerShell(shell) {
		return []string{shell, "-NoLogo", "-Command", shellenv.PSCommand(argv)}
	}
	if filepath.Base(shell) == "fish" {
		return append([]string{shell, "-c", "$argv"}, argv...)
	}
	return append([]string{shell, "-c", `"$@"`, shell}, argv...)
}

func quote(s string) string { return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'" }
