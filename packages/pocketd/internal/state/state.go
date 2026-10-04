// Package state keeps what pocketd needs to bring its Terminals back after a
// restart. It never holds a title, env, argv or conversation text.
package state

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"

	"pocketd/internal/atomicfile"
)

const Version = 1

type Launch struct {
	Access string `json:"access"`
	Plan   bool   `json:"plan,omitempty"`
	Model  string `json:"model,omitempty"`
	Effort string `json:"effort,omitempty"`
}

type Terminal struct {
	TerminalID     string  `json:"terminalId"`
	LaunchDir      string  `json:"launchDir"`
	Cols           int     `json:"cols"`
	Rows           int     `json:"rows"`
	Provider       string  `json:"provider,omitempty"`
	ConversationID string  `json:"conversationId,omitempty"`
	TranscriptPath string  `json:"transcriptPath,omitempty"`
	Launch         *Launch `json:"launch,omitempty"`
	AgentID        string  `json:"agentId,omitempty"`
	CreatedAt      int64   `json:"createdAt,omitempty"`
	Status         string  `json:"status,omitempty"`
	Failed         bool    `json:"failed,omitempty"`
	Pinned         bool    `json:"pinned,omitempty"`
	Origin         string  `json:"origin,omitempty"`
}

type File struct {
	Version   int        `json:"version"`
	Terminals []Terminal `json:"terminals"`
}

func Path(home string) string { return filepath.Join(home, "state", "terminals.json") }

// Load reads the file at path; a missing one is empty. A file it can't use is
// renamed to .bad, so the next save doesn't erase it, and Load reports why.
func Load(path string) (File, error) {
	raw, err := os.ReadFile(path)
	if errors.Is(err, fs.ErrNotExist) {
		return File{Version: Version}, nil
	}
	var f File
	if err == nil {
		err = json.Unmarshal(raw, &f)
	}
	if err == nil && f.Version != Version {
		err = fmt.Errorf("version %d, want %d", f.Version, Version)
	}
	if err != nil {
		err = fmt.Errorf("state %s: %w", path, err)
		if rerr := os.Rename(path, path+".bad"); rerr != nil {
			err = errors.Join(err, rerr)
		}
		return File{Version: Version}, err
	}
	return f, nil
}

// Save replaces the file at path whole, so a crash leaves the old one or the new one.
func Save(path string, f File) error {
	raw, err := json.MarshalIndent(f, "", "  ")
	if err != nil {
		return err
	}
	return write(path, raw)
}

func write(path string, raw []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	return atomicfile.Write(path, raw, 0o600)
}
