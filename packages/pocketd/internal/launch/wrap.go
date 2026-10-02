package launch

import (
	"path/filepath"
	"strings"
)

// Wrap is the shell's argument list: setup, then argv, then a login shell.
// Either exit is reported through `pocketd hook exit` while the Terminal
// still shows its output; a Terminal's screen is gone once it closes. A setup
// that passes is reported too, before the agent starts. Setup runs in the
// shell itself, so what it exports reaches the agent; an exit trap reports a
// setup that calls exit.
func Wrap(shell, exe, setup string, argv []string) []string {
	hook := quote(exe) + " hook exit"
	if filepath.Base(shell) == "fish" {
		script := ""
		if setup != "" {
			script = "function pocket_setup_exit --on-event fish_exit; " + hook + " setup $status; end\nbegin\n" + setup + "\nend; or exit\nfunctions -e pocket_setup_exit\n" + hook + " setup 0\n"
		}
		script += "command $argv; set s $status; " + hook + " agent $s; exec " + quote(shell) + " -l"
		return append([]string{"-l", "-c", script}, argv...)
	}
	script := ""
	if setup != "" {
		script = "pocket_setup_exit() { " + hook + " setup $1; }\ntrap 'pocket_setup_exit $?' EXIT\n{\n" + setup + "\n} || exit\ntrap - EXIT\n" + hook + " setup 0\n"
	}
	script += `"$@"; s=$?; ` + hook + " agent $s; exec " + quote(shell) + " -l"
	return append([]string{"-l", "-c", script, shell}, argv...)
}

func quote(s string) string { return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'" }
