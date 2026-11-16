package hosts

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"

	"pocketd/internal/atomicfile"
)

// Owner is the token a Windows host's pocketd mints for this Mac. Its sockets can't tell who the owner is,
// so every connection to its owner TCP port sends the token first.
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

// saveOwner writes o readable only by this user.
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

// parsePair is what `pocketd pair --json` printed, past any login noise.
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

// pairCommand mints a token named name with the app's pocketd under the host's home.
func pairCommand(homeDirName, name string) string {
	// cmd.exe runs the shim and reads its arguments in the console's code page.
	name = strings.Map(func(c rune) rune {
		switch {
		case c == '’':
			return '\''
		case c < 0x80 && c != '"':
			return c
		}
		return '?'
	}, name)
	shim := psQuote(homeDirName + `\bin\pocketd.cmd`)
	return powershell("& (Join-Path $HOME " + shim + ") pair --json --name " + psQuote(name) + "; exit $LASTEXITCODE")
}
