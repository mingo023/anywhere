package config

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSettingPhoneAccessKeepsOtherKeys(t *testing.T) {
	home := t.TempDir()
	path := filepath.Join(home, "config.json")
	os.WriteFile(path, []byte(`{"token":"t","port":4517,"phone":{"nickname":"x"}}`), 0o600)
	s := NewSettings(home)
	if got := s.PhoneMaxAccess(); got != "ask" {
		t.Fatalf("default: got %q", got)
	}
	if err := s.SetPhoneMaxAccess("auto"); err != nil {
		t.Fatal(err)
	}
	if got := s.PhoneMaxAccess(); got != "auto" {
		t.Fatalf("got %q", got)
	}
	raw, _ := os.ReadFile(path)
	var f struct {
		Token string
		Port  int
		Phone map[string]string
	}
	json.Unmarshal(raw, &f)
	if f.Token != "t" || f.Port != 4517 || f.Phone["nickname"] != "x" || f.Phone["maxAccess"] != "auto" {
		t.Fatalf("got %s", raw)
	}
	if fi, _ := os.Stat(path); fi.Mode().Perm() != 0o600 {
		t.Fatalf("mode %v", fi.Mode())
	}
}

func TestResumeAgentsDefaultsOnAndPersists(t *testing.T) {
	home := t.TempDir()
	path := filepath.Join(home, "config.json")
	os.WriteFile(path, []byte(`{"token":"t","port":1}`), 0o600)
	s := NewSettings(home)
	if !s.ResumeAgents() {
		t.Fatal("resumeAgents is off by default")
	}
	if err := s.SetResumeAgents(false); err != nil {
		t.Fatal(err)
	}
	if raw, _ := os.ReadFile(path); !strings.Contains(string(raw), `"resumeAgents": false`) || !strings.Contains(string(raw), `"token"`) {
		t.Fatalf("config = %s", raw)
	}
}

func TestFullIsNeverAcceptedForThePhone(t *testing.T) {
	home := t.TempDir()
	s := NewSettings(home)
	for _, v := range []string{"full", "", "Auto"} {
		if err := s.SetPhoneMaxAccess(v); !errors.Is(err, ErrInvalid) {
			t.Errorf("%q: got %v", v, err)
		}
	}
	os.WriteFile(filepath.Join(home, "config.json"), []byte(`{"phone":{"maxAccess":"full"}}`), 0o600)
	if got := s.PhoneMaxAccess(); got != "ask" {
		t.Fatalf("a hand-edited full reads as %q", got)
	}
}
