package github

import (
	"context"
	"errors"
	"fmt"
	"os/exec"
	"slices"
	"strings"
	"time"

	"pocketd/internal/hostop"
)

// ErrMissing: the login PATH has no gh.
var ErrMissing = errors.New("gh: command not found")

// Failure is a run that exited non-zero, with what it said on stderr.
type Failure struct{ Stderr string }

func (f *Failure) Error() string { return strings.TrimSpace(f.Stderr) }

// Env is the environment gh runs with in dir: the login shell's, with
// GH_TOKEN set to the token of the account dir's Project picked (daemon.GhEnv).
type Env func(dir string) []string

// run runs argv in dir with stdin, killed after timeout. A token GitHub
// refuses is asked for again once, as one renewed by signing in again is
// another token.
func (env Env) run(ctx context.Context, dir string, argv []string, stdin string, timeout time.Duration) (string, error) {
	out, _, err := env.both(ctx, dir, argv, stdin, timeout)
	return out, err
}

// both is run with what argv printed on stderr too.
func (env Env) both(ctx context.Context, dir string, argv []string, stdin string, timeout time.Duration) (string, string, error) {
	out, errOut, err := env.once(ctx, dir, argv, stdin, timeout)
	if f, ok := errors.AsType[*Failure](err); ok && unauthorized(f.Stderr) {
		return env.once(ctx, dir, argv, stdin, timeout)
	}
	return out, errOut, err
}

func (env Env) once(ctx context.Context, dir string, argv []string, stdin string, timeout time.Duration) (string, string, error) {
	e := append(slices.Clip(env(dir)), "GH_PROMPT_DISABLED=1", "GIT_TERMINAL_PROMPT=0", "NO_COLOR=1")
	code, stdout, stderr, err := hostop.Run(ctx, e, dir, argv, []byte(stdin), timeout)
	switch {
	case argv[0] == "gh" && errors.Is(err, exec.ErrNotFound):
		return "", "", ErrMissing
	case err != nil:
		return "", "", err
	case code == -1:
		return "", "", fmt.Errorf("%s didn't answer in %s", argv[0], timeout)
	case code != 0:
		return string(stdout), string(stderr), &Failure{Stderr: string(stderr)}
	}
	return string(stdout), string(stderr), nil
}
