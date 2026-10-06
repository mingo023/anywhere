// Package handoff is the file one pocketd leaves for the binary it execs
// into: its live Terminals, their open pty fds, and their Agents.
package handoff

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"golang.org/x/sys/unix"

	"pocketd/internal/atomicfile"
	"pocketd/internal/state"
)

// Format is the format this release writes; MinFormat is the oldest it
// reads. A release must read the file the release before it wrote.
const (
	Format    = 1
	MinFormat = 1
)

type File struct {
	Format    int        `json:"format"`
	Terminals []Terminal `json:"terminals"`
}

type Terminal struct {
	Saved  state.Terminal `json:"saved"`
	Cmd    string         `json:"cmd"`
	Args   []string       `json:"args,omitempty"`
	Cwd    string         `json:"cwd"`
	FD     int            `json:"fd"`
	Pid    int            `json:"pid"`
	Screen []byte         `json:"screen"`
	Agent  *Agent         `json:"agent,omitempty"`
}

// Agent is what an Agent keeps across the exec beyond its saved entry.
type Agent struct {
	Named string `json:"named,omitempty"`
	Phase string `json:"phase"`
	Epoch int64  `json:"epoch"`
	Seq   int64  `json:"seq"`
}

func Path(home string) string { return filepath.Join(home, "handoff.json") }

func Write(path string, f File) error {
	raw, err := json.Marshal(f)
	if err != nil {
		return err
	}
	return atomicfile.Write(path, raw, 0o600)
}

func Read(path string) (File, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return File{}, err
	}
	var f File
	if err := json.Unmarshal(raw, &f); err != nil {
		return File{}, err
	}
	if f.Format < MinFormat || f.Format > Format {
		return File{}, fmt.Errorf("handoff format %d; this pocketd reads %d to %d", f.Format, MinFormat, Format)
	}
	return f, nil
}

// Check is the dry run's test: every fd in f is open here and survives exec.
func Check(f File) error {
	for _, t := range f.Terminals {
		flags, err := unix.FcntlInt(uintptr(t.FD), unix.F_GETFD, 0)
		if err != nil {
			return fmt.Errorf("terminal %s: fd %d: %w", t.Saved.TerminalID, t.FD, err)
		}
		if flags&unix.FD_CLOEXEC != 0 {
			return fmt.Errorf("terminal %s: fd %d closes on exec", t.Saved.TerminalID, t.FD)
		}
	}
	return nil
}
