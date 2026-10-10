package config

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"
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

func TestNewSettingsKeepTodaysBehaviourUntilSet(t *testing.T) {
	s := NewSettings(t.TempDir())
	if !s.KeepAwake() || s.AwakeLinger() != 2*time.Minute || s.AutomationGrace() != 12*time.Hour || s.AutomationHistory() != 50 {
		t.Fatalf("defaults: %v", s.Values())
	}
}

func TestSettingsReadBackWhatConfigSetWrote(t *testing.T) {
	home := t.TempDir()
	os.WriteFile(filepath.Join(home, "config.json"), []byte(`{"token":"t","port":4517}`), 0o600)
	s := NewSettings(home)
	for key, value := range map[string]string{"awake.enabled": "false", "awake.lingerMinutes": "0", "automations.graceHours": "24", "automations.history": "200", "restore.resumeAgents": "false"} {
		if err := s.Set(key, value); err != nil {
			t.Fatalf("%s: %v", key, err)
		}
	}
	if s.KeepAwake() || s.AwakeLinger() != 0 || s.AutomationGrace() != 24*time.Hour || s.AutomationHistory() != 200 || s.ResumeAgents() {
		t.Fatalf("got %v", s.Values())
	}
	if raw, _ := os.ReadFile(filepath.Join(home, "config.json")); !strings.Contains(string(raw), `"token": "t"`) {
		t.Fatalf("lost other keys: %s", raw)
	}
}

func TestSettingsRefuseValuesOutsideTheirRange(t *testing.T) {
	s := NewSettings(t.TempDir())
	for key, value := range map[string]string{"automations.history": "5", "automations.graceHours": "48", "awake.lingerMinutes": "x", "awake.enabled": "maybe", "port": "1"} {
		if err := s.Set(key, value); err == nil {
			t.Errorf("%s=%s accepted", key, value)
		}
	}
	if s.AutomationHistory() != 50 {
		t.Fatal("a refused value was written")
	}
}

func TestNeverRunLateStillStartsARunThatIsOnTime(t *testing.T) {
	s := NewSettings(t.TempDir())
	s.Set("automations.graceHours", "0")
	if g := s.AutomationGrace(); g <= 10*time.Second || g > time.Minute {
		t.Fatalf("grace %v", g)
	}
}

func TestListenAndPortSitAtTheTopOfConfigJSONWhereLoadReadsThem(t *testing.T) {
	home := t.TempDir()
	path := filepath.Join(home, "config.json")
	os.WriteFile(path, []byte(`{"port":4517,"phone":{"maxAccess":"edits"}}`), 0o600)
	s := NewSettings(home)
	if s.Listen() != "auto" || s.Port() != 4517 {
		t.Fatalf("defaults: %v", s.Values())
	}
	if err := s.SetListen("loopback"); err != nil {
		t.Fatal(err)
	}
	if err := s.SetPort(4600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("POCKET_HOME", home)
	c, err := Load(func(string) error { return nil })
	if err != nil || c.Listen != "loopback" || c.Port != 4600 || s.PhoneMaxAccess() != "edits" {
		t.Fatalf("%+v %v %v", c, err, s.Values())
	}
}

func TestOnlyUnprivilegedPortsAndKnownListenModesAreAccepted(t *testing.T) {
	for _, v := range []string{"80", "65536", "x", "-1"} {
		if _, err := ParsePort(v); err == nil {
			t.Errorf("port %s accepted", v)
		}
	}
	if n, err := ParsePort(""); err != nil || n != DefaultPort() {
		t.Fatalf("empty port: %d %v", n, err)
	}
	if ParseListen("lan") == nil || ParseListen("loopback") != nil {
		t.Fatal("listen")
	}
}

func TestAProviderCommandIsANameOnPathOrAFullPath(t *testing.T) {
	s := NewSettings(t.TempDir())
	if s.AgentCommand("claude") != "claude" || s.Values()["codex.command"] != "" {
		t.Fatalf("defaults: %v", s.Values())
	}
	for _, bad := range []string{"-rf", "my agent", "bin/claude", "/opt/claude\n"} {
		if err := s.Set("claude.command", bad); err == nil {
			t.Errorf("%q accepted", bad)
		}
	}
	if err := s.Set("claude.command", "/opt/homebrew/bin/claude"); err != nil || s.AgentCommand("claude") != "/opt/homebrew/bin/claude" {
		t.Fatalf("err %v, got %q", err, s.AgentCommand("claude"))
	}
	if err := s.Set("codex.command", "codex-nightly"); err != nil || s.AgentCommand("codex") != "codex-nightly" {
		t.Fatalf("err %v, got %q", err, s.AgentCommand("codex"))
	}
	if err := s.Set("claude.command", ""); err != nil || s.AgentCommand("claude") != "claude" {
		t.Fatalf("empty: err %v, got %q", err, s.AgentCommand("claude"))
	}
}

func TestAgentArgsSplitLikeAShellAndFallBackToTheirDefaults(t *testing.T) {
	s := NewSettings(t.TempDir())
	if got := s.AgentArgs("claude"); !slices.Equal(got.Fork, []string{"--resume", SessionID, "--fork-session"}) || !slices.Equal(got.Prompt, []string{"--"}) {
		t.Fatalf("defaults: %q", got)
	}
	if s.Values()["codex.resumeArgs"] != "resume" || s.Values()["codex.forkArgs"] != "fork "+SessionID {
		t.Fatalf("values: %v", s.Values())
	}
	if err := s.Set("claude.forkArgs", `--resume "{sessionId}" --fork-session --name 'my fork'`); err != nil {
		t.Fatal(err)
	}
	if got := s.AgentArgs("claude").Fork; !slices.Equal(got, []string{"--resume", SessionID, "--fork-session", "--name", "my fork"}) {
		t.Fatalf("fork = %q", got)
	}
	if err := s.Set("codex.resumeArgs", ""); err != nil || len(s.AgentArgs("codex").Resume) != 0 || s.Values()["codex.resumeArgs"] != "" {
		t.Fatalf("empty resume: %v %q", err, s.AgentArgs("codex").Resume)
	}
}

func TestBadAgentArgsAreRefused(t *testing.T) {
	s := NewSettings(t.TempDir())
	for key, bad := range map[string]string{
		"claude.forkArgs":   "--fork-session",
		"claude.promptArgs": `"--prompt`,
		"codex.resumeArgs":  `"it's"`,
		"claude.resumeArgs": "--resume\x07",
		"gemini.promptArgs": "--",
	} {
		if err := s.Set(key, bad); err == nil {
			t.Errorf("%s %q accepted", key, bad)
		}
	}
	if got := s.Values()["claude.forkArgs"]; got != "--resume "+SessionID+" --fork-session" {
		t.Fatalf("fork args = %q", got)
	}
}
