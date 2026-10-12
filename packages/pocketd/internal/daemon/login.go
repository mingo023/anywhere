package daemon

import (
	"fmt"
	"path/filepath"
	"strings"

	"pocketd/internal/terminal"
)

// loginSpec makes spec the host's login shell in its login env, for a desktop
// that can't know either. cwd "~" or empty is home, and a relative cwd is
// under it. argv runs first, then the shell takes over, as the desktop's
// agent_args does.
func loginSpec(spec terminal.Spec, shell, home string, argv []string) terminal.Spec {
	spec.Cmd, spec.Args, spec.Env = shell, []string{"-l"}, nil
	if rest, ok := strings.CutPrefix(spec.Cwd, "~"); ok && (rest == "" || rest[0] == '/') {
		spec.Cwd = strings.TrimPrefix(rest, "/")
	}
	if !filepath.IsAbs(spec.Cwd) {
		spec.Cwd = filepath.Join(home, spec.Cwd)
	}
	if len(argv) == 0 {
		return spec
	}
	back := fmt.Sprintf("exec '%s' -l", shell)
	if filepath.Base(shell) == "fish" {
		spec.Args = append(spec.Args, "-c", "$argv; "+back)
	} else {
		// `sh -c` binds the first argument after the script to $0.
		spec.Args = append(spec.Args, "-c", `"$@"; `+back, shell)
	}
	spec.Args = append(spec.Args, argv...)
	return spec
}
