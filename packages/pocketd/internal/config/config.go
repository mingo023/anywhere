package config

import (
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
)

func Home() string {
	if h := os.Getenv("POCKET_HOME"); h != "" {
		return h
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".coding-pocket")
}

func Sock() string {
	if s := os.Getenv("POCKETD_SOCK"); s != "" {
		return s
	}
	return filepath.Join(Home(), "pocketd.sock")
}

type Config struct {
	Token string `json:"token"`
	Port  int    `json:"port"`
}

// Load reads config.json, creating it with a fresh token on first run. Fields
// left by the TypeScript daemon (profiles) are ignored.
func Load() (Config, error) {
	path := filepath.Join(Home(), "config.json")
	raw, err := os.ReadFile(path)
	if errors.Is(err, fs.ErrNotExist) {
		return create(path)
	}
	if err != nil {
		return Config{}, err
	}
	var c Config
	if err := json.Unmarshal(raw, &c); err != nil || c.Token == "" || c.Port == 0 {
		return Config{}, fmt.Errorf("invalid config %s: need token and port", path)
	}
	return c, nil
}

func create(path string) (Config, error) {
	b := make([]byte, 24)
	rand.Read(b)
	c := Config{Token: base64.RawURLEncoding.EncodeToString(b), Port: 4517}
	raw, _ := json.MarshalIndent(c, "", "  ")
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return Config{}, err
	}
	return c, os.WriteFile(path, append(raw, '\n'), 0o600)
}
