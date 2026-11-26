package main

import (
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"strings"

	"pocketd/internal/atomicfile"
	"pocketd/internal/config"
	"pocketd/internal/ops"
	"pocketd/internal/owners"
)

// dial connects to the pocketd on sock for an op only its owner may send. Where a pid can't prove the owner
// (Windows), it sends the owner token it keeps in home, minting one the first time, as the desktop does with
// desktop.token: writing home is the gate, as for pocketd pair --json. pocketd refuses the token from inside a
// Pocket Terminal and hangs up; dialling again without it lets the op meet pocketd's own refusal, which says why.
func dial(sock string) (*ops.Conn, error) {
	c, err := ops.Dial(sock)
	if err != nil {
		return nil, err
	}
	if own(c) != nil {
		c.Close()
		return ops.Dial(sock)
	}
	return c, nil
}

func own(c *ops.Conn) error {
	if runtime.GOOS != "windows" {
		return nil
	}
	token, err := cliToken(config.Home())
	if err != nil {
		return err
	}
	return auth(c, token)
}

// cliToken is the owner token kept in home, minted again when there is none or it was revoked. A token the
// owners file couldn't be read to check is not taken for revoked.
func cliToken(home string) (string, error) {
	file := filepath.Join(home, "cli.token")
	store := owners.Store{Path: owners.Path(home)}
	raw, _ := os.ReadFile(file)
	if token := strings.TrimSpace(string(raw)); token != "" {
		if _, ok := store.Lookup(token); ok {
			return token, nil
		}
		if _, err := store.List(); err != nil {
			return "", err
		}
	}
	o, token, err := store.Mint("pocketd CLI")
	if err != nil {
		return "", err
	}
	if err := atomicfile.Write(file, []byte(token), 0o600); err != nil {
		store.Revoke(o.ID)
		return "", err
	}
	return token, nil
}

func auth(c *ops.Conn, token string) error {
	if err := c.Send(ops.Msg{Op: "auth", Token: token}); err != nil {
		return err
	}
	m, err := c.Recv()
	if err != nil {
		return err
	}
	if m.Ev != "ok" {
		return errors.New(m.Error)
	}
	return nil
}
