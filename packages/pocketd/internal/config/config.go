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
	"slices"
	"strconv"
)

// channel is "release" in builds made by scripts/release-mac.sh
// (-ldflags "-X pocketd/internal/config.channel=release"). Every other build is Dev,
// so it never touches the installed app's home, service or phone port (ADR 0004).
var channel string

func Release() bool { return channel == "release" }

// HomeName is where the home goes, under the user's home. Hosts' shells read it too, so its separator is always /.
func HomeName() string {
	if Release() {
		return ".zoes"
	}
	return ".zoes/dev"
}

// LegacyHomeNames are the homes before HomeName, oldest first.
func LegacyHomeNames() []string {
	if Release() {
		return releaseLegacy
	}
	return []string{".coding-pocket-dev", ".anywhere/dev"}
}

var releaseLegacy = []string{".coding-pocket", ".anywhere"}

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

// Home is POCKET_HOME, else DefaultHome. POCKET_HOME naming a legacy home counts as unset: host services set it.
func Home() string {
	if h := os.Getenv("POCKET_HOME"); h != "" && !slices.Contains(under(LegacyHomeNames()), filepath.Clean(h)) {
		return h
	}
	return DefaultHome()
}

// DefaultHome is the oldest legacy home still holding the home, else where it goes.
func DefaultHome() string {
	return current(moves())
}

// SharedHome holds the worktrees and Scratch folders of every channel, and the dev home: ~/.anywhere until the release
// home is ~/.zoes.
func SharedHome() string {
	user, _ := os.UserHomeDir()
	if zoes := filepath.Join(user, ".zoes"); current(under(releaseLegacy), zoes) == zoes {
		return zoes
	}
	return filepath.Join(user, ".anywhere")
}

// AccountsDir holds the managed accounts of every channel. It stays under ~/.anywhere where they were made there:
// Claude Code names an account's Keychain item by its config folder's path.
func AccountsDir() string {
	user, _ := os.UserHomeDir()
	if made := filepath.Join(user, ".anywhere", "accounts"); exists(made) {
		return made
	}
	return filepath.Join(SharedHome(), "accounts")
}

// moves is the legacy homes that move into this channel's home, oldest first, and that home. A dev home inside the
// release home moves with it.
func moves() ([]string, string) {
	if Release() {
		user, _ := os.UserHomeDir()
		return under(releaseLegacy), filepath.Join(user, ".zoes")
	}
	return under([]string{".coding-pocket-dev"}), filepath.Join(SharedHome(), "dev")
}

// current is the oldest of legacy still holding the home, else home.
func current(legacy []string, home string) string {
	for i, l := range legacy {
		if holds(legacy, i, home) {
			return l
		}
	}
	return home
}

// holds is whether legacy[i] is a home of its own: a folder that leads neither into home nor, as the link a move
// leaves, into a newer legacy home.
func holds(legacy []string, i int, home string) bool {
	l, err := os.Stat(legacy[i])
	if err != nil || !l.IsDir() || same(l, home) {
		return false
	}
	return !isLink(legacy[i]) || !slices.ContainsFunc(legacy[i+1:], func(n string) bool { return same(l, n) })
}

func isLink(path string) bool {
	fi, err := os.Lstat(path)
	return err == nil && fi.Mode()&fs.ModeSymlink != 0
}

func same(fi fs.FileInfo, path string) bool {
	o, err := os.Stat(path)
	return err == nil && os.SameFile(fi, o)
}

func exists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

func under(names []string) []string {
	user, _ := os.UserHomeDir()
	paths := make([]string, len(names))
	for i, n := range names {
		paths[i] = filepath.Join(user, n)
	}
	return paths
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
