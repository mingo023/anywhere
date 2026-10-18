// Package accounts keeps the Claude Code and Codex sign-ins pocketd can run an
// agent on: each provider's built-in login, plus accounts in their own folders.
package accounts

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"os"
	"os/exec"
	"os/user"
	"path/filepath"
	"slices"
	"strings"
	"sync"

	"pocketd/internal/atomicfile"
	"pocketd/internal/providers"
)

// BuiltIn is the id of a provider's own login in ~/.claude or ~/.codex.
const BuiltIn = "builtin"

// login is what pocketd knows of one provider's sign-in.
type login interface {
	// envKey points the CLI at an account's folder.
	envKey() string
	// unset are what would sign its agent in as someone else.
	unset() []string
	identity(s *Store, id string) (email, plan string)
	signedIn(s *Store, id string) bool
	usage(ctx context.Context, s *Store, id string) (Usage, error)
	// shared are the entries of its own folder that a managed account reads through.
	shared() []string
	// history is the shared entry that holds what an account has already run.
	// A real folder in its place is merged into the user's first, so a session
	// started before sharing can still resume.
	history() string
	shareMore(s *Store, id string) error
}

var logins = map[string]login{providers.Claude.ID: claude{}, providers.Codex.ID: codex{}}

// loginOf reads a provider without a login as Claude.
func loginOf(provider string) login {
	if l, ok := logins[provider]; ok {
		return l
	}
	return claude{}
}

// Providers are the ones with accounts, in providers.All order.
var Providers = func() []string {
	var ids []string
	for _, p := range providers.All {
		if _, ok := logins[p.ID]; ok {
			ids = append(ids, p.ID)
		}
	}
	return ids
}()

// Unset are what would sign a managed account's agent in as someone else.
var Unset = func() []string {
	var keys []string
	for _, p := range Providers {
		keys = append(keys, logins[p].unset()...)
	}
	return keys
}()

var (
	ErrUnknown   = errors.New("That account isn't on this machine anymore")
	ErrRunning   = errors.New("Sessions are running on this account. Stop them first, then remove it.")
	ErrSignedOut = errors.New("Sign in to this account before making it the default")
	ErrBuiltIn   = errors.New("The built-in login stays. Sign out instead.")
)

type Account struct {
	ID       string `json:"id"`
	Provider string `json:"provider"`
	Label    string `json:"label,omitempty"`
	Email    string `json:"email,omitempty"`
	Plan     string `json:"plan,omitempty"`
	SignedIn bool   `json:"signedIn"`
	// Default starts its provider's sessions when a launch names no account.
	Default bool   `json:"default,omitempty"`
	Running int    `json:"running,omitempty"`
	Usage   *Usage `json:"usage,omitempty"`
	// Pending is signing in; Error says why that failed.
	Pending bool   `json:"pending,omitempty"`
	Error   string `json:"error,omitempty"`
}

type entry struct {
	ID       string `json:"id"`
	Provider string `json:"provider"`
	Label    string `json:"label,omitempty"`
	Email    string `json:"email,omitempty"`
	Plan     string `json:"plan,omitempty"`
}

type index struct {
	Accounts []entry           `json:"accounts"`
	Defaults map[string]string `json:"defaults,omitempty"`
}

type Store struct {
	// Root holds an account's folder at <provider>/<id> and the index.
	Root string
	// Home holds the built-in logins.
	Home string
	// Secret reads a Keychain item's password; nil reads Claude's credentials file.
	Secret func(service string) (string, bool)

	mu      sync.Mutex
	share   sync.Mutex // one Share at a time: launches and moves can overlap
	usage   map[string]Usage
	pending map[string]pending
}

type pending struct{ provider, err string }

// Dir is where provider's account id keeps its login.
func (s *Store) Dir(provider, id string) string {
	if id == BuiltIn {
		return filepath.Join(s.Home, "."+provider)
	}
	return filepath.Join(s.Root, provider, id)
}

// Env is what points provider's CLI at account id; the built-in one needs nothing.
func (s *Store) Env(provider, id string) []string {
	if id == BuiltIn {
		return nil
	}
	return []string{envKey(provider) + "=" + s.Dir(provider, id)}
}

func envKey(provider string) string {
	return loginOf(provider).envKey()
}

// Of is the managed account a process with env runs on, or "" for any other login.
func (s *Store) Of(provider string, env []string) string {
	dir := ""
	for _, kv := range env {
		if v, ok := strings.CutPrefix(kv, envKey(provider)+"="); ok {
			dir = v
		}
	}
	if dir == "" || filepath.Dir(filepath.Clean(dir)) != filepath.Join(s.Root, provider) {
		return ""
	}
	return filepath.Base(dir)
}

