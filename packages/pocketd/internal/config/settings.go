package config

import (
	"encoding/json"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"sync"

	"pocketd/internal/atomicfile"
	"pocketd/internal/proto"
)

var ErrInvalid = errors.New("phone.maxAccess is ask, edits or auto")

// Settings are the owner's choices in config.json.
type Settings struct {
	path string
	mu   sync.Mutex
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

// set writes section.key, keeping every other key in the file.
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
	if p, ok := top[section]; ok {
		if err := json.Unmarshal(p, &inner); err != nil {
			return err
		}
	}
	inner[key], _ = json.Marshal(v)
	top[section], _ = json.Marshal(inner)
	out, _ := json.MarshalIndent(top, "", "  ")
	return atomicfile.Write(s.path, append(out, '\n'), 0o600)
}
