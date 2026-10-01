package launch

import (
	"path/filepath"
	"strings"
)

// Wrap is the shell's argument list: setup, then argv, then a login shell.
// Either exit is reported through `pocketd hook exit` while the Terminal
// still shows its output; a Terminal's screen is gone once it closes.
func Wrap(shell, exe, setup string, argv []string) []string {
	hook := quote(exe) + " hook exit"
	if filepath.Base(shell) == "fish" {
		script := ""
		if setup != "" {
			script = "begin\n" + setup + "\nend; or begin; set s $status; " + hook + " setup $s; exit $s; end\n"
		}
		script += "command $argv; set s $status; " + hook + " agent $s; exec " + quote(shell) + " -l"
		return append([]string{"-l", "-c", script}, argv...)
	}
	script := ""
	if setup != "" {
		script = "{\n" + setup + "\n} || { s=$?; " + hook + " setup $s; exit $s; }\n"
	}
	script += `"$@"; s=$?; ` + hook + " agent $s; exec " + quote(shell) + " -l"
	return append([]string{"-l", "-c", script, shell}, argv...)
}

func quote(s string) string { return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'" }
