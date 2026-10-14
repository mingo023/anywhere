package accounts

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func readJSON(t *testing.T, path string) map[string]any {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var v map[string]any
	if err := json.Unmarshal(raw, &v); err != nil {
		t.Fatal(err)
	}
	return v
}

func TestAManagedAccountReadsTheUsersClaudeFolderThroughLinks(t *testing.T) {
	s := store(t)
	builtIn := s.Dir("claude", BuiltIn)
	os.MkdirAll(filepath.Join(builtIn, "skills"), 0o700)
	os.WriteFile(filepath.Join(builtIn, "settings.json"), []byte(`{"model":"opus"}`), 0o600)
	os.WriteFile(filepath.Join(builtIn, ".credentials.json"), []byte(`{"claudeAiOauth":{"accessToken":"mine"}}`), 0o600)
	id := add(t, s, "claude", "work@example.com")

	for range 2 {
		if err := s.Share("claude", id); err != nil {
			t.Fatal(err)
		}
	}

	dir := s.Dir("claude", id)
	if got, _ := os.ReadFile(filepath.Join(dir, "settings.json")); string(got) != `{"model":"opus"}` {
		t.Fatalf("settings.json: %q", got)
	}
	if _, err := os.Stat(filepath.Join(dir, "skills")); err != nil {
		t.Fatalf("skills: %v", err)
	}
	if _, err := os.Lstat(filepath.Join(dir, "CLAUDE.md")); err == nil {
		t.Fatal("linked a file the user doesn't have")
	}
	if tok := s.claudeToken(id).AccessToken; tok != "tok-work@example.com" {
		t.Fatalf("credentials: %q", tok)
	}
}

func TestSharingKeepsWhatTheAccountAlreadyHas(t *testing.T) {
	s := store(t)
	os.MkdirAll(s.Dir("codex", BuiltIn), 0o700)
	os.WriteFile(filepath.Join(s.Dir("codex", BuiltIn), "config.toml"), []byte("user"), 0o600)
	id := add(t, s, "codex", "work@example.com")
	own := filepath.Join(s.Dir("codex", id), "config.toml")
	os.WriteFile(own, []byte("own"), 0o600)

	if err := s.Share("codex", id); err != nil {
		t.Fatal(err)
	}

	if got, _ := os.ReadFile(own); string(got) != "own" {
		t.Fatalf("config.toml: %q", got)
	}
}

func TestRemovingAnAccountLeavesTheUsersFolderAlone(t *testing.T) {
	s := store(t)
	builtIn := s.Dir("claude", BuiltIn)
	os.MkdirAll(filepath.Join(builtIn, "projects", "p"), 0o700)
	os.WriteFile(filepath.Join(builtIn, "projects", "p", "s.jsonl"), []byte("history"), 0o600)
	id := add(t, s, "claude", "work@example.com")
	if err := s.Share("claude", id); err != nil {
		t.Fatal(err)
	}

	if err := s.Remove("claude", id, 0, func() {}); err != nil {
		t.Fatal(err)
	}

	if got, _ := os.ReadFile(filepath.Join(builtIn, "projects", "p", "s.jsonl")); string(got) != "history" {
		t.Fatalf("history: %q", got)
	}
}

