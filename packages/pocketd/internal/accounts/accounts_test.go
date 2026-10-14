package accounts

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func store(t *testing.T) *Store {
	home := t.TempDir()
	return &Store{Root: filepath.Join(home, ".anywhere", "accounts"), Home: home}
}

func write(t *testing.T, path string, v any) {
	t.Helper()
	raw, _ := json.Marshal(v)
	os.MkdirAll(filepath.Dir(path), 0o700)
	if err := os.WriteFile(path, raw, 0o600); err != nil {
		t.Fatal(err)
	}
}

func jwt(claims map[string]any) string {
	raw, _ := json.Marshal(claims)
	return "e30." + base64.RawURLEncoding.EncodeToString(raw) + ".sig"
}

func signInClaude(t *testing.T, s *Store, id, email string) {
	dir := s.Dir("claude", id)
	cfg := filepath.Join(dir, ".claude.json")
	if id == BuiltIn {
		cfg = filepath.Join(s.Home, ".claude.json")
	}
	write(t, cfg, map[string]any{"oauthAccount": map[string]any{"emailAddress": email, "organizationType": "claude_max"}})
	write(t, filepath.Join(dir, ".credentials.json"), map[string]any{"claudeAiOauth": map[string]any{"accessToken": "tok-" + email, "expiresAt": time.Now().Add(time.Hour).UnixMilli()}})
}

func signInCodex(t *testing.T, s *Store, id, email string) {
	write(t, filepath.Join(s.Dir("codex", id), "auth.json"), map[string]any{"tokens": map[string]any{
		"id_token":     jwt(map[string]any{"email": email, "https://api.openai.com/auth": map[string]any{"chatgpt_plan_type": "plus"}}),
		"access_token": jwt(map[string]any{"exp": time.Now().Add(time.Hour).Unix()}),
		"account_id":   "acct-" + email,
	}})
}

func add(t *testing.T, s *Store, provider, email string) string {
	t.Helper()
	id, err := s.Add(provider)
	if err != nil {
		t.Fatal(err)
	}
	if provider == "codex" {
		signInCodex(t, s, id, email)
	} else {
		signInClaude(t, s, id, email)
	}
	s.Finish(provider, id, func() { t.Fatal("signed out a new account") })
	return id
}

func none(string, string) int { return 0 }

func find(list []Account, id string) (Account, bool) {
	for _, a := range list {
		if a.ID == id {
			return a, true
		}
	}
	return Account{}, false
}

func TestAnAccountsEnvPointsItsCLIAtItsFolderAndMapsBack(t *testing.T) {
	s := store(t)
	env := s.Env("codex", "abc")
	if len(env) != 1 || env[0] != "CODEX_HOME="+filepath.Join(s.Root, "codex", "abc") {
		t.Fatalf("%v", env)
	}
	if got := s.Of("codex", append([]string{"HOME=/x"}, env...)); got != "abc" {
		t.Fatalf("%q", got)
	}
	if s.Env("claude", BuiltIn) != nil {
		t.Fatal("the built-in login set an env var")
	}
	if got := s.Of("claude", []string{"CLAUDE_CONFIG_DIR=/elsewhere/claude/abc"}); got != "" {
		t.Fatalf("a folder Anywhere doesn't manage mapped to %q", got)
	}
}

func TestASignInIsKeptUnderItsEmailAndPlan(t *testing.T) {
	s := store(t)
	claude := add(t, s, "claude", "a@x.com")
	codex := add(t, s, "codex", "b@x.com")
	list := s.List(none)
	if a, _ := find(list, claude); a.Email != "a@x.com" || a.Plan != "Max" || !a.SignedIn || a.Pending {
		t.Fatalf("%+v", a)
	}
	if a, _ := find(list, codex); a.Email != "b@x.com" || a.Plan != "Plus" || !a.SignedIn {
		t.Fatalf("%+v", a)
	}
	if len(list) != 4 || list[0].ID != BuiltIn || list[0].SignedIn || !list[0].Default {
		t.Fatalf("%+v", list)
	}
}

func TestASignInThatNeverFinishedDeletesItsFolder(t *testing.T) {
	s := store(t)
	id, _ := s.Add("claude")
	s.Finish("claude", id, func() { t.Fatal("signed out nobody") })
	if _, err := os.Stat(s.Dir("claude", id)); !os.IsNotExist(err) {
		t.Fatal("folder kept")
	}
	if a, _ := find(s.List(none), id); !a.Pending || a.Error != "Sign-in didn't finish" {
		t.Fatalf("%+v", a)
	}
	s.Dismiss(id)
	if _, ok := find(s.List(none), id); ok {
		t.Fatal("dismissed failure still listed")
	}
}

func TestTheSameEmailCantBeAddedTwice(t *testing.T) {
	s := store(t)
	signInClaude(t, s, BuiltIn, "a@x.com")
	id, _ := s.Add("claude")
	signInClaude(t, s, id, "a@x.com")
	signedOut := false
	s.Finish("claude", id, func() { signedOut = true })
	if !signedOut {
		t.Fatal("the duplicate stayed signed in")
	}
	if a, _ := find(s.List(none), id); a.Error != "a@x.com is already added" {
		t.Fatalf("%+v", a)
	}
}