// Resolve is the account a launch of provider runs on: id, else the
// provider's default, else the built-in one.
func (s *Store) Resolve(provider, id string) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	x := s.load()
	if id == "" {
		id = x.Defaults[provider]
		if !slices.ContainsFunc(x.Accounts, func(e entry) bool { return e.ID == id && e.Provider == provider }) {
			return BuiltIn, nil
		}
	}
	if id == BuiltIn || slices.ContainsFunc(x.Accounts, func(e entry) bool { return e.ID == id && e.Provider == provider }) {
		return id, nil
	}
	return "", ErrUnknown
}

// List is every account, built-in first. running counts the sessions on one.
func (s *Store) List(running func(provider, id string) int) []Account {
	s.mu.Lock()
	defer s.mu.Unlock()
	x := s.load()
	var out []Account
	for _, p := range Providers {
		def := x.Defaults[p]
		if !slices.ContainsFunc(x.Accounts, func(e entry) bool { return e.ID == def && e.Provider == p }) {
			def = BuiltIn
		}
		email, plan := s.identity(p, BuiltIn)
		rows := []entry{{ID: BuiltIn, Provider: p, Email: email, Plan: plan}}
		for _, e := range x.Accounts {
			if e.Provider == p {
				rows = append(rows, e)
			}
		}
		for _, e := range rows {
			a := Account{ID: e.ID, Provider: p, Label: e.Label, Email: e.Email, Plan: e.Plan, SignedIn: s.SignedIn(p, e.ID), Default: e.ID == def, Running: running(p, e.ID)}
			if u, ok := s.usage[p+"/"+e.ID]; ok {
				a.Usage = &u
			}
			out = append(out, a)
		}
		for id, w := range s.pending {
			if w.provider == p {
				out = append(out, Account{ID: id, Provider: p, Pending: true, Error: w.err})
			}
		}
	}
	return out
}

// Add makes a folder for a new account of provider, which signs in there.
func (s *Store) Add(provider string) (string, error) {
	if !slices.Contains(Providers, provider) {
		return "", ErrUnknown
	}
	b := make([]byte, 6)
	rand.Read(b)
	id := hex.EncodeToString(b)
	if err := os.MkdirAll(s.Dir(provider, id), 0o700); err != nil {
		return "", err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.pending == nil {
		s.pending = map[string]pending{}
	}
	for old, w := range s.pending {
		if w.provider == provider && w.err != "" {
			delete(s.pending, old)
		}
	}
	s.pending[id] = pending{provider: provider}
	return id, nil
}

// Finish keeps a new account id once its sign-in has ended, if it signed in
// as someone not already listed. signOut undoes a sign-in it doesn't keep.
// A listed account takes the identity it signed in again as.
func (s *Store) Finish(provider, id string, signOut func()) {
	email, plan := s.identity(provider, id)
	s.mu.Lock()
	x := s.load()
	if _, ok := s.pending[id]; !ok {
		if i := slices.IndexFunc(x.Accounts, func(e entry) bool { return e.ID == id && e.Provider == provider }); i >= 0 && email != "" {
			x.Accounts[i].Email, x.Accounts[i].Plan = email, plan
			s.save(x)
		}
		s.mu.Unlock()
		return
	}
	if email != "" && !slices.Contains(s.emails(x, provider), email) {
		delete(s.pending, id)
		x.Accounts = append(x.Accounts, entry{ID: id, Provider: provider, Email: email, Plan: plan})
		s.save(x)
		s.mu.Unlock()
		return
	}
	fail := "Sign-in didn't finish"
	if email != "" {
		fail = email + " is already added"
	}
	// Shown before signing out, which can take seconds, so a list read once the sign-in ends sees it.
	s.pending[id] = pending{provider, fail}
	s.mu.Unlock()
	if email != "" {
		signOut()
	}
	os.RemoveAll(s.Dir(provider, id))
}

// emails is every email provider's accounts are signed in as.
func (s *Store) emails(x index, provider string) []string {
	emails := []string{}
	if e, _ := s.identity(provider, BuiltIn); e != "" {
		emails = append(emails, e)
	}
	for _, e := range x.Accounts {
		if e.Provider == provider {
			emails = append(emails, e.Email)
		}
	}
	return emails
}

// Dismiss forgets a sign-in that failed, reporting whether id was one.
func (s *Store) Dismiss(id string) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	w, ok := s.pending[id]
	if ok && w.err != "" {
		delete(s.pending, id)
	}
	return ok && w.err != ""
}

