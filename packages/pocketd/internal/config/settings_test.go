package config

import (
	"database/sql"
	"errors"
	"path/filepath"
	"reflect"
	"runtime"
	"slices"
	"testing"
	"time"

	"pocketd/internal/statedb/dbtest"
)

// newDB is a new database whose config table holds rows, each key's JSON value by its dotted name.
func newDB(t *testing.T, rows map[string]string) *sql.DB {
	db := dbtest.Open(t, filepath.Join(t.TempDir(), "pocketd.db"))
	for k, v := range rows {
		if _, err := db.Exec(`INSERT INTO config (key, value) VALUES (?, ?)`, k, v); err != nil {
			t.Fatal(err)
		}
	}
	return db
}

func newSettings(t *testing.T, rows map[string]string) *Settings {
	s, err := NewSettings(newDB(t, rows))
	if err != nil {
		t.Fatal(err)
	}
	return s
}

func configRows(t *testing.T, db *sql.DB) map[string]string {
	rows, err := readRows(db)
	if err != nil {
		t.Fatal(err)
	}
	return rows
}

func TestSettingPhoneAccessKeepsOtherKeys(t *testing.T) {
	db := newDB(t, map[string]string{"token": `"t"`, "port": "4517", "phone.nickname": `"x"`})
	s, _ := NewSettings(db)
	if got := s.PhoneMaxAccess(); got != "ask" {
		t.Fatalf("default: got %q", got)
	}
	if err := s.SetPhoneMaxAccess("auto"); err != nil {
		t.Fatal(err)
	}
	if got := s.PhoneMaxAccess(); got != "auto" {
		t.Fatalf("got %q", got)
	}
	want := map[string]string{"token": `"t"`, "port": "4517", "phone.nickname": `"x"`, "phone.maxAccess": `"auto"`}
	if got := configRows(t, db); !reflect.DeepEqual(got, want) {
		t.Fatalf("got %v", got)
	}
}

func TestResumeAgentsDefaultsOnAndPersists(t *testing.T) {
	db := newDB(t, map[string]string{"token": `"t"`, "port": "1"})
	s, _ := NewSettings(db)
	if !s.ResumeAgents() {
		t.Fatal("resumeAgents is off by default")
	}
	if err := s.SetResumeAgents(false); err != nil {
		t.Fatal(err)
	}
	if rows := configRows(t, db); rows["restore.resumeAgents"] != "false" || rows["token"] != `"t"` {
		t.Fatalf("config = %v", rows)
	}
}

func TestThePhoneCeilingTakesOnlyAPhoneAccess(t *testing.T) {
	s := newSettings(t, nil)
	for _, v := range []string{"settings", "", "Auto"} {
		if err := s.SetPhoneMaxAccess(v); !errors.Is(err, ErrInvalid) {
			t.Errorf("%q: got %v", v, err)
		}
	}
	if err := s.SetPhoneMaxAccess("full"); err != nil || s.PhoneMaxAccess() != "full" {
		t.Fatalf("full: %v, reads %q", err, s.PhoneMaxAccess())
	}
	if got := newSettings(t, map[string]string{"phone.maxAccess": `"settings"`}).PhoneMaxAccess(); got != "ask" {
		t.Fatalf("a hand-edited settings reads as %q", got)
	}
}

func TestNewSettingsKeepTodaysBehaviourUntilSet(t *testing.T) {
	s := newSettings(t, nil)
	if !s.KeepAwake() || s.AwakeLinger() != 2*time.Minute || s.AutomationGrace() != 12*time.Hour || s.AutomationHistory() != 50 {
		t.Fatalf("defaults: %v", s.Values())
	}
}

func TestSettingsReadBackWhatConfigSetWrote(t *testing.T) {
	db := newDB(t, map[string]string{"token": `"t"`, "port": "4517"})
	s, _ := NewSettings(db)
	for key, value := range map[string]string{"awake.enabled": "false", "awake.lingerMinutes": "0", "automations.graceHours": "24", "automations.history": "200", "restore.resumeAgents": "false"} {
		if err := s.Set(key, value); err != nil {
			t.Fatalf("%s: %v", key, err)
		}
	}
	if s.KeepAwake() || s.AwakeLinger() != 0 || s.AutomationGrace() != 24*time.Hour || s.AutomationHistory() != 200 || s.ResumeAgents() {
		t.Fatalf("got %v", s.Values())
	}
	if rows := configRows(t, db); rows["token"] != `"t"` || rows["automations.history"] != "200" {
		t.Fatalf("rows: %v", rows)
	}
	again, _ := NewSettings(db)
	if again.KeepAwake() || again.AutomationHistory() != 200 || again.ResumeAgents() {
		t.Fatalf("after a reopen: %v", again.Values())
	}
}

func TestSettingsRefuseValuesOutsideTheirRange(t *testing.T) {
	s := newSettings(t, nil)
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
	s := newSettings(t, nil)
	s.Set("automations.graceHours", "0")
	if g := s.AutomationGrace(); g <= 10*time.Second || g > time.Minute {
		t.Fatalf("grace %v", g)
	}
}