func TestClaudeJSONTakesTheUsersConfigAndKeepsTheAccountsLogin(t *testing.T) {
	s := store(t)
	write(t, filepath.Join(s.Home, ".claude.json"), map[string]any{
		"oauthAccount":           map[string]any{"emailAddress": "me@example.com"},
		"passesEligibilityCache": map[string]any{"org": "mine"},
		"mcpServers":             map[string]any{"linear": map[string]any{"url": "https://mcp"}},
		"hasCompletedOnboarding": true,
		"projects":               map[string]any{"/a": map[string]any{"hasTrustDialogAccepted": true}},
	})
	id := add(t, s, "claude", "work@example.com")
	path := filepath.Join(s.Dir("claude", id), ".claude.json")
	own := readJSON(t, path)
	own["projects"] = map[string]any{"/b": map[string]any{"hasTrustDialogAccepted": true}}
	write(t, path, own)

	if err := s.Share("claude", id); err != nil {
		t.Fatal(err)
	}

	got := readJSON(t, path)
	if email := got["oauthAccount"].(map[string]any)["emailAddress"]; email != "work@example.com" {
		t.Fatalf("login: %v", email)
	}
	if _, ok := got["passesEligibilityCache"]; ok {
		t.Fatal("took the user's account-bound cache")
	}
	if _, ok := got["mcpServers"].(map[string]any)["linear"]; !ok {
		t.Fatalf("mcpServers: %v", got["mcpServers"])
	}
	if got["hasCompletedOnboarding"] != true {
		t.Fatalf("onboarding: %v", got["hasCompletedOnboarding"])
	}
	projects := got["projects"].(map[string]any)
	if _, ok := projects["/a"]; !ok {
		t.Fatalf("projects: %v", projects)
	}
	if _, ok := projects["/b"]; !ok {
		t.Fatalf("lost the account's own project: %v", projects)
	}
}

func TestAClaudeJSONTheUserHasNotWrittenIsNotShared(t *testing.T) {
	s := store(t)
	id := add(t, s, "claude", "work@example.com")
	path := filepath.Join(s.Dir("claude", id), ".claude.json")
	before, _ := os.ReadFile(path)

	if err := s.Share("claude", id); err != nil {
		t.Fatal(err)
	}

	if after, _ := os.ReadFile(path); string(after) != string(before) {
		t.Fatalf("rewrote %s", path)
	}
}

func TestHistoryAnAccountAlreadyHasJoinsTheUsersAndTheFolderBecomesALink(t *testing.T) {
	s := store(t)
	builtIn := s.Dir("claude", BuiltIn)
	os.MkdirAll(filepath.Join(builtIn, "projects", "p"), 0o700)
	os.WriteFile(filepath.Join(builtIn, "projects", "p", "mine.jsonl"), []byte("mine"), 0o600)
	id := add(t, s, "claude", "work@example.com")
	own := filepath.Join(s.Dir("claude", id), "projects")
	os.MkdirAll(filepath.Join(own, "p"), 0o700)
	os.WriteFile(filepath.Join(own, "p", "theirs.jsonl"), []byte("theirs"), 0o600)

	if err := s.Share("claude", id); err != nil {
		t.Fatal(err)
	}

	if st, err := os.Lstat(own); err != nil || st.Mode()&os.ModeSymlink == 0 {
		t.Fatalf("projects is not a link: %v %v", st, err)
	}
	for _, f := range []string{"mine.jsonl", "theirs.jsonl"} {
		if _, err := os.Stat(filepath.Join(builtIn, "projects", "p", f)); err != nil {
			t.Fatalf("%s: %v", f, err)
		}
	}
}

func TestHistoryThatClashesStaysInTheAccountAndIsNotLinkedOver(t *testing.T) {
	s := store(t)
	builtIn := s.Dir("claude", BuiltIn)
	os.MkdirAll(filepath.Join(builtIn, "projects"), 0o700)
	os.WriteFile(filepath.Join(builtIn, "projects", "same.jsonl"), []byte("mine"), 0o600)
	id := add(t, s, "claude", "work@example.com")
	own := filepath.Join(s.Dir("claude", id), "projects")
	os.MkdirAll(own, 0o700)
	os.WriteFile(filepath.Join(own, "same.jsonl"), []byte("theirs"), 0o600)

	if err := s.Share("claude", id); err == nil {
		t.Fatal("linked over a clashing file without saying so")
	}

	if got, _ := os.ReadFile(filepath.Join(own, "same.jsonl")); string(got) != "theirs" {
		t.Fatalf("lost the account's file: %q", got)
	}
	if got, _ := os.ReadFile(filepath.Join(builtIn, "projects", "same.jsonl")); string(got) != "mine" {
		t.Fatalf("overwrote the user's file: %q", got)
	}
}
