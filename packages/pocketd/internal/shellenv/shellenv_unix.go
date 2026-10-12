//go:build unix

package shellenv

/*
#include <pwd.h>
#include <unistd.h>
*/
import "C"

import (
	"cmp"
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"os/exec"
	"strings"
	"syscall"
	"time"
)

// LoginShell is the shell set with chsh; $SHELL is whatever launched pocketd.
func LoginShell() string {
	if pw := C.getpwuid(C.getuid()); pw != nil && pw.pw_shell != nil {
		if s := C.GoString(pw.pw_shell); s != "" {
			return s
		}
	}
	return fallbackShell
}

// Base is the env a new Terminal.app window starts from, as the desktop
// builds it (terminal_env): the account's basics and nothing from the
// terminal pocketd was started in.
func Base(environ []string, shell string) []string {
	have := map[string]string{}
	for _, kv := range environ {
		if k, v, ok := strings.Cut(kv, "="); ok && v != "" {
			have[k] = v
		}
	}
	var out []string
	for _, k := range kept {
		if v, ok := have[k]; ok {
			out = append(out, k+"="+v)
		}
	}
	return append(out,
		"LANG="+cmp.Or(have["LANG"], "en_US.UTF-8"),
		"PATH=/usr/bin:/bin:/usr/sbin:/sbin",
		"SHELL="+shell,
		"TERM=xterm-256color",
		"COLORTERM=truecolor",
		// Lets Claude Code open links on click: mouse reports can't carry ⌘.
		"TERM_PROGRAM=ghostty",
	)
}

// Capture runs the login shell interactively (so zsh reads .zshrc), then as a
// plain login shell, each within limit. If both fail it keeps env.
func Capture(ctx context.Context, shell string, env []string, limit time.Duration) Result {
	start := time.Now()
	var errs []string
	for _, m := range []struct {
		mode  string
		flags []string
	}{{"interactive", []string{"-l", "-i", "-c"}}, {"login", []string{"-l", "-c"}}} {
		got, err := run(ctx, shell, m.flags, env, limit)
		if err == nil {
			return Result{Env: got, Mode: m.mode, Took: time.Since(start)}
		}
		errs = append(errs, m.mode+": "+err.Error())
	}
	return Result{Env: env, Mode: "base", Took: time.Since(start), Err: strings.Join(errs, "; ")}
}

// env -0 refuses to run a command, so it runs on its own between the markers.
const script = `/usr/bin/printf "%%s" B%[1]s; /usr/bin/env -0; /usr/bin/printf "%%s" E%[1]s`

func run(ctx context.Context, shell string, flags, env []string, limit time.Duration) ([]string, error) {
	var b [8]byte
	rand.Read(b[:])
	nonce := hex.EncodeToString(b[:])
	ctx, cancel := context.WithTimeout(ctx, limit)
	defer cancel()
	cmd := exec.CommandContext(ctx, shell, append(flags, fmt.Sprintf(script, nonce))...)
	cmd.Env = env
	// A session without a controlling tty, so an interactive shell can't stop on
	// SIGTTOU/SIGTTIN; it leads its own group, so a hung rc's children die with it.
	cmd.SysProcAttr = &syscall.SysProcAttr{Setsid: true}
	cmd.Cancel = func() error { return syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL) }
	cmd.WaitDelay = time.Second
	out, err := cmd.Output()
	if ctx.Err() != nil {
		return nil, errors.New("timeout")
	}
	got, perr := Parse(out, nonce)
	if perr != nil && err != nil {
		return nil, err
	}
	return got, perr
}
