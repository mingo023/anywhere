package hosts

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"

	"pocketd/internal/atomicfile"
)

const version = 1

type file struct {
	Version int    `json:"version"`
	Hosts   []Host `json:"hosts"`
}

func listPath(home string) string { return filepath.Join(home, "state", "hosts.json") }

// Load reads the saved hosts from home/state/hosts.json; none when the file isn't there yet.
func Load(home string) ([]Host, error) {
	raw, err := os.ReadFile(listPath(home))
	if errors.Is(err, fs.ErrNotExist) {
		return nil, nil
	}
	var f file
	if err == nil {
		err = json.Unmarshal(raw, &f)
	}
	if err == nil && f.Version != version {
		err = fmt.Errorf("version %d, want %d", f.Version, version)
	}
	if err != nil {
		return nil, fmt.Errorf("hosts %s: %w", listPath(home), err)
	}
	return f.Hosts, nil
}

// Save writes hosts to home/state/hosts.json, readable only by this user.
func Save(home string, hosts []Host) error {
	raw, err := json.MarshalIndent(file{Version: version, Hosts: hosts}, "", "  ")
	if err == nil {
		err = os.MkdirAll(filepath.Dir(listPath(home)), 0o700)
	}
	if err == nil {
		err = atomicfile.Write(listPath(home), raw, 0o600)
	}
	return err
}
