package config

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"net"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"sync"
	"time"
	"unicode"

	"pocketd/internal/atomicfile"
	"pocketd/internal/proto"
	"pocketd/internal/providers"
)

var (
	ErrInvalid      = errors.New("phone.maxAccess is ask, edits or auto")
	ErrInvalidRelay = errors.New("push.relay is an https URL, or http on this Mac")
)

const DefaultPushRelay = "https://push.getanywhere.dev"

// Settings are the owner's choices in config.json.
type Settings struct {
	// Changed, when set before use, hears each key Set saves.
	Changed func(key string)
	path    string
	mu      sync.Mutex
}

func NewSettings(home string) *Settings {
	return &Settings{path: filepath.Join(home, "config.json")}
}

// PhoneMaxAccess is "ask" when unset or not a phone access.
func (s *Settings) PhoneMaxAccess() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	var f struct {
		Phone struct {
			MaxAccess string `json:"maxAccess"`
		} `json:"phone"`
	}
	raw, _ := os.ReadFile(s.path)
	json.Unmarshal(raw, &f)
	if slices.Contains(proto.PhoneAccesses, f.Phone.MaxAccess) {
		return f.Phone.MaxAccess
	}
	return "ask"
}

func (s *Settings) SetPhoneMaxAccess(v string) error {
	if !slices.Contains(proto.PhoneAccesses, v) {
		return ErrInvalid
	}
	return s.set("phone", "maxAccess", v)
}

// PhoneTerminals is whether phones may type into, resize, open and close terminals: on unless phone.terminals is "off".
func (s *Settings) PhoneTerminals() bool {
	v := "on"
	s.get("phone", "terminals", &v)
	return v != "off"
}

// PushRelay is the relay base URL pocketd sends pushes through, DefaultPushRelay when unset or not valid.
func (s *Settings) PushRelay() string {
	v := ""
	if s.get("push", "relay", &v) && ValidRelay(v) == nil {
		return v
	}
	return DefaultPushRelay
}

// ValidRelay takes an https URL, or plain http to this Mac for a relay under test.
func ValidRelay(v string) error {
	u, err := url.Parse(v)
	if err != nil || u.Host == "" || u.User != nil || u.RawQuery != "" || u.Fragment != "" {
		return ErrInvalidRelay
	}
	if u.Scheme == "https" || u.Scheme == "http" && (u.Hostname() == "localhost" || net.ParseIP(u.Hostname()).IsLoopback()) {
		return nil
	}
	return ErrInvalidRelay
}

// ResumeAgents is true unless restore.resumeAgents is false.
func (s *Settings) ResumeAgents() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	var f struct {
		Restore struct {
			ResumeAgents *bool `json:"resumeAgents"`
		} `json:"restore"`
	}
	raw, _ := os.ReadFile(s.path)
	json.Unmarshal(raw, &f)
	return f.Restore.ResumeAgents == nil || *f.Restore.ResumeAgents
}

func (s *Settings) SetResumeAgents(on bool) error { return s.set("restore", "resumeAgents", on) }

// KeepAwake is true unless awake.enabled is false.
func (s *Settings) KeepAwake() bool {
	on := true
	s.get("awake", "enabled", &on)
	return on
}

// AwakeLinger is how long the Mac stays awake after agents stop, 2 minutes when unset.
func (s *Settings) AwakeLinger() time.Duration {
	return time.Duration(s.number("awake", "lingerMinutes", 2, 0, 60)) * time.Minute
}

// AutomationGrace is how late a due run may still start, 12 hours when unset.
// 0 hours means never late; the scheduler ticks every 10 s, so a run on time is up to a tick late.
func (s *Settings) AutomationGrace() time.Duration {
	if h := s.number("automations", "graceHours", 12, 0, 24); h > 0 {
		return time.Duration(h) * time.Hour
	}
	return time.Minute
}

// AutomationHistory is how many runs each automation keeps, 50 when unset.
func (s *Settings) AutomationHistory() int { return s.number("automations", "history", 50, 10, 500) }

var commandName = regexp.MustCompile(`^[A-Za-z0-9._+][A-Za-z0-9._+-]{0,63}$`)

// ValidCommand checks a <provider>.command: empty for the provider's own name, a name the user's shell runs, or a full path.
func ValidCommand(cmd string) error {
	if cmd == "" || commandName.MatchString(cmd) || filepath.IsAbs(cmd) && len(cmd) <= 1024 && !strings.ContainsFunc(cmd, unicode.IsControl) {
		return nil
	}
	return errors.New("the command is a name on your PATH or a full path starting with /")
}

// command is <provider>.command, or "" when unset or not valid.
func (s *Settings) command(provider string) string {
	cmd := ""
	if s.get(provider, "command", &cmd) && ValidCommand(cmd) == nil {
		return cmd
	}
	return ""
}

// AgentCommand is what starts provider: its command in config.json, else its own name.
func (s *Settings) AgentCommand(provider string) string {
	if cmd := s.command(provider); cmd != "" {
		return cmd
	}
	return provider
}

