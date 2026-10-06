package config

import (
	"cmp"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"

	"pocketd/internal/atomicfile"
)

// channel is "release" in builds made by scripts/release-mac.sh
// (-ldflags "-X pocketd/internal/config.channel=release"). Every other build is Dev,
// so it never touches the installed app's home, service or phone port (ADR 0004).
var channel string

func Release() bool { return channel == "release" }

func HomeName() string {
	if Release() {
		return ".coding-pocket"
	}
	return ".coding-pocket-dev"
}

func Label() string {
	if Release() {
		return "dev.mingo.anywhere.pocketd"
	}
	return "dev.mingo.anywhere.dev.pocketd"
}

// DefaultPort is the phone port a new config.json gets. Release and Dev differ so both can run.
func DefaultPort() int {
	if Release() {
		return 4517
	}
	return 4518
}

func Home() string {
	if h := os.Getenv("POCKET_HOME"); h != "" {
		return h
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, HomeName())
}

func Sock() string {
	if s := os.Getenv("POCKETD_SOCK"); s != "" {
		return s
	}
	return filepath.Join(Home(), "pocketd.sock")
}

type Config struct {
	Port int `json:"port"`
	// Listen is "auto" (loopback and the tailnet) or "loopback".
	Listen string `json:"listen,omitempty"`
}

// Load reads config.json, creating it on first run. A token left there by an
// older pocketd goes to adopt, then leaves the file. Fields left by the
// TypeScript daemon (profiles) are kept but ignored.
func Load(adopt func(token string) error) (Config, error) {
	path := filepath.Join(Home(), "config.json")
	raw, err := os.ReadFile(path)
	if errors.Is(err, fs.ErrNotExist) {
		return create(path)
	}
	if err != nil {
		return Config{}, err
	}
	var c Config
	if err := json.Unmarshal(raw, &c); err != nil || c.Port == 0 {
		return Config{}, fmt.Errorf("invalid config %s: need port", path)
	}
	c.Listen = cmp.Or(c.Listen, "auto")
	if c.Listen != "auto" && c.Listen != "loopback" {
		return Config{}, fmt.Errorf("invalid config %s: listen must be auto or loopback", path)
	}
	var fields map[string]json.RawMessage
	json.Unmarshal(raw, &fields)
	var token string
	if json.Unmarshal(fields["token"], &token) != nil || token == "" {
		return c, nil
	}
	if err := adopt(token); err != nil {
		return Config{}, err
	}
	delete(fields, "token")
	out, _ := json.MarshalIndent(fields, "", "  ")
	return c, atomicfile.Write(path, append(out, '\n'), 0o600)
}

func create(path string) (Config, error) {
	c := Config{Port: DefaultPort(), Listen: "auto"}
	raw, _ := json.MarshalIndent(c, "", "  ")
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return Config{}, err
	}
	return c, atomicfile.Write(path, append(raw, '\n'), 0o600)
}
