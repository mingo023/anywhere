package shellenv

import (
	"cmp"
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"time"
)

// LoginShell is PowerShell 7 when installed, else the Windows PowerShell every Windows has.
func LoginShell() string {
	if p, err := exec.LookPath("pwsh.exe"); err == nil {
		return p
	}
	return filepath.Join(cmp.Or(os.Getenv("SystemRoot"), `C:\Windows`), "System32", "WindowsPowerShell", "v1.0", "powershell.exe")
}

// Base is pocketd's own env: Task Scheduler starts it with the account's
// full env, which is what a new terminal window gets too.
func Base(environ []string, shell string) []string {
	out := slices.DeleteFunc(slices.Clone(environ), func(kv string) bool {
		k, _, _ := strings.Cut(kv, "=")
		return dropped[k] || slices.ContainsFunc([]string{"SHELL", "COLORTERM", "TERM_PROGRAM"}, func(d string) bool { return strings.EqualFold(k, d) })
	})
	return append(out,
		"SHELL="+shell,
		"COLORTERM=truecolor",
		// Lets Claude Code open links on click: mouse reports can't carry ⌘.
		"TERM_PROGRAM=ghostty",
	)
}

// Capture keeps env: PowerShell profiles rarely touch PATH, and running one
// costs seconds.
func Capture(ctx context.Context, shell string, env []string, limit time.Duration) Result {
	return Result{Env: env, Mode: "base"}
}
