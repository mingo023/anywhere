package state

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
)

// HostID is this host's id, made on first call and kept in <home>/state/host.json,
// so a desktop knows the host again under a new address.
func HostID(home string) (string, error) {
	path := filepath.Join(home, "state", "host.json")
	var f struct {
		ID string `json:"id"`
	}
	raw, err := os.ReadFile(path)
	if err == nil && json.Unmarshal(raw, &f) == nil && f.ID != "" {
		return f.ID, nil
	}
	if err != nil && !errors.Is(err, fs.ErrNotExist) {
		return "", err
	}
	b := make([]byte, 16)
	rand.Read(b)
	f.ID = hex.EncodeToString(b)
	raw, _ = json.Marshal(f)
	return f.ID, write(path, raw)
}
