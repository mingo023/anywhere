// Package hosts is the machines this Mac reaches over SSH: their ids, files and ssh commands.
package hosts

import (
	"fmt"
	"net/netip"
	"path/filepath"
	"slices"
	"strings"
	"unicode"
)

const (
	KindAlias   = "alias"
	KindAddress = "address"

	idLen = 24
)

// Target is how ssh reaches a host: an alias from ~/.ssh/config, or an address with the port, user and key it was given.
// An alias's Port and User, when set, win over its config's.
type Target struct {
	Kind string `json:"kind"`
	Name string `json:"name,omitempty"`
	Addr string `json:"addr,omitempty"`
	Port int    `json:"port,omitempty"`
	// User empty signs in as the user on this Mac, or for an alias as its config says.
	User    string `json:"user,omitempty"`
	KeyFile string `json:"keyFile,omitempty"`
}

type Host struct {
	// ID names the host's files here; short so its socket path fits a unix socket's limit.
	ID     string `json:"id"`
	Name   string `json:"name"`
	Target Target `json:"target"`
	// Windows hosts are reached on pocketd's owner TCP port with a token, not on its socket.
	Windows bool `json:"windows,omitempty"`
}

// InvalidError is a target ssh could take for an option or can't use; it says what to fix.
type InvalidError string

func (e InvalidError) Error() string { return string(e) }

type ExistsError struct{ Name string }

func (e ExistsError) Error() string { return e.Name + " is already added" }

// Valid refuses what a saved target must never hold: an empty or option-like name, a port that isn't one.
func Valid(t Target) error {
	const noName = InvalidError("Enter the host's address or name")
	const badAddr = InvalidError("Enter an address like 10.0.4.12 or devbox.example.com")
	const badPort = InvalidError("The port is a number, usually 22")
	const badUser = InvalidError("Enter a username without spaces")
	unsafe := func(s string) bool { return strings.ContainsFunc(s, unicode.IsSpace) || strings.HasPrefix(s, "-") }
	switch t.Kind {
	case KindAlias:
		if t.Name == "" {
			return noName
		}
		if unsafe(t.Name) {
			return badAddr
		}
		if t.Port < 0 || t.Port > 65535 {
			return badPort
		}
		if unsafe(t.User) {
			return badUser
		}
	case KindAddress:
		if t.Addr == "" {
			return noName
		}
		if unsafe(t.Addr) {
			return badAddr
		}
		if t.Port < 1 || t.Port > 65535 {
			return badPort
		}
		if unsafe(t.User) {
			return badUser
		}
	default:
		return noName
	}
	return nil
}

func New(t Target, taken []Host) Host {
	name := t.Name
	if t.Kind == KindAddress {
		name = shortName(t.Addr)
	}
	return Host{ID: unique(slug(name), taken), Name: name, Target: t}
}

func Add(taken []Host, t Target) (Host, error) {
	if err := check(taken, t, ""); err != nil {
		return Host{}, err
	}
	return New(t, taken), nil
}

// Edit is h reaching t instead, under the same id. A name that was only the old target's own follows the new one.
func Edit(taken []Host, h Host, t Target) (Host, error) {
	if err := check(taken, t, h.ID); err != nil {
		return Host{}, err
	}
	if h.Name == New(h.Target, nil).Name {
		h.Name = New(t, nil).Name
	}
	h.Target = t
	return h, nil
}

// check refuses an invalid t, or one a host other than self already has.
func check(taken []Host, t Target, self string) error {
	if err := Valid(t); err != nil {
		return err
	}
	for _, h := range taken {
		if h.Target == t && h.ID != self {
			return ExistsError{h.Name}
		}
	}
	return nil
}

func shortName(addr string) string {
	first, _, _ := strings.Cut(addr, ".")
	if _, err := netip.ParseAddr(addr); err == nil || first == "" {
		return addr
	}
	return first
}

func slug(name string) string {
	var b strings.Builder
	for i, c := range []rune(name) {
		if i == idLen {
			break
		}
		switch {
		case c >= 'a' && c <= 'z' || c >= '0' && c <= '9':
			b.WriteRune(c)
		case c >= 'A' && c <= 'Z':
			b.WriteRune(c - 'A' + 'a')
		default:
			b.WriteByte('-')
		}
	}
	if s := strings.Trim(b.String(), "-"); s != "" {
		return s
	}
	return "host"
}

func unique(base string, taken []Host) string {
	free := func(id string) bool {
		for _, h := range taken {
			if h.ID == id {
				return false
			}
		}
		return true
	}
	id := base
	for n := 2; !free(id); n++ {
		id = fmt.Sprintf("%s-%d", base, n)
	}
	return id
}

// Tag is the id a host's own id has here; pocketd's own ids hold no "/".
func Tag(host, theirs string) string { return host + "/" + theirs }

// Untag is the host and its own id for a tagged id; the host is empty for an id of this Mac's.
func Untag(id string) (host, theirs string) {
	if host, theirs, ok := strings.Cut(id, "/"); ok {
		return host, theirs
	}
	return "", id
}

func Tagged(id string) bool { return strings.Contains(id, "/") }

// Paths is where a host's files live on this Mac and where its pocketd lives on the host: RemoteDir, or one of
// LegacyDirs, oldest first, on a host whose pocketd hasn't moved its home out of it yet.
type Paths struct {
	Home       string
	RemoteDir  string
	LegacyDirs []string
}

// dirs are the folders under the host's home pocketd may live in, the current one first.
func (p Paths) dirs() []string {
	dirs := []string{p.RemoteDir}
	for _, d := range slices.Backward(p.LegacyDirs) {
		dirs = append(dirs, d)
	}
	return dirs
}

func (p Paths) file(id, ext string) string { return filepath.Join(p.Home, "hosts", id+ext) }

func (p Paths) Socket(id string) string { return p.file(id, ".sock") }

func (p Paths) Token(id string) string { return p.file(id, ".token") }

func (p Paths) KnownHosts(id string) string { return p.file(id, ".known_hosts") }

func (p Paths) Key(id string) string { return p.file(id, ".key") }
