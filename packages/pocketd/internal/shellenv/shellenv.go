// Package shellenv reads the env the user's login shell builds, so a pocketd
// started by launchd finds the same CLIs their terminal does.
package shellenv

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"time"
)

// IsExecutable says whether path is absolute and names a file someone may execute.
func IsExecutable(path string) bool {
	st, err := os.Stat(path)
	return filepath.IsAbs(path) && err == nil && !st.IsDir() && st.Mode()&0o111 != 0
}

type Result struct {
	Env  []string
	Mode string // "interactive", "login" or "base"
	Took time.Duration
	Err  string
}

func (r Result) String() string {
	if r.Err != "" {
		return fmt.Sprintf("%s (%s)", r.Mode, r.Err)
	}
	return fmt.Sprintf("%s %s", r.Mode, r.Took.Round(10*time.Millisecond))
}

var dropped = map[string]bool{"PWD": true, "OLDPWD": true, "SHLVL": true, "_": true}

// Parse reads the NUL-separated env between the markers; rc output around them is ignored.
func Parse(out []byte, nonce string) ([]string, error) {
	begin, end := []byte("B"+nonce), []byte("E"+nonce)
	i := bytes.Index(out, begin)
	if i < 0 {
		return nil, errors.New("no start marker")
	}
	body := out[i+len(begin):]
	j := bytes.LastIndex(body, end)
	if j < 0 {
		return nil, errors.New("no end marker")
	}
	var env []string
	for _, kv := range bytes.Split(body[:j], []byte{0}) {
		k, _, ok := bytes.Cut(kv, []byte("="))
		if ok && len(k) > 0 && !dropped[string(k)] {
			env = append(env, string(kv))
		}
	}
	return env, nil
}
