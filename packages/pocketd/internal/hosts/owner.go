package hosts

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"

	"pocketd/internal/atomicfile"
)

// Owner is the token a Windows host's pocketd mints for this Mac; its owner TCP port wants it on every connection.
type Owner struct {
	Token string `json:"token"`
	// Port is pocketd's owner TCP port on the host's loopback.
	Port uint16 `json:"port"`
}

func (o Owner) valid() bool { return o.Token != "" && o.Port != 0 }

func loadOwner(path string) (Owner, bool) {
	raw, err := os.ReadFile(path)
	var o Owner
	if err != nil || json.Unmarshal(raw, &o) != nil {
		return Owner{}, false
	}
	return o, o.valid()
}

func saveOwner(path string, o Owner) error {
	raw, err := json.Marshal(o)
	if err == nil {
		err = os.MkdirAll(filepath.Dir(path), 0o700)
	}
	if err == nil {
		err = atomicfile.Write(path, raw, 0o600)
	}
	return err
}

func parsePair(stdout string) (Owner, bool) {
	lines := strings.Split(stdout, "\n")
	for i := len(lines) - 1; i >= 0; i-- {
		var o Owner
		if json.Unmarshal([]byte(strings.TrimSpace(lines[i])), &o) == nil && o.valid() {
			return o, true
		}
	}
	return Owner{}, false
}

func pairCommand(homeDirName, name string) string {
	// cmd.exe runs the shim and reads its arguments in the console's code page.
	name = strings.Map(func(c rune) rune {
		switch {
		case c == '\u2019':
			return '\''
		case c < 0x80 && c != '"':
			return c
		}
		return '?'
	}, name)
	shim := psQuote(homeDirName + `\bin\pocketd.cmd`)
	return powershell("& (Join-Path $HOME " + shim + ") pair --json --name " + psQuote(name) + "; exit $LASTEXITCODE")
}

// pair has the host's pocketd mint a token for this machine, and keeps it beside the host's other files here.
func (m *Manager) pair(ctx context.Context, h Host) (Owner, Failure) {
	stdout, _, _, err := m.run(ctx, h, pairCommand(m.paths.RemoteDir, m.name), nil)
	if err != nil {
		return Owner{}, failureOf(err)
	}
	o, ok := parsePair(string(stdout))
	if !ok || saveOwner(m.paths.Token(h.ID), o) != nil {
		return Owner{}, FailOther
	}
	return o, ""
}