func TestALaunchFallsBackToTheDefaultThenTheBuiltInLogin(t *testing.T) {
	s := store(t)
	if got, _ := s.Resolve("claude", ""); got != BuiltIn {
		t.Fatalf("%q", got)
	}
	id := add(t, s, "claude", "a@x.com")
	s.SetDefault("claude", id)
	if got, _ := s.Resolve("claude", ""); got != id {
		t.Fatalf("%q", got)
	}
	if got, _ := s.Resolve("claude", BuiltIn); got != BuiltIn {
		t.Fatalf("%q", got)
	}
	if _, err := s.Resolve("codex", id); err != ErrUnknown {
		t.Fatalf("another provider's account resolved: %v", err)
	}
	s.Remove("claude", id, 0, func() {})
	if got, _ := s.Resolve("claude", ""); got != BuiltIn {
		t.Fatalf("removed default still resolves: %q", got)
	}
}

func TestASignedOutAccountCannotBecomeTheDefault(t *testing.T) {
	s := store(t)
	id, err := s.Add("claude")
	if err != nil {
		t.Fatal(err)
	}

	if err := s.SetDefault("claude", id); err != ErrSignedOut {
		t.Fatalf("err = %v", err)
	}

	if got, _ := s.Resolve("claude", ""); got != BuiltIn {
		t.Fatalf("default = %q", got)
	}
}

func TestRemoveSignsOutAndDeletesTheFolderButNotWhileRunning(t *testing.T) {
	s := store(t)
	id := add(t, s, "codex", "b@x.com")
	if err := s.Remove("codex", id, 1, func() {}); err != ErrRunning {
		t.Fatalf("%v", err)
	}
	if err := s.Remove("codex", BuiltIn, 0, func() {}); err != ErrBuiltIn {
		t.Fatalf("%v", err)
	}
	signedOut := false
	if err := s.Remove("codex", id, 0, func() { signedOut = true }); err != nil || !signedOut {
		t.Fatalf("%v %v", err, signedOut)
	}
	if _, err := os.Stat(s.Dir("codex", id)); !os.IsNotExist(err) {
		t.Fatal("folder kept")
	}
	if _, ok := find(s.List(none), id); ok {
		t.Fatal("still listed")
	}
}

func TestRenameLabelsAnAccount(t *testing.T) {
	s := store(t)
	id := add(t, s, "claude", "a@x.com")
	if err := s.Rename("claude", id, "  Work "); err != nil {
		t.Fatal(err)
	}
	if a, _ := find(s.List(none), id); a.Label != "Work" {
		t.Fatalf("%+v", a)
	}
}

func TestTheKeychainItemIsNamedAfterTheFolder(t *testing.T) {
	if got := KeychainService("/Users/me/.anywhere/accounts/claude/abc", false); len(got) != len("Claude Code-credentials-")+8 {
		t.Fatalf("%q", got)
	}
	if got := KeychainService("", true); got != "Claude Code-credentials" {
		t.Fatalf("%q", got)
	}
}

func TestUsageReadsBothProvidersLimits(t *testing.T) {
	s := store(t)
	claude := add(t, s, "claude", "a@x.com")
	codex := add(t, s, "codex", "b@x.com")
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/claude" {
			if r.Header.Get("Authorization") != "Bearer tok-a@x.com" {
				w.WriteHeader(401)
				return
			}
			w.Write([]byte(`{"five_hour":{"utilization":42,"resets_at":"2026-10-13T10:00:00Z"},"seven_day":{"utilization":7,"resets_at":null}}`))
			return
		}
		if r.Header.Get("ChatGPT-Account-Id") != "acct-b@x.com" {
			w.WriteHeader(401)
			return
		}
		w.Write([]byte(`{"rate_limit":{"primary_window":{"used_percent":12,"reset_at":1760000000},"secondary_window":{"used_percent":80,"reset_at":1760500000}}}`))
	}))
	defer srv.Close()
	t.Cleanup(func(claude, codex string) func() {
		return func() { ClaudeUsageURL, CodexUsageURL = claude, codex }
	}(ClaudeUsageURL, CodexUsageURL))
	ClaudeUsageURL, CodexUsageURL = srv.URL+"/claude", srv.URL+"/codex"
	s.Refresh(context.Background(), "")
	list := s.List(none)
	a, _ := find(list, claude)
	if a.Usage == nil || a.Usage.Session.Percent != 42 || a.Usage.Session.ResetsAt != time.Date(2026, 10, 13, 10, 0, 0, 0, time.UTC).UnixMilli() || a.Usage.Week.Percent != 7 || a.Usage.Week.ResetsAt != 0 {
		t.Fatalf("%+v", a.Usage)
	}
	b, _ := find(list, codex)
	if b.Usage == nil || b.Usage.Session.Percent != 12 || b.Usage.Week.ResetsAt != 1760500000000 {
		t.Fatalf("%+v", b.Usage)
	}

	srv.Config.Handler = http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(401) })
	s.Refresh(context.Background(), claude)
	list = s.List(none)
	if a, _ := find(list, claude); !a.Usage.Stale || a.Usage.Session.Percent != 42 {
		t.Fatalf("a failed fetch lost the last values: %+v", a.Usage)
	}
	if b, _ := find(list, codex); b.Usage.Stale {
		t.Fatal("refreshing one account touched another")
	}
}