func TestListenAndPortSitAtTheTopOfTheConfigWhereLoadReadsThem(t *testing.T) {
	db := newDB(t, map[string]string{"port": "4517", "phone.maxAccess": `"edits"`})
	s, _ := NewSettings(db)
	if s.Listen() != "auto" || s.Port() != 4517 {
		t.Fatalf("defaults: %v", s.Values())
	}
	if err := s.SetListen("loopback"); err != nil {
		t.Fatal(err)
	}
	if err := s.SetPort(4600); err != nil {
		t.Fatal(err)
	}
	c, err := Load(db, func(string) error { return nil })
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

// fullPath is unix as a full path on this OS: Windows needs a drive.
func fullPath(unix string) string {
	if runtime.GOOS == "windows" {
		return "C:" + filepath.FromSlash(unix)
	}
	return unix
}

func TestAProviderCommandIsANameOnPathOrAFullPath(t *testing.T) {
	s := newSettings(t, nil)
	if s.AgentCommand("claude") != "claude" || s.Values()["codex.command"] != "" {
		t.Fatalf("defaults: %v", s.Values())
	}
	for _, bad := range []string{"-rf", "my agent", "bin/claude", "/opt/claude\n"} {
		if err := s.Set("claude.command", bad); err == nil {
			t.Errorf("%q accepted", bad)
		}
	}
	if err := s.Set("claude.command", fullPath("/opt/homebrew/bin/claude")); err != nil || s.AgentCommand("claude") != fullPath("/opt/homebrew/bin/claude") {
		t.Fatalf("err %v, got %q", err, s.AgentCommand("claude"))
	}
	if err := s.Set("codex.command", "codex-nightly"); err != nil || s.AgentCommand("codex") != "codex-nightly" {
		t.Fatalf("err %v, got %q", err, s.AgentCommand("codex"))
	}
	if err := s.Set("claude.command", ""); err != nil || s.AgentCommand("claude") != "claude" {
		t.Fatalf("empty: err %v, got %q", err, s.AgentCommand("claude"))
	}
}

func TestAProviderStartsAsItsOwnCommandUnlessConfigNamesAnother(t *testing.T) {
	s := newSettings(t, nil)
	for provider, want := range map[string]string{"cursor": "cursor-agent", "antigravity": "agy", "pi": "pi"} {
		if got := s.AgentCommand(provider); got != want {
			t.Errorf("%s: got %q", provider, got)
		}
	}
	if err := s.Set("cursor.command", fullPath("/opt/cursor/agent")); err != nil || s.AgentCommand("cursor") != fullPath("/opt/cursor/agent") {
		t.Fatalf("err %v, got %q", err, s.AgentCommand("cursor"))
	}
	if v := s.Values(); v["hermes.resumeArgs"] != "chat --resume" || v["opencode.forkArgs"] != "-s {sessionId} --fork" || v["pi.promptArgs"] != "--" {
		t.Fatalf("got %v", v)
	}
}

func TestAgentArgsSplitLikeAShellAndFallBackToTheirDefaults(t *testing.T) {
	s := newSettings(t, nil)
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
	s := newSettings(t, nil)
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

func TestPhoneTerminalsAreOnUntilTheOwnerTurnsThemOff(t *testing.T) {
	s := newSettings(t, nil)
	if !s.PhoneTerminals() || s.Values()["phone.terminals"] != "on" {
		t.Fatalf("default: %v", s.Values()["phone.terminals"])
	}
	if err := s.Set("phone.terminals", "off"); err != nil || s.PhoneTerminals() || s.Values()["phone.terminals"] != "off" {
		t.Fatalf("off: %v %v", err, s.Values()["phone.terminals"])
	}
	if err := s.Set("phone.terminals", "false"); err == nil || s.PhoneTerminals() {
		t.Fatalf("false accepted: %v", err)
	}
	if err := s.Set("phone.terminals", "on"); err != nil || !s.PhoneTerminals() {
		t.Fatalf("on: %v", err)
	}
}

func TestPushRelayIsTheDefaultUntilSetToAnHttpsURL(t *testing.T) {
	s := newSettings(t, nil)
	if s.PushRelay() != DefaultPushRelay || s.Values()["push.relay"] != DefaultPushRelay {
		t.Fatalf("default: %q", s.PushRelay())
	}
	for _, bad := range []string{"", "push.example.com", "http://push.example.com", "ftp://push.example.com", "https://u:p@push.example.com", "https://push.example.com/?k=1"} {
		if err := s.Set("push.relay", bad); !errors.Is(err, ErrInvalidRelay) {
			t.Errorf("%q: %v", bad, err)
		}
	}
	for _, good := range []string{"https://relay.example.com/base", "http://127.0.0.1:8787", "http://localhost:8787"} {
		if err := s.Set("push.relay", good); err != nil || s.PushRelay() != good {
			t.Errorf("%q: %v, got %q", good, err, s.PushRelay())
		}
	}
	if got := newSettings(t, map[string]string{"push.relay": `"http://evil.example.com"`}).PushRelay(); got != DefaultPushRelay {
		t.Fatalf("a hand-edited plain http relay reads as %q", got)
	}
}

func TestAMissingSectionDoesNotReadTheTopLevelKey(t *testing.T) {
	if got := newSettings(t, map[string]string{"port": "4242"}).MCPPort(); got != 0 {
		t.Fatalf("MCPPort = %d, want 0", got)
	}
}

func TestOwnerListenIsSetByConfigSetAndRefusesAPublicAddress(t *testing.T) {
	db := newDB(t, map[string]string{"port": "4517"})
	s, _ := NewSettings(db)
	for _, bad := range []string{"0.0.0.0:4519", "localhost:4519", "4519"} {
		if err := s.Set("ownerListen", bad); err == nil {
			t.Errorf("%q accepted", bad)
		}
	}
	if err := s.Set("ownerListen", "127.0.0.1:4519"); err != nil {
		t.Fatal(err)
	}
	if got, err := OwnerListen(db); err != nil || got != "127.0.0.1:4519" {
		t.Fatalf("%q %v", got, err)
	}
}