// Values is every setting by its config-set key, for the owner's settings screen.
func (s *Settings) Values() map[string]any {
	terminals := "off"
	if s.PhoneTerminals() {
		terminals = "on"
	}
	v := map[string]any{
		"phone.maxAccess":        s.PhoneMaxAccess(),
		"phone.terminals":        terminals,
		"push.relay":             s.PushRelay(),
		"restore.resumeAgents":   s.ResumeAgents(),
		"awake.enabled":          s.KeepAwake(),
		"awake.lingerMinutes":    s.number("awake", "lingerMinutes", 2, 0, 60),
		"automations.graceHours": s.number("automations", "graceHours", 12, 0, 24),
		"automations.history":    s.AutomationHistory(),
		"listen":                 s.Listen(),
		"port":                   s.Port(),
	}
	for _, p := range providers.All {
		v[p.ID+".command"] = s.command(p.ID)
		for _, key := range ArgKeys {
			v[p.ID+"."+key] = s.argText(p.ID, key)
		}
	}
	return v
}

// Listen is where phones reach pocketd: "auto" (loopback and the tailnet) unless config.json says "loopback".
func (s *Settings) Listen() string {
	mode := "auto"
	if s.get("", "listen", &mode) && mode != "loopback" {
		return "auto"
	}
	return mode
}

// Port is the phone port in config.json, DefaultPort when unset.
func (s *Settings) Port() int { return s.number("", "port", DefaultPort(), 1024, 65535) }

// ParseListen checks a "listen" value for config-set.
func ParseListen(v string) error {
	if v != "auto" && v != "loopback" {
		return errors.New("listen is auto or loopback")
	}
	return nil
}

// ParsePort reads a "port" value for config-set; empty is DefaultPort.
func ParsePort(v string) (int, error) {
	if v == "" {
		return DefaultPort(), nil
	}
	n, err := strconv.Atoi(v)
	if err != nil || n < 1024 || n > 65535 {
		return 0, errors.New("port is a whole number from 1024 to 65535")
	}
	return n, nil
}

func (s *Settings) SetListen(v string) error { return s.set("", "listen", v) }

func (s *Settings) SetPort(n int) error { return s.set("", "port", n) }

// Set parses value for one of the keys below; phone.maxAccess goes through the launcher.
// Changed hears the key once it is saved.
func (s *Settings) Set(key, value string) error {
	err := s.parseAndSet(key, value)
	if err == nil && s.Changed != nil {
		s.Changed(key)
	}
	return err
}

func (s *Settings) parseAndSet(key, value string) error {
	switch key {
	case "restore.resumeAgents", "awake.enabled":
		on, err := strconv.ParseBool(value)
		if err != nil {
			return err
		}
		section, name, _ := strings.Cut(key, ".")
		return s.set(section, name, on)
	case "phone.terminals":
		if value != "on" && value != "off" {
			return errors.New("phone.terminals is on or off")
		}
		return s.set("phone", "terminals", value)
	case "push.relay":
		if err := ValidRelay(value); err != nil {
			return err
		}
		return s.set("push", "relay", value)
	case "awake.lingerMinutes":
		return s.setNumber(key, value, 0, 60)
	case "automations.graceHours":
		return s.setNumber(key, value, 0, 24)
	case "automations.history":
		return s.setNumber(key, value, 10, 500)
	}
	if id, name, _ := strings.Cut(key, "."); providers.Get(id) != nil {
		if name == "command" {
			if err := ValidCommand(value); err != nil {
				return err
			}
			return s.set(id, "command", value)
		}
		if slices.Contains(ArgKeys, name) {
			if err := ValidArgs(name, value); err != nil {
				return err
			}
			return s.set(id, name, value)
		}
	}
	return fmt.Errorf("unknown key %q", key)
}

func (s *Settings) setNumber(key, value string, lo, hi int) error {
	n, err := strconv.Atoi(value)
	if err != nil || n < lo || n > hi {
		return fmt.Errorf("%s is a whole number from %d to %d", key, lo, hi)
	}
	section, name, _ := strings.Cut(key, ".")
	return s.set(section, name, n)
}

// number is section.key, or def when unset or outside lo..hi.
func (s *Settings) number(section, key string, def, lo, hi int) int {
	n := def
	if s.get(section, key, &n) && (n < lo || n > hi) {
		return def
	}
	return n
}

// get decodes section.key, or a top-level key when section is "", into v and reports whether it was there; v keeps its value otherwise.
func (s *Settings) get(section, key string, v any) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	var top, inner map[string]json.RawMessage
	raw, _ := os.ReadFile(s.path)
	json.Unmarshal(raw, &top)
	if inner = top; section != "" {
		json.Unmarshal(top[section], &inner)
	}
	p, ok := inner[key]
	return ok && json.Unmarshal(p, v) == nil
}

// set writes section.key, or a top-level key when section is "", keeping every other key in the file.
func (s *Settings) set(section, key string, v any) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	top, inner := map[string]json.RawMessage{}, map[string]json.RawMessage{}
	raw, err := os.ReadFile(s.path)
	if err != nil && !errors.Is(err, fs.ErrNotExist) {
		return err
	}
	if len(raw) > 0 {
		if err := json.Unmarshal(raw, &top); err != nil {
			return err
		}
	}
	if section == "" {
		top[key], _ = json.Marshal(v)
	} else {
		if p, ok := top[section]; ok {
			if err := json.Unmarshal(p, &inner); err != nil {
				return err
			}
		}
		inner[key], _ = json.Marshal(v)
		top[section], _ = json.Marshal(inner)
	}
	out, _ := json.MarshalIndent(top, "", "  ")
	return atomicfile.Write(s.path, append(out, '\n'), 0o600)
}