func (s *Store) Rename(provider, id, label string) error {
	if id == BuiltIn {
		return ErrBuiltIn
	}
	return s.edit(provider, id, func(x *index, i int) error {
		x.Accounts[i].Label = strings.TrimSpace(label)
		return nil
	})
}

// SetDefault makes id start provider's sessions that name no account.
func (s *Store) SetDefault(provider, id string) error {
	if id != BuiltIn && id != "" && !s.SignedIn(provider, id) {
		return ErrSignedOut
	}
	return s.edit(provider, id, func(x *index, _ int) error {
		if x.Defaults == nil {
			x.Defaults = map[string]string{}
		}
		x.Defaults[provider] = id
		return nil
	})
}

// Remove forgets id and deletes its folder. signOut runs first, so no
// Keychain item outlives it.
func (s *Store) Remove(provider, id string, running int, signOut func()) error {
	if id == BuiltIn {
		return ErrBuiltIn
	}
	if id == "" {
		return ErrUnknown
	}
	if running > 0 {
		return ErrRunning
	}
	if _, err := s.Resolve(provider, id); err != nil {
		return err
	}
	if s.SignedIn(provider, id) {
		signOut()
	}
	return s.edit(provider, id, func(x *index, i int) error {
		x.Accounts = slices.Delete(x.Accounts, i, i+1)
		if x.Defaults[provider] == id {
			delete(x.Defaults, provider)
		}
		delete(s.usage, provider+"/"+id)
		return os.RemoveAll(s.Dir(provider, id))
	})
}

// edit runs f on the index with id's position, -1 for the built-in one, and saves it.
func (s *Store) edit(provider, id string, f func(x *index, i int) error) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	x := s.load()
	i := slices.IndexFunc(x.Accounts, func(e entry) bool { return e.ID == id && e.Provider == provider })
	if i < 0 && id != BuiltIn {
		return ErrUnknown
	}
	if err := f(&x, i); err != nil {
		return err
	}
	return s.save(x)
}

func (s *Store) load() index {
	var x index
	raw, _ := os.ReadFile(filepath.Join(s.Root, "accounts.json"))
	json.Unmarshal(raw, &x)
	return x
}

func (s *Store) save(x index) error {
	raw, _ := json.MarshalIndent(x, "", "  ")
	if err := os.MkdirAll(s.Root, 0o700); err != nil {
		return err
	}
	return atomicfile.Write(filepath.Join(s.Root, "accounts.json"), raw, 0o600)
}

// identity is the email and plan account id signed in as, from its CLI's own files.
func (s *Store) identity(provider, id string) (email, plan string) {
	return loginOf(provider).identity(s, id)
}

func title(s string) string {
	if s == "" {
		return ""
	}
	return strings.ToUpper(s[:1]) + s[1:]
}

// SignedIn is whether account id of provider holds a login.
func (s *Store) SignedIn(provider, id string) bool {
	return loginOf(provider).signedIn(s, id)
}

// Keychain reads a macOS Keychain item's password with the security tool,
// which the items Claude Code writes trust.
func Keychain(service string) (string, bool) {
	return findPassword(service, keychainAccount())
}

// findPassword reads the item for service under account, in keychains or
// else the user's search list. Without the account, security returns
// whichever item with that service it finds first, which may be a stale one
// that holds no login.
func findPassword(service, account string, keychains ...string) (string, bool) {
	args := append([]string{"find-generic-password", "-s", service, "-a", account, "-w"}, keychains...)
	out, err := exec.Command("/usr/bin/security", args...).Output()
	return strings.TrimSpace(string(out)), err == nil
}

// keychainAccount is the account Claude Code files its credentials under: the user's name.
func keychainAccount() string {
	if name := os.Getenv("USER"); name != "" {
		return name
	}
	u, err := user.Current()
	if err != nil {
		return ""
	}
	return u.Username
}

// KeychainService mirrors Claude Code 2.1.296: a CLAUDE_CONFIG_DIR adds the
// first 8 hex of its path's sha256.
func KeychainService(dir string, builtIn bool) string {
	if builtIn {
		return "Claude Code-credentials"
	}
	sum := sha256.Sum256([]byte(dir))
	return "Claude Code-credentials-" + hex.EncodeToString(sum[:])[:8]
}
