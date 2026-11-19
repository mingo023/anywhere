// Package handoff is the file one pocketd leaves for the next: its Terminals
// and their Agents, and the fds an upgrade's exec keeps open. A pocketd from
// before holders also handed down each Terminal's pty fd.
package handoff

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"pocketd/internal/atomicfile"
	"pocketd/internal/state"
	"pocketd/internal/timeline"
)

// Format is the format this release writes; MinFormat is the oldest it
// reads. A release must read the file the release before it wrote. A release
// before holders refuses format 2 instead of orphaning every holder.
const (
	Format    = 2
	MinFormat = 1
)

type File struct {
	Format    int        `json:"format"`
	Terminals []Terminal `json:"terminals"`
	// MCPFD is the open MCP listener the new pocketd takes over; 0 when none came.
	MCPFD int `json:"mcpFd,omitempty"`
	// LockFD is pocketd.lock, held across exec; 0 when none came.
	LockFD int `json:"lockFd,omitempty"`
}

type Terminal struct {
	Saved state.Terminal `json:"saved"`
	Cmd   string         `json:"cmd"`
	Args  []string       `json:"args,omitempty"`
	Cwd   string         `json:"cwd"`
	// FD and Pid are 0 under a holder.
	FD     int    `json:"fd"`
	Pid    int    `json:"pid"`
	Screen []byte `json:"screen"`
	Agent  *Agent `json:"agent,omitempty"`
}

// Agent is what an Agent keeps across the exec beyond its saved entry.
type Agent struct {
	Named string `json:"named,omitempty"`
	Phase string `json:"phase"`
	Epoch int64  `json:"epoch"`
	Seq   int64  `json:"seq"`
	// Read is how far its Claude transcript is in its timeline: to the end of a whole line.
	Read       int64               `json:"read,omitempty"`
	Background timeline.Background `json:"background,omitzero"`
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
