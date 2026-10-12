// Package hostop is what a desktop does on this host's files and tools when
// the project lives here: run a program, list, read and change files.
package hostop

import (
	"bytes"
	"context"
	"errors"
	"os/exec"
	"time"

	"pocketd/internal/terminal"
)

// Limit is how much Run keeps of each output stream.
const Limit = 64 << 20

// Run runs argv in cwd with env and stdin, finding argv[0] on env's PATH.
// code is -1 when timeout (zero for none) or ctx ends it.
func Run(ctx context.Context, env []string, cwd string, argv []string, stdin []byte, timeout time.Duration) (code int, stdout, stderr []byte, err error) {
	if len(argv) == 0 {
		return 0, nil, nil, errors.New("no argv")
	}
	path, err := terminal.LookPath(argv[0], env)
	if err != nil {
		return 0, nil, nil, err
	}
	if timeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, timeout)
		defer cancel()
	}
	cmd := exec.CommandContext(ctx, path, argv[1:]...)
	cmd.Dir, cmd.Env, cmd.Stdin = cwd, env, bytes.NewReader(stdin)
	// A child that outlives the kill (git's helpers) keeps the pipes open.
	cmd.WaitDelay = time.Second
	out, errOut := capped{limit: Limit}, capped{limit: Limit}
	cmd.Stdout, cmd.Stderr = &out, &errOut
	err = cmd.Run()
	if errors.Is(err, exec.ErrWaitDelay) {
		err = nil
	}
	switch exit, ok := errors.AsType[*exec.ExitError](err); {
	case err != nil && ctx.Err() != nil:
		return -1, out.b, errOut.b, nil
	case ok:
		return exit.ExitCode(), out.b, errOut.b, nil
	case err != nil:
		return 0, nil, nil, err
	}
	return 0, out.b, errOut.b, nil
}

type capped struct {
	b     []byte
	limit int
}

func (c *capped) Write(p []byte) (int, error) {
	if room := c.limit - len(c.b); room > 0 {
		c.b = append(c.b, p[:min(room, len(p))]...)
	}
	return len(p), nil
}
