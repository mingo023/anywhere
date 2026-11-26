package config

import (
	"cmp"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"net"
	"net/netip"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
)

// channel is "release" in builds made by scripts/release-mac.sh
// (-ldflags "-X pocketd/internal/config.channel=release"). Every other build is Dev,
// so it never touches the installed app's home, service or phone port (ADR 0004).
var channel string

func Release() bool { return channel == "release" }

// HomeName is a path hosts' shells read too, so its separator is always /.
func HomeName() string {
	if Release() {
		return ".anywhere"
	}
	return ".anywhere/dev"
}

// LegacyHomeName is the home before ~/.anywhere; MoveLegacyHome empties it.
func LegacyHomeName() string {
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

// HostLabel names the service `pocketd service install` sets up on a host, apart from the app's.
func HostLabel() string {
	if Release() {
		return "dev.mingo.anywhere.host.pocketd"
	}
	return "dev.mingo.anywhere.dev.host.pocketd"
}

// DefaultPort is the phone port a first run gets. Release and Dev differ so both can run.
func DefaultPort() int {
	if Release() {
		return 4517
	}
	return 4518
}

// DefaultOwnerPort is the owner's TCP port where ownerListen is on by default.
func DefaultOwnerPort() int {
	if Release() {
		return 4519
	}
	return 4520
}

// Home is POCKET_HOME, else DefaultHome. POCKET_HOME naming the legacy home counts as unset: host services set it.
func Home() string {
	if h := os.Getenv("POCKET_HOME"); h != "" && filepath.Clean(h) != legacyHome() {
		return h
	}
	return DefaultHome()
}

// DefaultHome is the legacy home while it still holds one, else HomeName.
func DefaultHome() string {
	user, _ := os.UserHomeDir()
	home := filepath.Join(user, HomeName())
	l, err := os.Stat(legacyHome())
	if err != nil || !l.IsDir() {
		return home
	}
	if h, err := os.Stat(home); err == nil && os.SameFile(l, h) {
		return home
	}
	return legacyHome()
}

func legacyHome() string {
	user, _ := os.UserHomeDir()
	return filepath.Join(user, LegacyHomeName())
}

func Sock() string {
	if s := os.Getenv("POCKETD_SOCK"); s != "" {
		return s
	}
	return filepath.Join(Home(), "pocketd.sock")
}

type Config struct {
	Port int
	// Listen is "auto" (loopback and the tailnet) or "loopback".
	Listen string
	// OwnerListen is "off" or the loopback ip:port where a peer that sends an
	// owner token is the owner. It is on by default only on Windows, whose
	// sockets can't prove a peer is the owner.
	OwnerListen string
}

// Load reads the config table, filling it with the defaults on first run. A token left there by an
// older pocketd goes to adopt, then leaves the table. Keys left by the
// TypeScript daemon (profiles) are kept but ignored.
func Load(db *sql.DB, adopt func(token string) error) (Config, error) {
	rows, err := readRows(db)
	if err != nil {
		return Config{}, err
	}
	if len(rows) == 0 {
		return create(db)
	}
	var c Config
	if err := json.Unmarshal([]byte(rows["port"]), &c.Port); err != nil || c.Port == 0 {
		return Config{}, errors.New("invalid config: need port")
	}
	if c.Listen, err = stringRow(rows, "listen"); err != nil {
		return Config{}, errors.New("invalid config: listen must be auto or loopback")
	}
	c.Listen = cmp.Or(c.Listen, "auto")
	if c.Listen != "auto" && c.Listen != "loopback" {
		return Config{}, errors.New("invalid config: listen must be auto or loopback")
	}
	if c.OwnerListen, err = ownerListenRow(rows); err != nil {
		return Config{}, fmt.Errorf("invalid config: %w", err)
	}
	var token string
	if json.Unmarshal([]byte(rows["token"]), &token) != nil || token == "" {
		return c, nil
	}
	if err := adopt(token); err != nil {
		return Config{}, err
	}
	_, err = db.Exec(`DELETE FROM config WHERE key = 'token'`)
	return c, err
}

// OwnerListen is the config table's ownerListen, read without Load's migration.
func OwnerListen(db *sql.DB) (string, error) {
	rows, err := readRows(db)
	if err != nil {
		return "", err
	}
	return ownerListenRow(rows)
}

// LegacyOwnerListen is the ownerListen that config.json in home holds, for a home no daemon has imported yet: the
// import will take its value into the table as it is. It only reads the file.
func LegacyOwnerListen(home string) (string, error) {
	var c struct {
		OwnerListen string `json:"ownerListen"`
	}
	raw, err := os.ReadFile(filepath.Join(home, "config.json"))
	if err == nil {
		err = json.Unmarshal(raw, &c)
	}
	if err != nil && !errors.Is(err, fs.ErrNotExist) {
		return "", err
	}
	return ownerListen(c.OwnerListen)
}

func ownerListenRow(rows map[string]string) (string, error) {
	v, err := stringRow(rows, "ownerListen")
	if err != nil {
		return "", errors.New("ownerListen must be off or a loopback ip:port")
	}
	return ownerListen(v)
}

// stringRow is key's string value, "" when there is no row.
func stringRow(rows map[string]string, key string) (string, error) {
	raw, ok := rows[key]
	if !ok {
		return "", nil
	}
	var v string
	err := json.Unmarshal([]byte(raw), &v)
	return v, err
}

func ownerListen(v string) (string, error) {
	switch {
	case v == "" && runtime.GOOS == "windows":
		return net.JoinHostPort("127.0.0.1", strconv.Itoa(DefaultOwnerPort())), nil
	case v == "", v == "off":
		return "off", nil
	}
	host, _, err := net.SplitHostPort(v)
	if ip, perr := netip.ParseAddr(host); err != nil || perr != nil || !ip.IsLoopback() {
		return "", errors.New("ownerListen must be off or a loopback ip:port")
	}
	return v, nil
}

func readRows(db *sql.DB) (map[string]string, error) {
	rows, err := db.Query(`SELECT key, value FROM config`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := map[string]string{}
	for rows.Next() {
		var key, value string
		if err := rows.Scan(&key, &value); err != nil {
			return nil, err
		}
		out[key] = value
	}
	return out, rows.Err()
}

func create(db *sql.DB) (Config, error) {
	c := Config{Port: DefaultPort(), Listen: "auto"}
	c.OwnerListen, _ = ownerListen("")
	tx, err := db.Begin()
	if err != nil {
		return Config{}, err
	}
	defer tx.Rollback()
	for key, value := range map[string]any{"port": c.Port, "listen": c.Listen} {
		raw, _ := json.Marshal(value)
		if _, err := tx.Exec(`INSERT INTO config (key, value) VALUES (?, ?)`, key, string(raw)); err != nil {
			return Config{}, err
		}
	}
	return c, tx.Commit()
}
